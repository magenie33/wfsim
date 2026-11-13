// SPDX-License-Identifier: AGPL-3.0-or-later
// Business writes and task registration recover independently of total queue size.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { pgDb, placeholders, postgresClient } from "../worker/db.js";
import { factory, canon } from "../worker/tasks.js";
import { BOARD_PRODUCERS, rowAsk, recoverBoardTasks } from "../worker/board_tasks.js";
import { forgetDials, setDial, DIAL_DEFAULTS } from "../worker/dials.js";
import { advanceBoardAdmission } from "../worker/board_admission.js";
import { officialRoute } from "../worker/factory.js";
import { tell } from "./order_credit.mjs";
import { boardAdmissionSchemaSql } from "./board_admission_schema.mjs";

const url = process.env.WFSIM_BOARD_ADMISSION_TEST_URL;
let ddl = ["schema", "library", "sync", "inventory", "build_workflows", "board_admission"]
  .map(name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), "utf8")).join("\n");
if (process.argv.includes("--break-checkpoint")) {
  const predicate = "WHERE (q.build_id,q.ruler,q.mode)>(cursor.build_id,cursor.ruler,cursor.mode)";
  assert(ddl.includes(predicate));ddl = ddl.replace(predicate, "WHERE (q.build_id,q.ruler,q.mode)>('','','')");
}
let db, sql, close;
if (url) {
  assert(["127.0.0.1", "localhost", "[::1]"].includes(new URL(url).hostname), "use isolated loopback Postgres");
  const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
  const schema = "board_admission_" + randomBytes(6).toString("hex");
  sql = postgres(url, { max: 6, fetch_types: false, onnotice() {},
    connection: { search_path: schema, statement_timeout: 10000, lock_timeout: 2000 },
    types: { bigint: { to: 20, from: [20, 1700], parse: Number, serialize: String } } });
  await sql.unsafe(`CREATE SCHEMA ${schema}`);await sql.unsafe(ddl);
  const client = postgresClient(sql);
  db = Object.assign(pgDb(client), {
    run: async (text, ...args) => (await client.query(placeholders(text), args)).affected,
    all: async (text, ...args) => (await client.query(placeholders(text), args)).rows,
    get: async (text, ...args) => (await client.query(placeholders(text), args)).rows[0] ?? null,
  });
  close = async () => { await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`);await sql.end(); };
} else {
  const { PGlite } = await import("./pg/node_modules/@electric-sql/pglite/dist/index.js");
  const pg = new PGlite({ parsers: { 20: Number, 1700: Number } });await pg.exec(ddl);
  const query = (text, args = []) => pg.query(placeholders(text), args);
  db = Object.assign(pgDb({ query, transaction: fn => pg.transaction(tx => fn({ query: (text, args) => tx.query(text, args) })) }), {
    run: async (text, ...args) => (await query(text, args)).affectedRows,
    all: async (text, ...args) => (await query(text, args)).rows,
    get: async (text, ...args) => (await query(text, args)).rows[0] ?? null,
  });
  close = () => pg.close();
}
let checks = 0;
const check = (name, value) => { assert(value, name);checks++;console.log("ok " + name); };
const now = Date.now() + 10000, at = new Date(now).toISOString(), env = { LIBRARY: db };
const f = factory({ verbs: { simulate: { cap: 2, pack: true, lease_ms: 60000,
  canon: (result, work) => canon({ score: result.score, metric: result.metric, work }) } }, producers: BOARD_PRODUCERS, spot: 0, recheck: 0 });
const record = JSON.stringify({ weapon: "braton", mods: ["serration"] });
const count = async table => (await db.get(`SELECT count(*) n FROM ${table}`)).n;
const queue = async (id, batch = "arrivals-2026-11-13", initiator = null, input = record) => {
  await db.run("INSERT INTO entity_compute_batches(id,at,total) VALUES(?,?,0) ON CONFLICT DO NOTHING", batch, batch);
  await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?) ON CONFLICT DO NOTHING", id, at, input);
  await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator) VALUES(?,?,'standard_single_target','base',?)", batch, id, initiator);
};
const advance = options => advanceBoardAdmission(env, { engine: "e", rowAsk, register: f.demandAll, now, recover: false, ...options });
const qid = async id => (await db.get("SELECT question FROM relation_task_consumers WHERE ref=?", `${id}|standard_single_target|base`))?.question;
try {
  if (process.env.WFSIM_ANSWER_BULK === "1") await setDial(env,"answer_bulk",1,"fixture",now);
  forgetDials();
  check("registration remains disabled until its rollout dial is enabled", (await recoverBoardTasks(env,"e",now)).mode === "disabled");
  await assert.rejects(db.prepare("SELECT board_admission_finish(?::text)").bind(JSON.stringify('[]')).run());
  await assert.rejects(db.prepare("SELECT board_admission_retry(?::text,?::text,'retry')").bind(JSON.stringify('[]'),at).run());
  check("double-encoded receipts fail without acknowledging work", await count("state_board_task_admissions") === 0);
  if (process.argv.includes("--break-trigger")) await db.run("DROP TRIGGER board_admission_changed ON state_board_pending_work");
  await queue("a", undefined, "anonymous");
  check("a business write atomically retains unfinished registration", await count("state_board_task_admissions") === 1);
  const before = { ...db.cost };const admitted = await advance();
  check("the existing task ledger registers and acknowledges the obligation together", admitted.asked === 1
    && await count("state_board_task_admissions") === 0 && await count("state_compute_work") === 1);
  check("registration uses bounded database calls", db.cost.calls - before.calls < 15);
  const id = await qid("a"), original = await db.get("SELECT * FROM state_compute_scheduling WHERE id=?", id);
  await db.run("UPDATE state_compute_scheduling SET lease='held',leased_to='original',lease_until='2999-01-01' WHERE id=?", id);
  await db.run("UPDATE state_board_pending_work SET initiator='user:owner' WHERE build_id='a'");
  await advance();const held = await db.get("SELECT * FROM state_compute_scheduling WHERE id=?", id);
  check("source updates preserve the computation, round, position and existing lease", held.id === original.id && held.task_id === original.task_id
    && held.zone_position === original.zone_position && held.lease === "held" && held.leased_to === "original");
  check("anonymous work has a durable business source", (await db.get("SELECT compute_task_has_source(?) valid", id)).valid);
  check("an idle pass leaves the same task and has no queue to rescan", (await advance()).asked === 0 && await count("state_compute_work") === 1);

  const pendingBefore = await count("state_board_pending_work");
  await assert.rejects(db.batch([
    db.prepare("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('arrivals-2026-11-13','cut','r','m')"),
    db.prepare("SELECT * FROM missing_admission_fixture"),
  ]));
  check("a failed business transaction leaves neither an obligation nor a registration", await count("state_board_pending_work") === pendingBefore
    && !(await db.get("SELECT 1 FROM state_board_task_admissions WHERE build_id='cut'")));

  await queue("backpressure");
  check("historical recovery yields while registration is already pending", (await advance({recover:true})).scanned === null
    && !(await db.get("SELECT 1 FROM state_board_admission_recovery")));
  await queue("race");
  const existingTasks = await count("entity_compute_tasks");
  await assert.rejects(advance({ register: async (env, asks, engine, now, options) => {
    await db.run("UPDATE state_board_pending_work SET initiator='user:replacement' WHERE build_id='race'");
    if (process.argv.includes("--break-ack-transaction")) {
      await f.demandAll(env, asks, engine, now);
      await options.after[0].run();
    } else await f.demandAll(env, asks, engine, now, options);
  } }), error => error.code === "40001");
  check("changed source or claim rolls back task admission before acknowledgement", await count("entity_compute_tasks") === existingTasks
    && !(await qid("race")));
  check("a superseding source remains immediately retryable", (await advance()).asked === 1 && !!await qid("race"));

  await queue("expiry");
  const claimed = await db.all("SELECT * FROM board_admission_claim(250,?::text,?::text)", at, new Date(now + 1000).toISOString());
  check("an interrupted worker retains its leased registration", claimed.length === 1 && (await advance()).asked === 0);
  check("an expired claim can resume on another worker", (await advance({ now: now + 2000 })).asked === 1 && !!await qid("expiry"));

  await queue("refresh", "refresh-history");
  await db.run(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
    VALUES('refresh','standard_single_target','base','old',1,'kpm',1,?,?)`, at, at);
  const refreshed = await advance();
  check("fresh refresh work stays durably deferred instead of being forgotten", refreshed.deferred === 1 && !await qid("refresh")
    && (await db.get("SELECT error FROM state_board_task_admissions WHERE build_id='refresh'")).error === "refresh_not_due");
  await db.run("DELETE FROM state_board_pending_work WHERE build_id='refresh'");
  check("business cancellation removes its unfinished registration", await count("state_board_task_admissions") === 0);
  await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('missing-batch','missing-source','r','base')");
  check("missing durable source data is reported and retained", (await advance()).has_deferred
    && (await db.get("SELECT error FROM state_board_task_admissions WHERE build_id='missing-source'")).error === "missing_source");
  await db.run("DELETE FROM state_board_pending_work WHERE build_id='missing-source'");

  await queue("cached");
  const [cached] = await f.demandAll(env, [rowAsk("arrivals-2026-11-13", { identity: "cached", ruler: "standard_single_target", mode: "base" }, record, at, null, now)], "e", now);
  const prior = await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?", cached);
  const canonical = canon({ score: 7, metric: "kpm", work: 7 });
  await db.run("SELECT factory_answer(?::text,'e','official:fixture','official:fixture','',1,?::text,?::text,7,NULL,NULL,1,?::text,?::bigint)",
    cached, canonical, JSON.stringify({ score: 7, metric: "kpm" }), at, prior.task_id);
  await db.run("SELECT compute_accept_result(?::text,?::bigint,'open','fact',?::text,'e',?::text,0,'[]')", cached, prior.task_id, canonical, at);
  const tasksBefore = await count("entity_compute_tasks");await advance();
  check("an accepted result awaiting delivery is reused without another execution round", await count("entity_compute_tasks") === tasksBefore
    && (await db.get("SELECT score FROM state_board_measurement_selection WHERE identity='cached'")).score === 7
    && !(await db.get("SELECT 1 FROM state_board_pending_work WHERE build_id='cached'")));

  await db.run("ALTER TABLE state_board_pending_work DISABLE TRIGGER board_admission_changed");
  for (const id of ["legacy-a", "legacy-b", "legacy-c", "legacy-d", "legacy-e"]) await queue(id, "rescore-old");
  await db.run("ALTER TABLE state_board_pending_work ENABLE TRIGGER board_admission_changed");
  await queue("window", "rescore-window", null, JSON.stringify({weapon:"braton",mods:["window"]}));await advance();
  const blocked = (await db.get("SELECT board_admission_seed(2,?::text,1) progress",at)).progress;
  check("a full execution window retains the historical cursor without reporting completion", blocked.window_full
    && !blocked.complete && blocked.scanned === 0);
  await f.answerOfficial(env,{device:"official:window",official:true},{question:await qid("window"),engine:"e",result:{score:1,metric:"kpm"},work:1},now);
  const positions = [];
  const lastKey = (await db.get("SELECT build_id FROM state_board_pending_work ORDER BY build_id DESC LIMIT 1")).build_id;
  for (let i = 0; i < 6; i++) {
    const { progress } = await db.get("SELECT board_admission_seed(2,?::text,?::bigint) progress", at, DIAL_DEFAULTS.board_recovery_open_max);
    positions.push(progress);if (progress.complete) break;
  }
  check("historical recovery resumes its durable cursor beyond an earlier pass boundary", positions.length >= 3
    && positions.every((value, i) => !i || (value.build_id > positions[i - 1].build_id
      || value.complete && value.scanned === positions[i - 1].scanned))
    && positions.at(-1).complete && positions.at(-1).build_id === lastKey);
  check("already registered live tasks retain their leases through the historical pass",
    (await db.get("SELECT lease FROM state_compute_scheduling WHERE id=?", id)).lease === "held");
  check("a completed historical scan becomes a constant-time no-op", (await db.get("SELECT board_admission_seed(2,?::text,?::bigint) progress", at, DIAL_DEFAULTS.board_recovery_open_max)).progress.complete);
  await advance();
  check("new records below a completed historical cursor are still registered", (await queue("000-new"), (await advance()).asked) === 1 && !!await qid("000-new"));
  await queue("official", "rescore-official", null, JSON.stringify({ weapon: "braton", mods: ["official"] }));
  await advance();
  const officialId = await qid("official"), reference = "official|standard_single_target|base";
  await db.run("UPDATE state_compute_scheduling SET due_at='2000',lease='captured',leased_to='official:scorer',lease_until='2999' WHERE id=?", officialId);
  const task = await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?", officialId);
  const claims = new Map([[reference,{question: officialId,task_id: task.task_id,lease: "captured"}]]);
  const measurement = { identity: "official", ruler: "standard_single_target", mode: "base", score: 9, metric: "kpm",
    measured_by: "e", cost_seconds: 1.5, started_at: new Date(now-2000).toISOString(), finished_at: new Date(now-500).toISOString() };
  const call = async (what, body) => (await officialRoute(new Request(`https://x/api/tasks/official/${what}`, {
    method: "POST",headers: {authorization: "Bearer fixture"},body: JSON.stringify({witness: "official:scorer", ...body}),
  }), {...env, FACTORY_TOKEN: "fixture"}, `/api/tasks/official/${what}`)).json();
  await db.run("UPDATE state_compute_scheduling SET lease='replacement' WHERE id=?", officialId);
  await assert.rejects(tell([measurement], "e", call, claims), /not fully acknowledged/);
  check("a superseded official claim cannot write or advance acknowledgement", !await db.get("SELECT 1 FROM record_candidate_answers WHERE question=?",officialId));
  await db.run("UPDATE state_compute_scheduling SET lease='captured' WHERE id=?", officialId);
  const batch = db.batch;let raced = false;
  db.batch = async statements => {
    if (!raced && statements.some(s => s.sql.includes('factory_assert_official_claim'))) {
      raced = true;await db.run("UPDATE state_compute_scheduling SET lease='raced' WHERE id=?",officialId);
      if (process.argv.includes('--break-official-fence')) statements = statements.filter(s => !s.sql.includes('factory_assert_official_claim'));
    }
    return batch(statements);
  };
  try { await assert.rejects(tell([measurement],"e",call,claims),/not fully acknowledged/); }
  finally { db.batch = batch; }
  check("the official claim is checked again under the result transaction's lock", raced
    && !await db.get("SELECT 1 FROM record_candidate_answers WHERE question=?",officialId));
  await db.run("UPDATE state_compute_scheduling SET lease='captured' WHERE id=?", officialId);
  check("official acceptance alone delivers the measurement", await tell([measurement], "e", call, claims) === 1);
  const selected = await db.get("SELECT * FROM state_board_measurement_selection WHERE identity='official'");
  check("official delivery preserves native timing, cost and provenance", selected.measured_by === "e" && selected.cost_seconds === 1.5
    && selected.started_at === measurement.started_at && selected.finished_at === measurement.finished_at);
  const answersBefore = await count("record_candidate_answers"), resultsBefore = await count("entity_compute_results");
  await tell([measurement], "e", call, claims);
  check("a lost official acknowledgement retries without another result or candidate", await count("record_candidate_answers") === answersBefore
    && await count("entity_compute_results") === resultsBefore);
  await db.run("SELECT compute_open_task(?::text,'fixture','e',?::text,true)", officialId, at);
  await assert.rejects(tell([measurement], "e", call, claims), /not fully acknowledged/);
  check("an old official round cannot answer its replacement", await count("record_candidate_answers") === answersBefore
    && (await db.get("SELECT state FROM state_compute_scheduling WHERE id=?",officialId)).state === "open");
  await assert.rejects(tell([measurement], "e", call, new Map()), /no captured claim/);
  check("uncaptured results fail before transmission", await count("record_candidate_answers") === answersBefore);
  await queue("interrupted", "rescore-interrupted", null, JSON.stringify({weapon:"braton",mods:["interrupted"]}));await advance();
  const interrupted = await qid("interrupted"), pending = await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",interrupted);
  const retained = {...measurement,identity:"interrupted",score:10,work:1};
  await db.run("UPDATE state_compute_scheduling SET lease='interrupted',leased_to='official:scorer',lease_until='2999' WHERE id=?",interrupted);
  await db.batch([
    db.prepare("SELECT factory_assert_official_claim(?::text,?::bigint,'interrupted','official:scorer')").bind(interrupted,pending.task_id),
    db.prepare("SELECT factory_answer(?::text,'e','official:scorer','official:scorer','',1,?::text,?::text,1,NULL,NULL,1,?::text,?::bigint)")
      .bind(interrupted,canon({score:10,metric:"kpm",work:1}),JSON.stringify({score:10,metric:"kpm",measurement}),at,pending.task_id),
  ]);
  await db.run("UPDATE state_compute_scheduling SET lease=NULL,leased_to=NULL,lease_until=NULL WHERE id=?",interrupted);
  const candidates = await count("record_candidate_answers");
  await tell([retained],"e",call,new Map([["interrupted|standard_single_target|base",{question:interrupted,task_id:pending.task_id,lease:"interrupted"}]]));
  check("a recorded official candidate resumes acceptance after an interrupted settlement", await count("record_candidate_answers") === candidates
    && (await db.get("SELECT score FROM state_board_measurement_selection WHERE identity='interrupted'")).score === 10);
  if (sql) {
    await queue("parallel-a");await queue("parallel-b");
    const results = await Promise.all([advance(), advance()]);
    check("concurrent consumers claim each registration once", results.reduce((n, x) => n + x.asked, 0) === 2
      && !!await qid("parallel-a") && !!await qid("parallel-b"));
  }
  await setDial(env,"board_admission",1,"fixture",now);forgetDials();
  check("the enabled daemon path uses the same bounded registration service", (await recoverBoardTasks(env,"e",now)).mode === "admission");
  await setDial(env,"board_admission",0,"fixture",now);forgetDials();
  if (sql) {
    const cursor = await db.get("SELECT * FROM state_board_admission_recovery"), tasks = await count("entity_compute_tasks");
    const connection = await sql.reserve();
    try { await connection.unsafe(boardAdmissionSchemaSql().replace('SET LOCAL ROLE wfsim;','')).simple(); }
    finally { connection.release(); }
    check("reinstalling the additive upgrade preserves historical progress and task rounds",
      JSON.stringify(await db.get("SELECT * FROM state_board_admission_recovery")) === JSON.stringify(cursor)
      && await count("entity_compute_tasks") === tasks);
  }
  console.log(`${checks} board admission checks passed${url ? " over TCP Postgres" : " in PGlite"}`);
} finally { await close(); }
