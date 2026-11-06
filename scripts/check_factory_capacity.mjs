// SPDX-License-Identifier: AGPL-3.0-or-later
// Capacity backs off background reads while the same facts, fairness and
// public counts survive; failures cannot wedge a cache or the background lane.
import assert from "node:assert/strict";
import { createServer } from "node:net";
import postgres from "./pg/node_modules/postgres/src/index.js";
import { POSTGRES_OPTIONS, placeholders } from "../worker/db.js";
import { capacity, backgroundLane, paceCache, claimDaemon } from "./factory_capacity.mjs";
import { openEnv } from "./factoryd.mjs";
import { demandOf, scorePace, tallyBase, tallyOf } from "../worker/contribution.js";
import { testDb } from "./pg/test_db.mjs";
import { factory } from "../worker/tasks.js";
import { cellsFilling } from "../worker/board_tasks.js";

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
assert.equal(slow.interval("costs", 60_000, 600_000), 600_000);
assert.equal(slow.interval("costs", 1_800_000, 600_000), 1_800_000);
const manual = capacity({ env: { WFSIM_BACKGROUND_AUTO: "0" }, sample: () => host });
manual.observe("costs", 1e9, true);
assert.equal(manual.interval("costs", 600_000), 600_000);
assert.equal(manual.interval("costs", 1_800_000, 600_000), 1_800_000);
const delayed = capacity({ env: {}, sample: () => host, critical: { settle: 2000 } });
delayed.observe("costs", 20_000); delayed.observe("settle", 10_000);
assert.ok(delayed.interval("costs", 600_000) > fast.interval("costs", 600_000));
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
let failedClosed = 0;
await assert.rejects(claimDaemon({ unsafe: async () => { throw Error("connect failed"); },
  end: async () => { failedClosed++; } }, () => {}), /connect failed/);
assert.equal(failedClosed, 1);
const connection = (held) => ({ unsafe: async () => [], reserve: async () => ({ unsafe: async () => [{ held }], release: () => { released++; } }), end: async () => { closed++; } });
await assert.rejects(claimDaemon(connection(false), () => {}), /another factory daemon/);
assert.equal(closed, 1); assert.equal(released, 1);
const owner = await claimDaemon(connection(true), () => { lost++; });
owner.lost(); owner.lost(); assert.equal(lost, 1);
await owner.release(); owner.lost(); assert.equal(lost, 1);
assert.equal(closed, 2); assert.equal(released, 2);
console.log("ok: a second daemon is refused, lost ownership stops once, shutdown releases it");

