// SPDX-License-Identifier: AGPL-3.0-or-later
// Capacity backs off background reads while the same facts, fairness and
// public counts survive; failures cannot wedge a cache or the background lane.
import assert from "node:assert/strict";
import { capacity, backgroundLane, paceCache, claimDaemon } from "./factory_capacity.mjs";
import { openEnv } from "./factoryd.mjs";
import { demandOf, scorePace, tallyBase, tallyOf } from "../worker/contribution.js";
import { testDb } from "./pg/test_db.mjs";
import { factory } from "../worker/tasks.js";

const host = { cpus: 4, load: 1, io_pct: 0, memory_pct: 0 };
const fast = capacity({ env: {}, sample: () => host });
const slow = capacity({ env: {}, sample: () => ({ ...host, cpus: 2, load: 8, io_pct: 60 }) });
for (const c of [fast, slow]) c.observe("costs", 20_000);
assert.equal(fast.interval("costs", 600_000), 600_000);
assert.ok(slow.interval("costs", 600_000) > fast.interval("costs", 600_000));
const memory = capacity({ env: {}, sample: () => ({ ...host, memory_pct: 90 }) });
memory.observe("costs", 20_000);
assert.ok(memory.interval("costs", 600_000) > fast.interval("costs", 600_000));
const before = slow.interval("costs", 600_000);
slow.observe("costs", 20_000, true);
assert.ok(slow.interval("costs", 600_000) > before);
for (let i = 0; i < 100; i++) slow.observe("costs", 1);
assert.equal(slow.interval("costs", 600_000), 600_000);
slow.observe("costs", 1e9, true);
assert.equal(slow.interval("costs", 600_000), 3_600_000);
const manual = capacity({ env: { WFSIM_BACKGROUND_AUTO: "0" }, sample: () => host });
manual.observe("costs", 1e9, true);
assert.equal(manual.interval("costs", 600_000), 600_000);
console.log("ok: host pressure, bounded backoff, gradual recovery and operator override");

let calls = 0, fail = false;
const pace = paceCache(async () => { calls++; if (fail) throw Error("cut"); return { volunteers: calls }; });
const [a, b] = await Promise.all([pace(1000), pace(1000)]);
assert.deepEqual(a, b); assert.equal(calls, 1);
await pace(60_999); assert.equal(calls, 1);
fail = true; await assert.rejects(pace(61_000), /cut/);
fail = false; await pace(61_000); assert.equal(calls, 3);
await pace(999); assert.equal(calls, 4);
console.log("ok: one pace read, expiry, failed read retry and a clock moving backwards");

const lane = backgroundLane(), order = [];
let release;
const first = lane(async () => { order.push("first"); await new Promise((r) => { release = r; }); throw Error("read failed"); });
const failed = assert.rejects(first, /read failed/);
const second = lane(async () => { order.push("second"); });
await Promise.resolve();
assert.deepEqual(order, ["first"]);
let critical = false; await Promise.resolve().then(() => { critical = true; });
assert.equal(critical, true); assert.deepEqual(order, ["first"]);
release(); await failed; await second;
assert.deepEqual(order, ["first", "second"]);
console.log("ok: background reads serialize, a failure releases the lane, critical work proceeds");

let lost = 0, closed = 0, released = 0;
const connection = (held) => ({ reserve: async () => ({ unsafe: async () => [{ held }], release: () => { released++; } }), end: async () => { closed++; } });
await assert.rejects(claimDaemon(connection(false), () => {}), /another factory daemon/);
assert.equal(closed, 1); assert.equal(released, 1);
const owner = await claimDaemon(connection(true), () => { lost++; });
owner.lost(); owner.lost(); assert.equal(lost, 1);
await owner.release(); owner.lost(); assert.equal(lost, 1);
assert.equal(closed, 2); assert.equal(released, 2);
console.log("ok: a second daemon is refused, lost ownership stops once, shutdown releases it");

const pools = [];
let backgroundRelease;
const opened = await openEnv({ PG_URL: "postgres://local/wfsim", PG_ACCOUNTS_URL: "postgres://local/accounts" }, (_, options) => {
  pools.push(options);
  return { unsafe: async () => {
    if (options.connection.application_name.endsWith("background")) await new Promise((r) => { backgroundRelease = r; });
    return [];
  }, end: async () => {} };
});
const waiting = opened.backgroundEnv.LIBRARY.prepare("SELECT 1").all();
await Promise.resolve();
await opened.env.LIBRARY.prepare("SELECT 1").all();
assert.deepEqual(pools.map((p) => p.max), [2, 2, 1]);
backgroundRelease(); await waiting; await opened.close();
console.log("ok: a blocked statistics pool does not consume the critical pool");

const L = await testDb("library"), now = Date.now(), at = new Date(now).toISOString();
for (const [id, by] of [["a", "verified:e1"], ["b", "official"]]) {
  await L.run("INSERT INTO scores (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?, 'r','m',?,1,'kpm',0,?,?)", id, by, at, at);
}
const wanted = await demandOf({ LIBRARY: L }, now), base = await tallyBase(L, now), tally = await tallyOf({ LIBRARY: L }, base, now);
let reads = 0;
const db = { ...L, prepare(sql) { if (/FROM scores WHERE finished_at >=/.test(sql)) reads++; return L.prepare(sql); } };
const env = { LIBRARY: db }, kept = await scorePace(env, now);
assert.deepEqual(await demandOf(env, now, undefined, { pace: kept }), wanted);
assert.deepEqual(await tallyOf(env, base, now, kept), tally);
assert.equal(reads, 1, "demand and tally must share one rolling score read");
console.log("ok: demand and tally stay byte-equivalent with one rolling score read");

await L.run("INSERT INTO batches (id,at,why,total) VALUES ('rescore-capacity','x','x',2)");
await L.run("INSERT INTO queue (batch,build_id,ruler,mode) VALUES ('rescore-capacity','one','r','m')");
let samples = 0;
const sampled = { LIBRARY: { ...L, prepare(sql) { if (/WITH s AS MATERIALIZED/.test(sql)) samples++; return L.prepare(sql); } } };
const options = { costEveryMs: 3_600_000 };
assert.equal((await demandOf(sampled, now, undefined, options)).owed.rescores, 1);
await L.run("INSERT INTO queue (batch,build_id,ruler,mode) VALUES ('rescore-capacity','two','r','m')");
assert.equal((await demandOf(sampled, now + 11 * 60_000, undefined, options)).owed.rescores, 2);
assert.equal(samples, 1, "slowing price estimates must not slow queue counts");
console.log("ok: queue totals refresh while an expensive compute-cost sample is kept");

for (const [id, state, odd, age] of [["both", "nondeterministic", 1, 0], ["recent", "fact", 1, 0],
  ["old", "fact", 1, 2 * 86_400_000], ["unsettled", "nondeterministic", 0, 2 * 86_400_000]]) {
  await L.run("INSERT INTO questions (id,verb,request,state,due_at,opened_at,odd,fact_at) VALUES (?,'simulate','{}',?,'x','x',?,?)", id, state, odd, new Date(now - age).toISOString());
}
const f = factory({ verbs: { simulate: { canon: () => "" } }, producers: {} });
assert.equal((await f.audit({ LIBRARY: L }, now)).find((x) => x.kind === "nondeterministic").questions, 3);
console.log("ok: the watchdog counts overlapping disagreement states once and excludes old flags");

await L.close();
console.log("factory capacity checks passed");
