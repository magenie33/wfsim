// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import postgres from "./pg/node_modules/postgres/src/index.js";
import { openEnv } from "./factoryd.mjs";
import { pgDb, placeholders, postgresClient } from "../worker/db.js";
import { signalTick, workWaiting } from "../worker/settler.js";

const testUrl = process.env.WFSIM_SIGNAL_TEST_URL;
assert(testUrl, "set WFSIM_SIGNAL_TEST_URL to an isolated local PostgreSQL");
const url = new URL(testUrl);
assert(["postgres:", "postgresql:"].includes(url.protocol) &&
  ["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) && !url.search && !url.hash,
"the signal test requires a loopback PostgreSQL URL without connection overrides");

const schema = "factory_signal_" + randomBytes(6).toString("hex");
const sql = postgres(testUrl, {
  max: 1, fetch_types: false, onnotice() {},
  connection: { search_path: schema, statement_timeout: 20_000, jit: "off" },
  types: { bigint: { to: 20, from: [20, 1700], parse: Number, serialize: String } },
});
const database = pgDb(postgresClient(sql));
const now = Date.parse("2026-11-22T04:19:00.000Z"), epoch = "2026-11-20T00:00:00.000Z";
const history = 50_000;
let sampled;
const measured = { ...database, prepare(statement) {
  const prepared = database.prepare(statement);
  if (!statement.includes("FROM stats_open")) return prepared;
  return { ...prepared, bind(...args) {
    sampled = [placeholders(statement), args];
    return prepared.bind(...args);
  } };
} };
const signal = () => signalTick({ LIBRARY: measured }, { engine: "fixture" }, now);
const wave = () => workWaiting(measured, now);
const totals = ({ open, urgent }) => [open, urgent];

function historyLoops(node) {
  if (["CTE a", "CTE queue_signal", "CTE queue_signal_candidates"].includes(node["Subplan Name"])) return [];
  const found = ["state_compute_scheduling", "entity_compute_tasks"].includes(node["Relation Name"])
    ? [node["Actual Loops"]] : [];
  return found.concat((node.Plans ?? []).flatMap(historyLoops));
}
async function scanLoops() {
  assert(sampled, "the signal statement was captured");
  const [statement, args] = sampled;
  sampled = undefined;
  const [row] = await sql.unsafe("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) " + statement, args);
  const plan = row["QUERY PLAN"][0].Plan;
  const loops = historyLoops(plan);
  assert(loops.length, "the plan includes the prior history path");
  return { loops: loops.reduce((n, count) => n + count, 0) };
}
async function addLive(id, task, waits, until = null) {
  await sql`INSERT INTO entity_compute_specs VALUES (${id},'simulate','{}')`;
  await sql`INSERT INTO entity_compute_tasks (id,computation,generation,created_at,reason)
    VALUES (${task},${id},1,'2026-11-22T04:18:00.000Z','fixture')`;
  await sql`INSERT INTO state_compute_scheduling (id,verb,task_id,state,due_at,opened_at,zone,waits,lease_until,leased_to)
    VALUES (${id},'simulate',${task},'open','2026-11-22T04:18:00.000Z','2026-11-22T04:18:00.000Z',3,${waits},${until},${until ? "holder" : null})`;
}
async function addCandidates(first,last) {
  await sql`INSERT INTO entity_compute_specs
    SELECT 'candidate-'||i,'simulate','{}' FROM generate_series(${first}::int,${last}::int) i`;
  await sql`INSERT INTO entity_compute_tasks (id,computation,generation,created_at,reason)
    SELECT ${history}+100+i,'candidate-'||i,1,'2026-11-22T04:18:00.000Z','fixture'
    FROM generate_series(${first}::int,${last}::int) i`;
  await sql`INSERT INTO state_compute_scheduling (id,verb,task_id,state,due_at,opened_at,zone)
    SELECT t.computation,'simulate',t.id,'open','2026-11-22T04:18:00.000Z','2026-11-22T04:18:00.000Z',3
    FROM entity_compute_tasks t WHERE t.id BETWEEN ${history+100+first} AND ${history+100+last}`;
}

try {
  await sql.unsafe(`CREATE SCHEMA ${schema}`);
  const daemon = await openEnv({ PG_URL: testUrl }, (target, options) => postgres(target, {
    ...options, connection: { jit: "on", ...options.connection, search_path: schema },
  }));
  try {
    const settings = `SELECT current_setting('application_name') AS name,current_setting('jit') AS jit`;
    assert.deepEqual(await daemon.signalEnv.LIBRARY.prepare(settings).first(), { name: "wfsim-factoryd-signal", jit: "off" });
    assert.deepEqual(await daemon.env.LIBRARY.prepare(settings).first(), { name: "wfsim-factoryd", jit: "on" });
  } finally { await daemon.close(); }
  for (const name of ["schema", "library", "sync", "inventory", "build_workflows", "board_admission"]) {
    await sql.unsafe(readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), "utf8"));
  }
  await sql`INSERT INTO entity_compute_specs SELECT md5(i::text),'simulate','{}' FROM generate_series(1,${history}::int) i`;
  await sql`INSERT INTO entity_compute_tasks (id,computation,generation,created_at,reason)
    SELECT i,md5(i::text),1,'2026-11-01T00:00:00.000Z','fixture' FROM generate_series(1,${history}::int) i`;
  await sql`INSERT INTO state_compute_scheduling (id,verb,task_id,state,due_at,opened_at,zone)
    SELECT md5(i::text),'simulate',i,'open','2026-11-01T00:00:00.000Z','2026-11-01T00:00:00.000Z',3
    FROM generate_series(1,${history}::int) i`;
  await sql`INSERT INTO entity_compute_order_cuts VALUES ('signal-cut',${epoch},${history},1,0,'',${epoch},'{}')`;
  await sql`INSERT INTO record_compute_order_cut_activations VALUES ('signal-cut',${epoch},0,'',0,'')`;
  await sql.unsafe(readFileSync(new URL("../worker/pg/queue_counts.sql", import.meta.url), "utf8"));
  await sql.unsafe("ANALYZE state_compute_scheduling; ANALYZE entity_compute_tasks");
  await sql`INSERT INTO entity_compute_devices (id,seen,asked_at,last_at)
    VALUES ('recent','2026-11-22T04:18:59.000Z','2026-11-22T04:18:59.000Z','2026-11-22T04:18:59.000Z')`;
  await sql`INSERT INTO state_queue_zone_seed (epoch,zones,sampled_at) VALUES ('other','[]',now())`;
  const before = await signal();
  assert.deepEqual(totals(before), [0, 0]);
  assert.equal(before.asking, 1);
  assert.equal(before.computing, 1);
  assert((await scanLoops()).loops > 0, "an unseeded current cut must read the authoritative schedule");

  await sql`INSERT INTO state_queue_zone_seed (epoch,zones,sampled_at) VALUES (${epoch},'[]',now())`;
  assert.deepEqual(await signal(), before);
  assert.equal((await scanLoops()).loops, 0, "an exact zero signal must skip the historical queue plan");
  assert.deepEqual(await wave(), { open: false, urgent: false });
  assert.equal((await scanLoops()).loops, 0, "an exact zero wave must skip the historical queue plan");
  await sql.unsafe("ALTER FUNCTION compute_order_stock_dispatchable(bigint,bigint) RENAME TO legacy_uninstalled_stock_guard");
  assert.deepEqual(await signal(), before);
  assert((await scanLoops()).loops > 0, "a partial stock schema must use the authoritative schedule");
  await sql.unsafe("ALTER FUNCTION legacy_uninstalled_stock_guard(bigint,bigint) RENAME TO compute_order_stock_dispatchable");


  await addLive("ready", history + 1, 0);
  await addLive("urgent", history + 2, 1);
  await addLive("leased", history + 3, 0, "2026-11-22T04:20:00.000Z");
  assert.deepEqual(totals(await signal()), [1, 1]);
  const active = await scanLoops();
  assert.equal(active.loops, 0, "a complete small signal must skip the historical queue plan");
  assert.deepEqual(await wave(), { open: true, urgent: true });
  assert.equal((await scanLoops()).loops, 0, "a complete small wave must skip the historical queue plan");
  await sql`UPDATE state_compute_work SET lease_until='2026-11-22T04:18:59.000Z' WHERE id='leased'`;
  assert.deepEqual(totals(await signal()), [2, 1]);
  for (let i = 0; i < 20; i++) await addLive(`hidden-${i}`, history + 4 + i, 0);
  await sql`INSERT INTO record_compute_order_cancellations (question,task_id,cut_id,at)
    SELECT id,task_id,'signal-cut',${epoch} FROM state_compute_scheduling WHERE id LIKE 'hidden-%'`;
  assert.deepEqual(totals(await signal()), [2, 1]);
  assert((await scanLoops()).loops > 0, "an incomplete candidate set must fall back to the authoritative schedule");
  await sql`UPDATE state_compute_work SET state='withdrawn',lease=NULL,lease_until=NULL,leased_to=NULL
    WHERE id IN ('ready','urgent','leased')`;
  assert.deepEqual(totals(await signal()), [0, 0]);
  assert.equal((await scanLoops()).loops, 0, "the settled queue must return to the fast zero path");

  await addCandidates(1,786);
  await sql`INSERT INTO state_compute_order_stock_holds VALUES (${history}+101,'fixture-window')`;
  await sql.unsafe("ANALYZE state_compute_scheduling; ANALYZE entity_compute_tasks");
  const ready = await signal();
  assert.deepEqual(totals(ready), [200, 0]);
  assert.equal((await scanLoops()).loops, 0, "the current production-sized queue must not scan historical open rows");
  assert.deepEqual(await wave(), {open:true,urgent:false});
  assert.equal((await scanLoops()).loops, 0);
  const body=readFileSync(new URL("../worker/pg/sync.sql", import.meta.url),"utf8")
    .match(/CREATE OR REPLACE FUNCTION factory_queue_signal\([^]*?RETURN QUERY\s+([^]*?);\nEND/)[1];
  const [bounded]=await sql.unsafe("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) "+body,[new Date(now).toISOString()]);
  const nodes=node=>[node,...(node.Plans??[]).flatMap(nodes)];
  const scans=nodes(bounded["QUERY PLAN"][0].Plan).filter(node=>node["Index Name"]==="compute_scheduling_zone");
  assert(scans.some(node=>node["Scan Direction"]==="Backward"&&node["Actual Rows"]<=802&&node["Actual Loops"]===1));
  assert(!nodes(bounded["QUERY PLAN"][0].Plan).some(node=>node["Relation Name"]==="entity_compute_tasks"));
  await sql.unsafe(`CREATE FUNCTION signal_fixture_history_forbidden() RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$
    BEGIN RAISE EXCEPTION 'the stale availability path walked history'; END $$;
    CREATE OR REPLACE VIEW state_compute_effective_open AS
      SELECT s.* FROM state_compute_scheduling s WHERE signal_fixture_history_forbidden()`);
  const [dispatch]=await sql`SELECT * FROM factory_open(${new Date(now).toISOString()}::text,NULL,
    '2026-11-22T04:18:00.000Z',1024)`;
  assert.deepEqual({...dispatch},{open:785,asking:1},"uncached dispatch uses the same exact ready count");
  const leased=await sql`SELECT id FROM factory_lease_pack_zoned(${new Date(now).toISOString()}::text,4,
    'fixture','[]','test-owner','',NULL,500,0,'["simulate"]',NULL,NULL,NULL,8,'pack',
    '2026-11-22T04:20:00.000Z','test-device',16)`;
  assert.equal(leased.length,8);
  assert(!leased.some(row=>row.id==="candidate-1"),"the unpublished FIFO head stays held");
  const at=new Date(now).toISOString();
  await addCandidates(787,2048);
  const [largest]=await sql`SELECT * FROM factory_queue_signal(${at})`;
  assert.deepEqual({...largest},{exact:true,ready:2039,urgent:0});
  await addCandidates(2049,2049);
  const [tooLarge]=await sql`SELECT exact FROM factory_queue_signal(${at})`;
  assert.equal(tooLarge.exact,false);
  await assert.rejects(sql`SELECT * FROM factory_open(${at}::text,NULL,'2026-11-22T04:18:00.000Z',1024)`,
    /the stale availability path walked history/);
  console.log("PASS real PostgreSQL signal, wave, uncached availability and actual lease retain exact counts and skip history at production watermarks");
} finally {
  try { await sql.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`); }
  finally { await sql.end(); }
}