// The real driver must reserve a cold pool with the daemon's fetch_types setting.
const packet = (tag, body) => {
  const size = Buffer.alloc(4); size.writeInt32BE(body.length + 4);
  return Buffer.concat([Buffer.from(tag), size, body]);
};
const ready = packet("Z", Buffer.from("I")), sockets = new Set();
const server = createServer((socket) => {
  sockets.add(socket); socket.on("close", () => sockets.delete(socket));
  let pending = Buffer.alloc(0), started = false;
  socket.on("data", (data) => {
    pending = Buffer.concat([pending, data]);
    while (pending.length >= (started ? 5 : 4)) {
      const size = pending.readInt32BE(started ? 1 : 0), length = size + (started ? 1 : 0);
      if (pending.length < length) return;
      const tag = started ? String.fromCharCode(pending[0]) : "startup";
      pending = pending.subarray(length);
      if (tag === "startup") {
        started = true; socket.write(Buffer.concat([packet("R", Buffer.alloc(4)), ready]));
      } else if (tag === "Q") {
        const column = Buffer.alloc(18);
        column.writeInt32BE(16, 6); column.writeInt16BE(1, 10); column.writeInt32BE(-1, 12);
        socket.write(Buffer.concat([
          packet("T", Buffer.concat([Buffer.from([0, 1]), Buffer.from("held\0"), column])),
          packet("D", Buffer.from([0, 1, 0, 0, 0, 1, 116])),
          packet("C", Buffer.from("SELECT 1\0")), ready,
        ]));
      } else if (tag === "X") socket.end();
      else socket.destroy(Error(`unexpected protocol message ${tag}`));
    }
  });
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const cold = postgres(`postgres://fixture@127.0.0.1:${server.address().port}/factory`, {
  ...POSTGRES_OPTIONS, max: 1, idle_timeout: 0, max_lifetime: 0,
});
let deadline;
try {
  const held = await Promise.race([claimDaemon(cold, () => {}), new Promise((_, reject) => {
    deadline = setTimeout(() => reject(Error("a cold singleton connection did not start")), 3000);
  })]);
  await held.release();
} finally {
  clearTimeout(deadline); await cold.end({ timeout: 0 });
  for (const socket of sockets) socket.destroy();
  await new Promise((resolve) => server.close(resolve));
}
console.log("ok: the actual Postgres driver starts singleton ownership from a cold pool");

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
const budgets = [];
const sampled = { LIBRARY: { ...L,
  prepare(sql) { if (/WITH s AS \(/.test(sql)) samples++; return L.prepare(sql); },
  batch(statements) {
    const budget = statements.find(s => /^SET LOCAL statement_timeout/.test(s.sql));
    if (budget) budgets.push(budget.sql);
    return L.batch(statements);
  },
} };
const options = { costEveryMs: 3_600_000 };
assert.equal((await demandOf(sampled, now, undefined, options)).owed.rescores, 1);
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = 120000", "cold demand reads need the full budget");
await demandOf(sampled, now + 60_000, undefined, options);
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = DEFAULT", "both warm caches use the normal budget");
await L.run("INSERT INTO queue (batch,build_id,ruler,mode) VALUES ('rescore-capacity','two','r','m')");
assert.equal((await demandOf(sampled, now + 11 * 60_000, undefined, options)).owed.rescores, 2);
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = 120000", "expired queue counts need the full budget while cost estimates stay cached");
assert.equal(samples, 1, "slowing price estimates must not slow queue counts");
await demandOf(sampled, now + 12 * 60_000, undefined, options);
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = DEFAULT", "refreshed counts and cached costs return to the normal budget");
await demandOf(sampled, now + 12 * 60_000, undefined, { costEveryMs: 600_000 });
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = 120000", "expired costs need the full budget while queue counts stay cached");
assert.equal(samples, 2, "only an expired cost sample is read again");
console.log("ok: either cache refresh keeps its full read budget; both warm caches use the normal budget");

for (const [id, state, odd, age] of [["both", "nondeterministic", 1, 0], ["recent", "fact", 1, 0],
  ["old", "fact", 1, 2 * 86_400_000], ["unsettled", "nondeterministic", 0, 2 * 86_400_000]]) {
  await L.run("INSERT INTO questions (id,verb,request,state,due_at,opened_at,odd,fact_at) VALUES (?,'simulate','{}',?,'x','x',?,?)", id, state, odd, new Date(now - age).toISOString());
}
const f = factory({ verbs: { simulate: { canon: () => "" } }, producers: {} });
assert.equal((await f.audit({ LIBRARY: L }, now)).find((x) => x.kind === "nondeterministic").questions, 3);
console.log("ok: the watchdog counts overlapping disagreement states once and excludes old flags");

await L.close();

const A = await testDb("library"), past = new Date(now - 3_600_000).toISOString();
await A.exec(`INSERT INTO questions (id,verb,request,state,opened_at,due_at)
  SELECT 'audit-' || i, 'simulate', '{}', 'open', '${past}', '${past}' FROM generate_series(1,20000) i;
  INSERT INTO demands (producer,ref,question,state,asked_at,due_at)
  SELECT CASE WHEN id IN ('audit-19999','audit-20000') THEN 'silent' ELSE 'busy' END,
    id,id,'live',opened_at,due_at FROM questions;
  INSERT INTO answers (question,engine,device,owner,net,canon,result,work,at)
  VALUES ('audit-1','current','witness','owner','','c','{}',1,'${at}');
  INSERT INTO verifiers (id,seen,last_at) VALUES ('witness','${at}','${at}');
  ANALYZE questions; ANALYZE demands; ANALYZE answers;`);
let auditSql, auditArgs, unsettledSql, unsettledArgs;
const observed = { ...A, prepare(sql) {
  const statement = A.prepare(sql);
  if (/count\(DISTINCT d\.question\)/i.test(sql)) return { bind(...args) { auditSql = sql; auditArgs = args; return statement.bind(...args); } };
  if (/SELECT count\(\*\) AS n FROM questions q WHERE q\.state IN/.test(sql)) return { bind(...args) { unsettledSql = sql; unsettledArgs = args; return statement.bind(...args); } };
  return statement;
} };
const findings = await f.audit({ LIBRARY: observed }, now);
assert.equal(findings.find(x => x.kind === "stalled:silent")?.questions, 2);
assert.ok(!findings.some(x => x.kind === "stalled:busy"));
assert.ok(auditSql, "capture the stalled-producer observation");
const literal = value => typeof value === "number" ? String(value) : value === null ? "NULL" : `'${String(value).replaceAll("'", "''")}'`;
await A.exec(`PREPARE stalled_audit AS ${placeholders(auditSql)}`);
for (const mode of ["force_generic_plan", "force_custom_plan"]) {
  await A.exec(`SET plan_cache_mode=${mode}`);
  const plan = await A.get(`EXPLAIN (ANALYZE,FORMAT JSON,TIMING OFF) EXECUTE stalled_audit(${auditArgs.map(literal).join(",")})`);
  let readQuestions = 0;
  const walk = node => {
    if (node["Relation Name"] === "questions") readQuestions += (node["Actual Rows"] + (node["Rows Removed by Filter"] || 0)) * node["Actual Loops"];
    for (const child of node.Plans || []) walk(child);
  };
  walk(plan["QUERY PLAN"][0].Plan);
  assert.ok(readQuestions <= 100, `${mode}: healthy producers must be excluded before their overdue questions are read (${readQuestions})`);
}
await A.exec("DEALLOCATE stalled_audit; RESET plan_cache_mode");
await A.run("UPDATE questions SET state='spot', opened_at=? WHERE id='audit-20000'", new Date(now - 3 * 3_600_000).toISOString());
await A.exec(`INSERT INTO answers (question,engine,device,owner,net,canon,result,work,at)
  SELECT question,'fresh','new-witness','new-owner','','c','{}',1,'${at}' FROM demands WHERE producer='busy'`);
const unsettledFindings = await f.audit({ LIBRARY: observed }, now);
assert.equal(unsettledFindings.find(x => x.kind === "unsettled")?.questions, 1);
assert.ok(unsettledSql, "capture the official settlement observation");
await A.exec(`PREPARE unsettled_audit AS ${placeholders(unsettledSql)}`);
for (const mode of ["force_generic_plan", "force_custom_plan"]) {
  await A.exec(`SET plan_cache_mode=${mode}`);
  const plan = await A.get(`EXPLAIN (ANALYZE,FORMAT JSON,TIMING OFF) EXECUTE unsettled_audit(${unsettledArgs.map(literal).join(",")})`);
  let readAnswers = 0;
  const walk = node => {
    if (node["Relation Name"] === "answers") readAnswers += Math.max(1, node["Actual Rows"] + (node["Rows Removed by Filter"] || 0)) * node["Actual Loops"];
    for (const child of node.Plans || []) walk(child);
  };
  walk(plan["QUERY PLAN"][0].Plan);
  assert.ok(readAnswers <= 100, `${mode}: official settlement must not scan other questions' recent answers (${readAnswers})`);
}
await A.exec("DEALLOCATE unsettled_audit; RESET plan_cache_mode");
await A.close();
console.log("ok: the watchdog counts silent producers without reading healthy producers' overdue questions");
console.log("ok: official settlement observations read only their questions' answers");

const C = await testDb("library");
await C.exec(`INSERT INTO queue (batch,build_id,ruler,mode)
  SELECT 'batch-' || ((i-1)/2000), 'queued-' || ((i-1)%1000), 'test__cell',
    CASE WHEN (i-1)%2000 < 1000 THEN 'base' ELSE 'cycle' END FROM generate_series(1,20000) i;
  ANALYZE queue;`);
let cellsSql;
const cellsDb = { ...C, prepare(sql) { if (/FROM queue WHERE ruler LIKE/.test(sql)) cellsSql=sql; return C.prepare(sql); } };
const cells = await cellsFilling({ LIBRARY: cellsDb }, now);
assert.equal(cells.find(x => x.ruler === "test__cell")?.owed, 2000);
const cellPlan = await C.get(`EXPLAIN (ANALYZE,FORMAT JSON,TIMING OFF) ${cellsSql}`);
let sortRows=0;
const countSort = node => {
  if (node["Node Type"] === "Sort") sortRows=Math.max(sortRows,node["Actual Rows"]*node["Actual Loops"]);
  for (const child of node.Plans || []) countSort(child);
};
countSort(cellPlan["QUERY PLAN"][0].Plan);
assert.ok(sortRows <= 4000, `cell counts must deduplicate batches before sorting the entire queue (${sortRows})`);
await C.close();
console.log("ok: cell counts preserve build and mode identity without sorting every queued copy");
console.log("factory capacity checks passed");
