// Resumable preparation keeps the live layout authoritative until a separately verified cutover.
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import postgres from "../pg/node_modules/postgres/src/index.js";

const here=new URL("./",import.meta.url), root=new URL("../../",here);
const manifest=JSON.parse(readFileSync(new URL("db-boundaries-2026-11-10.json",here),"utf8"));
const schema=readFileSync(new URL("worker/pg/schema.sql",root),"utf8");
const digest=createHash("sha256").update(readFileSync(new URL(import.meta.url)))
  .update(JSON.stringify(manifest)).update(schema).digest("hex");
const stage="wfsim_migrate_"+digest.slice(0,12);
const [command="status",...args]=process.argv.slice(2);
const option=(name,fallback)=>{
  const value=args.find(a=>a.startsWith("--"+name+"="))?.split("=")[1];
  const n=value===undefined ? fallback : Number(value);
  if (!Number.isSafeInteger(n) || n<1) throw new Error("invalid "+name);
  return n;
};
const batch=option("batch",512), maxBatches=option("batches",Number.MAX_SAFE_INTEGER);
const byteBudget=option("bytes",4*1024*1024);
if (batch>2048 || byteBudget>16*1024*1024) throw new Error("migration batch exceeds its hard budget");
const emit=value=>console.log(JSON.stringify(value));
const columns=["id","verb","request","state","due_at","waits","opened_at","lanes","lease","lease_until",
  "leased_to","leased_at","lease_prev","leased_to_prev","lease_prev_until","needs","canon","fact_engine",
  "fact_at","odd","producer","initiator","withdrawn_at","witnesses"];
const scheduling=columns.filter(c=>!["request","canon","witnesses"].includes(c));
const slice=(start,end)=>schema.slice(schema.indexOf(start),schema.indexOf(end));
const core=slice("CREATE TABLE IF NOT EXISTS entity_compute_specs (","CREATE TABLE IF NOT EXISTS relation_task_consumers (");
const reports=slice("CREATE TABLE IF NOT EXISTS entity_calculation_reports (","CREATE TABLE IF NOT EXISTS state_request_cancellation_tombstones (");
const evidence=slice("CREATE TABLE IF NOT EXISTS relation_task_answer_evidence (","CREATE OR REPLACE VIEW state_compute_candidate_answers AS")
  .replace("REFERENCES record_candidate_answers(id)","REFERENCES public.answers(id)");
const qualified=(alias,names)=>names.map(name=>alias+"."+name).join(",");
const identifier=name=>'"'+name.replaceAll('"','""')+'"';

if (command==="plan") {
  emit({id:manifest.id,digest,stage,commands:["prepare","copy","copy-evidence","check","refresh","status"],
    batch,byteBudget,activation_implemented:false,production_changes:false});
  process.exit(0);
}
const url=process.env.WFSIM_MIGRATION_URL;
if (!url) throw new Error("WFSIM_MIGRATION_URL is required");
const host=new URL(url).hostname;
if (!["localhost","127.0.0.1","[::1]"].includes(host)
    && process.env.WFSIM_MIGRATION_PRODUCTION!==manifest.id) throw new Error("production execution requires its explicit operation id");
const sql=postgres(url,{max:1,fetch_types:false,onnotice:()=>{},connection:{application_name:"wfsim-db-boundaries"}});
const query=(tx,text,values=[])=>tx.unsafe(text,values);
const inStage=async(tx)=>{
  await query(tx,`SET LOCAL search_path TO ${stage},public`);
  await query(tx,"SET LOCAL lock_timeout='3s'");
  await query(tx,"SET LOCAL statement_timeout='120s'");
};
const lock=async(tx)=>query(tx,"SELECT pg_advisory_xact_lock(hashtextextended('wfsim-db-boundaries',0))");
const progress=async(tx,id)=>{
  const [row]=await query(tx,`SELECT * FROM state_migration_progress WHERE id=$1 FOR UPDATE`,[id]);
  if (!row || row.digest!==digest) throw new Error("migration checkpoint does not match this implementation");
  return row;
};

