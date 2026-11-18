import {seedComputeWork,seedComputeWorkMany,fixtureDemand} from './pg/fixtures.mjs';
// SPDX-License-Identifier: AGPL-3.0-or-later
// Capacity backs off background reads while the same facts, fairness and
// public counts survive; failures cannot wedge a cache or the background lane.
import assert from "node:assert/strict";
import { createServer } from "node:net";
import postgres from "./pg/node_modules/postgres/src/index.js";
import { POSTGRES_OPTIONS, placeholders } from "../worker/db.js";
import { capacity, backgroundLane, paceCache, claimDaemon } from "./factory_capacity.mjs";
const {openEnv}=await import(process.env.WFSIM_FACTORYD_MODULE||"./factoryd.mjs");
import {demandOf as renderDemand,scorePace,tallyBase,tallyOf} from '../worker/contribution.js';
import { testDb } from "./pg/test_db.mjs";
import { factory } from "../worker/tasks.js";
import { cellsFilling } from "../worker/board_tasks.js";
const demandOf=fixtureDemand(renderDemand);

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

let recoveryHost = { ...host, io_pct: 1 };
const recovery = capacity({ env: { FACTORY_STATISTICS_CATCHUP_DUTY_PCT: "20" }, sample: () => recoveryHost,
  critical: { settle: 2000 } });
const catchupInterval = () => recovery.interval("statistics", 1000, undefined, { catchup: true });
recovery.observe("statistics", 1000);
assert.equal(catchupInterval(), 20200);
recovery.observe("settle", 100);
assert.equal(catchupInterval(), 5050);
assert.equal(recovery.interval("statistics", 1000), 20200);
recovery.observe("tally", 1000);
assert.equal(recovery.interval("tally", 1000, undefined, { catchup: true }), 20200);
recoveryHost = { ...host, io_pct: 23 };
assert.equal(catchupInterval(), 24600);
recoveryHost = { ...host };
recovery.observe("settle", 4000, true);
assert.equal(catchupInterval(), 40000);
recovery.observe("statistics", 1000, true);
assert.equal(catchupInterval(), 80000);
console.log("ok: an explicit catch-up duty applies only to unsealed statistics with observed foreground headroom");

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
const connection = (held) => ({ unsafe: async () => [], reserve: async () => ({ unsafe: async () => [{ held, pid:123 }], release: () => { released++; } }), end: async () => { closed++; } });
await assert.rejects(claimDaemon(connection(false), () => {}), /another factory daemon/);
assert.equal(closed, 1); assert.equal(released, 1);
const owner = await claimDaemon(connection(true), () => { lost++; });
owner.lost(); owner.lost(); assert.equal(lost, 1);
await owner.release(); owner.lost(); assert.equal(lost, 1);
assert.equal(closed, 2); assert.equal(released, 2);
console.log("ok: a second daemon is refused, lost ownership stops once, shutdown releases it");

const pulseDb=await testDb("library");
let pulseQueries=0,pulseLost=0,lostSessionQueries=0;
const {pid:pulsePid}=await pulseDb.get('SELECT pg_backend_pid() pid');
const pulseSession={unsafe:async(sql,args=[])=>{
  if(pulseLost){lostSessionQueries++;throw Error('lost connection');}
  if(sql.startsWith('UPDATE state_factory_snapshot')){pulseQueries++;return pulseDb.all(sql,...args);}
  return [{held:true,pid:pulsePid}];
},release:()=>{}};
const pulsing=await claimDaemon({unsafe:async()=>[],reserve:async()=>pulseSession,end:async()=>{}},()=>{pulseLost++;});
assert.equal(await pulsing.pulse('owner',1000),false);
assert.equal(await pulseDb.get('SELECT 1 FROM state_factory_snapshot'),null);
await pulseDb.run(`INSERT INTO state_factory_snapshot(id,at,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit)
  VALUES(1,'old-statistics',7,2,3,4,5,6,'1970-01-01T00:00:00.000Z','')`);
