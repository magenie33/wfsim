// SPDX-License-Identifier: AGPL-3.0-or-later
// Browser-owned requests consume facts from the same fair factory as the board.
import { facts, DEADLINE_MS } from "./factory.js";
import { canon, questionId, witnessOf } from "./tasks.js";
import { sameSite, sha256, limited } from "./accounts.js";
import { json, servedRelease, netOf, unrecognised, ENGINE_ID, LEASE_ID, QUESTION_ID, countsOf } from "./verify.js";
import { dialsOf } from "./dials.js";
import { iso } from "./instant.js";
import { COMPUTATION_KEEP_MS, purgeComputationRecords } from "./computation_records.js";
import { answerDeliverySource, localDeliverySource } from "./delivery_sources.js";

export const USER_TASK_KEEP_MS = 30 * 86_400_000;
const taskKeepMs = kind => kind === "simulation" ? USER_TASK_KEEP_MS : COMPUTATION_KEEP_MS;
const UUID = /^[a-f0-9-]{36}$/;
const OWNER = /^[a-f0-9]{64}$/;
const VERBS = { simulation: "simulate_report", optimization: "optimize_report", appraisal: "optimize_report" };
// Wall-clock cost is observation, never part of the reproduced calculation.
export function calculationReport(value) {
  if (Array.isArray(value)) return value.map(calculationReport);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value).filter(([key]) => !["cpu_ms", "elapsed_ms", "elapsed_s", "elapsed_seconds"].includes(key))
    .map(([key, item]) => [key, calculationReport(item)]));
}
export function reportCanon(result, work) { return canon({ report: calculationReport(result.report || null), work }); }
const wanted = async (env, demand) => {
  const row = await env.LIBRARY.prepare("SELECT engine FROM entity_user_calculation_requests WHERE id = ? AND state = 'waiting' AND expires_at > ?")
    .bind(demand.ref, iso(Date.now())).first();
  return !!row && row.engine === (await servedRelease(env)).engine;
};
export const USER_TASK_PRODUCERS = { user_calculation: {
  still_wanted: wanted,
  on_fact: async (env, demand, fact) => {
    const report = calculationReport(fact.result?.report);
    if (!report) return;
    const completedAt = fact.at || iso(Date.now());
    await env.LIBRARY.batch([
      env.LIBRARY.prepare("SELECT compute_select_report(?1::text,?2::text,?3::text)")
        .bind(demand.question,fact.engine,completedAt),
      env.LIBRARY.prepare(`INSERT INTO relation_calculation_report_access(id,owner,question,engine,task,kind,created_at,completed_at,expires_at)
        SELECT t.id,t.owner,t.question,t.engine,t.task,t.kind,t.created_at,r.at,
          to_char((r.retained_from::timestamptz+(CASE WHEN t.kind='simulation' THEN ?1::bigint ELSE ?4::bigint END)*interval '1 millisecond')
            AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
        FROM entity_user_calculation_requests t JOIN state_current_calculation_reports r ON r.question=t.question AND r.engine=t.engine
        WHERE t.id=?2::text AND t.state='waiting' AND t.engine=?3::text ON CONFLICT(id) DO NOTHING`)
        .bind(USER_TASK_KEEP_MS,demand.ref,fact.engine,COMPUTATION_KEEP_MS),
      env.LIBRARY.prepare(`UPDATE entity_user_calculation_requests t SET state='succeeded',completed_at=r.at,
          expires_at=to_char((r.retained_from::timestamptz + (CASE WHEN t.kind='simulation' THEN ?1::bigint ELSE ?4::bigint END)
            * interval '1 millisecond') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
        FROM state_current_calculation_reports r WHERE t.id=?2::text AND t.state='waiting' AND t.engine=?3::text AND r.question=t.question AND r.engine=t.engine`)
        .bind(USER_TASK_KEEP_MS, demand.ref, fact.engine, COMPUTATION_KEEP_MS),
    ]);
  },
} };
export function userCalculationWork(leased) {
  return { kind: "user_calculation", lease: leased.lease, question: leased.question,
    calculation: leased.verb === "simulate_report" ? "simulation" : "optimization", request: leased.request };
}
export async function purgeUserCalculations(env, now = Date.now()) {
  const db = env.LIBRARY;
  if (!db) return;
  await purgeComputationRecords(env, now);
  const { results } = await db.prepare("SELECT id FROM entity_user_calculation_requests WHERE expires_at < ? AND state = 'waiting' LIMIT 100").bind(iso(now)).all();
  for (const row of results) await facts().withdraw(env, "user_calculation", row.id, now);
  await db.prepare("DELETE FROM entity_user_calculation_requests WHERE expires_at < ? AND state <> 'waiting'").bind(iso(now)).run();
  await db.prepare("DELETE FROM state_request_cancellation_tombstones WHERE expires_at < ?").bind(iso(now)).run();
  await db.prepare("UPDATE entity_user_calculation_requests SET state = 'expired' WHERE expires_at < ?").bind(iso(now)).run();
  const { results: orphaned } = await db.prepare(`SELECT q.id FROM state_compute_work q WHERE q.verb IN ('simulate_report', 'optimize_report')
      AND NOT EXISTS (SELECT 1 FROM entity_user_calculation_requests t WHERE t.question = q.id)
      AND NOT EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.question = q.id AND d.producer = 'family_build')`).all();
  for (const row of orphaned) {
    const unowned = `NOT EXISTS (SELECT 1 FROM entity_user_calculation_requests t WHERE t.question = ?2::text)
      AND NOT EXISTS (SELECT 1 FROM relation_task_consumers d WHERE d.question = ?2::text AND d.producer = 'family_build')`;
    const tables = ["relation_task_consumers", "record_candidate_answers", "record_engine_refusals", "state_pending_verifications", "state_task_answer_heads"];
    await db.batch([
      db.prepare("SELECT id FROM state_compute_work WHERE id = ? FOR UPDATE").bind(row.id),
      ...tables.map((table) => db.prepare(`DELETE FROM ${table} WHERE question = ?1::text AND ${unowned}`).bind(row.id, row.id)),
      db.prepare(`DELETE FROM state_compute_work WHERE id = ?1::text AND ${unowned}`).bind(row.id, row.id),
    ]);
  }
  await db.prepare("DELETE FROM relation_calculation_report_access WHERE expires_at<=?").bind(iso(now)).run();
  await db.prepare(`DELETE FROM state_calculation_report_selection r WHERE NOT EXISTS
    (SELECT 1 FROM relation_calculation_report_access a WHERE a.question=r.question AND a.engine=r.engine)`).run();
  await db.prepare(`DELETE FROM entity_compute_results r USING entity_calculation_reports p WHERE p.result_id=r.id
    AND NOT EXISTS(SELECT 1 FROM relation_calculation_report_access a WHERE a.question=p.question AND a.engine=p.engine)
    AND NOT EXISTS(SELECT 1 FROM state_compute_scheduling s WHERE s.id=p.question)`).run();
  await db.prepare(`DELETE FROM entity_calculation_reports p WHERE p.origin='imported'
    AND NOT EXISTS(SELECT 1 FROM relation_calculation_report_access a WHERE a.question=p.question AND a.engine=p.engine)
    AND NOT EXISTS(SELECT 1 FROM entity_user_calculation_requests t WHERE t.question=p.question AND t.engine=p.engine)`).run();
  await db.prepare(`DELETE FROM entity_compute_tasks t USING entity_compute_specs s WHERE s.id=t.computation
    AND s.verb IN ('simulate_report','optimize_report')
    AND NOT EXISTS(SELECT 1 FROM state_compute_scheduling q WHERE q.id=s.id)
    AND NOT EXISTS(SELECT 1 FROM entity_compute_results r WHERE r.computation=s.id)
    AND NOT EXISTS(SELECT 1 FROM record_candidate_answers a WHERE a.task_id=t.id)
    AND NOT EXISTS(SELECT 1 FROM relation_task_answer_evidence e WHERE e.task_id=t.id)`).run();
  await db.prepare(`DELETE FROM entity_compute_specs s WHERE s.verb IN ('simulate_report','optimize_report')
    AND NOT EXISTS(SELECT 1 FROM entity_compute_tasks t WHERE t.computation=s.id)
    AND NOT EXISTS(SELECT 1 FROM entity_compute_results r WHERE r.computation=s.id)
    AND NOT EXISTS(SELECT 1 FROM relation_calculation_report_access a WHERE a.question=s.id)`).run();
}
export async function userTasksRoute(request, env, path) {
  const now = Date.now(), db = env.LIBRARY;
  if (path === "/api/user-tasks/capabilities" && request.method === "GET")
    return json({ ok: true, available: !!db && env.FACTORY_PAUSED !== "1", kinds: Object.keys(VERBS), keep_days: USER_TASK_KEEP_MS / 86_400_000,
      optimization_keep_days: COMPUTATION_KEEP_MS / 86_400_000 });
  if (!db) return json({ ok: false, error: "Community execution is unavailable." }, 503);
  if (!sameSite(request) || request.method !== "POST") return json({ ok: false, error: "bad request" }, 403);
  const raw = await request.text();
  if (raw.length > (path.endsWith("/answer") ? 8_388_608 : 131_072)) return json({ ok: false, error: "payload too large" }, 413);
  let body;
  try { body = JSON.parse(raw); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  if (!body || typeof body !== "object" || Array.isArray(body)) return json({ ok: false, error: "bad request" }, 400);
  if (path === "/api/user-tasks/answer") return answer(request, env, body);
  if (!OWNER.test(body.owner || "")) return json({ ok: false, error: "needs browser ownership" }, 401);
  const owner = await sha256(body.owner);
  if (path === "/api/user-tasks/source") {
    if (await limited(env, request, "delivery_source")) return json({ ok: false, error: "slow down" }, 429);
    if (!UUID.test(body.task || "") || !ENGINE_ID.test(body.engine || "") || !/^[a-z0-9_]{1,100}$/.test(body.weapon || ""))
      return json({ ok: false, error: "invalid source" }, 400);
    return json({ ok: true, source: await localDeliverySource(env, request, body) });
  }
  if (path === "/api/user-tasks/accept") {
    if (!UUID.test(body.id || "") || !UUID.test(body.task || "") || !VERBS[body.kind] || !ENGINE_ID.test(body.engine || "")
        || !body.request || typeof body.request !== "object" || !body.request.weapon)
      return json({ ok: false, error: "invalid calculation" }, 400);
    if (!Number.isSafeInteger(body.request.runs) || body.request.runs < 1 || body.request.runs > 1_000_000
        || (body.kind !== "simulation" && (body.request.strategy !== "quick" || !Array.isArray(body.request.starts))))
      return json({ ok: false, error: "invalid calculation" }, 400);
    const served = await servedRelease(env);
    if (!served.engine || served.engine !== body.engine) return json({ ok: false, error: "The engine changed. Run this calculation again from its page." }, 409);
    if (env.FACTORY_PAUSED === "1") return json({ ok: false, error: "Community execution is paused." }, 503);
    const cancelled = await db.prepare("SELECT id FROM state_request_cancellation_tombstones WHERE id = ? AND owner = ?").bind(body.id, owner).first();
    if (cancelled) return json({ ok: false, error: "This community request has ended." }, 410);
    const old = await db.prepare("SELECT * FROM entity_user_calculation_requests WHERE id = ?").bind(body.id).first();
    // A completed cache entry cannot outlive its window through a new consumer.
    const original = await questionId(VERBS[body.kind], body.request);
    const keepMs = taskKeepMs(body.kind), execution = old?.question === original ? body.request : { ...body.request,
      __retention_window: Math.floor((old ? Date.parse(old.created_at) : now) / keepMs) };
    const question = await questionId(VERBS[body.kind], execution);
    if (old && (old.owner !== owner || old.question !== question || old.engine !== body.engine || old.task !== body.task))
      return json({ ok: false, error: "This command already describes another request." }, 409);
    const before=[];
    if (!old) {
      const count = await db.prepare("SELECT count(*) AS n FROM entity_user_calculation_requests t WHERE owner = ? AND state = 'waiting' AND expires_at > ? AND NOT EXISTS (SELECT 1 FROM state_compute_work q WHERE q.id = t.question AND q.state IN ('disputed', 'nondeterministic', 'withdrawn'))").bind(owner, iso(now)).first();
      if (count.n >= 100) return json({ ok: false, error: "This browser already has 100 pending calculations." }, 429);
      before.push(db.prepare("SELECT id FROM state_compute_work WHERE id = ? FOR UPDATE").bind(question),
        db.prepare("SELECT compute_admit_user_request(?1::text,?2::text,?3::text,?4::text,?5::text,?6::text,?7::text,?8::text)")
        .bind(body.id, body.task, owner, body.kind, body.engine, question, iso(now), iso(now + keepMs)),
      );
    }
    // Consumer ownership, frozen input and durable work are admitted in one transaction.
    if (!old || old.state === "waiting") {
      try {
        await facts().demand(env,{producer:"user_calculation",ref:body.id,verb:VERBS[body.kind],request:execution,
          due_in_ms:DEADLINE_MS.user_calculation,waits:false,initiator:`user:${owner.slice(0,16)}`},
          body.engine,old ? Date.parse(old.created_at) : now,{before});
      } catch (error) {
        if (error.message.includes("user calculation already cancelled"))
          return json({ok:false,error:"This community request has ended."},410);
        if (error.code==="23505" && error.message.includes("user calculation identity conflict"))
          return json({ok:false,error:"This command already describes another request."},409);
        throw error;
      }
    }
    const stored = await db.prepare("SELECT owner, question, task, engine FROM entity_user_calculation_requests WHERE id = ?").bind(body.id).first();
    if (stored.owner !== owner || stored.question !== question || stored.task !== body.task || stored.engine !== body.engine)
      return json({ ok: false, error: "This command already describes another request." }, 409);
    if (body.kind === "appraisal" && old?.kind === "optimization")
      await db.prepare("UPDATE entity_user_calculation_requests SET kind='appraisal' WHERE id=? AND owner=?").bind(body.id, owner).run();
    const kept = await db.prepare("SELECT expires_at FROM entity_user_calculation_requests WHERE id=?").bind(body.id).first();
    return json({ ok: true, id: body.id, question, keep_until: kept.expires_at });
  }
  const ids = Array.isArray(body.ids) ? body.ids : [body.id];
  if (!ids.length || ids.length > 100 || !ids.every((id) => UUID.test(id || ""))) return json({ ok: false, error: "invalid task ids" }, 400);
  if (path === "/api/user-tasks/cancel") {
    for (const id of ids) {
      const changed=await db.prepare("SELECT compute_cancel_user_request(?1::text,?2::text,?3::text,?4::text) AS changed")
        .bind(id,owner,iso(now),iso(now+USER_TASK_KEEP_MS)).first();
      if (changed.changed) await facts().withdraw(env,"user_calculation",id,now);
    }
  } else if (path !== "/api/user-tasks/status") return json({ ok: false, error: "not found" }, 404);
  const after = body.after || {};
  if (!after || typeof after !== "object" || Array.isArray(after) || Object.keys(after).length > 100
    || Object.entries(after).some(([id, n]) => !UUID.test(id) || !Number.isSafeInteger(n) || n < 0))
    return json({ ok: false, error: "invalid cursor" }, 400);
  const currentEngine = (await servedRelease(env)).engine;
  const { results } = await db.prepare(`WITH owned AS (
      SELECT t.* FROM entity_user_calculation_requests t WHERE t.owner=?1::text
      UNION ALL SELECT a.id,a.task,a.owner,a.kind,a.engine,a.question,'succeeded',a.created_at,a.completed_at,a.expires_at
        FROM relation_calculation_report_access a WHERE a.owner=?1::text
        AND NOT EXISTS(SELECT 1 FROM entity_user_calculation_requests t WHERE t.id=a.id))
    SELECT t.id, t.task, t.kind, t.question, t.state, t.engine, t.expires_at, t.completed_at, q.state AS question_state,
      q.leased_to IS NOT NULL AND q.lease_until > ?3::text AS computing, CASE WHEN ?5::boolean THEN r.result ELSE NULL END AS result,r.origin AS report_origin,
      a.id AS candidate,a.device,a.owner AS calculator,a.official,a.at,
      CASE WHEN ?5::boolean THEN a.result ELSE NULL END AS candidate_result
      FROM owned t LEFT JOIN state_compute_work q ON q.id = t.question
      LEFT JOIN state_current_calculation_reports r ON r.question = t.question AND r.engine = t.engine
      LEFT JOIN LATERAL (SELECT id,device,owner,official,at,result FROM record_candidate_answers
        WHERE question=t.question AND engine=t.engine AND void=0 AND t.kind='appraisal'
        AND id>COALESCE(((?4::text)::jsonb->>t.id)::bigint,0) ORDER BY at,id LIMIT 1) a ON true
      WHERE t.owner = ?1::text AND t.id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb))`)
    .bind(owner, JSON.stringify(ids), iso(now), JSON.stringify(after), body.include_results === true).all();
  return json({ ok: true, next_ms: 10_000, tasks: await Promise.all(results.map(async row => {
    const available = ["waiting", "succeeded"].includes(row.state) && Date.parse(row.expires_at) > now && row.engine === currentEngine && row.question_state !== "withdrawn";
    const candidate = available && row.candidate ? row.candidate : null;
    const report = candidate && body.include_results === true ? calculationReport(JSON.parse(row.candidate_result).report) : null;
    const source = report ? await answerDeliverySource(env, row.question, { engine: row.engine, owner: row.calculator,
      device: row.device, official: row.official, at: row.at }, report.weapon || report.results?.[0]?.replay?.weapon) : null;
    return { id: row.id, task: row.task, ...(candidate ? { candidate } : {}),
    state: row.state === "waiting" && currentEngine && row.engine !== currentEngine ? "failed" : Date.parse(row.expires_at) <= now ? "expired" : row.state === "waiting" && ["disputed", "nondeterministic", "withdrawn"].includes(row.question_state) ? "failed" : row.state,
    ...(row.state === "waiting" && currentEngine && row.engine !== currentEngine ? { error: "The engine changed. Run this calculation again from its page." } : {}),
    ...(row.state === "waiting" && row.question_state === "withdrawn" ? { error: "The engine refused this calculation." } : {}),
    execution_state: row.computing ? "running" : "queued",
    phase: row.computing ? "Computing and verifying" : "Waiting for community compute", engine: row.engine, keep_until: row.expires_at,
    ...(report ? { result: report, delivery: source } : body.include_results === true && row.state === "succeeded" && row.result && Date.parse(row.expires_at) > now
      ? { result: JSON.parse(row.result), computation: { question: row.question, engine: row.engine, completed_at: row.completed_at,origin:row.report_origin } } : {}) };
  })) });
}
async function answer(request, env, body) {
  const refused = unrecognised(body.verifier);
  if (refused) return refused;
  if (!LEASE_ID.test(body.lease || "") || !QUESTION_ID.test(body.question || "") || !ENGINE_ID.test(body.engine || "")
      || (body.refused !== true && (!body.report || body.report.ok === false || !Number.isSafeInteger(body.work) || body.work < 0)))
    return json({ ok: false, error: "invalid report" }, 400);
  if ((await servedRelease(env)).engine !== body.engine) return json({ ok: true, taken: false, stale: true });
  const question = await env.LIBRARY.prepare("SELECT verb, request FROM state_compute_work WHERE id = ? AND lease = ? AND leased_to = ?")
    .bind(body.question, body.lease, body.verifier).first();
  if (!question || !["simulate_report", "optimize_report"].includes(question.verb)) return json({ ok: true, taken: false });
  const who = await witnessOf(env, body.verifier);
  if (body.refused === true) {
    const dials = await dialsOf(env);
    await facts().refuseRows(env, { device: body.verifier, owner: who.owner }, body.engine, body.engine,
      [{ lease: body.lease, question: body.question, step: "calculation" }], dials.refusals_to_withdraw);
    return json({ ok: true, taken: true });
  }
  const input = JSON.parse(question.request), report = body.report;
  const complete = question.verb === "simulate_report"
    ? report.runs === input.runs && Array.isArray(report.run) && report.run.length === 2 && Number.isFinite(report.dps)
    : Array.isArray(report.results) && report.results.length <= input.finalists
      && report.results.every((row) => row.replay?.weapon === input.weapon && row.forma && Array.isArray(row.mods));
  if (report.ok !== true || !report.target || !Number.isFinite(report.duration) || !complete)
    return json({ ok: false, error: "needs a complete calculation report" }, 400);
  const result = await facts().answer(env, { device: body.verifier, owner: who.owner, net: await netOf(request, env) },
    { lease: body.lease, engine: body.engine, result: { report: calculationReport(body.report) }, work: body.work,
      compute_ms: Number.isSafeInteger(body.compute_ms) ? body.compute_ms : null, counts: countsOf(body.counts), lanes: body.lanes || 1 });
  return json({ ok: true, taken: result.taken });
}
