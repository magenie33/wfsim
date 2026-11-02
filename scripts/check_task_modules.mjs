// Independent task modules, consumer failure recovery and registry extension.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { factory, canon } from "../worker/tasks.js";
import { createDispatch } from "../worker/task_dispatch.js";
import { testDb } from "./pg/test_db.mjs";
import { taskSchemaSql } from "./task_schema.mjs";

for (const name of ["contract", "ledger", "dispatch", "verification", "delivery", "credit", "usage", "observation"]) {
  const text = readFileSync(new URL(`../worker/task_${name}.js`, import.meta.url), "utf8");
  const imports = [...text.matchAll(/from\s+["']([^"']+)["']/g)].map(m => m[1]);
  assert(imports.every(p => ["./task_contract.js", "./dials.js", "./instant.js"].includes(p)), `${name} imports a business or peer implementation`);
}
const dispatchText = readFileSync(new URL("../worker/task_dispatch.js", import.meta.url), "utf8");
assert(!/producerOf|on_fact|on_answer|verifier_hours|points_pool|credit\(/.test(dispatchText), "dispatch reaches settlement or a business adapter");
console.log("ok: modules depend on common contracts; dispatch owns only selection and leases");

const now = Date.now(), db = await testDb("library"), env = { LIBRARY: db };
const upgrade = taskSchemaSql();
assert(upgrade.includes("CREATE INDEX CONCURRENTLY IF NOT EXISTS demands_delivery"));
await db.exec("ALTER TABLE demands DROP COLUMN delivery_at; DROP FUNCTION factory_past_safe(text,text) CASCADE; ALTER TABLE answers DROP COLUMN credit_work; ALTER TABLE answers DROP COLUMN credit_at; DROP FUNCTION factory_delivery_guard(text,text,text,text,text,text,text)");
const localUpgrade = upgrade.replace("SET ROLE wfsim;", "").replaceAll("CREATE INDEX CONCURRENTLY", "CREATE INDEX");
await db.exec(localUpgrade);
await db.exec(localUpgrade);
console.log("ok: the additive upgrade derives from the schema and can be applied twice before deployment");
await db.exec("CREATE TABLE received(ref text PRIMARY KEY); CREATE TABLE delayed(ref text PRIMARY KEY)");
for (const id of ["a", "b", "c"]) await db.run("INSERT INTO verifiers(id,seen,work) VALUES (?,'x',0)", id);
let broken = true;
const producers = {
  broken: { on_fact_writes: (env, d) => [env.LIBRARY.prepare(`INSERT INTO ${broken ? "missing_consumer_table" : "delayed"}(ref) VALUES (?) ON CONFLICT DO NOTHING`).bind(d.ref)] },
  healthy: { on_fact_writes: (env, d) => [env.LIBRARY.prepare("INSERT INTO received(ref) VALUES (?) ON CONFLICT DO NOTHING").bind(d.ref)] },
};
const verbs = { future_compute: { cap: 2, lease_ms: 60000, pack: true, canon: result => canon(result) } };
const f = factory({ verbs, producers, spot: 0, recheck: 0, random: () => 1 });
const asks = Object.keys(producers).map(producer => ({ producer, ref: producer, verb: "future_compute", request: { value: 7 }, due_in_ms: 1000 }));
const ids = await f.demandAll(env, asks, "e", now);
assert.equal(new Set(ids).size, 1, "two consumers of one input must share a computation");
const witness = id => ({ device: id, owner: id, net: id });
for (const id of ["a", "b"]) {
  const w = witness(id), lease = await f.next(env, w, "e", now);
  assert.equal(lease.verb, "future_compute");
  try { await f.answer(env, w, { lease: lease.lease, engine: "e", result: { value: 49 }, work: 10 }, now); }
  catch (error) { if (id !== "b" || !String(error).includes("missing_consumer_table")) throw error; }
}
assert.equal((await db.get("SELECT state FROM questions WHERE id=?", ids[0])).state, "fact", "consumer failure rolled back verified work");
assert.equal(await f.next(env, witness("c"), "e", now), null, "failed delivery must not request another computation");
assert.equal((await db.get("SELECT count(*) n FROM received")).n, 1, "one failing consumer blocked an independent consumer");
assert.equal((await db.get("SELECT sum(work) n FROM verifiers")).n, 20);
broken = false;
assert.equal(await f.redeliver(env, now + 11 * 60000), 1);
assert.equal((await db.get("SELECT count(*) n FROM delayed")).n, 1);
assert.equal(await f.redeliver(env, now + 12 * 60000), 0);
assert.equal((await db.get("SELECT count(*) n FROM answers")).n, 2);
assert.equal((await db.get("SELECT sum(work) n FROM verifiers")).n, 20, "delivery retry paid the computation twice");
console.log("ok: a new registered verb shares work; failed consumers retry independently without recomputation or duplicate credit");

const cachedAt = now + 13 * 60000;
await db.run("UPDATE questions SET fact_at=? WHERE id=?", new Date(cachedAt - 13 * 86400000).toISOString(), ids[0]);
broken = true;
const cached = await f.demand(env, { ...asks[0], ref: "cached-retry" }, "e", cachedAt);
assert.equal(cached, ids[0]);
assert((await db.get("SELECT delivery_at FROM demands WHERE ref='cached-retry'")).delivery_at);
broken = false;
assert.equal(await f.redeliver(env, cachedAt + 61000), 1, "a new delivery of an old fact was lost behind the fact-time cursor");
assert.equal((await db.get("SELECT count(*) n FROM answers")).n, 2);
assert.equal((await db.get("SELECT count(*) n FROM delayed")).n, 2);
console.log("ok: a new consumer of a thirteen-day-old fact retries without creating or paying another computation");

const restricted = factory({ verbs: { future_restricted: { cap: 2, lease_ms: 60000, canon, official_producers: ["future_owner"] } },
  producers: { future_owner: { on_fact: async () => {} } } });
await restricted.demand(env, { producer: "future_owner", ref: "one", verb: "future_restricted", request: { value: 1 }, due_in_ms: -1 }, "e", now);
const official = { device: "official:test", owner: "official:test", official: true };
assert.equal((await restricted.claim(env, official, { states: ["open"] }, "e", now)).length, 0);
assert.equal((await restricted.claim(env, official, { states: ["open"], producers: ["future_owner"] }, "e", now)).length, 1);
console.log("ok: registered official eligibility works for a new type without type names in the dispatcher");

let picked = 0;
const alternate = factory({ verbs, producers, dispatcher: ports => {
  for (const forbidden of ["producers", "producerOf", "demand", "commit", "credit", "deliver"])
    assert(!Object.hasOwn(ports, forbidden), `dispatcher receives ${forbidden}`);
  assert(!Object.hasOwn(ports.verbs.future_compute, "canon"));
  return { ...createDispatch(ports), next: async () => { picked++; return null; } };
} });
assert.equal(await alternate.next(env, witness("c"), "e", now), null);
assert.equal(picked, 1);
assert(!Object.hasOwn(alternate, "commit"), "the compatibility API exposes ledger internals");
console.log("ok: a dispatch strategy can be replaced without access to registration, verification or consumers");

for (const officialAnswer of [false, true]) {
  const L = await testDb("library");
  await L.exec("CREATE TABLE received(ref text PRIMARY KEY)");
  for (const id of ["a", "b"]) await L.run("INSERT INTO verifiers(id,seen,work) VALUES (?,'x',0)", id);
  let failCredit = true;
  const E = { LIBRARY: { ...L, batch: async list => {
    if (failCredit && list.some(s => /WITH owed AS/.test(s.sql))) throw new Error("credit unavailable");
    return L.batch(list);
  } } };
  const F = factory({ verbs, producers: { healthy: producers.healthy }, spot: 0, random: () => 1 });
  const [qid] = await F.demandAll(E, [{ ...asks[1], request: { value: officialAnswer ? 2 : 3 } }], "e", now);
  const one = await F.next(E, witness("a"), "e", now);
  await F.answer(E, witness("a"), { lease: one.lease, engine: "e", result: { value: 49 }, work: 10 }, now);
  try {
    if (officialAnswer) await F.answerOfficial(E, official, { question: qid, engine: "e", result: { value: 49 }, work: 10 }, now);
    else {
      const two = await F.next(E, witness("b"), "e", now);
      await F.answer(E, witness("b"), { lease: two.lease, engine: "e", result: { value: 49 }, work: 10 }, now);
    }
  } catch (error) { assert.match(String(error), /credit unavailable/); }
  assert.equal((await L.get("SELECT state FROM questions WHERE id=?", qid)).state, "fact");
  assert.equal((await L.get("SELECT count(*) n FROM received")).n, 1, "failed credit blocked result delivery");
  failCredit = false;
  await F.repay(E, now + 11 * 60000);
  const paid = (await L.get("SELECT sum(work) n FROM verifiers")).n;
  assert.equal(paid, officialAnswer ? 10 : 20);
  await F.repay(E, now + 12 * 60000);
  assert.equal((await L.get("SELECT sum(work) n FROM verifiers")).n, paid);
  await L.close();
}
console.log("ok: credit outages preserve volunteer and official facts, delivery continues, repayment is idempotent");

{
  const L = await testDb("library");
  await L.exec("CREATE TABLE received(ref text PRIMARY KEY)");
  await L.run("INSERT INTO verifiers(id,seen,work) VALUES ('a','x',0)");
  let failed = true;
  const E = { LIBRARY: { ...L, batch: list => {
    if (failed && list.some(s => /WITH owed AS/.test(s.sql))) throw new Error("credit unavailable");
    return L.batch(list);
  } } };
  const F = factory({ verbs: { measured: { cap: 2, lease_ms: 60000, canon: (result, work) => canon({ score: result.score, work }) } },
    producers: { healthy: producers.healthy }, spot: 0, random: () => 1 });
  const qid = await F.demand(E, { ...asks[1], verb: "measured" }, "e", now);
  const lease = await F.next(E, witness("a"), "e", now);
  await F.answer(E, witness("a"), { lease: lease.lease, engine: "e", result: { score: 49 }, work: 999 }, now);
  await assert.rejects(F.answerOfficial(E, official, { question: qid, engine: "e", result: { score: 49 }, work: 10 }, now), /credit unavailable/);
  assert.equal((await L.get("SELECT credit_work FROM answers WHERE official=0")).credit_work, 10);
  await L.run("SELECT factory_forget_past(?::text,'e','',200)", new Date(now + 86400000).toISOString());
  assert(await L.get("SELECT id FROM questions WHERE id=?", qid), "retention discarded an unpaid corrected decision");
  failed = false;
  await F.repay(E, now + 15 * 86400000);
  assert.equal((await L.get("SELECT work FROM verifiers WHERE id='a'")).work, 10, "recovery ignored the official payment decision");
  assert.equal((await L.get("SELECT work FROM verifier_hours WHERE verifier='a'")).work, 10);
  assert.equal((await L.get("SELECT hour FROM verifier_hours WHERE verifier='a'")).hour, new Date(now).toISOString().slice(0, 13));
  await F.repay(E, now + 16 * 86400000);
  assert.equal((await L.get("SELECT work FROM verifiers WHERE id='a'")).work, 10);
  await L.close();
}
console.log("ok: durable credit recovery preserves corrected work and its original hour beyond the legacy cursor window");

for (const superseded of ["fact", "demand"]) {
  const L = await testDb("library");
  await L.exec("CREATE TABLE received(ref text PRIMARY KEY)");
  for (const id of ["a", "b"]) await L.run("INSERT INTO verifiers(id,seen,work) VALUES (?,'x',0)", id);
  let supersede = true;
  const E = { LIBRARY: { ...L, batch: async list => {
    if (supersede && list.some(s => s.sql.includes("factory_delivery_guard"))) {
      supersede = false;
      if (superseded === "fact") {
        await L.run("UPDATE questions SET state='open',canon=NULL,fact_engine=NULL");
        await L.run("UPDATE demands SET delivery_at=NULL");
      } else await L.run("UPDATE demands SET payload='{}',asked_at=?", new Date(now + 1).toISOString());
    }
    return L.batch(list);
  } } };
  const F = factory({ verbs, producers: { healthy: producers.healthy }, spot: 0, random: () => 1 });
  await F.demand(E, { ...asks[1], request: { value: "superseded" } }, "e", now);
  for (const id of ["a", "b"]) {
    const w = witness(id), work = await F.next(E, w, "e", now);
    await F.answer(E, w, { lease: work.lease, engine: "e", result: { value: 49 }, work: 10 }, now);
  }
  assert.equal((await L.get("SELECT count(*) n FROM received")).n, 0, "a stale result reached its consumer");
  assert.equal((await L.get("SELECT state FROM demands")).state, "live", "stale delivery acknowledged newer work");
  await L.close();
}
console.log("ok: superseded facts and demands cannot apply SQL consumer writes or acknowledge newer work");

const costs = [];
for (const count of [1, 24]) {
  const L = await testDb("library"), E = { LIBRARY: L };
  await L.exec("CREATE TABLE received(ref text PRIMARY KEY)");
  for (const id of ["a", "b"]) await L.run("INSERT INTO verifiers(id,seen,work) VALUES (?,'x',0)", id);
  const F = factory({ verbs, producers: { healthy: producers.healthy }, spot: 0, random: () => 1 });
  await F.demandAll(E, Array.from({ length: count }, (_, i) => ({ producer: "healthy", ref: String(i), verb: "future_compute",
    request: { value: i }, due_in_ms: 1000 })), "e", now);
  for (const id of ["a", "b"]) {
    const w = witness(id), pack = await F.nextPack(E, w, "e", { n: count, lease_ms: 60000 }, now);
    assert.equal(pack.questions.length, count);
    const before = L.cost.calls;
    await F.answerPack(E, w, { lease: pack.lease, engine: "e", answers: pack.questions.map(q => ({ question: q.question, result: { value: 49 }, work: 10 })) }, now);
    if (id === "b") costs.push(L.cost.calls - before);
  }
  assert.equal((await L.get("SELECT count(*) n FROM received")).n, count);
  await L.close();
}
assert(costs[1] <= costs[0] + 1 && costs[1] <= 15, `delivery calls scale with task count: ${costs}`);
console.log(`ok: normal delivery stays batched (${costs.join('/')} database calls for1/24tasks)`);
await db.close();