assert.equal(await pulsing.pulse('owner',2000),true);
const pulseRow=await pulseDb.get('SELECT * FROM state_factory_snapshot WHERE id=1');
assert.equal(pulseRow.daemon_at,new Date(2000).toISOString());assert.equal(pulseRow.daemon_commit,'owner');
const wrongSession={...pulseSession,unsafe:async(sql,args=[])=>sql.includes('pg_try_advisory_lock')
  ? [{held:true,pid:pulsePid+1}] : pulseSession.unsafe(sql,args)};
const wrongOwner=await claimDaemon({unsafe:async()=>[],reserve:async()=>wrongSession,end:async()=>{}},()=>{});
assert.equal(await wrongOwner.pulse('wrong-backend',3000),false);
assert.equal((await pulseDb.get('SELECT daemon_at FROM state_factory_snapshot')).daemon_at,pulseRow.daemon_at);
await wrongOwner.release();
assert.deepEqual([pulseRow.at,pulseRow.open,pulseRow.urgent,pulseRow.asking,pulseRow.computing,pulseRow.queue_rows,pulseRow.settle_queue_rows],
  ['old-statistics',7,2,3,4,5,6]);
assert.equal(await pulsing.pulse('old-clock',1000),false);
assert.equal((await pulseDb.get('SELECT daemon_at FROM state_factory_snapshot')).daemon_at,pulseRow.daemon_at);
pulsing.lost();const stoppedQueries=pulseQueries;
assert.equal(await pulsing.pulse('lost',3000),false);assert.equal(pulseQueries,stoppedQueries);assert.equal(pulseLost,1);
await pulsing.release();assert.equal(lostSessionQueries,0);
assert.equal(await pulsing.pulse('released',4000),false);assert.equal(pulseQueries,stoppedQueries);
await pulseDb.close();
console.log('ok: only a live reserved owner pulses existing rows, preserves counts, never regresses and stops on loss/release');


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
          packet("T", Buffer.concat([Buffer.from([0, 2]), Buffer.from("held\0"), column,
            Buffer.from("pid\0"), (()=>{const pid=Buffer.from(column);pid.writeInt32BE(23,6);pid.writeInt16BE(4,10);return pid;})()])),
          packet("D", Buffer.concat([Buffer.from([0, 2, 0, 0, 0, 1, 116, 0, 0, 0, 4]),Buffer.from('4242')])) ,
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

const pools = [],blockedPools=new Map();
const opened = await openEnv({ PG_URL: "postgres://local/wfsim", PG_ACCOUNTS_URL: "postgres://local/accounts" }, (_, options) => {
  pools.push(options);
  return { unsafe: async () => {
    if (["wfsim-factoryd-background","wfsim-factoryd"].includes(options.connection.application_name))
      await new Promise(resolve=>blockedPools.set(options.connection.application_name,resolve));
    return [];
  }, end: async () => {} };
});
const waiting = opened.backgroundEnv.LIBRARY.prepare("SELECT 1").all();
const primary = opened.env.LIBRARY.prepare("SELECT 1").all();
await Promise.resolve();await opened.signalEnv.LIBRARY.prepare("SELECT 1").all();
await opened.statisticsEnv.LIBRARY.prepare("SELECT 1").all();
assert.deepEqual(pools.map((p) => p.max), [1, 2, 1, 1, 1, 1, 1, 1, 1]);
assert.equal(pools.reduce((n,p)=>n+p.max,0),10);
assert.equal(pools[2].connection.application_name,"wfsim-factoryd-signal");
assert.equal(pools[2].connection.statement_timeout,30000);
assert.equal(pools[3].connection.application_name,"wfsim-factoryd-statistics-control");
assert.deepEqual(pools.slice(5).map(p=>p.connection.application_name),
  ["wfsim-factoryd-admission","wfsim-factoryd-verification","wfsim-factoryd-delivery","wfsim-factoryd-contribution"]);
