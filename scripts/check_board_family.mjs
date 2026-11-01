import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { testDb } from "./pg/test_db.mjs";
import { familyRoute, askFamilyTransfers, FAMILY_PRODUCERS } from "../worker/board_family.js";
import { facts, workOf } from "../worker/factory.js";
import { userTasksRoute, purgeUserCalculations } from "../worker/user_tasks.js";
import { syncRoute } from "../worker/sync.js";
import { setDial } from "../worker/dials.js";
import { shipFamily } from "./ship_family.mjs";
import worker from "../worker/index.js";

const db = await testDb("library"), engine = "test-engine";
const env = { LIBRARY: db, FACTORY_TOKEN: "check", ASSETS: { fetch: async () => Response.json({ engine }) } };
const source = "a".repeat(32), ruler = "standard_single_target";
const job = { source, weapon: "burston", ruler, request: { weapon: "burston", strategy: "quick", runs: 2, finalists: 1,
  starts: [{ slots: ["serration", null, "riven:board"], fixed: [{ kind: "mods", idx: 2 }] }] },
  context: { weapon: "burston", benchmark: ruler, mode: "base", rivens: { "riven:board": { riven_pos: ["critical_damage", "multishot"], riven_neg: "" } } } };
const call = async (jobs = [job], token = "check") => {
  const response = await familyRoute(new Request("https://wfsim.test/api/board/family", { method: "POST",
    headers: { authorization: `Bearer ${token}` }, body: JSON.stringify({ jobs }) }), env);
  return { status: response.status, ...await response.json() };
};
let passed = 0;
const check = async (name, run) => { await run(); passed++; console.log(`  ok  ${name}`); };
const count = async table => (await db.get(`SELECT count(*) AS n FROM ${table}`)).n;
await check("only the trusted intake can admit family work", async () => {
  assert.equal((await call([job], "wrong")).status, 401);
  assert.equal(await count("family_transfers"), 0);
});
await check("the source build must land before its family requests", async () => {
  assert.equal((await call()).status, 409);
  assert.equal(await count("family_transfers"), 0);
  await db.run("INSERT INTO builds (id, at, record) VALUES (?, ?, ?)", source, "2026-11-01", JSON.stringify({ weapon: "burston_prime" }));
});
await check("an empty admission never scans historical builds", async () => {
  assert.equal((await call([])).status, 200);
  assert.equal(await count("family_transfers"), 0);
});
await check("interrupted admission resumes only its durable transfer", async () => {
  env.FACTORY_PAUSED = "1";
  assert.equal((await call()).status, 200);
  assert.equal(await count("family_transfers"), 1); assert.equal(await count("demands"), 0);
  delete env.FACTORY_PAUSED;
  await askFamilyTransfers(env);
  assert.equal(await count("demands"), 1);
});
await check("concurrent resubmissions coalesce into one independent background search", async () => {
  await Promise.all([call(), call()]);
  assert.equal(await count("family_transfers"), 1); assert.equal(await count("questions"), 1);
  const demand = await db.get("SELECT * FROM demands");
  assert.equal(demand.producer, "family_build"); assert.equal(demand.waits, 0); assert.equal(demand.initiator, "system");
  assert.equal((await db.get("SELECT * FROM questions")).verb, "optimize_report");
});
await check("an interrupted engine rebind remains retryable after its metadata write", async () => {
  const row = await db.get("SELECT * FROM family_transfers");
  await db.run("UPDATE family_transfers SET asked_engine = NULL WHERE id = ?", row.id);
  const demand = facts().demand;
  facts().demand = async () => { throw new Error("interrupted ask"); };
  try { await assert.rejects(askFamilyTransfers(env), /interrupted ask/); }
  finally { facts().demand = demand; }
  assert.equal((await db.get("SELECT asked_engine FROM family_transfers")).asked_engine, null);
  await askFamilyTransfers(env);
  assert.equal((await db.get("SELECT asked_engine FROM family_transfers")).asked_engine, engine);
  assert.equal(await count("questions"), 1);
});
await check("user report retention preserves a live family search", async () => {
  await purgeUserCalculations(env);
  assert.equal(await count("questions"), 1); assert.equal(await count("demands"), 1);
});
await check("only capable clients lease family optimization reports on either sync path", async () => {
  for (const proc of [0, 1]) {
    await setDial(env, "sync_proc", proc, "check");
    const sync = async (capabilities, digit) => {
      const response = await syncRoute(new Request("https://wfsim.test/api/board/sync", { method: "POST", body: JSON.stringify({
        verifier: digit.repeat(24), engine, protocol: 8, capabilities, consent: { v: 3, at: new Date().toISOString() }, lanes: 2, want_ms: 1000 }) }), env);
      return response.json();
    };
    assert.ok(!(await sync([], String(proc + 1))).work);
    const got = await sync(["calculation-reports-v1"], String(proc + 3));
    assert.equal(got.work?.kind, "user_calculation", JSON.stringify(got));
    assert.equal(got.work.calculation, "optimization"); assert.deepEqual(got.work.request, job.request);
    await facts().release(env, String(proc + 3).repeat(24), got.work.lease);
  }
  await setDial(env, "sync_proc", 0, "check");
});
const witness = digit => ({ device: digit.repeat(24), owner: `owner-${digit}`, siblings: [digit.repeat(24), "cap:calculation-reports-v1"], net: `net-${digit}` });
const best = { mods: ["serration", "speed_trigger", "riven:board"], evolutions: [], arcane: ["primary_merciless"],
  mode: "base", replay: { weapon: "burston" }, forma: {} };
