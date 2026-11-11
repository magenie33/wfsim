// SPDX-License-Identifier: AGPL-3.0-or-later
// Ordinary reviews retain the baseline; explicit corrections retire it until independent agreement.
import { facts, DEADLINE_MS, factoryAuthed } from "./factory.js";
import { calculationId } from "./board_tasks.js";
import { questionId } from "./tasks.js";
import { sameSite, limited } from "./accounts.js";
import { json, servedRelease, ENGINE_ID } from "./verify.js";
import { iso } from "./instant.js";
import { scoreWitnessWrites } from "./computation_records.js";

export const BOARD_REVIEW_COOLDOWN_MS = 3_600_000;
const ID = /^[a-f0-9]{32}$/, SLUG = /^[a-z0-9_]{1,128}$/, UUID = /^[a-f0-9-]{36}$/;
export const BOARD_REVIEW_PRODUCER = {
  wanted_in_sql: true,
  still_wanted: async (env, demand) => !!(await env.LIBRARY.prepare("SELECT board_review_wanted(?, ?) AS wanted").bind(demand.ref, demand.question).first()).wanted,
  wanted_many: async (env, demands) => {
    const { results } = await env.LIBRARY.prepare(`SELECT x.ref FROM jsonb_to_recordset((?::text)::jsonb) x(ref text, question text)
      WHERE board_review_wanted(x.ref, x.question)`).bind(JSON.stringify(demands.map(d => ({ ref: d.ref, question: d.question })))).all();
    return new Set(results.map(row => row.ref));
  },
  on_fact_writes(env, demand, fact) {
    if (!fact.result) return [];
    const at = iso(Date.now()), db = env.LIBRARY;
    return [db.prepare(`UPDATE entity_measurement_reviews SET result_engine=?1::text WHERE id=?2::text
      AND state='waiting' AND (task_id IS NULL OR task_id=?3::bigint)`).bind(fact.engine,demand.payload.review,fact.task_id),
      db.prepare(`WITH applied AS (
      UPDATE state_board_measurement_selection s SET score = ?1::double precision, metric = ?2::text, measured_by = ?3::text,
        started_at = ?4::text, finished_at = ?4::text, cost_seconds = 0, review_id = r.id
      FROM entity_measurement_reviews r WHERE r.ref = ?5::text AND r.id = ?6::text AND r.question = ?7::text AND r.state = 'waiting' AND (r.task_id IS NULL OR r.task_id=?9::bigint)
        AND s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode
        AND s.score = r.baseline_score AND s.measured_by = r.baseline_by AND s.finished_at = r.baseline_finished_at
        AND (s.review_id IS NOT DISTINCT FROM r.baseline_review_id OR s.review_id = r.id)
      RETURNING s.identity)
      UPDATE entity_measurement_reviews SET state = CASE WHEN EXISTS (SELECT 1 FROM applied) THEN 'succeeded' ELSE 'superseded' END,
        completed_at = ?4::text, result_engine=?8::text,task_id=coalesce(task_id,?9::bigint) WHERE ref = ?5::text AND id = ?6::text AND question = ?7::text AND state = 'waiting' AND (task_id IS NULL OR task_id=?9::bigint)`)
      .bind(fact.result.score, fact.result.metric, `verified:${fact.engine}`, at, demand.ref, demand.payload.review, demand.question, fact.engine, fact.task_id),
      ...scoreWitnessWrites(db, demand.payload.identity, demand.payload.ruler, demand.payload.mode, at, fact),
      db.prepare(`DELETE FROM state_board_pending_work q USING entity_measurement_reviews r WHERE r.id = ? AND r.state = 'succeeded'
        AND q.build_id = r.identity AND q.ruler = r.ruler AND q.mode = r.mode`).bind(demand.payload.review)];
  },
  async on_retract(env, demand) {
    await env.LIBRARY.batch([
      env.LIBRARY.prepare(`WITH RECURSIVE prior AS (
        SELECT r.* FROM entity_measurement_reviews r WHERE r.ref = ?1::text AND r.question = ?2::text AND r.id=?3::text
        UNION ALL SELECT older.* FROM prior p JOIN entity_measurement_reviews older ON older.id = p.baseline_review_id
          WHERE older.state <> 'succeeded'
      ), restored AS (SELECT p.* FROM prior p WHERE p.invalidated_at IS NULL AND
        ((p.baseline_review_id IS NULL AND p.baseline_by NOT LIKE 'verified:%')
        OR EXISTS (SELECT 1 FROM entity_measurement_reviews proof WHERE proof.id = p.baseline_review_id AND proof.state = 'succeeded')))
        UPDATE state_board_measurement_selection s SET score = r.baseline_score, metric = r.baseline_metric, measured_by = r.baseline_by,
        started_at = r.baseline_started_at, finished_at = r.baseline_finished_at, cost_seconds = r.baseline_cost_seconds, review_id = r.baseline_review_id
        FROM restored r WHERE s.review_id = ?3::text
          AND s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode`).bind(demand.ref, demand.question, demand.payload.review),
      env.LIBRARY.prepare(`INSERT INTO entity_compute_batches (id, at, why, total)
        SELECT 'review-repair:' || r.id, ?1::text, 'replace an invalidated board review', 1 FROM entity_measurement_reviews r
        WHERE r.ref = ?2::text AND r.question = ?3::text AND r.id=?4::text AND NOT EXISTS
          (SELECT 1 FROM state_board_measurement_selection s WHERE s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode)
        ON CONFLICT DO NOTHING`).bind(iso(Date.now()), demand.ref, demand.question, demand.payload.review),
      env.LIBRARY.prepare(`INSERT INTO state_board_pending_work (batch, build_id, ruler, mode)
        SELECT 'review-repair:' || r.id, r.identity, r.ruler, r.mode FROM entity_measurement_reviews r
        WHERE r.ref = ? AND r.question = ? AND r.id=? AND NOT EXISTS
          (SELECT 1 FROM state_board_measurement_selection s WHERE s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode)
        ON CONFLICT DO NOTHING`).bind(demand.ref, demand.question, demand.payload.review),
      env.LIBRARY.prepare(`UPDATE entity_measurement_reviews r SET state = 'failed', completed_at = ?1::text,
        invalidated_at = CASE WHEN EXISTS (SELECT 1 FROM state_board_measurement_selection s WHERE s.review_id = r.id) THEN ?1::text ELSE invalidated_at END
        WHERE ref = ?2::text AND question = ?3::text AND id=?4::text`).bind(iso(Date.now()), demand.ref, demand.question, demand.payload.review),
    ]);
  },
};

