// Result reception stays atomic, bounded in database calls and ordered beside settlement.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { pgDb, placeholders, postgresClient } from "../worker/db.js";
import { factory, canon } from "../worker/tasks.js";
import { forgetDials, setDial } from "../worker/dials.js";

const url = process.env.WFSIM_ANSWER_TEST_URL;
let ddl = ["schema", "library", "sync", "inventory", "build_workflows"].map(name =>
  readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), "utf8")).join("\n");
if (process.argv.includes("--break-order")) {
  const order = 'ORDER BY x.a ->> \'question\' COLLATE "C", x.k';
  assert.ok(ddl.includes(order));
  ddl = ddl.replace(order, "ORDER BY x.k");
}
if (process.argv.includes("--break-procedure-locks")) {
  assert.ok(url, "procedure lock control requires isolated TCP Postgres");
  const locks = /  PERFORM 1 FROM state_compute_scheduling q WHERE q\.id IN \([\s\S]*?AND \(q\.leased_to=v_dev OR q\.leased_to_prev=v_dev\) ORDER BY q\.id FOR UPDATE;\n/;
  assert.match(ddl, locks);
  ddl = ddl.replace(locks, "");
}
if (process.argv.includes("--break-envelope")) {
  const guard = /  IF jsonb_typeof\(p\) IS DISTINCT FROM 'object'[\s\S]*?invalid answer batch envelope[\s\S]*?  END IF;\n/;
  assert.match(ddl, guard);
  ddl = ddl.replace(guard, "");
}
let db, sql, close;
const adapterErrors = [], commands = [];
if (url) {
  assert.ok(["localhost", "127.0.0.1", "[::1]"].includes(new URL(url).hostname), "answer checks require isolated localhost Postgres");
  const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
  const schema = "answer_batch_" + randomBytes(6).toString("hex");
  sql = postgres(url, { max: 8, fetch_types: false, onnotice() {}, debug(_id, query) { commands.push(query.trim().toLowerCase()); },
    connection: { search_path: schema, statement_timeout: 10000, lock_timeout: 2000 },
    types: { bigint: { to: 20, from: [20, 1700], parse: Number, serialize: String } } });
  await sql.unsafe(`CREATE SCHEMA ${schema}`);
  await sql.unsafe(ddl);
  const client = postgresClient(sql);
  const transaction = client.transaction, query = client.query;
  client.transaction = fn => transaction(fn).catch(error => { adapterErrors.push(error.code); throw error; });
  client.query = (text, args) => query(text, args).catch(error => { adapterErrors.push(error.code); throw error; });
  db = Object.assign(pgDb(client), {
    run: async (text, ...args) => (await client.query(placeholders(text), args)).affected,
    all: async (text, ...args) => (await client.query(placeholders(text), args)).rows,
    get: async (text, ...args) => (await client.query(placeholders(text), args)).rows[0] ?? null,
  });
  close = async () => { await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`); await sql.end(); };
} else {
  const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
  const pg = new PGlite({ parsers: { 20: Number, 1700: Number } });
  await pg.exec(ddl);
  const query = (text, args = []) => pg.query(placeholders(text), args);
  db = Object.assign(pgDb({ query, transaction: fn => pg.transaction(tx => fn({ query: (text, args) => tx.query(text, args) })) }), {
    run: async (text, ...args) => (await query(text, args)).affectedRows,
    all: async (text, ...args) => (await query(text, args)).rows,
    get: async (text, ...args) => (await query(text, args)).rows[0] ?? null,
  });
  close = () => pg.close();
}

let checks = 0;
const check = (name, value) => { assert.ok(value, name); checks++; console.log("ok " + name); };
const at = Date.parse("2026-11-13T00:00:00Z"), stamp = new Date(at).toISOString();
const env = { LIBRARY: db };
const f = factory({ verbs: { simulate: { cap: 2, pack: true, lease_ms: 60000, canon } },
  producers: { board: { on_fact: async () => {} } }, spot: 0, recheck: 0 });
const payload = (device, rows, extra = {}) => ({ device, owner: device, net: device, official: 0, at: stamp,
  rows: rows.map(q => ({ question: q.id, task_id: q.task_id, engine: "e1", canon: '{"score":7}', result: '{"score":7}',
    work: 7, compute_ms: 10, counts: null, lanes: 1 })),
  queue: { engine: "e1", peer: "", joined: 0 }, learn: [{ p: "board", ms: rows.length * 10, n: rows.length }], share: 1 / 60, ...extra });
const receive = p => db.batch([db.prepare("SELECT factory_record_answers(?1::text)").bind(JSON.stringify(p))]);
const parallel = async jobs => {
  const results = await Promise.allSettled(jobs);
  const failure = results.find(result => result.status === "rejected");
  if (failure) throw failure.reason;
};
const seed = async (tag, count) => {
  const ids = await f.demandAll(env, Array.from({ length: count }, (_, i) => ({ producer: "board", ref: `${tag}-${i}`, verb: "simulate",
    request: { tag, i }, due_in_ms: 60000, payload: {} })), "e1", at);
  return db.all("SELECT id,task_id FROM state_compute_work WHERE id IN (SELECT jsonb_array_elements_text((?::text)::jsonb)) ORDER BY id", JSON.stringify(ids));
};
const count = async (table, where = "true") => (await db.get(`SELECT count(*) AS n FROM ${table} WHERE ${where}`)).n;

try {
  const rows = await seed("atomic", 8);
  const original = payload("first", [...rows].reverse());
  await assert.rejects(receive(JSON.stringify(original)), error => error.code === "22023");
  check("double-encoded transport fails before recording or acknowledging results", await count("record_candidate_answers") === 0);
  await assert.rejects(receive({ ...original, rows: null }), error => error.code === "22023");
  check("an incomplete batch envelope is refused without candidates", await count("record_candidate_answers") === 0);
  await receive(payload("empty-probe", [], { queue: null, learn: [] }));
  check("a valid empty transport probe commits no candidate or queue rows", await count("record_candidate_answers") === 0 && await count("state_pending_verifications") === 0);
  const before = { ...db.cost }, observer = sql ? await sql.reserve() : null, commandStart = commands.length;
  let receptionCommands;
  try {
    await receive(original);
    receptionCommands = commands.slice(commandStart);
    if (observer) {
      const visible = await observer.unsafe("SELECT count(*) AS n FROM record_candidate_answers WHERE device='first'");
      const released = await observer.unsafe("SELECT pg_try_advisory_xact_lock(hashtextextended('first',0)) AS free");
      check("acknowledgment exposes committed candidates and releases the device lock to another connection", visible[0].n === 8 && released[0].free);
    }
  } finally { observer?.release(); }
  check("one database statement accepts an entire reversed result batch", db.cost.statements - before.statements === 1 && await count("record_candidate_answers") === 8);
  if (sql) check("a singleton reception needs no separate BEGIN or COMMIT exchange", !receptionCommands.some(query => /^(begin|commit)\b/.test(query)));
  check("candidate rows are received in the same id order as settlement", (await db.all("SELECT question FROM record_candidate_answers ORDER BY id")).map(x => x.question).join() === rows.map(x => x.id).join());
  check("results and their settlement queue commit together", await count("state_pending_verifications") === 8);
  const accepted = await db.get("SELECT engine,canon,result,work,compute_ms,lanes,owner,net FROM record_candidate_answers ORDER BY id LIMIT 1");
  check("bulk transport preserves canonical bytes, cost and witness identity", accepted.engine === "e1" && accepted.canon === '{"score":7}' && accepted.result === '{"score":7}'
    && accepted.work === 7 && accepted.compute_ms === 10 && accepted.lanes === 1 && accepted.owner === "first" && accepted.net === "first");
  await receive(original);
  check("a lost response can be retried without a second candidate", await count("record_candidate_answers") === 8);
  const fresh = await seed("abort", 4);
  const broken = payload("aborted", fresh);
  broken.rows.at(-1).task_id = -1;
  const learned = await db.get("SELECT answer_ms FROM state_producer_compute_usage WHERE producer='board'");
  await assert.rejects(receive(broken), error => error.code === "40001");
  check("a stale task rolls back every preceding result in its batch", await count("record_candidate_answers", "device='aborted'") === 0);
  check("a failed reception adds no queue rows or learned cost", await count("state_pending_verifications") === 8
    && (await db.get("SELECT answer_ms FROM state_producer_compute_usage WHERE producer='board'")).answer_ms === learned.answer_ms);
  await receive(payload("recovered", fresh));
  check("a fresh submission works after failed transaction retries", await count("record_candidate_answers", "device='recovered'") === 4);
  const queuesBefore = await count("state_pending_verifications"), invalidQueue = payload("queue-error", fresh);
  invalidQueue.queue.engine = null;
  await assert.rejects(receive(invalidQueue), error => error.code === "23502");
  check("failure after candidate insertion rolls its new witnesses back", await count("record_candidate_answers", "device='queue-error'") === 0);
  check("a queue failure preserves all previously committed results and queue rows", await count("state_pending_verifications") === queuesBefore
    && await count("record_candidate_answers") === 12);
  await db.run("CREATE TABLE answer_atomicity(id integer PRIMARY KEY)");
  await assert.rejects(db.batch([db.prepare("INSERT INTO answer_atomicity VALUES(1)"), db.prepare("SELECT 1/0")]), error => error.code === "22012");
  check("a failing multi-statement batch still rolls its earlier write back", await count("answer_atomicity") === 0);
  await db.run("CREATE SEQUENCE answer_retry_attempt");
  await db.run(`CREATE FUNCTION retry_single_answer() RETURNS void LANGUAGE plpgsql AS $$ BEGIN
    INSERT INTO answer_atomicity VALUES(2);
    IF nextval('answer_retry_attempt')<3 THEN RAISE EXCEPTION 'retry answer' USING ERRCODE='40001'; END IF;
  END $$`);
  await db.batch([db.prepare("SELECT retry_single_answer()")]);
  check("a singleton serialization retry commits exactly one durable effect", await count("answer_atomicity") === 1
    && (await db.get("SELECT last_value FROM answer_retry_attempt")).last_value === 3);
  await db.run("CREATE TABLE answer_lock_trace(question text, device text, seq bigint GENERATED ALWAYS AS IDENTITY)");
  await db.run(`CREATE FUNCTION trace_answer_lock() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
    INSERT INTO answer_lock_trace(question,device) VALUES(NEW.question,NEW.device); RETURN NEW; END $$`);
  await db.run("CREATE TRIGGER trace_answer BEFORE INSERT ON record_candidate_answers FOR EACH ROW EXECUTE FUNCTION trace_answer_lock()");

  for (const bulk of [0, 1]) {
    const work = await seed("worker-" + bulk, 200);
    const device = "worker-" + bulk;
    await db.run("INSERT INTO entity_compute_devices(id,seen) VALUES(?,?)", device, stamp);
    await db.run("UPDATE state_compute_scheduling SET lease=?,leased_to=?,lease_until='2999-01-01' WHERE id IN (SELECT jsonb_array_elements_text((?::text)::jsonb))", device, device, JSON.stringify(work.map(x => x.id)));
    await setDial(env, "answer_bulk", process.argv.includes("--break-worker-bulk") ? 0 : bulk, "check", at);
    await setDial(env, "settle_async", 1, "check", at);
    const start = { ...db.cost };
    const result = await f.answerPack(env, { device, owner: device, net: device }, { lease: device, engine: "e1", late: true,
      answers: [...work].reverse().map(q => ({ question: q.id, result: { score: 7 }, work: 7, compute_ms: 10, lanes: 1 })) }, at);
    const statements = db.cost.statements - start.statements;
    check(`worker path${bulk} acknowledges every input row without reordering its reply`, result.taken.join() === [...work].reverse().map(q => q.id).join());
    check(`worker path${bulk} locks rows in settlement order`, (await db.all("SELECT question FROM answer_lock_trace WHERE device=? ORDER BY seq", device)).map(x => x.question).join() === work.map(q => q.id).join());
    check(`worker path${bulk} preserves every accepted result and queue entry`, await count("record_candidate_answers", `device='${device}'`) === 200);
    if (bulk) check("the live worker reception uses a bounded number of database statements for 200 results", statements <= 5);
    else check("the fallback remains available for a controlled rollout", statements >= 200);
  }
  if (sql) {
    const errorsBefore = adapterErrors.length;
    const collision = await seed("collision", 16);
    await db.run(`CREATE OR REPLACE FUNCTION trace_answer_lock() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
      PERFORM pg_sleep(0.005); INSERT INTO answer_lock_trace(question,device) VALUES(NEW.question,NEW.device); RETURN NEW; END $$`);
    await parallel([receive(payload("concurrent-a", collision)), receive(payload("concurrent-b", [...collision].reverse()))]);
    check("opposite-order concurrent witnesses each commit one complete batch", await count("record_candidate_answers", "device IN ('concurrent-a','concurrent-b')") === 32);
    await parallel([receive(payload("same-device", collision)), receive(payload("same-device", [...collision].reverse()))]);
    check("concurrent retries from one device retain exactly one witness per task", await count("record_candidate_answers", "device='same-device'") === 16);
    const settlement = await seed("settlement-collision", 8);
    await parallel([
      receive(payload("settlement-device", [...settlement].reverse())),
      sql.begin(async tx => {
        await tx.unsafe("SELECT id FROM state_compute_scheduling WHERE id IN (SELECT jsonb_array_elements_text(($1::text)::jsonb)) ORDER BY id FOR UPDATE", [JSON.stringify(settlement.map(x => x.id))]);
        await tx.unsafe("SELECT pg_sleep(0.03)");
      }),
    ]);
    check("bulk reception and the settler's sorted row locks can complete concurrently", await count("record_candidate_answers", "device='settlement-device'") === 8);
    const grouped = await seed("grouped-procedure", 4);
    for (const device of ["proc-a", "proc-b"]) await db.run("INSERT INTO entity_compute_devices(id,seen) VALUES(?,?)", device, stamp);
    for (let i = 0; i < grouped.length; i++) {
      const part = i < 2 ? "one" : "two";
      await db.run("UPDATE state_compute_scheduling SET lease=?,leased_to='proc-a',lease_until='2999-01-01',lease_prev=?,leased_to_prev='proc-b',lease_prev_until='2999-01-01' WHERE id=?",
        "proc-a-" + part, "proc-b-" + part, grouped[i].id);
    }
    const procedure = (device, reverse) => {
      const groups = [["one", grouped.slice(0, 2)], ["two", grouped.slice(2)]].map(([part, rows]) => ({ lease: device + "-" + part,
        rows: rows.map(q => ({ question: q.id, canons: { simulate: '{"score":7}' }, result: '{"score":7}', work: 7, compute_ms: 10, counts: null, lanes: 1 })) }));
      const p = { id: device, owner: device, net: device, at: stamp, engine: "e1", seen: false, stale: false,
        groups: reverse ? groups.reverse() : groups, taken: [], dropped: [], record: { engine: "e1", peer: "", joined: 0 },
        packable: ["simulate"], queueable: ["board"], share: 1 / 60, refused: [], held: [], states: [],
        consented: true, want: 0, packs_held: 2 };
      return db.batch([db.prepare("CALL board_sync((?1::text)::jsonb,NULL)").bind(JSON.stringify(p))]);
    };
    await parallel([procedure("proc-a", false), procedure("proc-b", true)]);
    check("procedure groups in opposite lease order retain all independent witnesses", await count("record_candidate_answers", "device IN ('proc-a','proc-b')") === 8);
    check("concurrent batches need no deadlock or serialization retries", adapterErrors.length === errorsBefore);
  }
  console.log(JSON.stringify({ ok: true, transport: sql ? "tcp" : "pglite", checks }));
} finally { forgetDials(); await close(); }
