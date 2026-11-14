// A source and its one active execution are admitted in the same transaction.
import assert from "node:assert/strict";
import { testDb } from "./pg/test_db.mjs";
import { readFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { pgDb, placeholders } from "../worker/db.js";
async function database() {
  const url = process.env.WFSIM_ADMISSION_TEST_URL;
  if (!url) return testDb("library");
  assert.ok(["127.0.0.1", "localhost", "[::1]"].includes(new URL(url).hostname), "admission race checks require isolated local PG");
  const { default: postgres } = await import("./pg/node_modules/postgres/src/index.js");
  const schema = "admission_" + randomBytes(8).toString("hex");
  const sql = postgres(url, { max: 8, fetch_types: false, onnotice: () => {}, connection: { search_path: schema, statement_timeout: 10000 },
    types: { bigint: { to: 20, from: [20], parse: Number, serialize: String } } });
  await sql.unsafe(`CREATE SCHEMA ${schema}`);
  for (const name of ["schema", "library", "sync", "inventory", "build_workflows"]) await sql.unsafe(readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), "utf8"));
  const query = async (text, args = []) => { const r = await sql.unsafe(text, args); return { rows: [...r], affected: r.count }; };
  return Object.assign(pgDb({ query, transaction: fn => sql.begin(tx => fn({ query: async (text, args) => {
    const r = await tx.unsafe(text, args); return { rows: [...r], affected: r.count };
  } })) }), {
    get: async (text, ...args) => (await query(placeholders(text),args)).rows[0] || null,
    all: async (text,...args) => (await query(placeholders(text),args)).rows,
    run: async (text, ...args) => (await query(placeholders(text),args)).affected,
    close: async () => { await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`); await sql.end(); },
  });
}
import { identityPage, applyIdentityEntry } from "./compute_identity.mjs";
import { queueSourcePage } from "./compute_queue_sources.mjs";
import { buildWorkflowCases,workflowProtocolCases,inboxReadPlanCases,queuedReviewAcrossReleaseCases } from "./check_build_workflows.mjs";
import { queueZoneCases } from "./check_queue_zones.mjs";
import { factory, questionId as productionQuestionId, canon } from "../worker/tasks.js";

const hashSource = readFileSync(new URL("../worker/computation_input.js", import.meta.url),"utf8");
const questionId = process.argv.includes("--break-identity-metadata")
  ? (await import("data:text/javascript,"+encodeURIComponent(hashSource.replace('"engine", ','')))).computationId : productionQuestionId;
const db = await database(), env = { LIBRARY: db }, at = Date.parse("2026-11-11T00:00:00Z");
const f = factory({ verbs: { simulate: { cap: 2, lease_ms: 60000, canon } }, producers: { first: {on_fact:async()=>{}}, second: {on_fact:async()=>{}} } });
const ask = (producer, ref, extra = {}) => ({ producer, ref, verb: "simulate", request: { build: "build", scenario: "scenario", runs: 1000 },
  due_in_ms: 60000, payload: { purpose: "fixture" }, ...extra });
try {
  if (process.argv.includes("--break-source-guard")) {
    const source=readFileSync(new URL("../worker/pg/schema.sql",import.meta.url),"utf8");
    const definition=source.match(/CREATE OR REPLACE FUNCTION compute_admit_tasks[^]*?END \$\$;/)[0];
    const from=definition.indexOf("  IF EXISTS(SELECT 1 FROM jsonb_array_elements(d) x WHERE"),until=definition.indexOf("  FOR source IN SELECT value",from);
    assert.ok(from>0 && until>from);await db.run(definition.slice(0,from)+definition.slice(until));
  }
  const base = { build: "build", scenario: "scenario", runs: 1000 };
  for (const metadata of [{ engine: "e2" }, { __board_upload: true }, { __retention_window: 7 }, { review: "r2" }])
    assert.equal(await questionId("simulate",base),await questionId("simulate",{...base,...metadata}));
  for (const change of [{ runs: 1001 }, { seed: 2 }, { scenario: "another" }])
    assert.notEqual(await questionId("simulate",base),await questionId("simulate",{...base,...change}));
  const id = await f.demand(env, ask("first", "request-one"), "engine-one", at);
  const original = await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?", id);
  const second = await f.demand(env, ask("second", "request-two", { due_in_ms: 1000 }), "engine-two", at);
  assert.equal(second, id);
  assert.equal((await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?", id)).task_id, original.task_id);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_compute_tasks WHERE computation=?", id)).n, 1);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_compute_task_sources WHERE task_id=?", original.task_id)).n, 2);
  assert.equal((await db.get("SELECT producer FROM state_compute_scheduling WHERE id=?", id)).producer, "first");
  await db.run("UPDATE state_compute_scheduling SET lease='held',leased_to='machine',lease_until='2999-01-01' WHERE id=?", id);
  await f.demand(env, ask("first", "request-three"), "engine-three", at + 1000);
  assert.equal((await db.get("SELECT lease FROM state_compute_scheduling WHERE id=?", id)).lease, "held");
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_compute_tasks WHERE computation=?", id)).n, 1);
  await f.demand(env, ask("first", "request-three"), "engine-three", at + 2000);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_compute_task_sources WHERE task_id=?", original.task_id)).n, 3);
  await f.demand(env, ask("first", "request-three", { trigger_id: "new-trigger", initiator: "system" }), "engine-three", at + 2000);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_compute_task_sources WHERE task_id=?", original.task_id)).n, 4);
  await assert.rejects(f.demand(env, ask("first", "request-three", { payload: { purpose: "changed" } }), "engine-one", at), /source identity conflict/);
  await assert.rejects(f.demand(env, ask("first", ""), "engine-one", at), /requires a source/);
  const emptyId = await questionId("simulate", { build: "source-less" });
  await assert.rejects(db.run("SELECT compute_admit_tasks(?1::text,'[]',?2::text,?3::text)",
    JSON.stringify([{ id: emptyId, verb: "simulate", request: '{}', due_at: 'x', producer: 'first', waits: 0 }]), 'e', 'x'), /requires a source/);
  assert.equal(await db.get("SELECT id FROM entity_compute_specs WHERE id=?", emptyId), null);
  await db.run("UPDATE state_compute_scheduling SET state='withdrawn',lease=NULL,leased_to=NULL,lease_until=NULL WHERE id=?", id);
  await f.demand(env, ask("second", "later-request"), "engine-three", at + 3000);
  assert.notEqual((await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?", id)).task_id, original.task_id);
  assert.equal((await db.get("SELECT count(*) AS n FROM state_compute_scheduling WHERE id=?", id)).n, 1);
  const race = { ...base, seed: 9876 };
  const raceIds = await Promise.all(Array.from({ length: 24 }, (_, i) => f.demand(env, ask("first", `concurrent-${i}`, { request: race }), `engine-${i}`, at)));
  assert.equal(new Set(raceIds).size, 1);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_compute_tasks WHERE computation=?",raceIds[0])).n, 1);
  assert.equal((await db.get("SELECT count(*) AS n FROM relation_compute_task_sources p JOIN entity_compute_tasks t ON t.id=p.task_id WHERE t.computation=?",raceIds[0])).n, 24);
  await db.run("SELECT compute_accept_result(?1::text,task_id,'open','fact','accepted','e1',?2::text,0,'[]') FROM state_compute_scheduling WHERE id=?1::text",id,new Date(at).toISOString());
  const settled = (await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",id)).task_id;
  await f.demand(env,ask("second","cached-consumer"),"new-deployment",at+4000);
  assert.equal((await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",id)).task_id,settled);
  await f.demand(env,ask("second","explicit-review",{force:true,trigger_id:"review-1"}),"new-deployment",at+5000);
  const reviewed = (await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",id)).task_id;
  assert.notEqual(reviewed,settled);
  await f.demand(env,ask("second","explicit-review",{force:true,trigger_id:"review-1"}),"new-deployment",at+6000);
  assert.equal((await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",id)).task_id,reviewed);
  await db.run("INSERT INTO entity_user_calculation_requests(id,task,owner,kind,engine,question,state,created_at,expires_at) VALUES('durable-source','local','owner','simulation','e',?,'waiting','2026-11-11','2027-11-11')",raceIds[0]);
  await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('user_calculation','durable-source',?,'live','2026-11-11','2027-11-11')",raceIds[0]);
  assert.equal((await db.get("SELECT compute_task_has_source(?) AS traced",raceIds[0])).traced,true);
  await db.run("UPDATE relation_task_consumers SET payload='invalid-json' WHERE producer='user_calculation' AND ref='durable-source'");
  assert.equal((await db.get("SELECT compute_task_has_source(?) AS traced",raceIds[0])).traced,true);
  const liveTask = (await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",raceIds[0])).task_id;
  assert.equal((await db.get("SELECT compute_remove_untraced(?,?,?) AS removed",raceIds[0],liveTask,'2026-11-11')).removed,false);
  const orphan = await f.demand(env,ask("first","lost-history",{request:{...base,seed:111}}),"e",at);
  const orphanTask = (await db.get("SELECT task_id FROM state_compute_scheduling WHERE id=?",orphan)).task_id;
  assert.equal((await db.get("SELECT compute_remove_untraced(?,?,?) AS removed",orphan,orphanTask,'2026-11-11')).removed,true);
  assert.equal((await db.get("SELECT state FROM state_compute_scheduling WHERE id=?",orphan)).state,'withdrawn');
  assert.ok(await db.get("SELECT id FROM entity_compute_specs WHERE id=?",orphan));
  assert.equal((await db.get("SELECT count(*) AS n FROM record_compute_queue_removals WHERE task_id=?",orphanTask)).n,1);
  const legacyInput = {weapon:"braton",runs:5,__board_upload:true};
  await db.run("SELECT compute_register_tasks(?1::text,'legacy-engine',?2::text)",JSON.stringify([{id:"legacy-input",verb:"simulate_report",request:canon(legacyInput),due_at:"2027-11-11",waits:0,producer:"user_calculation"}]),"2026-11-11");
  await db.run("INSERT INTO entity_user_calculation_requests(id,task,owner,kind,engine,question,state,created_at,expires_at) VALUES('legacy-request','local','owner','simulation','legacy-engine','legacy-input','waiting','2026-11-11','2027-11-11')");
  await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('user_calculation','legacy-request','legacy-input','live','2026-11-11','2027-11-11')");
  const plan = await identityPage(db,async (_kind,request) => ({request:{...request,seed:7},aliases:{}}),{engine:"current-engine"});
  const entry = plan.entries.find(entry => entry.id==="legacy-input");
  assert.equal(entry.action,"rekey");
  assert.equal(JSON.parse(entry.demands[0].payload).board_upload,true);
  const invalid=structuredClone(entry); invalid.demands[0].ref="";
  await assert.rejects(applyIdentityEntry(db,invalid),/requires a source/);
  assert.equal((await db.get("SELECT state FROM state_compute_scheduling WHERE id='legacy-input'")).state,'open');
  assert.equal((await db.get("SELECT question FROM entity_user_calculation_requests WHERE id='legacy-request'")).question,'legacy-input');
  const moved=await applyIdentityEntry(db,entry), retried=await applyIdentityEntry(db,entry);
  assert.equal(moved.task_id,retried.task_id);
  assert.equal((await db.get("SELECT state FROM state_compute_scheduling WHERE id='legacy-input'")).state,'withdrawn');
  assert.equal((await db.get("SELECT question FROM entity_user_calculation_requests WHERE id='legacy-request'")).question,entry.spec.id);
  assert.equal((await db.get("SELECT count(*) AS n FROM entity_compute_tasks WHERE computation=?",entry.spec.id)).n,1);
  assert.equal((await db.get("SELECT asked_at FROM relation_compute_task_sources WHERE task_id=?",moved.task_id)).asked_at,'2026-11-11');
  const audit=await queueSourcePage(db,{limit:100}); assert.ok(audit.scanned>0); assert.equal(audit.removed,0);
  const duplicateRequest={weapon:'braton',runs:11,seed:432198};
  for(const [index,name] of ['duplicate-old-a','duplicate-old-b','duplicate-running'].entries()) {
    await db.run("SELECT compute_register_tasks(?::text,'old-engine',?::text)",JSON.stringify([{id:name,verb:'simulate_report',
      request:canon({...duplicateRequest,engine:'old-'+index}),due_at:'2027-11-11',waits:0,producer:'user_calculation',zone:[3,0,1][index]}]),'2026-11-11');
    await db.run(`INSERT INTO entity_user_calculation_requests(id,task,owner,kind,engine,question,state,created_at,expires_at)
      VALUES(?,?,'owner','simulation','old-engine',?,'waiting','2026-11-11','2027-11-11')`,name,name,name);
    await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,zone) VALUES('user_calculation',?,?,'live','2026-11-11','2027-11-11',?)",name,name,[3,0,1][index]);
  }
  await db.run("UPDATE state_compute_scheduling SET lease='keep-running',leased_to='device',lease_until='2999-01-01' WHERE id='duplicate-running'");
  const duplicatePlan=await identityPage(db,async(_kind,request)=>({request:computationWithoutMetadata(request),aliases:{}}),{engine:'current-engine',duplicatesOnly:true});
  const duplicates=duplicatePlan.entries.filter(e=>e.action==='rekey' && e.id.startsWith('duplicate-old'));
  assert.equal(duplicates.length,2);assert.equal(duplicates[0].spec.id,duplicates[1].spec.id);
  assert.equal(duplicatePlan.entries.find(e=>e.id==='duplicate-running').action,'deferred');
  for(const entry of duplicates)await applyIdentityEntry(db,entry);
  assert.equal((await db.get('SELECT zone FROM state_compute_work WHERE id=?',duplicates[0].spec.id)).zone,0);
  assert.equal((await db.get('SELECT count(*) n FROM relation_task_consumers WHERE question=? AND state=\'live\'',duplicates[0].spec.id)).n,2);
  assert.equal((await db.get("SELECT lease FROM state_compute_work WHERE id='duplicate-running'")).lease,'keep-running');
  assert.equal((await db.get("SELECT count(*) n FROM entity_compute_specs WHERE id LIKE 'duplicate-old-%'")).n,2,'duplicate consolidation retains original immutable inputs');
  const heldPlan=structuredClone(duplicates[0]);heldPlan.id='duplicate-running';heldPlan.task_id=(await db.get("SELECT task_id FROM state_compute_work WHERE id='duplicate-running'")).task_id;
  await assert.rejects(applyIdentityEntry(db,heldPlan),e=>e.code==='40001');
  console.log('duplicate consolidation: proof-only merge, source/zone preservation, history retention and running-lease deferral pass');
  await queueZoneCases(db);
  await buildWorkflowCases(db);
  await workflowProtocolCases();
  await inboxReadPlanCases();
  await queuedReviewAcrossReleaseCases();
  console.log("compute admission: shared identity, queue zones, held execution, all origins, retries and atomic rejection pass");
} finally { await db.close(); }

function computationWithoutMetadata(request) {
  const copy={...request};delete copy.engine;return copy;
}