async function prepare() {
  await sql.begin(async tx=>{
    await lock(tx);
    if (!(await query(tx,"SELECT to_regclass('public.questions') AS q,to_regclass('public.answers') AS a"))[0].q)
      throw new Error("expected the source public layout");
    const [existing]=await query(tx,"SELECT nspname FROM pg_namespace WHERE nspname LIKE 'wfsim_migrate_%' AND nspname<>$1",[stage]);
    if (existing && !args.includes("--separate-rehearsal")) throw new Error("another protected migration stage exists: "+existing.nspname);
    await query(tx,`CREATE SCHEMA IF NOT EXISTS ${stage}`);
    await query(tx,`REVOKE ALL ON SCHEMA ${stage} FROM PUBLIC`);
    await inStage(tx);
    await query(tx,`CREATE TABLE IF NOT EXISTS state_migration_progress(
      id text PRIMARY KEY,digest text NOT NULL,cursor text COLLATE "C",rows bigint NOT NULL DEFAULT 0,
      batches bigint NOT NULL DEFAULT 0,complete boolean NOT NULL DEFAULT false,updated_at timestamptz NOT NULL DEFAULT clock_timestamp());
      CREATE TABLE IF NOT EXISTS record_migration_changes(
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,source text NOT NULL,question text COLLATE "C" NOT NULL,
        operation text NOT NULL,transaction_id xid8 NOT NULL DEFAULT pg_current_xact_id(),at timestamptz NOT NULL DEFAULT clock_timestamp());
      CREATE INDEX IF NOT EXISTS migration_changes_by_question ON record_migration_changes(question);
      CREATE TABLE IF NOT EXISTS record_migration_checks(
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,kind text NOT NULL,cursor text COLLATE "C",
        rows bigint NOT NULL,dirty_keys bigint NOT NULL,at timestamptz NOT NULL DEFAULT clock_timestamp());`);
    for (const id of ["compute","evidence","audit"]) await query(tx,
      "INSERT INTO state_migration_progress(id,digest) VALUES($1,$2) ON CONFLICT(id) DO NOTHING",[id,digest]);
    await progress(tx,"compute");
    await query(tx,core+reports+evidence);
    await query(tx,"ALTER TABLE state_compute_scheduling DISABLE TRIGGER compute_withdrawn_at");
    await query(tx,`CREATE OR REPLACE FUNCTION ${stage}.capture_change() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog AS $$
      DECLARE before_key text; after_key text;
      BEGIN
        IF TG_TABLE_NAME='questions' THEN
          IF TG_OP<>'INSERT' THEN before_key:=OLD.id; END IF;
          IF TG_OP<>'DELETE' THEN after_key:=NEW.id; END IF;
        ELSE
          IF TG_OP<>'INSERT' THEN before_key:=OLD.question; END IF;
          IF TG_OP<>'DELETE' THEN after_key:=NEW.question; END IF;
        END IF;
        IF before_key IS NOT NULL THEN INSERT INTO ${stage}.record_migration_changes(source,question,operation) VALUES(TG_TABLE_NAME,before_key,TG_OP); END IF;
        IF after_key IS NOT NULL AND after_key IS DISTINCT FROM before_key THEN
          INSERT INTO ${stage}.record_migration_changes(source,question,operation) VALUES(TG_TABLE_NAME,after_key,TG_OP);
        END IF;
        RETURN NULL;
      END $$`);
    // Capture is append only: no added conflict on a shared dirty-key row in an application transaction.
    for (const table of ["questions","answers"]) {
      const name="migration_capture_"+digest.slice(0,12);
      const [trigger]=await query(tx,"SELECT tgname FROM pg_trigger WHERE tgrelid=$1::regclass AND tgname=$2",["public."+table,name]);
      if (!trigger) await query(tx,`CREATE TRIGGER ${name} AFTER INSERT OR UPDATE OR DELETE ON public.${table}
        FOR EACH ROW EXECUTE FUNCTION ${stage}.capture_change()`);
    }
  });
  emit({ok:true,command,digest,stage});
}

