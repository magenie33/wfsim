import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { testDb } from "./pg/test_db.mjs";
import { userTasksRoute, reportCanon, purgeUserCalculations, USER_TASK_PRODUCERS } from "../worker/user_tasks.js";
import { syncRoute } from "../worker/sync.js";
import { setDial } from "../worker/dials.js";
import { facts } from "../worker/factory.js";

const db = await testDb("library");
const env = { LIBRARY: db, ASSETS: { fetch: async () => Response.json({ engine: "test-engine", release: "test-release" }) } };
const owner = "a".repeat(64), another = "b".repeat(64), engine = "test-engine";
const request = { weapon: "torid", mods: [], runs: 2, replay: true };
const accepted = (kind = "simulation", body = request) => ({ owner, id: randomUUID(), task: randomUUID(), engine, kind, request: body });
const call = async (action, body) => {
  const path = `/api/user-tasks/${action}`;
  const response = await userTasksRoute(new Request(`https://wfsim.test${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }), env, path);
  return { status: response.status, ...await response.json() };
};
let passed = 0;
async function check(name, test) { await test(); passed++; console.log(`  ok  ${name}`); }
const first = accepted();
await check("acceptance is durable and idempotent", async () => {
  const a = await call("accept", first), b = await call("accept", first);
  assert.equal(a.ok, true); assert.equal(a.id, b.id); assert.equal(a.question, b.question);
  assert.equal((await db.get("SELECT count(*) AS n FROM user_calculations")).n, 1);
});
await check("an acceptance identity cannot change its inputs or owner", async () => {
  assert.equal((await call("accept", { ...first, request: { ...request, runs: 99 } })).status, 409);
  assert.equal((await call("accept", { ...first, owner: another })).status, 409);
});
await check("another browser cannot inspect or cancel a request", async () => {
  assert.equal((await call("status", { owner: another, ids: [first.id] })).tasks.length, 0);
  await call("cancel", { owner: another, ids: [first.id] });
  assert.equal((await call("status", { owner, ids: [first.id] })).tasks[0].state, "waiting");
});
const shared = { ...accepted(), owner: another };
await check("equal inputs share computation while consumers retain independent cancellation", async () => {
  const got = await call("accept", shared);
  assert.equal(got.question, (await call("accept", first)).question);
  await call("cancel", { owner, ids: [first.id] });
  assert.equal((await call("status", { owner, ids: [first.id] })).tasks[0].state, "cancelled");
  assert.equal((await call("status", { owner: another, ids: [shared.id] })).tasks[0].state, "waiting");
});
await check("older clients never lease a complete-report request", async () => {
  const work = await facts().next(env, { device: "1".repeat(24), owner: "old", siblings: ["1".repeat(24)], net: "old" }, engine);
  assert.equal(work, null);
});
const witness = (digit) => ({ device: digit.repeat(24), owner: `owner-${digit}`, siblings: [digit.repeat(24), "cap:calculation-reports-v1"], net: `net-${digit}` });
const report = { ok: true, runs: 2, work: 100, mean_dps: 30, representative: { rng: "123" }, sources: [{ damage: 42 }], cpu_ms: 2 };
await check("all report fields participate in verification", async () => {
  assert.notEqual(reportCanon({ report }, 100), reportCanon({ report: { ...report, sources: [{ damage: 43 }] } }, 100));
  assert.equal(reportCanon({ report }, 100), reportCanon({ report: { ...report, cpu_ms: 999 } }, 100));
});
await check("the report endpoint refuses a headline without its complete report", async () => {
  const who = witness('7'), work = await facts().next(env, who, engine);
  assert.ok(work);
  const response = await call('answer', { verifier: who.device, engine, lease: work.lease, question: work.question, work: 100, report: { ok: true, dps: 99 } });
  assert.equal(response.status, 400);
  await facts().release(env, who.device, work.lease);
});
await check("two independent full reports complete the surviving consumer", async () => {
  for (const digit of ["2", "3"]) {
    const who = witness(digit), work = await facts().next(env, who, engine);
    assert.ok(work?.question);
    const answer = await facts().answer(env, who, { lease: work.lease, engine, result: { report }, work: 100 });
    assert.equal(answer.taken, true);
  }
  const state = (await call("status", { owner: another, ids: [shared.id], include_results: true })).tasks[0];
  assert.equal(state.state, "succeeded"); assert.equal(state.result.sources[0].damage, 42);
  assert.equal((await call("status", { owner, ids: [first.id] })).tasks[0].state, "cancelled");
});
await check("a later equal request adopts the existing complete report", async () => {
  const next = accepted(); await call("accept", next);
  const state = (await call("status", { owner, ids: [next.id], include_results: true })).tasks[0];
  assert.equal(state.state, "succeeded"); assert.equal(state.result.representative.rng, "123");
});
await check("a cancellation arriving before acceptance cannot be lost", async () => {
  const pending = accepted();
  await call("cancel", { owner, ids: [pending.id] });
  assert.equal((await call("accept", pending)).status, 410);
  assert.equal((await call("status", { owner, ids: [pending.id] })).tasks.length, 0);
});
await check("engine mismatches are rejected before admission", async () => {
  assert.equal((await call("accept", { ...accepted(), engine: "old-engine" })).status, 409);
});
await check("both protocol 8 paths lease reports only to capable clients", async () => {
  for (const proc of [0, 1]) {
    await setDial(env, 'sync_proc', proc, 'check');
    const request = accepted('simulation', { weapon: 'torid', runs: 20 + proc });
    await call('accept', request);
    const body = { verifier: String(8 + proc).repeat(24), engine, protocol: 8, capabilities: ['calculation-reports-v1'],
      consent: { v: 3, at: new Date().toISOString() }, lanes: 2, want_ms: 1000 };
    const response = await syncRoute(new Request('https://wfsim.test/api/board/sync', { method: 'POST',
      headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }), env);
    const result = await response.json();
    assert.equal(result.work?.kind, 'user_calculation', JSON.stringify(result));
    assert.equal(result.work.request.runs, 20 + proc);
    await facts().release(env, body.verifier, result.work.lease);
    await call('cancel', { owner, ids: [request.id] });
  }
  await setDial(env, 'sync_proc', 0, 'check');
});
await check("a new engine cannot silently answer an older accepted request", async () => {
  const pending = accepted('simulation', { weapon: 'torid', runs: 55 }); await call('accept', pending);
  const prior = env.ASSETS;
  env.ASSETS = { fetch: async () => Response.json({ engine: 'next-engine' }) };
  const status = (await call('status', { owner, ids: [pending.id] })).tasks[0];
  assert.equal(status.state, 'failed'); assert.match(status.error, /engine changed/);
  assert.equal(await facts().next(env, witness('6'), 'next-engine'), null);
  env.ASSETS = prior;
});
await check("engine refusals end an invalid request without publishing a number", async () => {
  const pending = accepted('simulation', { weapon: 'unknown-weapon', runs: 1 }); await call('accept', pending);
  for (const digit of ['a', 'b', 'c']) {
    const who = witness(digit), work = await facts().next(env, who, engine);
    assert.ok(work);
    const response = await call('answer', { verifier: who.device, engine, lease: work.lease, question: work.question, refused: true });
    assert.equal(response.ok, true);
  }
  const status = (await call('status', { owner, ids: [pending.id] })).tasks[0];
  assert.equal(status.state, 'failed'); assert.equal(status.result, undefined);
});
await check("expiry removes ownership and reports after retention", async () => {
  await db.run("UPDATE user_calculations SET expires_at = '2000-01-01T00:00:00.000Z'");
  await purgeUserCalculations(env); await purgeUserCalculations(env);
  assert.equal((await db.get("SELECT count(*) AS n FROM user_calculations")).n, 0);
  assert.equal((await db.get("SELECT count(*) AS n FROM calculation_reports")).n, 0);
});
await check("the deployed daemon role can deliver a completed calculation", async () => {
  const pending = accepted();
  const got = await call("accept", pending);
  const grants = readFileSync(new URL("../deploy/factoryd_grants.sql", import.meta.url), "utf8");
  const schema = (await db.get("SELECT current_schema() AS name")).name;
  await db.exec(`CREATE ROLE wfsim_factoryd; GRANT USAGE ON SCHEMA ${schema} TO wfsim_factoryd;`);
  await db.exec([...grants.matchAll(/GRANT (?!CONNECT)[^;]+;/g)].map((m) => m[0]).join("\n"));
  await db.exec("SET ROLE wfsim_factoryd");
  try {
    const producer = USER_TASK_PRODUCERS.user_calculation;
    assert.equal(await producer.still_wanted(env, { ref: pending.id }), true);
    await producer.on_fact(env, { ref: pending.id, question: got.question }, { engine, result: { report } });
  } finally { await db.exec("RESET ROLE"); }
  assert.equal((await call("status", { owner, ids: [pending.id], include_results: true })).tasks[0].state, "succeeded");
});
await db.close();
console.log(`\n${passed} user task service checks passed`);
