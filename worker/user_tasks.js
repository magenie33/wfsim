// SPDX-License-Identifier: AGPL-3.0-or-later
// Browser-owned requests consume facts from the same fair factory as the board.
import { facts, DEADLINE_MS } from "./factory.js";
import { canon, questionId, witnessOf } from "./tasks.js";
import { sameSite, sha256 } from "./accounts.js";
import { json, servedRelease, netOf, unrecognised, ENGINE_ID, LEASE_ID, QUESTION_ID, countsOf } from "./verify.js";
import { dialsOf } from "./dials.js";
import { iso } from "./instant.js";

export const USER_TASK_KEEP_MS = 30 * 86_400_000;
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
  const row = await env.LIBRARY.prepare("SELECT engine FROM user_calculations WHERE id = ? AND state = 'waiting' AND expires_at > ?")
    .bind(demand.ref, iso(Date.now())).first();
  return !!row && row.engine === (await servedRelease(env)).engine;
};
export const USER_TASK_PRODUCERS = { user_calculation: {
  still_wanted: wanted,
  on_fact: async (env, demand, fact) => {
    const report = calculationReport(fact.result?.report);
    if (!report) return;
    await env.LIBRARY.batch([
      env.LIBRARY.prepare("INSERT INTO calculation_reports (question, engine, result, at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING")
        .bind(demand.question, fact.engine, JSON.stringify(report), fact.at || iso(Date.now())),
      env.LIBRARY.prepare("UPDATE user_calculations SET state = 'succeeded', completed_at = ? WHERE id = ? AND state = 'waiting' AND engine = ?")
        .bind(iso(Date.now()), demand.ref, fact.engine),
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
  const { results } = await db.prepare("SELECT id FROM user_calculations WHERE expires_at < ? AND state = 'waiting' LIMIT 100").bind(iso(now)).all();
  for (const row of results) await facts().withdraw(env, "user_calculation", row.id, now);
  await db.prepare("DELETE FROM user_calculations WHERE expires_at < ? AND state <> 'waiting'").bind(iso(now)).run();
  await db.prepare("DELETE FROM user_calculation_cancellations WHERE expires_at < ?").bind(iso(now)).run();
  await db.prepare("UPDATE user_calculations SET state = 'expired' WHERE expires_at < ?").bind(iso(now)).run();
  const { results: orphaned } = await db.prepare(`SELECT q.id FROM questions q WHERE q.verb IN ('simulate_report', 'optimize_report')
      AND NOT EXISTS (SELECT 1 FROM user_calculations t WHERE t.question = q.id)`).all();
  for (const row of orphaned) {
    const unowned = "NOT EXISTS (SELECT 1 FROM user_calculations t WHERE t.question = ?2::text)";
    const tables = ["demands", "answers", "engine_refusals", "settle_queue", "led_questions"];
    await db.batch([
      db.prepare("SELECT id FROM questions WHERE id = ? FOR UPDATE").bind(row.id),
      ...tables.map((table) => db.prepare(`DELETE FROM ${table} WHERE question = ?1::text AND ${unowned}`).bind(row.id, row.id)),
      db.prepare(`DELETE FROM questions WHERE id = ?1::text AND ${unowned}`).bind(row.id, row.id),
    ]);
  }
  await db.prepare("DELETE FROM calculation_reports r WHERE NOT EXISTS (SELECT 1 FROM user_calculations t WHERE t.question = r.question AND t.engine = r.engine)").run();
}
export async function userTasksRoute(request, env, path) {
  const now = Date.now(), db = env.LIBRARY;
  if (path === "/api/user-tasks/capabilities" && request.method === "GET")
    return json({ ok: true, available: !!db && env.FACTORY_PAUSED !== "1", kinds: Object.keys(VERBS), keep_days: 30 });
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
    const cancelled = await db.prepare("SELECT id FROM user_calculation_cancellations WHERE id = ? AND owner = ?").bind(body.id, owner).first();
    if (cancelled) return json({ ok: false, error: "This community request has ended." }, 410);
    const old = await db.prepare("SELECT * FROM user_calculations WHERE id = ?").bind(body.id).first();
    const question = await questionId(VERBS[body.kind], body.request);
    if (old && (old.owner !== owner || old.question !== question || old.engine !== body.engine || old.task !== body.task))
      return json({ ok: false, error: "This command already describes another request." }, 409);
    if (!old) {
      const count = await db.prepare("SELECT count(*) AS n FROM user_calculations t WHERE owner = ? AND state = 'waiting' AND expires_at > ? AND NOT EXISTS (SELECT 1 FROM questions q WHERE q.id = t.question AND q.state IN ('disputed', 'nondeterministic', 'withdrawn'))").bind(owner, iso(now)).first();
      if (count.n >= 100) return json({ ok: false, error: "This browser already has 100 pending calculations." }, 429);
      await db.batch([db.prepare("SELECT id FROM questions WHERE id = ? FOR UPDATE").bind(question),
        db.prepare("INSERT INTO user_calculations (id, task, owner, kind, engine, question, state, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, 'waiting', ?, ?) ON CONFLICT DO NOTHING")
        .bind(body.id, body.task, owner, body.kind, body.engine, question, iso(now), iso(now + USER_TASK_KEEP_MS)),
      ]);
    }
    const stored = await db.prepare("SELECT owner, question, task, engine FROM user_calculations WHERE id = ?").bind(body.id).first();
    if (stored.owner !== owner || stored.question !== question || stored.task !== body.task || stored.engine !== body.engine)
      return json({ ok: false, error: "This command already describes another request." }, 409);
    // Repeating a lost acceptance finishes its demand, without reopening a cancelled one.
    if (!old || old.state === "waiting") await facts().demand(env, { producer: "user_calculation", ref: body.id,
      verb: VERBS[body.kind], request: body.request, due_in_ms: DEADLINE_MS.user_calculation,
      waits: false, initiator: `user:${owner.slice(0, 16)}` }, body.engine, old ? Date.parse(old.created_at) : now);
    return json({ ok: true, id: body.id, question, keep_until: old?.expires_at || iso(now + USER_TASK_KEEP_MS) });
  }
  const ids = Array.isArray(body.ids) ? body.ids : [body.id];
  if (!ids.length || ids.length > 100 || !ids.every((id) => UUID.test(id || ""))) return json({ ok: false, error: "invalid task ids" }, 400);
  if (path === "/api/user-tasks/cancel") {
    for (const id of ids) {
      await db.prepare("INSERT INTO user_calculation_cancellations (id, owner, expires_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
        .bind(id, owner, iso(now + USER_TASK_KEEP_MS)).run();
      const changed = await db.prepare("UPDATE user_calculations SET state = 'cancelled', completed_at = ? WHERE id = ? AND owner = ? AND state = 'waiting'")
        .bind(iso(now), id, owner).run();
      if (changed.meta.changes) await facts().withdraw(env, "user_calculation", id, now);
    }
  } else if (path !== "/api/user-tasks/status") return json({ ok: false, error: "not found" }, 404);
  const currentEngine = (await servedRelease(env)).engine;
  const { results } = await db.prepare(`SELECT t.id, t.task, t.state, t.engine, t.expires_at, t.completed_at, q.state AS question_state,
      q.leased_to IS NOT NULL AND q.lease_until > ?3::text AS computing, r.result
      FROM user_calculations t LEFT JOIN questions q ON q.id = t.question
      LEFT JOIN calculation_reports r ON r.question = t.question AND r.engine = t.engine
      WHERE t.owner = ?1::text AND t.id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb))`)
    .bind(owner, JSON.stringify(ids), iso(now)).all();
  return json({ ok: true, next_ms: 10_000, tasks: results.map((row) => ({ id: row.id, task: row.task,
    state: row.state === "waiting" && currentEngine && row.engine !== currentEngine ? "failed" : Date.parse(row.expires_at) <= now ? "expired" : row.state === "waiting" && ["disputed", "nondeterministic", "withdrawn"].includes(row.question_state) ? "failed" : row.state,
    ...(row.state === "waiting" && currentEngine && row.engine !== currentEngine ? { error: "The engine changed. Run this calculation again from its page." } : {}),
    ...(row.state === "waiting" && row.question_state === "withdrawn" ? { error: "The engine refused this calculation." } : {}),
    execution_state: row.computing ? "running" : "queued",
    phase: row.computing ? "Computing and verifying" : "Waiting for community compute", engine: row.engine, keep_until: row.expires_at,
    ...(body.include_results === true && row.state === "succeeded" && row.result ? { result: JSON.parse(row.result) } : {}) })) });
}
async function answer(request, env, body) {
  const refused = unrecognised(body.verifier);
  if (refused) return refused;
  if (!LEASE_ID.test(body.lease || "") || !QUESTION_ID.test(body.question || "") || !ENGINE_ID.test(body.engine || "")
      || (body.refused !== true && (!body.report || body.report.ok === false || !Number.isSafeInteger(body.work) || body.work < 0)))
    return json({ ok: false, error: "invalid report" }, 400);
  if ((await servedRelease(env)).engine !== body.engine) return json({ ok: true, taken: false, stale: true });
  const question = await env.LIBRARY.prepare("SELECT verb, request FROM questions WHERE id = ? AND lease = ? AND leased_to = ?")
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
