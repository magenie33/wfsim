// Exercise durable checkpoints, changed keys and the real field-audit failure on isolated PostgreSQL.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import postgres from "./pg/node_modules/postgres/src/index.js";

const base=process.env.WFSIM_MIGRATION_TEST_URL;
if(!base || !["127.0.0.1","localhost","[::1]"].includes(new URL(base).hostname))
  throw new Error("this check needs an isolated localhost PostgreSQL admin URL");
const database="wfsim_db_boundaries_test_"+randomBytes(6).toString("hex");
const source=new URL(base);source.pathname="/"+database;
const options={max:1,fetch_types:false,onnotice:()=>{}};
const admin=postgres(base,options);
await admin.unsafe(`CREATE DATABASE ${database}`);
const db=postgres(source.toString(),options), other=postgres(source.toString(),options);
const env={...process.env,WFSIM_MIGRATION_URL:source.toString()};
const run=(command,...args)=>{
  const result=spawnSync(process.execPath,["scripts/migrations/db_boundaries.mjs",command,...args],{env,encoding:"utf8",timeout:30000});
  if(result.status!==0)throw new Error(result.stderr || String(result.error));
  return result.stdout.trim().split("\n").filter(Boolean).map(line=>JSON.parse(line));
};
let passed=0,complete=false;
const check=(name,ok)=>{assert.ok(ok,name);passed++;console.log("ok "+name);};
try {
  await db.unsafe(`CREATE TABLE questions(
    id text COLLATE "C" PRIMARY KEY,verb text NOT NULL,request text NOT NULL,state text NOT NULL,
    due_at text NOT NULL,waits bigint NOT NULL DEFAULT 0,opened_at text NOT NULL,lanes text,
    lease text,lease_until text,leased_to text,leased_at text,lease_prev text,leased_to_prev text,lease_prev_until text,
    needs bigint,canon text,fact_engine text,fact_at text,odd bigint NOT NULL DEFAULT 0,
    producer text NOT NULL DEFAULT '',initiator text,withdrawn_at text,witnesses text NOT NULL DEFAULT '[]');
    CREATE TABLE answers(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,question text NOT NULL);
    INSERT INTO questions(id,verb,request,state,due_at,opened_at,canon,fact_engine,fact_at,witnesses)
      SELECT 'b'||lpad(i::text,3,'0'),'simulate','{ "weapon": "原文 '||i||'" }','fact',
        '2026-10-01T00:00:00.000Z','2026-09-01T00:00:00.000Z','{ "score": 42 }','old-engine',
        '2026-09-02T00:00:00.000Z','[ { "device": "original" } ]' FROM generate_series(1,19)i;
    INSERT INTO answers(question) SELECT id FROM questions;`);
  const {stage}=run("prepare")[0];
  const q=text=>db.unsafe(`SET search_path TO ${stage},public`).then(()=>db.unsafe(text));
  const first=run("copy","--batch=2","--batches=1").at(-1);
  check("a bounded copy commits its cursor with exactly its own two rows",first.rows==="2"
    && (await q("SELECT count(*) AS n FROM entity_compute_specs"))[0].n==="2");

  await q(`CREATE FUNCTION pause_copy() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM pg_sleep(1);RETURN NEW;END$$;
    CREATE TRIGGER pause_copy BEFORE INSERT ON entity_compute_specs FOR EACH ROW EXECUTE FUNCTION pause_copy()`);
  const child=spawn(process.execPath,["scripts/migrations/db_boundaries.mjs","copy","--batch=2"],{env,stdio:"ignore"});
  let inFlight=false;
  for(let i=0;i<100;i++) {
    const [row]=await other.unsafe("SELECT count(*) AS n FROM pg_stat_activity WHERE application_name='wfsim-db-boundaries' AND datname=$1 AND wait_event='PgSleep'",[database]);
    if(row.n!=="0"){inFlight=true;break;}
    await new Promise(resolve=>setTimeout(resolve,20));
  }
  assert.ok(inFlight,"the interruption must occur inside the actual copy transaction");
  const exited=new Promise(resolve=>child.once("exit",resolve));child.kill("SIGKILL");await exited;
  // Waiting for this lock also waits for PostgreSQL to roll back the terminated client's transaction.
  await q("SELECT pg_advisory_xact_lock(hashtextextended('wfsim-db-boundaries',0))");
  check("an interrupted chunk advances neither its data nor its checkpoint",(await q("SELECT rows FROM state_migration_progress WHERE id='compute'"))[0].rows==="2"
    && (await q("SELECT count(*) AS n FROM entity_compute_specs"))[0].n==="2");
  await q("DROP TRIGGER pause_copy ON entity_compute_specs;DROP FUNCTION pause_copy()");
  run("copy","--batch=3");run("copy-evidence","--batch=3");
  check("resuming completes inputs, outputs and candidate links without duplicates",(await q("SELECT count(*) AS n FROM entity_compute_specs"))[0].n==="19"
    && (await q("SELECT count(*) AS n FROM entity_compute_results"))[0].n==="19"
    && (await q("SELECT count(*) AS n FROM relation_task_answer_evidence"))[0].n==="19");

  const held=await db.reserve();await held.unsafe("BEGIN");
  await held.unsafe("UPDATE public.questions SET due_at='late-commit' WHERE id='b001'");
  await other.unsafe("UPDATE public.questions SET due_at='early-commit' WHERE id='b019'");
  run("refresh");
  await held.unsafe("COMMIT");held.release();
  run("refresh");
  check("an earlier allocated journal row committed late is still replayed",(await q("SELECT due_at FROM state_compute_work WHERE id='b001'"))[0].due_at==="late-commit");
  await db.unsafe(`INSERT INTO public.questions SELECT 'a000',verb,request,state,due_at,waits,opened_at,lanes,lease,lease_until,
    leased_to,leased_at,lease_prev,leased_to_prev,lease_prev_until,needs,canon,fact_engine,fact_at,odd,producer,initiator,withdrawn_at,witnesses
    FROM public.questions WHERE id='b003';DELETE FROM public.questions WHERE id='b002';DELETE FROM public.answers WHERE question='b002'`);
  run("refresh");
  check("new keys behind the copy cursor and deletions are reconciled",(await q("SELECT count(*) AS n FROM state_compute_work WHERE id='a000'"))[0].n==="1"
    && (await q("SELECT count(*) AS n FROM state_compute_work WHERE id='b002'"))[0].n==="0"
    && (await q("SELECT count(*) AS n FROM relation_task_answer_evidence"))[0].n==="18");
  const [different]=await q(`SELECT count(*) AS n FROM public.questions q FULL JOIN state_compute_work v ON v.id=q.id
    WHERE to_jsonb(q) IS DISTINCT FROM (to_jsonb(v)-'task_id'-'result_id')`);
  check("all legacy fields, opaque text and original clocks remain identical",different.n==="0");
  run("check","--batch=3");
  await q("UPDATE state_compute_scheduling SET due_at='broken' WHERE id='b003';UPDATE state_migration_progress SET cursor=NULL,complete=false WHERE id='audit'");
  let rejected=false;try{run("check");}catch(error){rejected=error.message.includes("independent field audit failed: b003");}
  check("the independent audit fails on an actual corrupted target field",rejected);
  await q("UPDATE state_compute_scheduling SET due_at=(SELECT due_at FROM public.questions WHERE id='b003') WHERE id='b003'");
  run("check");
  const before=(await q("SELECT request FROM entity_compute_specs WHERE id='b001'"))[0].request;
  await db.unsafe("UPDATE public.questions SET request='changed-input' WHERE id='b001'");
  rejected=false;try{run("refresh");}catch(error){rejected=error.message.includes("source changed an existing immutable input: b001");}
  check("a reused id with different source text cannot silently merge inputs",rejected
    && (await q("SELECT request FROM entity_compute_specs WHERE id='b001'"))[0].request===before);
  complete=true;console.log(JSON.stringify({ok:true,checks:passed,stage,transport:"tcp",database}));
} finally {
  await db.end();await other.end();
  if(complete)await admin.unsafe(`DROP DATABASE ${database}`);
  else console.error("preserved failed isolated database: "+database);
  await admin.end();
}