for(const resolve of blockedPools.values())resolve();await Promise.all([waiting,primary]);await opened.close();
console.log("ok: signal and statistics control stay independent while primary and background pools are busy");

const writeDb=await testDb('library');
for(const stopAt of ['before','last-sql','last-check']){
  let active=stopAt!=='before',wrote=false,checks=0;
  const live=await openEnv({PG_URL:'postgres://isolated/write'},(_,options)=>{
    const unsafe=async(sql,args)=>{
      const rows=await writeDb.all(sql,...args);
      if(sql.startsWith('INSERT INTO state_factory_snapshot')){wrote=true;if(stopAt==='last-sql')active=false;}
      return rows;
    };
    return {unsafe,end:async()=>{},begin:async fn=>{
      await writeDb.exec('BEGIN');
      try{const value=await fn({unsafe});await writeDb.exec('COMMIT');return value;}
      catch(error){await writeDb.exec('ROLLBACK');throw error;}
    }};
  });
  try{
    const statement=writeDb.prepare("INSERT INTO state_factory_snapshot(id,at,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit) VALUES(1,'late',0,0,0,0,0,0,'1970-01-01T00:00:00.000Z','')");
    const result=await live.writeSnapshot(statement,async()=>{checks++;await Promise.resolve();if(stopAt==='last-check'&&checks===2)active=false;return active;});
    assert.equal(result,false,stopAt+' must reject a stopped snapshot');
    assert.equal(wrote,stopAt!=='before');
    assert.equal(await writeDb.get('SELECT at FROM state_factory_snapshot'),null,stopAt+' committed late statistics');
  }finally{await live.close();}
}
await writeDb.close();
console.log('ok: observed pause or owner loss rolls back a real snapshot INSERT, including stop during its final SQL/check await');

const L = await testDb("library"), now = Date.now(), at = new Date(now).toISOString();
for (const [id, by] of [["a", "verified:e1"], ["b", "official"]]) {
  await L.run("INSERT INTO state_board_measurement_selection (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?, 'r','m',?,1,'kpm',0,?,?)", id, by, at, at);
}
const wanted = await demandOf({ LIBRARY: L }, now), base = await tallyBase(L, now), tally = await tallyOf({ LIBRARY: L }, base, now);
let reads = 0;
const db = { ...L, prepare(sql) { if (/FROM state_board_measurement_selection WHERE finished_at >=/.test(sql)) reads++; return L.prepare(sql); } };
const env = { LIBRARY: db }, kept = await scorePace(env, now);
assert.deepEqual(await demandOf(env, now, undefined, { pace: kept }), wanted);
assert.deepEqual(await tallyOf(env, base, now, kept), tally);
assert.equal(reads, 1, "demand and tally must share one rolling score read");
console.log("ok: demand and tally stay byte-equivalent with one rolling score read");

