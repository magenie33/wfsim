import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { testDb } from "./pg/test_db.mjs";
import { userTasksRoute, calculationReport, reportCanon, purgeUserCalculations, USER_TASK_PRODUCERS, USER_TASK_KEEP_MS } from "../worker/user_tasks.js";
import { COMPUTATION_KEEP_MS } from "../worker/computation_records.js";
import { syncRoute } from "../worker/sync.js";
import { setDial } from "../worker/dials.js";
import { facts } from "../worker/factory.js";

const db = await testDb("library");
const env = { LIBRARY: db, ASSETS: { fetch: async () => Response.json({ engine: "test-engine", release: "test-release" }) } };
const owner = "a".repeat(64), another = "b".repeat(64), engine = "test-engine";
const request = { weapon: "torid", mods: [], runs: 2, replay: true };
const accepted = (kind = "simulation", body = request) => ({ input_schema: 1, owner, id: randomUUID(), task: randomUUID(), engine, kind, request: body });
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
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_user_calculation_requests")).n, 1);
});
await check("failed task admission rolls back its consumer request",async()=>{
  const pending=accepted("simulation",{...request,unique:"atomic-admission"}),batch=db.batch;
  db.batch=statements=>{
    const index=statements.findIndex(statement=>statement.sql.includes("compute_admit_tasks("));
    return batch(index<0 ? statements : [...statements.slice(0,index),db.prepare("SELECT 1/0"),...statements.slice(index)]);
  };
  try { await assert.rejects(call("accept",pending)); }
  finally { db.batch=batch; }
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_user_calculation_requests WHERE id=?",pending.id)).n,0);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers WHERE ref=?",pending.id)).n,0);
});
await check("cancellation between validation and admission wins durably",async()=>{
  const pending=accepted("simulation",{...request,unique:"cancel-admission-race"}),batch=db.batch;
  let interrupted=false;
  db.batch=async statements=>{
    if (!interrupted && statements.some(statement=>statement.sql.includes("compute_admit_tasks("))) {
      interrupted=true;
      await call("cancel",{owner,ids:[pending.id]});
    }
    return batch(statements);
  };
  let response;
  try { response=await call("accept",pending); }
  finally { db.batch=batch; }
  assert.equal(response.status,410);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_user_calculation_requests WHERE id=?",pending.id)).n,0);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_task_consumers WHERE ref=?",pending.id)).n,0);
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
  assert.equal(state.computation.question, (await call('accept', next)).question);
  assert.equal(state.keep_until, (await call('status', { owner: another, ids: [shared.id] })).tasks[0].keep_until);
});
await check("removing task history preserves separately authorized reports", async () => {
  const pending=accepted("simulation",{...request,unique:"report-history"}), got=await call("accept",pending);
  await facts().answerOfficial(env,{device:"official:history",owner:"official:history",official:true},
    {question:got.question,engine,work:100,result:{report}});
  await db.run("DELETE FROM entity_user_calculation_requests WHERE id=?",pending.id);
  await purgeUserCalculations(env);
  const kept=(await call("status",{owner,ids:[pending.id],include_results:true})).tasks[0];
  assert.equal(kept.state,"succeeded");
  assert.deepEqual(kept.result,calculationReport(report));
});
await check("a corrected report selects the new output under the same engine",async()=>{
  const pending=accepted("simulation",{...request,unique:"report-correction"}), got=await call("accept",pending);
  await facts().answerOfficial(env,{device:"official:correction",owner:"official:correction",official:true},
    {question:got.question,engine,work:100,result:{report}});
  const old=await db.get("SELECT r.report_id,r.retained_from,q.task_id FROM state_current_calculation_reports r JOIN state_compute_work q ON q.id=r.question WHERE r.question=? AND r.engine=?",got.question,engine);
  const corrected={...report,correction:"verified replacement"};
  await facts().answerOfficial(env,{device:"official:corrected",owner:"official:corrected",official:true},
    {question:got.question,engine,work:100,result:{report:corrected}});
  const current=await db.get("SELECT r.report_id,r.retained_from,q.task_id FROM state_current_calculation_reports r JOIN state_compute_work q ON q.id=r.question WHERE r.question=? AND r.engine=?",got.question,engine);
  assert.notEqual(current.task_id,old.task_id,"a changed official report must start a new execution round");
  assert.notEqual(current.report_id,old.report_id);
  assert.equal(current.retained_from,old.retained_from);
  assert.deepEqual((await call("status",{owner,ids:[pending.id],include_results:true})).tasks[0].result,calculationReport(corrected));
});
await check("an imported report survives without invented task or input records",async()=>{
  const id=randomUUID(),task=randomUUID(),question="f".repeat(32),at=new Date(Date.now()-10*86_400_000).toISOString();
  const storedOwner=(await db.get("SELECT owner FROM entity_user_calculation_requests WHERE id=?",first.id)).owner;
  const saved=await db.get("INSERT INTO entity_calculation_reports(question,engine,at,origin,body) VALUES(?1,?2,?3,'imported',?4) RETURNING id",
    question,engine,at,JSON.stringify(calculationReport(report)));
  await db.run("INSERT INTO state_calculation_report_selection(question,engine,report_id,at,retained_from) VALUES(?,?,?,?,?)",
    question,engine,saved.id,at,at);
  await db.run("INSERT INTO relation_calculation_report_access(id,owner,question,engine,task,kind,created_at,completed_at,expires_at) VALUES(?,?,?,?,?,'simulation',?,?,?)",
    id,storedOwner,question,engine,task,at,at,new Date(Date.now()+86_400_000).toISOString());
  await purgeUserCalculations(env);
  const kept=(await call("status",{owner,ids:[id],include_results:true})).tasks[0];
  assert.deepEqual(kept.result,calculationReport(report));
  assert.equal(kept.computation.completed_at,at);
  assert.equal(kept.computation.origin,"imported");
  assert.equal((await call("status",{owner:another,ids:[id],include_results:true})).tasks.length,0);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_compute_specs WHERE id=?",question)).n,0);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_compute_tasks WHERE computation=?",question)).n,0);
});
await check('optimization requests expire after fourteen days and new windows cannot revive a report', async () => {
  const OriginalDate = Date;
  let clock = Math.ceil(OriginalDate.now() / COMPUTATION_KEEP_MS) * COMPUTATION_KEEP_MS + 1000;
  globalThis.Date = class extends OriginalDate { constructor(...args) { super(...(args.length ? args : [clock])); } static now() { return clock; } };
  try {
    const pending = accepted('optimization', { weapon: 'torid', runs: 2, strategy: 'quick', starts: [], unique: 'retention' }), got = await call('accept', pending);
    assert.equal(got.ok, true, JSON.stringify(got));
    assert.equal(Date.parse(got.keep_until) - clock, COMPUTATION_KEEP_MS);
    await facts().answerOfficial(env, { device: 'official:retention', owner: 'official:retention', official: true },
      { question: got.question, engine, work: 100, result: { report } });
    clock += 5000;
    await facts().answerOfficial(env, { device: 'official:retention', owner: 'official:retention', official: true },
      { question: got.question, engine, work: 100, result: { report } });
    const next = accepted('optimization', pending.request), cached = await call('accept', next);
    assert.equal(cached.question, got.question);
    assert.equal(cached.keep_until, got.keep_until);
    clock += COMPUTATION_KEEP_MS;
    assert.equal((await call('status', { owner, ids: [pending.id], include_results: true })).tasks[0].result, undefined);
    const fresh = await call('accept', accepted('optimization', pending.request));
    assert.equal(fresh.question, got.question);
    assert.equal((await call('status', { owner, ids: [fresh.id] })).tasks[0].state, 'waiting');
    await facts().answerOfficial(env,{device:'official:renewal',owner:'official:renewal',official:true},
      {question:fresh.question,engine,work:100,result:{report}});
    const renewed=(await call('status',{owner,ids:[fresh.id],include_results:true})).tasks[0];
    assert.equal(renewed.state,'succeeded');assert.ok(renewed.result);
    assert.equal(Date.parse(renewed.keep_until)-clock,COMPUTATION_KEEP_MS);
    assert.equal((await call('status',{owner,ids:[pending.id],include_results:true})).tasks[0].result,undefined);
    const sim = await call('accept', accepted('simulation', { weapon: 'torid', runs: 2, unique: 'sim-retention' }));
    assert.equal(Date.parse(sim.keep_until) - clock, USER_TASK_KEEP_MS);
  } finally { globalThis.Date = OriginalDate; }
});
await check("a cancellation arriving before acceptance cannot be lost", async () => {
  const pending = accepted();
  await call("cancel", { owner, ids: [pending.id] });
  assert.equal((await call("accept", pending)).status, 410);
  assert.equal((await call("status", { owner, ids: [pending.id] })).tasks.length, 0);
});
await check("a deployment change does not change the computation identity", async () => {
  const current = await call("accept", accepted());
  const old = await call("accept", { ...accepted(), engine: "old-engine" });
  assert.equal(old.status, 200); assert.equal(old.question, current.question);
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
await check("queued inputs survive a deployment change", async () => {
  const pending = accepted('simulation', { weapon: 'torid', runs: 55 }); await call('accept', pending);
  const prior = env.ASSETS;
  env.ASSETS = { fetch: async () => Response.json({ engine: 'next-engine' }) };
  const status = (await call('status', { owner, ids: [pending.id] })).tasks[0];
  assert.equal(status.state, 'waiting'); assert.equal(status.engine, 'next-engine');
  const work = await facts().next(env, witness('6'), 'next-engine');
  assert.ok(work); await facts().release(env, witness('6').device, work.lease);
  await call('cancel', { owner, ids: [pending.id] });
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
  await db.run("INSERT INTO record_compute_identity_changes(task_id,previous_computation,computation,replacement_task_id,changed_at,snapshot) VALUES(98765,'expired-old','expired-new',98766,'2026-11-11',?)",JSON.stringify({scheduling:{verb:'simulate_report'},input:{private:'expired'}}));
  await db.run("UPDATE entity_user_calculation_requests SET expires_at = '2000-01-01T00:00:00.000Z'");
  await db.run("UPDATE relation_calculation_report_access SET expires_at = '2000-01-01T00:00:00.000Z'");
  await purgeUserCalculations(env); await purgeUserCalculations(env);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_user_calculation_requests")).n, 0);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_calculation_reports")).n, 0);
  assert.equal((await db.get("SELECT count(*) AS n FROM record_compute_identity_changes")).n, 0);
});
await check("the deployed daemon role can deliver a completed calculation", async () => {
  const pending = accepted();
  const got = await call("accept", pending);
  const task = await db.get("SELECT task_id FROM state_compute_work WHERE id=?",got.question);
  const completedAt = new Date().toISOString();
  await db.run("SELECT compute_accept_result(?1::text,?2::bigint,'open','fact',?3::text,?4::text,?5::text,0,'[]')",
    got.question,task.task_id,reportCanon({report},100),engine,completedAt);
  const grants = readFileSync(new URL("../deploy/factoryd_grants.sql", import.meta.url), "utf8");
  const schema = (await db.get("SELECT current_schema() AS name")).name;
  await db.exec(`CREATE ROLE wfsim_factoryd; GRANT USAGE ON SCHEMA ${schema} TO wfsim_factoryd;`);
  await db.exec([...grants.matchAll(/GRANT (?!CONNECT)[^;]+;/g)].map((m) => m[0]).join("\n"));
  await db.exec("SET ROLE wfsim_factoryd");
  try {
    const producer = USER_TASK_PRODUCERS.user_calculation;
    assert.equal(await producer.still_wanted(env, { ref: pending.id }), true);
    await producer.on_fact(env, { ref: pending.id, question: got.question }, { engine, at:completedAt, result: { report } });
  } finally { await db.exec("RESET ROLE"); }
  assert.equal((await call("status", { owner, ids: [pending.id], include_results: true })).tasks[0].state, "succeeded");
});
await db.close();
console.log(`\n${passed} user task service checks passed`);