async function copyKeys(tx,ids) {
  const selected="id IN (SELECT jsonb_array_elements_text(($1::text)::jsonb))", values=[JSON.stringify(ids)];
  await query(tx,`CREATE TEMP TABLE migration_chunk ON COMMIT DROP AS SELECT * FROM public.questions WHERE ${selected}`,values);
  const [bad]=await query(tx,`SELECT id FROM migration_chunk WHERE request IS NULL OR verb IS NULL
    OR ((canon IS NULL)::int+(fact_engine IS NULL)::int+(fact_at IS NULL)::int) NOT IN (0,3) LIMIT 1`);
  if (bad) throw new Error("source contains incomplete input or result metadata: "+bad.id);
  await query(tx,`INSERT INTO entity_compute_specs(id,verb,request) SELECT id,verb,request FROM migration_chunk ON CONFLICT DO NOTHING`);
  const [mismatch]=await query(tx,`SELECT q.id FROM migration_chunk q JOIN entity_compute_specs d ON d.id=q.id
    WHERE (d.verb,d.request) IS DISTINCT FROM (q.verb,q.request) LIMIT 1`);
  if (mismatch) throw new Error("source changed an existing immutable input: "+mismatch.id);
  await query(tx,`INSERT INTO entity_compute_tasks(computation,generation,created_at,reason,origin)
    SELECT id,1,opened_at,'imported_current_state','imported' FROM migration_chunk ON CONFLICT(computation,generation) DO NOTHING`);
  await query(tx,`WITH inserted AS(INSERT INTO entity_compute_results(computation,task_id,canon,engine,accepted_at)
    SELECT q.id,t.id,q.canon,q.fact_engine,q.fact_at FROM migration_chunk q
      JOIN entity_compute_tasks t ON t.computation=q.id AND t.generation=1
    WHERE q.canon IS NOT NULL AND NOT EXISTS(SELECT 1 FROM entity_compute_results r
      JOIN record_compute_result_proofs p ON p.result_id=r.id WHERE r.computation=q.id
      AND (r.canon,r.engine,r.accepted_at,p.witnesses) IS NOT DISTINCT FROM (q.canon,q.fact_engine,q.fact_at,q.witnesses)) RETURNING id,computation)
    INSERT INTO record_compute_result_proofs(result_id,witnesses) SELECT r.id,q.witnesses FROM inserted r JOIN migration_chunk q ON q.id=r.computation`);
  await query(tx,`INSERT INTO record_compute_result_selections(computation,task_id,result_id,reason,at)
    SELECT r.computation,r.task_id,r.id,'imported_selection',r.accepted_at FROM entity_compute_results r
    WHERE r.computation IN(SELECT id FROM migration_chunk) AND NOT EXISTS(
      SELECT 1 FROM record_compute_result_selections h WHERE h.computation=r.computation AND h.result_id=r.id)`);
  const stateColumns=[...scheduling,"task_id","result_id"];
  await query(tx,`INSERT INTO state_compute_scheduling(${stateColumns.join(",")})
    SELECT ${qualified("q",scheduling)},t.id,r.id FROM migration_chunk q
      JOIN entity_compute_tasks t ON t.computation=q.id AND t.generation=1
      LEFT JOIN entity_compute_results r ON r.computation=q.id AND
        (r.canon,r.engine,r.accepted_at) IS NOT DISTINCT FROM (q.canon,q.fact_engine,q.fact_at)
        AND EXISTS(SELECT 1 FROM record_compute_result_proofs p WHERE p.result_id=r.id AND p.witnesses=q.witnesses)
    ON CONFLICT(id) DO UPDATE SET ${stateColumns.filter(c=>c!=="id").map(c=>c+"=excluded."+c).join(",")}`);
  await query(tx,`DELETE FROM state_compute_scheduling WHERE ${selected} AND id NOT IN(SELECT id FROM migration_chunk)`,values);
}

async function copy() {
  for (let n=0;n<maxBatches;n++) {
    const result=await sql.begin("isolation level repeatable read",async tx=>{
      await lock(tx); await inStage(tx); const p=await progress(tx,"compute");
      if (p.complete) return {complete:true,rows:p.rows};
      const ids=await query(tx,`WITH candidates AS MATERIALIZED(SELECT id,
          octet_length(request)+COALESCE(octet_length(canon),0)+octet_length(witnesses)+512 AS bytes
          FROM public.questions WHERE ($1::text IS NULL OR id>$1) ORDER BY id LIMIT $2),
        budget AS(SELECT *,sum(bytes) OVER(ORDER BY id) AS accumulated,row_number() OVER(ORDER BY id) AS ordinal FROM candidates)
        SELECT id,bytes FROM budget WHERE accumulated<=$3 OR ordinal=1 ORDER BY id`,[p.cursor,batch,byteBudget]);
      if (!ids.length) {await query(tx,"UPDATE state_migration_progress SET complete=true,updated_at=clock_timestamp() WHERE id='compute'");return {complete:true,rows:p.rows};}
      if (Number(ids[0].bytes)>16*1024*1024) throw new Error("one source row exceeds the migration byte limit: "+ids[0].id);
      await copyKeys(tx,ids.map(row=>row.id));
      const cursor=ids.at(-1).id;
      await query(tx,"UPDATE state_migration_progress SET cursor=$1,rows=rows+$2,batches=batches+1,updated_at=clock_timestamp() WHERE id='compute'",[cursor,ids.length]);
      return {complete:false,cursor,rows:String(BigInt(p.rows)+BigInt(ids.length)),bytes:ids.reduce((sum,row)=>sum+Number(row.bytes),0)};
    });
    emit({ok:true,command,...result}); if (result.complete) break;
  }
}

