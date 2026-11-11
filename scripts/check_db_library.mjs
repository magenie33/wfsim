// Run the actual old-schema activation, interruption and recovery paths on isolated PostgreSQL.
import assert from "node:assert/strict";
import {randomBytes,createHash} from "node:crypto";
import {spawn,spawnSync} from "node:child_process";
import postgres from "./pg/node_modules/postgres/src/index.js";

const base=process.env.WFSIM_MIGRATION_TEST_URL;
if(!base || !["127.0.0.1","localhost","[::1]"].includes(new URL(base).hostname))throw new Error("isolated localhost admin URL is required");
const database="wfsim_migration_cutover_"+randomBytes(6).toString("hex");
const url=new URL(base);url.pathname="/"+database;
const options={max:1,fetch_types:false,onnotice:()=>{}};
const admin=postgres(base,options);await admin.unsafe(`CREATE DATABASE ${database}`);
const db=postgres(url.toString(),options),env={...process.env,WFSIM_MIGRATION_URL:url.toString(),WFSIM_MIGRATION_REQUIRE_FENCE:"0",WFSIM_MIGRATION_FENCE:"",WFSIM_MIGRATION_PRODUCTION:""};
const run=(tool,command,...args)=>spawnSync(process.execPath,["scripts/migrations/"+tool+".mjs",command,...args],{env,encoding:"utf8",timeout:90000,maxBuffer:1024*1024});
const good=(tool,command,...args)=>{
  const result=run(tool,command,...args);
  if(result.status!==0)throw new Error(result.stderr || String(result.error));
  return result.stdout.trim().split("\n").filter(Boolean).map(line=>JSON.parse(line)).at(-1);
};
let checks=0,complete=false;
const check=(name,value)=>{assert.ok(value,name);console.log("ok "+name);checks++;};
const get=async(text,params=[])=>(await db.unsafe(text,params))[0];
const fingerprint=async()=>createHash("sha256").update(JSON.stringify(await db.unsafe(`SELECT row_to_json(q)::text AS body FROM
  (SELECT id,verb,request,state,due_at,waits,opened_at,lanes,lease,lease_until,leased_to,leased_at,lease_prev,
    leased_to_prev,lease_prev_until,needs,canon,fact_engine,fact_at,odd,producer,initiator,withdrawn_at,witnesses
    FROM public.questions ORDER BY id)q`))).digest("hex");