const report = { ok: true, target: "check", duration: 2, results: [best] };
await check("one witness cannot publish a derived build", async () => {
  const who = witness("5"), work = await facts().next(env, who, engine);
  assert.equal(workOf(work).calculation, "optimization");
  const path = "/api/user-tasks/answer";
  const response = await userTasksRoute(new Request(`https://wfsim.test${path}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({
    verifier: who.device, engine, lease: work.lease, question: work.question, work: 100, report }) }), env, path);
  assert.equal(response.status, 200); assert.equal(await count("inbox"), 0);
});
await check("independent agreement submits the target build with trusted nonrecursive provenance", async () => {
  const who = witness("6"), work = await facts().next(env, who, engine);
  assert.ok(work); await facts().answer(env, who, { lease: work.lease, engine, result: { report }, work: 100 });
  assert.equal(await count("inbox"), 1); assert.equal(await count("scores"), 0);
  const inbox = await db.get("SELECT * FROM inbox"), transfer = await db.get("SELECT * FROM family_transfers");
  assert.equal(inbox.family_source, transfer.id); assert.equal(inbox.initiator, "system");
  assert.equal(transfer.state, "succeeded");
  const record = JSON.parse(inbox.record);
  assert.equal(record.weapon, "burston"); assert.deepEqual(record.mods, ["serration", "speed_trigger", "riven"]);
  assert.deepEqual(record.riven_pos, ["critical_damage", "multishot"]);
  assert.equal(record.family_source, undefined);
});
await check("completed transfers and replayed facts cannot submit again", async () => {
  await call();
  const row = await db.get("SELECT * FROM family_transfers");
  await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } });
  assert.equal(await count("inbox"), 1); assert.equal(await count("questions"), 1);
});
await check("a crash after derived submission resumes without a second inbox record", async () => {
  const row = await db.get("SELECT * FROM family_transfers"), prepare = db.prepare;
  await db.run("UPDATE family_transfers SET state = 'waiting' WHERE id = ?", row.id);
  db.prepare = sql => {
    if (sql.includes("SET state = 'succeeded'")) throw new Error("interrupted completion");
    return prepare(sql);
  };
  try {
    await assert.rejects(FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } }), /interrupted completion/);
  } finally { db.prepare = prepare; }
  await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } });
  assert.equal(await count("inbox"), 1);
});
await check("the public submission cannot forge derivation provenance", async () => {
  const response = await worker.fetch(new Request("https://wfsim.test/api/board/submit", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({
    weapon: "burston", mods: ["serration"], family_source: "forged" }) }), env, { waitUntil() {} });
  assert.equal(response.status, 200, await response.clone().text());
  const rows = await db.all("SELECT * FROM inbox WHERE family_source IS NULL");
  assert.equal(rows.length, 1); assert.equal(JSON.parse(rows[0].record).family_source, undefined);
});
await check("an empty search terminates without inventing a score", async () => {
  const row = await db.get("SELECT * FROM family_transfers");
  await db.run("UPDATE family_transfers SET state = 'waiting' WHERE id = ?", row.id);
  await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report: { results: [] } } });
  assert.equal((await db.get("SELECT state FROM family_transfers")).state, "failed");
  assert.equal(await count("scores"), 0);
});
await check("transfer transport preserves requests and requires acknowledged durable admission", async () => {
  const sent = [];
  await shipFamily([{ family: [job] }], { token: "check", send: async (url, options) => {
    sent.push(JSON.parse(options.body)); return Response.json({ ok: true });
  } });
  assert.deepEqual(sent, [{ jobs: [job] }]);
  await assert.rejects(shipFamily([{ family: [job] }], { token: "check", send: async () => Response.json({ ok: false }, { status: 503 }) }));
  await shipFamily([{ record: {} }], { send: () => { throw new Error("empty transfers must not send"); } });
});
await check("the deployed daemon role can deliver family facts", async () => {
  const role = `family_daemon_${Date.now()}`;
  await db.exec(readFileSync(new URL("../deploy/factoryd_grants.sql", import.meta.url), "utf8")
    .replaceAll("wfsim_factoryd", role).replace(/GRANT CONNECT ON DATABASE wfsim TO [^;]+;/, ""));
  const schema = (await db.get("SELECT current_schema() AS name")).name;
  await db.exec(`GRANT USAGE ON SCHEMA ${schema} TO ${role}; SET ROLE ${role}`);
  try {
    const row = await db.get("SELECT * FROM family_transfers");
    await db.run("UPDATE family_transfers SET state = 'waiting' WHERE id = ?", row.id);
    await FAMILY_PRODUCERS.family_build.on_fact(env, { ref: row.id }, { engine, result: { report } });
    assert.equal((await db.get("SELECT state FROM family_transfers")).state, "succeeded");
  } finally { await db.exec("RESET ROLE"); }
});
await check("a future arrival can seed the same family on a new engine without a historical sweep", async () => {
  env.ASSETS = { fetch: async () => Response.json({ engine: "next-engine" }) };
  await call([]);
  assert.equal(await count("family_transfers"), 1);
  await call();
  assert.equal(await count("family_transfers"), 2);
  assert.equal((await db.get("SELECT count(*) AS n FROM family_transfers WHERE engine = 'next-engine'")).n, 1);
});
await db.close();
console.log(`\n${passed} family spread checks passed`);
