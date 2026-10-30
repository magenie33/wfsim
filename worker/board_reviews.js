// SPDX-License-Identifier: AGPL-3.0-or-later
// A fresh verification round of one stored board row; the previous score stays public until agreement.
import { facts, DEADLINE_MS } from "./factory.js";
import { rowRef } from "./board_tasks.js";
import { questionId } from "./tasks.js";
import { sameSite, limited } from "./accounts.js";
import { json, servedRelease, ENGINE_ID } from "./verify.js";
import { iso } from "./instant.js";

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
    return [db.prepare(`WITH applied AS (
      UPDATE scores s SET score = ?1::double precision, metric = ?2::text, measured_by = ?3::text,
        started_at = ?4::text, finished_at = ?4::text, cost_seconds = 0, review_id = r.id
      FROM board_reviews r WHERE r.ref = ?5::text AND r.id = ?6::text AND r.question = ?7::text AND r.state = 'waiting' AND r.engine = ?8::text
        AND s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode
        AND s.score = r.baseline_score AND s.measured_by = r.baseline_by AND s.finished_at = r.baseline_finished_at
      RETURNING s.identity)
      UPDATE board_reviews SET state = CASE WHEN EXISTS (SELECT 1 FROM applied) THEN 'succeeded' ELSE 'superseded' END,
        completed_at = ?4::text WHERE ref = ?5::text AND id = ?6::text AND question = ?7::text AND state = 'waiting'`)
      .bind(fact.result.score, fact.result.metric, `verified:${fact.engine}`, at, demand.ref, demand.payload.review, demand.question, fact.engine)];
  },
  async on_retract(env, demand) {
    await env.LIBRARY.batch([
      env.LIBRARY.prepare(`WITH RECURSIVE prior AS (
        SELECT r.* FROM board_reviews r WHERE r.ref = ?1::text AND r.question = ?2::text
        UNION ALL SELECT older.* FROM prior p JOIN board_reviews older ON older.id = p.baseline_review_id
          WHERE older.state <> 'succeeded'
      ), restored AS (SELECT p.* FROM prior p WHERE (p.baseline_review_id IS NULL AND p.baseline_by NOT LIKE 'verified:%')
        OR EXISTS (SELECT 1 FROM board_reviews proof WHERE proof.id = p.baseline_review_id AND proof.state = 'succeeded'))
        UPDATE scores s SET score = r.baseline_score, metric = r.baseline_metric, measured_by = r.baseline_by,
        started_at = r.baseline_started_at, finished_at = r.baseline_finished_at, cost_seconds = r.baseline_cost_seconds, review_id = r.baseline_review_id
        FROM restored r WHERE s.review_id = (SELECT id FROM board_reviews WHERE ref = ?1::text AND question = ?2::text)
          AND s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode`).bind(demand.ref, demand.question),
      env.LIBRARY.prepare(`DELETE FROM scores s USING board_reviews r WHERE r.ref = ? AND r.question = ?
        AND s.review_id = r.id AND s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode`)
        .bind(demand.ref, demand.question),
      env.LIBRARY.prepare(`INSERT INTO batches (id, at, why, total)
        SELECT 'review-repair:' || r.id, ?1::text, 'replace an invalidated board review', 1 FROM board_reviews r
        WHERE r.ref = ?2::text AND r.question = ?3::text AND NOT EXISTS
          (SELECT 1 FROM scores s WHERE s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode)
        ON CONFLICT DO NOTHING`).bind(iso(Date.now()), demand.ref, demand.question),
      env.LIBRARY.prepare(`INSERT INTO queue (batch, build_id, ruler, mode)
        SELECT 'review-repair:' || r.id, r.identity, r.ruler, r.mode FROM board_reviews r
        WHERE r.ref = ? AND r.question = ? AND NOT EXISTS
          (SELECT 1 FROM scores s WHERE s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode)
        ON CONFLICT DO NOTHING`).bind(demand.ref, demand.question),
      env.LIBRARY.prepare("UPDATE board_reviews SET state = 'failed', completed_at = ? WHERE ref = ? AND question = ?")
        .bind(iso(Date.now()), demand.ref, demand.question),
    ]);
  },
};