export async function boardReviewsRoute(request, env, path, ctx) {
  const db = env.LIBRARY;
  if (!db) return json({ ok: false, error: "Board review is unavailable." }, 503);
  if (path === "/api/board/retire") return retireScores(request, env);
  if (request.method !== "POST" || !sameSite(request)) return json({ ok: false, error: "bad request" }, 403);
  const raw = await request.text();
  if (raw.length > 8192) return json({ ok: false, error: "payload too large" }, 413);
  let body;
  try { body = JSON.parse(raw); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!body || typeof body !== "object" || Array.isArray(body)) return json({ ok: false, error: "bad request" }, 400);
  if (path === "/api/board/calibrate") {
    if (!SLUG.test(body.weapon || "") || !SLUG.test(body.ruler || "") || !ENGINE_ID.test(body.engine || "")
        || Object.keys(body).some(key => !["weapon", "ruler", "engine"].includes(key)))
      return json({ ok: false, error: "invalid board" }, 400);
    return json({ ok: true, accepted: false, disabled: true });
  }
  if (path === "/api/board/review/status") {
    if (!Array.isArray(body.ids) || !body.ids.length || body.ids.length > 50 || !body.ids.every(id => UUID.test(id)))
      return json({ ok: false, error: "invalid review ids" }, 400);
    return reviewStatus(env, body.ids);
  }
  if (path !== "/api/board/review") return json({ ok: false, error: "not found" }, 404);
  if (!ID.test(body.identity || "") || !SLUG.test(body.ruler || "") || !SLUG.test(body.mode || "") || !ENGINE_ID.test(body.engine || "")
      || !Number.isFinite(body.observed_score) || !Number.isFinite(body.baseline_score) || !SLUG.test(body.metric || "")
      || Object.keys(body).some(key => !["identity", "ruler", "mode", "engine", "observed_score", "baseline_score", "metric"].includes(key)))
    return json({ ok: false, error: "invalid board row" }, 400);
  const served = await servedRelease(env);
  if (!served.engine || served.engine !== body.engine) return json({ ok: false, error: "The engine changed. Run this calculation again from its page." }, 409);
  if (env.FACTORY_PAUSED === "1") return json({ ok: false, error: "Community execution is paused." }, 503);
  if (await limited(env, request, "board-review")) return json({ ok: false, error: "Too many review requests. Try again later." }, 429);
  return requestReview(env, body);
}
async function requestReview(env, body, captured = null, repair = false) {
  const db = env.LIBRARY;
  const baseline = captured || await db.prepare(`SELECT s.*, b.record FROM state_board_measurement_selection s JOIN entity_weapon_builds b ON b.id = s.identity
    WHERE s.identity = ? AND s.ruler = ? AND s.mode = ?`).bind(body.identity, body.ruler, body.mode).first();
  if (!baseline) return json({ ok: false, error: "This row is no longer on the board." }, 404);
  if (!captured && !repair) {
    if (baseline.score !== body.baseline_score || baseline.metric !== body.metric)
      return json({ ok: false, error: "The board changed. Run this calculation again." }, 409);
    if (Math.abs(body.observed_score - baseline.score) <= 1e-12 * Math.max(Math.abs(body.observed_score), Math.abs(baseline.score)))
      return json({ ok: true, accepted: false, skipped: true });
  }
  const observation = captured || repair ? null : { score: body.observed_score, metric: body.metric,
    engine: body.engine, baseline_score: baseline.score, baseline_finished_at: baseline.finished_at };
  const ref = calculationId(body.identity, body.ruler, body.mode), now = Date.now();
  await closeWithdrawn(env, ref);
  const round = repair ? iso(now) : iso(Math.floor(now / BOARD_REVIEW_COOLDOWN_MS) * BOARD_REVIEW_COOLDOWN_MS);
  const old = await db.prepare("SELECT * FROM entity_measurement_reviews WHERE ref = ? ORDER BY (state='waiting') DESC,created_at DESC LIMIT 1").bind(ref).first();
  if (old && (old.state === "waiting" || (!repair && Date.parse(old.created_at) > now - BOARD_REVIEW_COOLDOWN_MS))) {
    if (old.state === "waiting" && !repair) await askReview(env, old, true, observation);
    return json({ ok: true, accepted: false, id: old.id, next_ms: 10_000 });
  }
  if (!repair && (await db.prepare("SELECT count(*) AS n FROM entity_measurement_reviews WHERE engine = ? AND state = 'waiting'").bind(body.engine).first()).n >= 1000)
    return json({ ok: false, error: "The review queue is full. Try again later." }, 429);
  const id = crypto.randomUUID();
  const input = { record: JSON.parse(baseline.record), ruler: body.ruler, mode: body.mode, review: id };
  const question = await questionId("simulate", input);
  const stored = await db.prepare(`INSERT INTO entity_measurement_reviews (ref, id, round_at, identity, ruler, mode, engine, question, record, state,
      baseline_score, baseline_metric, baseline_cost_seconds, baseline_review_id, baseline_by, baseline_started_at, baseline_finished_at, created_at)
    SELECT ?1::text, ?2::text, ?12::text, s.identity, s.ruler, s.mode, ?3::text, ?4::text, ?13::text, 'waiting', s.score, s.metric,
      s.cost_seconds, s.review_id, s.measured_by, s.started_at, s.finished_at, ?5::text FROM state_board_measurement_selection s
      WHERE s.identity = ?6::text AND s.ruler = ?7::text AND s.mode = ?8::text
      AND s.finished_at = ?9::text AND s.measured_by = ?10::text AND s.score = ?11::double precision
    ON CONFLICT DO NOTHING RETURNING *`).bind(ref, id, body.engine, question, iso(now), body.identity, body.ruler, body.mode,
      baseline.finished_at, baseline.measured_by, baseline.score, round, baseline.record).first();
  const review = stored || await db.prepare("SELECT * FROM entity_measurement_reviews WHERE ref = ? ORDER BY (state='waiting') DESC,created_at DESC LIMIT 1").bind(ref).first();
  if (!review) return json({ ok: false, error: "The board changed. Run this calculation again." }, 409);
  if (review.state === "waiting" && !repair) await askReview(env, review, true, observation);
  return json({ ok: true, accepted: review.id === id, id: review.id, next_ms: 10_000 });
}

