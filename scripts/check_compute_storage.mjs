// Task rounds, accepted outputs and queue cleanup have separate authorities on both PostgreSQL transports.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { pgDb, placeholders } from "../worker/db.js";

const broken = process.argv.includes("--break-stale-acceptance"), url = process.env.WFSIM_MIGRATION_TEST_URL;
const ddl = ["schema", "library", "sync", "inventory"].map(name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), "utf8")).join("\n");
let source = ddl;
if (broken) {
  assert.ok(source.includes("held.task_id<>$2 OR"), "stale-acceptance control must alter the actual guard");
  source = source.replace("held.task_id<>$2 OR", "");
}
let db, finish;
if (url) {
  const parsed = new URL(url);
  assert.ok(["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname), "TCP storage checks require an isolated local database");
  const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
  const schema = "storage_" + randomBytes(8).toString("hex");
  const sql = postgres(url, { max: 1, onnotice: () => {}, fetch_types: false,
    types: { bigint: { to: 20, from: [20], parse: Number, serialize: String } } });
  await sql.unsafe(`CREATE SCHEMA ${schema}`);
  await sql.unsafe(`SET search_path TO ${schema}`);
  await sql.unsafe(source);
  const query = async (text, args = []) => {
    const rows = await sql.unsafe(placeholders(text), args);
    return { rows: [...rows], affected: rows.count };
  };
  db = Object.assign(pgDb({ query, transaction: fn => sql.begin(tx => fn({ query: async (text, args) => {
    const rows = await tx.unsafe(text, args);
    return { rows: [...rows], affected: rows.count };
  } })) }), {
    get: async (text, ...args) => (await query(text, args)).rows[0],
    run: async (text, ...args) => (await query(text, args)).affected,
  });
  finish = async () => { await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`); await sql.end(); };
} else {
  const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
  const pg = new PGlite({ parsers: { 20: Number } });
  await pg.exec(source);
  const query = (text, args = []) => pg.query(placeholders(text), args);
  db = Object.assign(pgDb({ query, transaction: fn => pg.transaction(tx => fn({ query: (text, args) => tx.query(text,args) })) }), {
    get: async (text, ...args) => (await query(text,args)).rows[0],
    run: async (text, ...args) => (await query(text,args)).affectedRows,
  });
  finish = () => pg.close();
}
let checks = 0;
const check = (name, value) => { assert.ok(value, name); checks++; console.log(`ok ${name}`); };
const fails = async (name, run, code) => {
  let error;
  try { await run(); } catch (caught) { error = caught; }
  check(name, !!error && (!code || error.code === code));
};
const at = "2026-11-10T00:00:00.000Z", later = "2026-11-10T01:00:00.000Z";
const input = JSON.stringify([{ id: "spec", verb: "simulate", request: '{"weapon":"braton"}',
  due_at: later, lanes: null, waits: 0, producer: "board", initiator: null }]);
try {
  await db.run("SELECT compute_register_tasks(?1::text,'e1',?2::text)",input,at);
  const first = await db.get("SELECT * FROM state_compute_work WHERE id='spec'");
  check("registration stores input and a distinct execution round",first.task_id>0 && first.canon===null);
  await db.run("SELECT compute_register_tasks(?1::text,'e1',?2::text)",input,at);
  check("a retried request shares its existing task",(await db.get("SELECT count(*) AS n FROM entity_compute_tasks")).n===1);
  await fails("identity cannot silently change frozen input",() => db.run("SELECT compute_register_tasks(?1::text,'e1',?2::text)",input.replace("braton","hek"),at));
  await fails("direct writes cannot mutate frozen input",() => db.run("UPDATE entity_compute_specs SET request='{}' WHERE id='spec'"));
  await db.run("SELECT factory_answer('spec','e1','device','owner','net',0,'old','{}',7,NULL,NULL,NULL,?1::text,?2::bigint)",at,first.task_id);
  await db.run("SELECT factory_answer('spec','e1','device','owner','net',0,'old','{}',7,NULL,NULL,NULL,?1::text,?2::bigint)",at,first.task_id);
  check("transport retry is one candidate",(await db.get("SELECT count(*) AS n FROM record_candidate_answers")).n===1);
  const old = await db.get("SELECT compute_accept_result('spec',?1::bigint,'open','fact','old','e1',?2::text,0,'[]') AS id",first.task_id,at);
  check("accepted result is independently stored",(await db.get("SELECT canon FROM entity_compute_results WHERE id=?",old.id)).canon==="old");
  check("a task stores no result body",(await db.get("SELECT count(*) AS n FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='entity_compute_tasks' AND column_name IN ('canon','result','score','report')")).n===0);
  await fails("accepted result body is immutable",() => db.run("UPDATE entity_compute_results SET canon='other' WHERE id=?",old.id));
  await db.run("SELECT compute_open_task('spec','correction','e1',?::text,true)",later);
  const next = await db.get("SELECT * FROM state_compute_work WHERE id='spec'");
  check("correction creates a new task and retires the selected result",next.task_id!==first.task_id && next.canon===null);
  check("retirement preserves the previous result",(await db.get("SELECT canon FROM entity_compute_results WHERE id=?",old.id)).canon==="old");
  check("a correction does not reuse old candidates implicitly",(await db.get("SELECT count(*) AS n FROM state_compute_candidate_answers")).n===0);
  await fails("an obsolete task cannot accept a late candidate",() => db.run("SELECT factory_answer('spec','e1','late','late','',0,'late','{}',7,NULL,NULL,NULL,?1::text,?2::bigint)",later,first.task_id),"40001");
  await fails("a candidate requires an actual task identity",() => db.run("SELECT factory_answer('missing','e1','late','late','',0,'late','{}',7,NULL,NULL,NULL,?1::text,NULL)",later),"40001");
  await fails("an obsolete task cannot replace the current result",() => db.run("SELECT compute_accept_result('spec',?1::bigint,'open','fact','stale','e1',?2::text,0,'[]')",first.task_id,later),"40001");
  await fails("result acceptance requires an actual task identity",() => db.run("SELECT compute_accept_result('spec',NULL,'open','fact','stale','e1',?::text,0,'[]')",later),"40001");
  await db.run("SELECT factory_answer('spec','e1','device','owner','net',0,'new','{}',7,NULL,NULL,NULL,?1::text,?2::bigint)",later,next.task_id);
  check("the same device may answer a fresh task",(await db.get("SELECT count(*) AS n FROM record_candidate_answers")).n===2);
  await db.run("SELECT compute_accept_result('spec',?1::bigint,'open','fact','new','e1',?2::text,0,'[]')",next.task_id,later);
  check("the current selection changes without overwriting history",(await db.get("SELECT canon FROM state_compute_work WHERE id='spec'")).canon==="new"
    && (await db.get("SELECT count(*) AS n FROM entity_compute_results")).n===2);
  await db.run("SELECT compute_open_task('spec','engine_recalibration','e2',?::text)",later);
  check("engine recalibration carries the preceding task's eligible evidence",(await db.get("SELECT count(*) AS n FROM state_compute_candidate_answers")).n===1);
  check("engine recalibration cannot revive a retired round's candidates",(await db.get("SELECT count(*) AS n FROM state_compute_candidate_answers WHERE canon='old'")).n===0);
  await fails("selection metadata must match its result",() => db.run("UPDATE state_compute_scheduling SET fact_engine='wrong' WHERE id='spec'"),"23503");
  await db.run("DELETE FROM state_compute_work WHERE id='spec'");
  check("queue cleanup preserves results and their input",(await db.get("SELECT count(*) AS n FROM entity_compute_results")).n===2
    && (await db.get("SELECT request FROM entity_compute_specs WHERE id='spec'")).request==='{"weapon":"braton"}');
  check("all application tables use the four named classes",(await db.get("SELECT count(*) AS n FROM information_schema.tables WHERE table_schema=current_schema() AND table_type='BASE TABLE' AND table_name !~ '^(entity|record|relation|state)_'")).n===0);
  console.log(JSON.stringify({ ok: true, transport: url ? "tcp" : "pglite", checks }));
} finally { await finish(); }