export async function boardReviewsRoute(request, env, path) {
  const db = env.LIBRARY;
  if (!db) return json({ ok: false, error: "Board review is unavailable." }, 503);
  if (request.method !== "POST" || !sameSite(request)) return json({ ok: false, error: "bad request" }, 403);
  const raw = await request.text();
  if (raw.length > 8192) return json({ ok: false, error: "payload too large" }, 413);
  let body;
  try { body = JSON.parse(raw); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!body || typeof body !== "object" || Array.isArray(body)) return json({ ok: false, error: "bad request" }, 400);
  if (path === "/api/board/review/status") {
    if (!Array.isArray(body.ids) || !body.ids.length || body.ids.length > 50 || !body.ids.every(id => UUID.test(id)))
      return json({ ok: false, error: "invalid review ids" }, 400);
    return reviewStatus(env, body.ids);
  }
  if (path !== "/api/board/review") return json({ ok: false, error: "not found" }, 404);
  if (!ID.test(body.identity || "") || !SLUG.test(body.ruler || "") || !SLUG.test(body.mode || "") || !ENGINE_ID.test(body.engine || "")
      || Object.keys(body).some(key => !["identity", "ruler", "mode", "engine"].includes(key)))
    return json({ ok: false, error: "invalid board row" }, 400);
  const served = await servedRelease(env);
  if (!served.engine || served.engine !== body.engine) return json({ ok: false, error: "The engine changed. Run this calculation again from its page." }, 409);
  if (env.FACTORY_PAUSED === "1") return json({ ok: false, error: "Community execution is paused." }, 503);
  if (await limited(env, request, "board-review")) return json({ ok: false, error: "Too many review requests. Try again later." }, 429);
  const baseline = await db.prepare(`SELECT s.*, b.record FROM scores s JOIN builds b ON b.id = s.identity
    WHERE s.identity = ? AND s.ruler = ? AND s.mode = ?`).bind(body.identity, body.ruler, body.mode).first();
  if (!baseline) return json({ ok: false, error: "This row is no longer on the board." }, 404);
  const ref = rowRef(body.identity, body.ruler, body.mode), now = Date.now();
  await closeWithdrawn(env, ref);
  const round = iso(Math.floor(now / BOARD_REVIEW_COOLDOWN_MS) * BOARD_REVIEW_COOLDOWN_MS);
  const old = await db.prepare("SELECT * FROM board_reviews WHERE ref = ? AND engine = ? ORDER BY created_at DESC LIMIT 1").bind(ref, body.engine).first();
  if (old?.engine === body.engine && (old.state === "waiting" || Date.parse(old.created_at) > now - BOARD_REVIEW_COOLDOWN_MS)) {
    if (old.state === "waiting") await askReview(env, old);
    return json({ ok: true, accepted: false, id: old.id, next_ms: 10_000 });
  }
  if ((await db.prepare("SELECT count(*) AS n FROM board_reviews WHERE engine = ? AND state = 'waiting'").bind(body.engine).first()).n >= 1000)
    return json({ ok: false, error: "The review queue is full. Try again later." }, 429);
  const id = crypto.randomUUID();
  const input = { record: JSON.parse(baseline.record), ruler: body.ruler, mode: body.mode, review: id };
  const question = await questionId("simulate", input);
  const stored = await db.prepare(`INSERT INTO board_reviews (ref, id, round_at, identity, ruler, mode, engine, question, record, state,
      baseline_score, baseline_metric, baseline_cost_seconds, baseline_review_id, baseline_by, baseline_started_at, baseline_finished_at, created_at)
    SELECT ?1::text, ?2::text, ?12::text, s.identity, s.ruler, s.mode, ?3::text, ?4::text, ?13::text, 'waiting', s.score, s.metric,
      s.cost_seconds, s.review_id, s.measured_by, s.started_at, s.finished_at, ?5::text FROM scores s
      WHERE s.identity = ?6::text AND s.ruler = ?7::text AND s.mode = ?8::text
      AND s.finished_at = ?9::text AND s.measured_by = ?10::text AND s.score = ?11::double precision
    ON CONFLICT DO NOTHING RETURNING *`).bind(ref, id, body.engine, question, iso(now), body.identity, body.ruler, body.mode,
      baseline.finished_at, baseline.measured_by, baseline.score, round, baseline.record).first();
  const review = stored || await db.prepare("SELECT * FROM board_reviews WHERE ref = ? AND engine = ? ORDER BY created_at DESC LIMIT 1").bind(ref, body.engine).first();
  if (!review) return json({ ok: false, error: "The board changed. Run this calculation again." }, 409);
  if (review.state === "waiting") await askReview(env, review);
  return json({ ok: true, accepted: review.id === id, id: review.id, next_ms: 10_000 });
}
async function askReview(env, row) {
  await facts().demand(env, { producer: "board_review", ref: row.ref, verb: "simulate",
    request: { record: JSON.parse(row.record), ruler: row.ruler, mode: row.mode, review: row.id }, due_in_ms: DEADLINE_MS.board_review,
    payload: { identity: row.identity, ruler: row.ruler, mode: row.mode, review: row.id }, waits: true, initiator: "anonymous" },
  row.engine, Date.parse(row.created_at));
}
async function reviewStatus(env, ids) {
  await closeWithdrawn(env);
  const engine = (await servedRelease(env)).engine, now = iso(Date.now());
  const { results } = await env.LIBRARY.prepare(`SELECT r.*, q.state AS phase, q.canon, q.lease_until,
      (SELECT count(DISTINCT a.owner) FROM answers a WHERE a.question = r.question AND a.void = 0 AND a.engine = r.engine) AS witnesses,
      s.score AS current_score, s.metric AS current_metric, s.measured_by AS current_by
    FROM board_reviews r LEFT JOIN questions q ON q.id = r.question
    LEFT JOIN scores s ON s.identity = r.identity AND s.ruler = r.ruler AND s.mode = r.mode
    WHERE r.id IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))`).bind(JSON.stringify(ids)).all();
  return json({ ok: true, next_ms: 10_000, reviews: results.map(row => ({ id: row.id, identity: row.identity, ruler: row.ruler, mode: row.mode,
    state: row.state === "waiting" && row.engine !== engine ? "failed" : row.state === "waiting" && row.phase === "withdrawn" ? "failed" : row.state,
    phase: ["disputed", "nondeterministic", "spot"].includes(row.phase) ? "Checking" : row.lease_until > now ? "Computing and verifying" : "Waiting for community compute",
    witnesses: Math.min(2, row.witnesses), created_at: row.created_at, completed_at: row.completed_at,
    engine: row.engine, score: row.state === "succeeded" && row.canon ? JSON.parse(row.canon).score : null,
    baseline: row.baseline_score, current_score: row.current_score, metric: row.current_metric, current_by: row.current_by })) });
}
async function closeWithdrawn(env, ref = null) {
  await env.LIBRARY.prepare(`UPDATE board_reviews r SET state = 'failed', completed_at = ?1::text
    WHERE state = 'waiting' AND (?2::text IS NULL OR ref = ?2::text)
      AND EXISTS (SELECT 1 FROM questions q WHERE q.id = r.question AND q.state = 'withdrawn')`)
    .bind(iso(Date.now()), ref).run();
}