// Retirement is an operator's correction, never a client-supplied score.
async function retireScores(request, env) {
  if (request.method !== 'POST' || !factoryAuthed(request, env)) return json({ok:false,error:'forbidden'},403);
  const raw = await request.text();
  if (raw.length > 65536) return json({ok:false,error:'payload too large'},413);
  let body;
  try { body = JSON.parse(raw); } catch (_) { return json({ok:false,error:'not json'},400); }
  const served = await servedRelease(env), db = env.LIBRARY;
  if (!body || typeof body !== 'object' || Array.isArray(body)) return json({ok:false,error:'bad request'},400);
  let rows, after = null;
  if (body.repair === true) {
    if (Object.keys(body).some(k => !['repair','after'].includes(k))
      || (body.after !== undefined && (!Array.isArray(body.after) || body.after.length !== 3 || !body.after.every(x => typeof x === 'string'))))
      return json({ok:false,error:'invalid repair cursor'},400);
    const cursor = body.after || ['','',''];
    ({results:rows} = await db.prepare(`SELECT s.identity,s.ruler,s.mode,r.invalidation_reason AS reason
      FROM state_board_measurement_selection s JOIN entity_measurement_reviews r ON r.id=s.review_id
      WHERE r.invalidated_at IS NOT NULL AND r.state <> 'succeeded' AND (?1::text IS NOT NULL AND r.state <> 'waiting'
        OR NOT EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.producer='board_review' AND d.ref=r.ref AND d.question=r.question AND d.state<>'withdrawn'))
        AND (s.identity,s.ruler,s.mode) > (?2::text,?3::text,?4::text)
      ORDER BY s.identity,s.ruler,s.mode LIMIT 50`).bind(served.engine,...cursor).all());
    after = rows.length === 50 ? [rows.at(-1).identity,rows.at(-1).ruler,rows.at(-1).mode] : null;
  } else {
    if (Object.keys(body).some(k => !['rows','engine','reason'].includes(k))
      || !Array.isArray(body.rows) || !body.rows.length || body.rows.length > 50
      || typeof body.reason !== 'string' || !body.reason.trim() || body.reason.length > 512
      || !body.rows.every(r => r && ID.test(r.identity || '') && SLUG.test(r.ruler || '') && SLUG.test(r.mode || '')
        && (r.measured_at === undefined || (typeof r.measured_at === 'string' && /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.test(r.measured_at) && Number.isFinite(Date.parse(r.measured_at))))
        && Object.keys(r).every(k => ['identity','ruler','mode','measured_at'].includes(k)))) return json({ok:false,error:'invalid retirement'},400);
    if (body.engine !== served.engine) return json({ok:false,error:'The engine changed.'},409);
    rows = body.rows.map(r => ({...r,reason:body.reason.trim()}));
  }
  return retireBoardRows(env,rows,served.engine,after);
}