async function copyEvidence() {
  for(let n=0;n<maxBatches;n++) {
    const result=await sql.begin("isolation level repeatable read",async tx=>{
      await lock(tx);await inStage(tx);const p=await progress(tx,"evidence");
      if(p.complete) return {complete:true,rows:p.rows};
      const ids=await query(tx,"SELECT id FROM public.answers WHERE id>COALESCE($1::bigint,0) ORDER BY id LIMIT $2",[p.cursor,batch]);
      if(!ids.length) {await query(tx,"UPDATE state_migration_progress SET complete=true,updated_at=clock_timestamp() WHERE id='evidence'");return {complete:true,rows:p.rows};}
      await query(tx,`INSERT INTO relation_task_answer_evidence(task_id,answer_id,role)
        SELECT t.id,a.id,'imported_current_state' FROM public.answers a JOIN entity_compute_tasks t ON t.computation=a.question AND t.generation=1
        WHERE a.id>COALESCE($1::bigint,0) AND a.id<=$2::bigint ON CONFLICT DO NOTHING`,[p.cursor,ids.at(-1).id]);
      await query(tx,"UPDATE state_migration_progress SET cursor=$1,rows=rows+$2,batches=batches+1,updated_at=clock_timestamp() WHERE id='evidence'",[String(ids.at(-1).id),ids.length]);
      return {complete:false,cursor:String(ids.at(-1).id),rows:String(BigInt(p.rows)+BigInt(ids.length))};
    });emit({ok:true,command,...result});if(result.complete)break;
  }
}

async function check() {
  for(let n=0;n<maxBatches;n++) {
    const result=await sql.begin("isolation level repeatable read",async tx=>{
      await lock(tx);await inStage(tx);const p=await progress(tx,"audit");
      if (!(await progress(tx,"compute")).complete) throw new Error("finish the initial compute copy before its independent audit");
      if(p.complete)return {complete:true,rows:p.rows};
      const ids=await query(tx,"SELECT id FROM public.questions WHERE ($1::text IS NULL OR id>$1) ORDER BY id LIMIT $2",[p.cursor,batch]);
      if(!ids.length){await query(tx,"UPDATE state_migration_progress SET complete=true,updated_at=clock_timestamp() WHERE id='audit'");return {complete:true,rows:p.rows};}
      const keys=[JSON.stringify(ids.map(row=>row.id))];
      const [bad]=await query(tx,`SELECT q.id FROM public.questions q LEFT JOIN state_compute_work v ON v.id=q.id
        WHERE q.id IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))
          AND NOT EXISTS(SELECT 1 FROM record_migration_changes j WHERE j.question=q.id)
          AND (v.id IS NULL OR ROW(${qualified("q",columns)}) IS DISTINCT FROM ROW(${qualified("v",columns)})) LIMIT 1`,keys);
      if(bad)throw new Error("independent field audit failed: "+bad.id);
      const [{n:dirty}]=await query(tx,`SELECT count(DISTINCT question) AS n FROM record_migration_changes
        WHERE question IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))`,keys);
      await query(tx,"INSERT INTO record_migration_checks(kind,cursor,rows,dirty_keys) VALUES('stable_key_content',$1,$2,$3)",[ids.at(-1).id,ids.length,dirty]);
      await query(tx,"UPDATE state_migration_progress SET cursor=$1,rows=rows+$2,batches=batches+1,updated_at=clock_timestamp() WHERE id='audit'",[ids.at(-1).id,ids.length]);
      return {complete:false,cursor:ids.at(-1).id,rows:String(BigInt(p.rows)+BigInt(ids.length)),deferred_dirty_keys:dirty};
    });emit({ok:true,command,...result});if(result.complete)break;
  }
}

async function refresh() {
  // This cursor is a key, never the journal's allocation sequence: late commits cannot hide behind it.
  let cursor=null;
  for(let n=0;n<maxBatches;n++) {
    const result=await sql.begin("isolation level repeatable read",async tx=>{
      await lock(tx);await inStage(tx);await progress(tx,"compute");
      const ids=await query(tx,"SELECT DISTINCT question FROM record_migration_changes WHERE ($1::text IS NULL OR question>$1) ORDER BY question LIMIT $2",[cursor,batch]);
      if(!ids.length)return {complete:true};
      await copyKeys(tx,ids.map(row=>row.question));
      await query(tx,`INSERT INTO relation_task_answer_evidence(task_id,answer_id,role)
        SELECT t.id,a.id,'imported_current_state' FROM public.answers a JOIN entity_compute_tasks t ON t.computation=a.question AND t.generation=1
        WHERE a.question IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ON CONFLICT DO NOTHING`,[JSON.stringify(ids.map(row=>row.question))]);
      return {complete:false,cursor:ids.at(-1).question,keys:ids.length};
    });emit({ok:true,command,...result});if(result.complete)break;cursor=result.cursor;
  }
}

try {
  if(command==="prepare")await prepare();
  else if(command==="copy")await copy();
  else if(command==="copy-evidence")await copyEvidence();
  else if(command==="check")await check();
  else if(command==="refresh")await refresh();
  else if(command==="status") {
    emit({digest,stage,progress:await sql.unsafe(`SELECT * FROM ${stage}.state_migration_progress ORDER BY id`)});
  } else throw new Error("unknown migration command: "+command);
} finally {await sql.end();}