await L.run("INSERT INTO entity_compute_batches (id,at,why,total) VALUES ('rescore-capacity','x','x',2)");
await L.run("INSERT INTO state_board_pending_work (batch,build_id,ruler,mode) VALUES ('rescore-capacity','one','r','m')");
let samples = 0;
const budgets = [];
const sampled = { LIBRARY: { ...L,
  prepare(sql) { if (/WITH s AS/.test(sql)) samples++; return L.prepare(sql); },
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
await L.run("INSERT INTO state_board_pending_work (batch,build_id,ruler,mode) VALUES ('rescore-capacity','two','r','m')");
assert.equal((await demandOf(sampled, now + 11 * 60_000, undefined, options)).owed.rescores, 2);
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = DEFAULT", "a newly sealed count does not re-enable historical count reads");
assert.equal(samples, 1, "slowing price estimates must not slow queue counts");
await demandOf(sampled, now + 12 * 60_000, undefined, options);
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = DEFAULT", "refreshed counts and cached costs return to the normal budget");
await demandOf(sampled, now + 12 * 60_000, undefined, { costEveryMs: 600_000 });
assert.equal(budgets.at(-1), "SET LOCAL statement_timeout = 120000", "expired costs need the full budget while queue counts stay cached");
assert.equal(samples, 2, "only an expired cost sample is read again");
console.log("ok: either cache refresh keeps its full read budget; both warm caches use the normal budget");

for (const [id, state, odd, age] of [["both", "nondeterministic", 1, 0], ["recent", "fact", 1, 0],
  ["old", "fact", 1, 2 * 86_400_000], ["unsettled", "nondeterministic", 0, 2 * 86_400_000]]) {
  await (async (fixtureDb,args)=>{await seedComputeWork(fixtureDb,{id:args[0],verb:"simulate",request:"{}",state:args[1],due_at:"x",opened_at:"x",odd:args[2],fact_at:args[3]});})(L,[id,state,odd,new Date(now - age).toISOString()]);
}
const f = factory({ verbs: { simulate: { canon: () => "" } }, producers: {} });
assert.equal((await f.audit({ LIBRARY: L }, now)).find((x) => x.kind === "nondeterministic").questions, 3);
console.log("ok: the watchdog counts overlapping disagreement states once and excludes old flags");

await L.close();

const A = await testDb("library"), past = new Date(now - 3_600_000).toISOString();
await seedComputeWorkMany(A,Array.from({length:20000},(_,i)=>({id:'audit-'+(i+1),verb:'simulate',request:'{}',state:'open',opened_at:past,due_at:past})));
await A.exec(`INSERT INTO relation_task_consumers (producer,ref,question,state,asked_at,due_at)
  SELECT CASE WHEN id IN ('audit-19999','audit-20000') THEN 'silent' ELSE 'busy' END,
    id,id,'live',opened_at,due_at FROM state_compute_work;
  INSERT INTO record_candidate_answers (question,engine,device,owner,net,canon,result,work,at)
  VALUES ('audit-1','current','witness','owner','','c','{}',1,'${at}');
  INSERT INTO entity_compute_devices (id,seen,last_at) VALUES ('witness','${at}','${at}');
  ANALYZE state_compute_scheduling; ANALYZE relation_task_consumers; ANALYZE record_candidate_answers;`);
let auditSql, auditArgs, unsettledSql, unsettledArgs;
const observed = { ...A, prepare(sql) {
  const statement = A.prepare(sql);
  if (/count\(DISTINCT d\.question\)/i.test(sql)) return { bind(...args) { auditSql = sql; auditArgs = args; return statement.bind(...args); } };
  if (/SELECT count\(\*\) AS n FROM state_compute_effective_work q WHERE q\.state IN/.test(sql)) return { bind(...args) { unsettledSql = sql; unsettledArgs = args; return statement.bind(...args); } };
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
  let readQuestions = 0, questionScans = 0;
  const walk = node => {
    if (node["Relation Name"] === "state_compute_scheduling") {
      questionScans++;
      readQuestions += (node["Actual Rows"] + (node["Rows Removed by Filter"] || 0)) * node["Actual Loops"];
    }
    for (const child of node.Plans || []) walk(child);
  };
  walk(plan["QUERY PLAN"][0].Plan);
  assert.ok(questionScans > 0 && readQuestions <= 100, `${mode}: healthy producers must be excluded before their overdue questions are read (${readQuestions})`);
}
await A.exec("DEALLOCATE stalled_audit; RESET plan_cache_mode");
await A.run("DELETE FROM record_candidate_answers WHERE question='audit-1'");
const crowded = (await f.audit({ LIBRARY: observed }, now)).find(row => row.kind === "stalled:busy");
assert.equal(crowded?.questions, 1001, "a large silent producer must stop at the observation bound");
assert.equal(crowded?.at_least, true, "a bounded observation must not claim an exact total");
await A.exec(`PREPARE crowded_stall AS ${placeholders(auditSql)}`);
const crowdedArgs = [...auditArgs]; crowdedArgs[1] = "busy";
for (const mode of ["force_generic_plan", "force_custom_plan"]) {
  await A.exec(`SET plan_cache_mode=${mode}`);
  const plan = await A.get(`EXPLAIN (ANALYZE,FORMAT JSON,TIMING OFF) EXECUTE crowded_stall(${crowdedArgs.map(literal).join(",")})`);
  let read = 0, questionScans = 0;
  const walk = node => {
    if (node["Relation Name"] === "state_compute_scheduling") {
      questionScans++;
      read += (node["Actual Rows"] + (node["Rows Removed by Filter"] || 0)) * node["Actual Loops"];
    }
    for (const child of node.Plans || []) walk(child);
  };
  walk(plan["QUERY PLAN"][0].Plan);
  assert(questionScans > 0 && read <= 1100, `${mode}: a silent producer caused ${read} question reads`);
}
await A.exec("DEALLOCATE crowded_stall; RESET plan_cache_mode");
console.log("ok: a large silent producer uses a bounded lower count under generic and custom plans");
await A.run("UPDATE state_compute_work SET state='spot', opened_at=? WHERE id='audit-20000'", new Date(now - 3 * 3_600_000).toISOString());
await A.exec(`INSERT INTO record_candidate_answers (question,engine,device,owner,net,canon,result,work,at)
  SELECT question,'fresh','new-witness','new-owner','','c','{}',1,'${at}' FROM relation_task_consumers WHERE producer='busy'`);
const unsettledFindings = await f.audit({ LIBRARY: observed }, now);
assert.equal(unsettledFindings.find(x => x.kind === "unsettled")?.questions, 1);
assert.ok(unsettledSql, "capture the official settlement observation");
await A.exec(`PREPARE unsettled_audit AS ${placeholders(unsettledSql)}`);
for (const mode of ["force_generic_plan", "force_custom_plan"]) {
  await A.exec(`SET plan_cache_mode=${mode}`);
  const plan = await A.get(`EXPLAIN (ANALYZE,FORMAT JSON,TIMING OFF) EXECUTE unsettled_audit(${unsettledArgs.map(literal).join(",")})`);
  let readAnswers = 0;
  const walk = node => {
    if (node["Relation Name"] === "record_candidate_answers") readAnswers += Math.max(1, node["Actual Rows"] + (node["Rows Removed by Filter"] || 0)) * node["Actual Loops"];
    for (const child of node.Plans || []) walk(child);
  };
  walk(plan["QUERY PLAN"][0].Plan);
  assert.ok(readAnswers > 0 && readAnswers <= 100, `${mode}: official settlement must read its candidates without scanning other questions' recent answers (${readAnswers})`);
}
await A.exec("DEALLOCATE unsettled_audit; RESET plan_cache_mode");
await A.close();
console.log("ok: the watchdog counts silent producers without reading healthy producers' overdue questions");
console.log("ok: official settlement observations read only their questions' answers");

const C = await testDb("library");
await C.exec(`INSERT INTO state_board_pending_work (batch,build_id,ruler,mode)
  SELECT 'batch-' || ((i-1)/2000), 'queued-' || ((i-1)%1000), 'test__cell',
    CASE WHEN (i-1)%2000 < 1000 THEN 'base' ELSE 'cycle' END FROM generate_series(1,20000) i;
  ANALYZE state_board_pending_work;`);
let cellsSql;
const cellsDb = { ...C, prepare(sql) { if (/FROM state_board_effective_pending_work WHERE ruler LIKE/.test(sql)) cellsSql=sql; return C.prepare(sql); } };
const cells = await cellsFilling({ LIBRARY: cellsDb }, now);
assert.equal(cells.find(x => x.ruler === "test__cell")?.owed, 2000);
assert.ok(cellsSql, "capture the effective board cell count");
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