export async function retireBoardRows(env,rows,engine,after=null) {
  const db=env.LIBRARY,reviews=[];
  for (const row of rows) {
    const captured=await db.prepare(`SELECT s.*,b.record FROM state_board_measurement_selection s JOIN entity_weapon_builds b ON b.id=s.identity
      WHERE s.identity=?1::text AND s.ruler=?2::text AND s.mode=?3::text AND (?4::text IS NULL OR s.finished_at=?4::text)`)
      .bind(row.identity,row.ruler,row.mode,row.measured_at || null).first();
    if (!captured) continue;
    const completed = await db.prepare(`SELECT r.id FROM entity_measurement_reviews r JOIN board_facts s
      ON s.identity=r.identity AND s.ruler=r.ruler AND s.mode=r.mode
      WHERE r.ref=?1::text||'|'||?2::text||'|'||?3::text AND r.state='succeeded' AND r.engine=?4::text AND r.invalidation_reason=?5::text
        AND s.retired_at IS NULL AND s.finished_at>=r.completed_at ORDER BY r.completed_at DESC LIMIT 1`)
      .bind(row.identity,row.ruler,row.mode,engine,row.reason).first();
    if (completed) {
      reviews.push({identity:row.identity,ruler:row.ruler,mode:row.mode,id:completed.id});
      continue;
    }
    const response = await requestReview(env,{identity:row.identity,ruler:row.ruler,mode:row.mode,engine},captured,true);
    const result = await response.json();
    if (response.status === 404) continue;
    if (!response.ok) return json({...result,reviews,after},response.status);
    const at = iso(Date.now());
    await db.batch([
      db.prepare(`UPDATE entity_measurement_reviews SET invalidated_at=coalesce(invalidated_at,?1::text),invalidation_reason=coalesce(invalidation_reason,?2::text)
        WHERE id=?3::text AND state='waiting'`).bind(at,row.reason || 'retry an invalidated measurement',result.id),
      db.prepare(`UPDATE state_board_measurement_selection s SET review_id=r.id FROM entity_measurement_reviews r WHERE r.id=?1::text AND r.state='waiting'
        AND s.identity=r.identity AND s.ruler=r.ruler AND s.mode=r.mode
        AND s.score=r.baseline_score AND s.measured_by=r.baseline_by AND s.finished_at=r.baseline_finished_at
        AND (s.review_id IS NOT DISTINCT FROM r.baseline_review_id OR s.review_id=r.id)`).bind(result.id),
    ]);
    const review = await db.prepare('SELECT * FROM entity_measurement_reviews WHERE id=?').bind(result.id).first();
    if (review?.state === 'waiting') await askReview(env,review);
    reviews.push({identity:row.identity,ruler:row.ruler,mode:row.mode,id:result.id});
  }
  return json({ok:true,reviews,after});
}