try {
  for(const name of ["schema","library","sync","inventory"]){
    const source=spawnSync("git",["show","898fb4f7729f6b4e4a5e4069a8f410782e21e405:worker/pg/"+name+".sql"],{encoding:"utf8"});
    if(source.status!==0)throw new Error(source.stderr);
    await db.unsafe(source.stdout);
  }
  await db.unsafe(`CREATE TABLE answers_dup_backup_20261014(id bigint PRIMARY KEY,raw text);
    CREATE TABLE watchdog_told(id text PRIMARY KEY,at text);
    INSERT INTO questions(id,verb,request,state,due_at,opened_at,canon,fact_engine,fact_at,witnesses)
      SELECT 'q'||i,'simulate_report','{ "input": "原文 '||i||'" }','fact','2026-11-10T00:00:00.000Z',
        '2026-11-09T00:00:00.000Z','{ "report": { "score": '||i||' } }','engine',
        '2026-11-09T01:00:00.000Z','[ { "device": "original" } ]' FROM generate_series(1,5)i;
    INSERT INTO answers(question,engine,device,owner,canon,result,work,at)
      SELECT id,'engine','device'||id,'owner'||id,canon,'{ "original": true }',100,'2026-11-09T00:30:00.000Z' FROM questions;
    INSERT INTO calculation_reports(question,engine,result,at)
      SELECT id,'engine','{ "score": '||right(id,1)||' }','2026-11-09T01:00:00.000Z' FROM questions WHERE id IN('q1','q2');
    INSERT INTO calculation_reports VALUES('cold-without-input','engine','{  "original": "冷报告"  }','2026-11-09T01:00:00.000Z');
    INSERT INTO user_calculations(id,task,owner,kind,engine,question,state,created_at,completed_at,expires_at)
      SELECT 'request:'||question,'browser:'||question,'private-owner','simulation',engine,question,'succeeded',
        '2026-11-09T00:00:00.000Z',at,'2026-11-23T01:00:00.000Z' FROM calculation_reports;`);
  if(!(await get("SELECT rolname FROM pg_roles WHERE rolname='wfsim_factoryd'")))await db.unsafe("CREATE ROLE wfsim_factoryd NOLOGIN");
  const oldGrants=spawnSync("git",["show","898fb4f7729f6b4e4a5e4069a8f410782e21e405:deploy/factoryd_grants.sql"],{encoding:"utf8"});
  assert.equal(oldGrants.status,0);
  for(const match of oldGrants.stdout.matchAll(/GRANT (?!CONNECT)[^;]+;/g))await db.unsafe(match[0]);
  if(!(await get("SELECT rolname FROM pg_roles WHERE rolname='wfsim'")))await db.unsafe("CREATE ROLE wfsim NOLOGIN");
  const owned=await db.unsafe("SELECT relname,relkind FROM pg_class WHERE relnamespace='public'::regnamespace AND relkind IN('r','v','S') ORDER BY CASE relkind WHEN 'r' THEN 0 WHEN 'v' THEN 1 ELSE 2 END");
  for(const relation of owned)await db.unsafe(`ALTER ${relation.relkind==='S' ? 'SEQUENCE' : relation.relkind==='v' ? 'VIEW' : 'TABLE'} public."${relation.relname}" OWNER TO wfsim`);
  const routines=await db.unsafe("SELECT oid::regprocedure::text AS signature FROM pg_proc WHERE pronamespace='public'::regnamespace AND prokind IN('f','p')");
  for(const routine of routines)await db.unsafe("ALTER ROUTINE public."+routine.signature+" OWNER TO wfsim");
  const stage=good("db_boundaries","prepare").stage;
  for(const command of ["copy","copy-evidence","check"])good("db_boundaries",command);
  good("db_evidence","--stage="+stage);
  const args=["--stage="+stage];
  const unfenced=spawnSync(process.execPath,["scripts/migrations/db_library.mjs","reconcile",...args],
    {env:{...env,WFSIM_MIGRATION_REQUIRE_FENCE:"1",WFSIM_MIGRATION_FENCE:"missing-"+database},encoding:"utf8",timeout:30000});
  check("a required fence cannot be replaced by an operation name alone",unfenced.status!==0
    && /exact cross-database fence is not active|no cross-database fence registry exists/.test(unfenced.stderr));
  const unsafeUrl=new URL(url.toString());
  unsafeUrl.pathname="/wfsim";
  const unguarded=spawnSync(process.execPath,["scripts/migrations/db_library.mjs","status",...args],
    {env:{...env,WFSIM_MIGRATION_URL:unsafeUrl.toString(),WFSIM_MIGRATION_PRODUCTION:""},encoding:"utf8",timeout:30000});
  check("production names fail before connecting without the exact socket and operation guard",unguarded.status!==0
    && unguarded.stderr.includes("production execution requires the exact operation id"));
  await db.unsafe(`UPDATE questions SET canon='{ "report": { "score": 99 } }',fact_at='2026-11-09T02:00:00.000Z' WHERE id='q1';
    UPDATE calculation_reports SET result='{  "score": 99  }',at='2026-11-09T02:00:00.000Z' WHERE question='q1';
    DELETE FROM questions WHERE id='q2';
    INSERT INTO questions(id,verb,request,state,due_at,opened_at) VALUES('a-new-low-key','simulate','{ "new": true }','open',
      '2026-11-10T00:00:00.000Z','2026-11-09T00:00:00.000Z');
    UPDATE answers SET question='a-new-low-key' WHERE question='q3';`);
  good("db_library","prepare",...args);
  const reconciled=good("db_library","reconcile",...args);
  check("final journal reconciliation audits changed, inserted and deleted keys",reconciled.dirty_keys_checked===4);
  check("raw cold reports and owner access survive without invented inputs",(await get(`SELECT count(*) AS n FROM ${stage}.entity_calculation_reports WHERE origin='imported'`)).n==="3"
    && (await get(`SELECT count(*) AS n FROM ${stage}.entity_compute_specs WHERE id='cold-without-input'`)).n==="0"
    && (await get(`SELECT count(*) AS n FROM ${stage}.relation_calculation_report_access`)).n==="3");
  await db.unsafe("UPDATE questions SET state='open',opened_at='2026-11-10T00:00:00.000Z',due_at='2026-11-11T00:00:00.000Z' WHERE id='q4'");
  const changed=run("db_library","activate",...args);
  check("activation rejects source changes committed after the final audit",changed.status!==0 && changed.stderr.includes("source changed after final reconciliation"));
  good("db_library","reconcile",...args);
  const reopened=await get(`SELECT t.generation,t.created_at,r.task_id<>s.task_id AS prior_result
    FROM ${stage}.state_compute_scheduling s JOIN ${stage}.entity_compute_tasks t ON t.id=s.task_id
    JOIN ${stage}.entity_compute_results r ON r.id=s.result_id WHERE s.id='q4'`);
  check("a legacy reopening becomes a distinct task while retaining its previous result",reopened.generation==="2"
    && reopened.created_at==="2026-11-10T00:00:00.000Z" && reopened.prior_result);
  check("moving a source candidate cannot leave evidence attached to another input",(await get(`SELECT count(*) AS n
    FROM ${stage}.relation_task_answer_evidence e JOIN ${stage}.entity_compute_tasks t ON t.id=e.task_id
    JOIN public.answers a ON a.id=e.answer_id WHERE a.question<>t.computation`)).n==="0");
  const before=await fingerprint();
  const interrupted=run("db_library","activate",...args,"--fail-after=7");
  check("interruption after seven actual renames rolls back the entire activation",interrupted.status!==0
    && interrupted.stderr.includes("injected activation interruption") && before===await fingerprint()
    && good("db_library","status",...args).phase==="reconciled");
  const child=spawn(process.execPath,["scripts/migrations/db_library.mjs","activate",...args,"--pause-after=7"],{env,stdio:"ignore"});
  let paused=false;
  for(let attempt=0;attempt<100;attempt++){
    const activity=await get("SELECT count(*) AS n FROM pg_stat_activity WHERE datname=$1 AND application_name='wfsim-library-layout-rehearsal' AND wait_event='PgSleep'",[database]);
    if(activity.n!=="0"){paused=true;break;}
    await new Promise(resolve=>setTimeout(resolve,20));
  }
  assert.ok(paused,"the process must be inside its actual rename transaction");
  const exited=new Promise(resolve=>child.once("exit",resolve));child.kill("SIGKILL");await exited;
  await db.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('wfsim-db-boundaries',0))");
  check("SIGKILL during actual metadata changes leaves the whole source layout intact",before===await fingerprint()
    && good("db_library","status",...args).phase==="reconciled");
  await db.unsafe("GRANT INSERT ON donors TO wfsim_factoryd");
  const widened=run("db_library","activate",...args);
  check("activation rejects an unexpected widening of existing table permissions",widened.status!==0
    && widened.stderr.includes("source object identity/privilege changed: donors"));
  await db.unsafe("REVOKE INSERT ON donors FROM wfsim_factoryd");
  good("db_library","activate",...args);
  check("dispatch indexes belong to the new scheduling heap",(await get("SELECT indrelid='state_compute_scheduling'::regclass AS correct FROM pg_index WHERE indexrelid='questions_by_producer'::regclass")).correct
    && (await get("SELECT indrelid='state_compute_scheduling'::regclass AS correct FROM pg_index WHERE indexrelid='questions_coded'::regclass")).correct);
  check("activation removes source runtime tables and switches task uniqueness",(await get("SELECT to_regclass('public.questions') AS old,to_regclass('public.answers') AS answers")).old===null
    && (await get("SELECT to_regclass('public.answers') AS old")).old===null
    && (await get("SELECT pg_get_indexdef('public.answers_once'::regclass) AS body")).body.includes("task_id, device, engine"));
  const rollbackProbe=new Error("rollback runtime probe");
  try {
    await db.begin(async tx=>{
      await tx.unsafe("SET LOCAL ROLE wfsim");
      const at="2026-11-10T01:00:00.000Z";
      await tx.unsafe("SELECT compute_register_tasks($1::text,'engine',$2::text)",[JSON.stringify([{id:"runtime-probe",verb:"simulate",request:'{ "raw": true }',due_at:at,waits:0,producer:"test"}]),at]);
      const [task]=await tx.unsafe("SELECT task_id FROM state_compute_scheduling WHERE id='runtime-probe'");
      const answer=async taskId=>tx.unsafe("SELECT factory_answer('runtime-probe','engine','same-device','same-owner','',0,'score','{}',10,1,NULL,1,$1::text,$2::bigint)",[at,taskId]);
      await answer(task.task_id);
      await tx.unsafe("SELECT compute_accept_result('runtime-probe',$1::bigint,'open','fact','first','engine',$2::text,0,'[]')",[task.task_id,at]);
      const [next]=await tx.unsafe("SELECT compute_prepare_correction('runtime-probe',$1::bigint,'second','engine',$2::text,'[]') AS id",[task.task_id,at]);
      await answer(next.id);
      check("a correction admits the same device and engine only under a new task",next.id!==task.task_id
        && (await tx.unsafe("SELECT count(*) AS n FROM record_candidate_answers WHERE question='runtime-probe'"))[0].n==="2");
      throw rollbackProbe;
    });
  } catch(error){if(error!==rollbackProbe)throw error;}
  check("rolled-back runtime probes do not cross the recovery boundary",(await get("SELECT count(*) AS n FROM wfsim_migration_ops.record_library_writes")).n==="0");
  try{
    await db.begin(async tx=>{
      await tx.unsafe("SET LOCAL ROLE wfsim_factoryd");
      await tx.unsafe("SELECT compute_select_report('q1','engine','2026-11-09T02:00:00.000Z')");
      check("the limited daemon role can select and deliver the migrated report",(await tx.unsafe("SELECT result::jsonb->>'score' AS score FROM state_current_calculation_reports WHERE question='q1'"))[0].score==="99");
      throw rollbackProbe;
    });
  }catch(error){if(error!==rollbackProbe)throw error;}
  good("db_library","backout",...args);
  check("pre-write backout restores original source bytes and old runtime functions",before===await fingerprint()
    && (await get("SELECT pg_get_function_result('public.factory_held(text,text,text,text,bigint)'::regprocedure) AS type")).type==="SETOF questions");
  check("backout retains additive workflow objects only inside its prepared stage",(await get("SELECT to_regclass('public.entity_build_workflows') AS workflow,to_regclass('public.relation_compute_task_sources') AS sources")).workflow===null
    && (await get("SELECT to_regclass('public.relation_compute_task_sources') AS sources")).sources===null);
  check("backout restores the original daemon privilege boundary",!(await get("SELECT has_table_privilege('wfsim_factoryd','producer_use','INSERT') AS widened")).widened);
  await db.unsafe("INSERT INTO questions(id,verb,request,state,due_at,opened_at) VALUES('after-backout-new-input','simulate','{}','open','2026-11-10T00:00:00.000Z','2026-11-10T00:00:00.000Z')");
  good("db_library","reconcile",...args);good("db_library","activate",...args);
  const at="2026-11-10T01:00:00.000Z";
  await db.unsafe("SELECT compute_register_tasks($1::text,'engine',$2::text)",[JSON.stringify([{id:"committed-new-task",verb:"simulate",request:'{ "must": "survive" }',due_at:at,waits:0,producer:"test"}]),at]);
  const unsafe=run("db_library","backout",...args);
  check("backout refuses committed new writes and keeps their frozen input",unsafe.status!==0 && unsafe.stderr.includes("preserve them and recover forward")
    && (await get("SELECT request FROM entity_compute_specs WHERE id='committed-new-task'")).request==='{ "must": "survive" }');
  await db.unsafe("DROP FUNCTION factory_renew(text,text,text)");
  good("db_library","repair-runtime",...args);
  check("forward runtime repair restores a missing routine without replacing new data",(await get("SELECT to_regprocedure('factory_renew(text,text,text)') AS fn")).fn!==null
    && (await get("SELECT request FROM entity_compute_specs WHERE id='committed-new-task'")).request==='{ "must": "survive" }'
    && (await get("SELECT count(*) AS n FROM record_candidate_answers")).n==="5");
  good("db_library","release",...args);
  const released=run("db_library","backout",...args);
  check("release removes temporary journals and permanently closes old-layout backout",released.status!==0
    && good("db_library","status",...args).phase==="live"
    && (await get("SELECT count(*) AS n FROM pg_trigger WHERE tgname='migration_new_write'")).n==="0");
  complete=true;console.log(JSON.stringify({ok:true,checks,database}));
} finally {
  await db.end();
  if(complete)await admin.unsafe(`DROP DATABASE ${database}`);
  else console.error("preserved failed rehearsal database: "+database);
  await admin.end();
}