// Retained imports and old clients cannot create version-based calibration.
export async function calibrateBoard() {}
export async function triggerBoardCalibration() {}
async function askReview(env, row, waited = false, observation = null) {
  const prior = await env.LIBRARY.prepare("SELECT waits,payload FROM relation_task_consumers WHERE producer='board_review' AND ref=? AND question=?")
    .bind(row.ref,row.question).first();
  observation = prior ? JSON.parse(prior.payload).observation || null : observation;
  const waits = waited || !row.invalidated_at || prior?.waits === 1;
  await facts().demand(env, { producer: "board_review", ref: row.ref, verb: "simulate",
    request: { record: JSON.parse(row.record), ruler: row.ruler, mode: row.mode, review: row.id }, due_in_ms: waits ? DEADLINE_MS.board_review : DEADLINE_MS.sweep,
    payload: { identity: row.identity, ruler: row.ruler, mode: row.mode, review: row.id, ...(observation ? { observation } : {}) }, force: true, invalidate: !!row.invalidated_at, waits, initiator: waits ? "anonymous" : "system" },
  row.engine, Date.parse(row.created_at));
  await env.LIBRARY.prepare(`UPDATE entity_measurement_reviews r SET task_id=q.task_id FROM state_compute_scheduling q
    WHERE r.id=?1::text AND r.task_id IS NULL AND q.id=r.question`).bind(row.id).run();
}
async function reviewStatus(env, ids) {
  await closeWithdrawn(env);
  const now = iso(Date.now());
  const { results } = await env.LIBRARY.prepare(`SELECT r.*, q.state AS phase, proof.canon, q.lease_until,
      (SELECT count(DISTINCT a.owner) FROM record_candidate_answers a JOIN relation_task_answer_evidence e ON e.answer_id=a.id
        WHERE e.task_id=coalesce(r.task_id,q.task_id) AND a.void = 0) AS witnesses,
      s.score AS current_score, s.metric AS current_metric, s.measured_by AS current_by
    FROM entity_measurement_reviews r LEFT JOIN state_compute_work q ON q.id = r.question
    LEFT JOIN LATERAL(SELECT canon FROM entity_compute_results WHERE task_id=coalesce(r.task_id,q.task_id) ORDER BY id DESC LIMIT 1) proof ON true
    LEFT JOIN state_board_measurement_selection s ON s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode
    WHERE r.id IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))`).bind(JSON.stringify(ids)).all();
  return json({ ok: true, next_ms: 10_000, reviews: results.map(row => ({ id: row.id, identity: row.identity, ruler: row.ruler, mode: row.mode,
    record: JSON.parse(row.record),
    state: row.state === "waiting" && row.phase === "withdrawn" ? "failed" : row.state,
    phase: ["disputed", "nondeterministic", "spot"].includes(row.phase) ? "Checking" : row.lease_until > now ? "Computing and verifying" : "Waiting for community compute",
    witnesses: Math.min(2, row.witnesses), created_at: row.created_at, completed_at: row.completed_at,
    engine: row.result_engine || row.engine, score: row.state === "succeeded" && row.canon ? JSON.parse(row.canon).score : null,
    baseline: row.baseline_score, current_score: row.current_score, metric: row.current_metric, current_by: row.current_by })) });
}
async function closeWithdrawn(env, ref = null) {
  await env.LIBRARY.prepare(`UPDATE entity_measurement_reviews r SET state = 'failed', completed_at = ?1::text
    WHERE state = 'waiting' AND (?2::text IS NULL OR ref = ?2::text)
      AND EXISTS (SELECT 1 FROM state_compute_work q WHERE q.id = r.question AND q.state = 'withdrawn')`)
    .bind(iso(Date.now()), ref).run();
}
