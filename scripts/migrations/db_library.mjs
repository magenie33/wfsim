// Activate separated library storage under a verified fence; committed application writes close layout reversal.
import {readFileSync} from "node:fs";
import {createHash} from "node:crypto";
import postgres from "../pg/node_modules/postgres/src/index.js";
import {migrationEndpoint,requireAdministrator,requireDatabaseFence} from "./operation.mjs";

const command=process.argv[2] || "status",url=process.env.WFSIM_MIGRATION_URL;
const mode=migrationEndpoint("library",url);
const stage=process.argv.find(arg=>arg.startsWith("--stage="))?.slice(8);
if(!/^wfsim_migrate_[a-f0-9]{12}$/.test(stage || ""))throw new Error("an exact prepared stage is required");
const root=new URL("../../",import.meta.url);
const manifest=JSON.parse(readFileSync(new URL("db-boundaries-2026-11-10.json",import.meta.url),"utf8"));
const mapping=manifest.databases.find(database=>database.name==="wfsim").tables;
const files=["schema","library","sync","inventory","build_workflows"];
const ddl=files.map(name=>readFileSync(new URL("worker/pg/"+name+".sql",root),"utf8"));
const signatures=new Set(ddl.flatMap(source=>[...source.matchAll(/CREATE OR REPLACE (?:FUNCTION|PROCEDURE) ([a-z_]+)\(([^)]*)\)/g)]
  .map(match=>match[1]+"("+match[2].split(",").map(arg=>arg.trim().replace(/\s+DEFAULT\s+.*$/i,"")
    .replace(/^(?:INOUT|IN|OUT)\s+/i,"").replace(/^[a-z_]+\s+(jsonb|text|bigint|boolean)$/, "$1").replace(/\s+/g," ")).join(",")+")")));
const digest=createHash("sha256").update(readFileSync(new URL(import.meta.url))).update(JSON.stringify(mapping))
  .update(ddl.join("\n")).update(readFileSync(new URL("operation.mjs",import.meta.url))).digest("hex");
const quote=value=>'"'+value.replaceAll('"','""')+'"';
const emit=value=>console.log(JSON.stringify({command,digest,stage,rehearsal_only:mode.local_only,production_changes:mode.production && command!=="status",...value}));
const sql=postgres(url,{max:1,fetch_types:false,onnotice:()=>{},connection:{application_name:"wfsim-library-layout-rehearsal"}});
const failure=Number(process.argv.find(arg=>arg.startsWith("--fail-after="))?.slice(13) || 0);
const pauseAfter=Number(process.argv.find(arg=>arg.startsWith("--pause-after="))?.slice(14) || 0);
const transformed=new Set(["questions","calculation_reports"]);
const core=["entity_compute_specs","entity_compute_tasks","entity_compute_results","record_compute_result_proofs",
  "record_compute_result_selections","state_compute_scheduling","relation_task_answer_evidence",
  "entity_calculation_reports","state_calculation_report_selection","relation_calculation_report_access",
  "relation_compute_task_sources","record_compute_queue_removals","record_compute_identity_changes",
  "entity_build_workflow_scopes","entity_build_workflows","entity_build_workflow_expansions",
  "state_build_workflows","state_build_workflow_nodes","record_build_workflow_triggers"];
const views=["state_compute_work","state_current_calculation_reports","state_build_inbox_inputs"];
const sequences=["compute_queue_position"];
const legacy=name=>"record_migration_legacy_"+name;
const names=active=>mapping.map(row=>active ? (transformed.has(row.source) ? legacy(row.source) : row.target) : row.source);
const sourceRelation=(item,active)=>active && transformed.has(item.source)
  ? "wfsim_migration_ops."+quote(legacy(item.source)) : "public."+quote(active ? item.target : item.source);
const query=(tx,text,params=[])=>tx.unsafe(text,params);
const setup=async tx=>{
  await query(tx,"SET TRANSACTION READ WRITE");
  await query(tx,"SET LOCAL lock_timeout='3s';SET LOCAL statement_timeout='15min';SET LOCAL work_mem='16MB';SET LOCAL temp_file_limit='256MB'");
  await query(tx,"SELECT pg_advisory_xact_lock(hashtextextended('wfsim-db-boundaries',0))");
};
const lockTables=async(tx,active)=>query(tx,"LOCK TABLE "+mapping.map(item=>sourceRelation(item,active)).join(",")+" IN ACCESS EXCLUSIVE MODE");
const catalog=async(tx,tableNames)=>[...await query(tx,`SELECT c.oid::text AS oid,c.relname AS name,c.relfilenode::text AS storage,
  c.relowner::text AS owner,c.relacl::text AS acl FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
  WHERE n.nspname IN('public','wfsim_migration_ops') AND c.relkind='r' AND c.relname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ORDER BY c.oid`,[JSON.stringify(tableNames)])];
const functions=async tx=>[...await query(tx,`SELECT p.oid::regprocedure::text AS signature,p.proname AS name,
  pg_get_functiondef(p.oid) AS definition,pg_get_userbyid(p.proowner) AS owner,p.proacl::text AS acl
  FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND p.prokind IN('f','p')
    AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e')
  ORDER BY p.oid`)];
const triggers=async(tx,namespace="public")=>[...await query(tx,`SELECT c.relname AS table_name,t.tgname AS name,t.tgenabled AS enabled,
  pg_get_triggerdef(t.oid) AS definition FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid
  JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND NOT t.tgisinternal ORDER BY c.oid,t.oid`,[namespace])];
const row=async tx=>(await query(tx,"SELECT * FROM wfsim_migration_ops.state_library_layout WHERE id=$1 FOR UPDATE",[manifest.id]))[0];
const phase=async(tx,value)=>query(tx,"UPDATE wfsim_migration_ops.state_library_layout SET phase=$2,at=clock_timestamp() WHERE id=$1",[manifest.id,value]);
const prepared=async tx=>{
  const p=await row(tx);
  if(!p || p.digest!==digest || p.stage!==stage)throw new Error("library checkpoint does not match this tool and stage");
  return p;
};
const sourceColumns=["id","verb","request","state","due_at","waits","opened_at","lanes","lease","lease_until",
  "leased_to","leased_at","lease_prev","leased_to_prev","lease_prev_until","needs","canon","fact_engine",
  "fact_at","odd","producer","initiator","withdrawn_at","witnesses"];
const stateColumns=sourceColumns.filter(name=>!["request","canon","witnesses"].includes(name));
const qualified=(alias,columns)=>columns.map(name=>alias+"."+name).join(",");

async function prepare() {
  await sql.begin(async tx=>{
    await setup(tx);
    const progress=await query(tx,`SELECT id,complete,digest FROM ${stage}.state_migration_progress`);
    if(!["compute","evidence","audit"].every(id=>progress.some(p=>p.id===id && p.complete))
        || !progress.some(p=>p.id.startsWith("evidence_audit_") && p.complete))
      throw new Error("complete both copies and both independent audits before layout preparation");
    await query(tx,`CREATE SCHEMA IF NOT EXISTS wfsim_migration_ops;REVOKE ALL ON SCHEMA wfsim_migration_ops FROM PUBLIC;
      CREATE TABLE IF NOT EXISTS wfsim_migration_ops.state_library_layout(id text PRIMARY KEY,digest text NOT NULL,
        stage text NOT NULL,phase text NOT NULL,baseline jsonb NOT NULL,at timestamptz NOT NULL DEFAULT clock_timestamp());
      CREATE TABLE IF NOT EXISTS wfsim_migration_ops.record_library_writes(
        id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,relation_name text NOT NULL,operation text NOT NULL,
        transaction_id xid8 NOT NULL DEFAULT pg_current_xact_id(),at timestamptz NOT NULL DEFAULT clock_timestamp());`);
    const [existing]=await query(tx,"SELECT * FROM wfsim_migration_ops.state_library_layout WHERE id=$1",[manifest.id]);
    if(existing){await prepared(tx);return;}
    await query(tx,"LOCK TABLE public.answers IN ACCESS EXCLUSIVE MODE");
    const tables=await catalog(tx,names(false));
    if(tables.length!==mapping.length)throw new Error("source application inventory is incomplete");
    const indexes=await query(tx,"SELECT pg_get_indexdef('public.answers_once'::regclass) AS definition");
    const stageOwnership=[...await query(tx,`SELECT c.relname AS name,pg_get_userbyid(c.relowner) AS owner
      FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1
        AND c.relname IN(SELECT jsonb_array_elements_text(($2::text)::jsonb))`,[stage,JSON.stringify([...core,...views])])];
    const baseline={tables,functions:await functions(tx),triggers:await triggers(tx),stage_triggers:await triggers(tx,stage),stage_ownership:stageOwnership,answer_index:indexes[0].definition};
    await query(tx,"INSERT INTO wfsim_migration_ops.state_library_layout(id,digest,stage,phase,baseline) VALUES($1,$2,$3,'prepared',($4::text)::jsonb)",
      [manifest.id,digest,stage,JSON.stringify(baseline)]);
    await query(tx,`ALTER TABLE public.answers ADD COLUMN task_id bigint;
      ALTER TABLE public.answers ADD COLUMN origin text COLLATE "C" NOT NULL DEFAULT 'imported';
      ALTER TABLE public.answers ADD CONSTRAINT migration_candidate_task FOREIGN KEY(task_id) REFERENCES ${stage}.entity_compute_tasks(id) NOT VALID`);
  });
  // Existing rows have NULL task_id; the old index continues to guard admission until activation.
  await query(sql,"SET default_transaction_read_only='off'");
  const [index]=await query(sql,"SELECT indisvalid FROM pg_index WHERE indexrelid=to_regclass('public.migration_answers_task_once')");
  if(index && !index.indisvalid)await query(sql,"DROP INDEX CONCURRENTLY public.migration_answers_task_once");
  await query(sql,"CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS migration_answers_task_once ON public.answers(task_id,device,engine) WHERE official=0");
  await query(sql,"ALTER TABLE public.answers VALIDATE CONSTRAINT migration_candidate_task");
  for(const name of ["questions_coded","questions_by_producer"]){
    const [existing]=await query(sql,"SELECT indisvalid FROM pg_index WHERE indexrelid=to_regclass($1)",[stage+"."+name]);
    if(existing && !existing.indisvalid)await query(sql,`DROP INDEX CONCURRENTLY ${stage}.${quote(name)}`);
    const source=ddl[0].match(new RegExp("CREATE INDEX IF NOT EXISTS "+name+"[^;]+;"))?.[0];
    if(!source)throw new Error("missing canonical scheduling index: "+name);
    await query(sql,source.replace("CREATE INDEX ","CREATE INDEX CONCURRENTLY ").replace("ON state_compute_scheduling","ON "+stage+".state_compute_scheduling"));
  }
  emit({ok:true,phase:"prepared"});
}

async function reconcileChunk(tx,ids) {
  const keys=[JSON.stringify(ids)];
  await query(tx,"CREATE TEMP TABLE migration_final_chunk ON COMMIT DROP AS SELECT * FROM public.questions WHERE id IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))",keys);
  await query(tx,"INSERT INTO entity_compute_specs(id,verb,request) SELECT id,verb,request FROM migration_final_chunk ON CONFLICT DO NOTHING");
  const [conflict]=await query(tx,"SELECT q.id FROM migration_final_chunk q JOIN entity_compute_specs d ON d.id=q.id WHERE (q.verb,q.request) IS DISTINCT FROM(d.verb,d.request) LIMIT 1");
  if(conflict)throw new Error("frozen input differs: "+conflict.id);
  await query(tx,`INSERT INTO entity_compute_tasks(computation,generation,created_at,reason,origin)
    SELECT q.id,COALESCE(t.generation,0)+1,q.opened_at,'imported_current_state','imported' FROM migration_final_chunk q
      LEFT JOIN LATERAL(SELECT generation,created_at FROM entity_compute_tasks t WHERE t.computation=q.id ORDER BY generation DESC LIMIT 1)t ON true
    WHERE t.generation IS NULL OR t.created_at IS DISTINCT FROM q.opened_at;
    WITH inserted AS(INSERT INTO entity_compute_results(computation,task_id,canon,engine,accepted_at)
      SELECT q.id,t.id,q.canon,q.fact_engine,q.fact_at FROM migration_final_chunk q
      JOIN LATERAL(SELECT id FROM entity_compute_tasks t WHERE t.computation=q.id ORDER BY generation DESC LIMIT 1)t ON true
      WHERE q.canon IS NOT NULL AND NOT EXISTS(SELECT 1 FROM entity_compute_results r JOIN record_compute_result_proofs p ON p.result_id=r.id
        WHERE r.computation=q.id AND (r.canon,r.engine,r.accepted_at,p.witnesses) IS NOT DISTINCT FROM(q.canon,q.fact_engine,q.fact_at,q.witnesses)) RETURNING id,computation)
      INSERT INTO record_compute_result_proofs(result_id,witnesses) SELECT r.id,q.witnesses FROM inserted r JOIN migration_final_chunk q ON q.id=r.computation;
    INSERT INTO record_compute_result_selections(computation,task_id,result_id,reason,at)
      SELECT r.computation,r.task_id,r.id,'imported_selection',r.accepted_at FROM entity_compute_results r
      WHERE r.computation IN(SELECT id FROM migration_final_chunk) AND NOT EXISTS(SELECT 1 FROM record_compute_result_selections h
        WHERE h.computation=r.computation AND h.result_id=r.id)`);
  await query(tx,`INSERT INTO state_compute_scheduling(${stateColumns.join(",")},task_id,result_id)
    SELECT ${qualified("q",stateColumns)},t.id,r.id FROM migration_final_chunk q
      JOIN LATERAL(SELECT id FROM entity_compute_tasks t WHERE t.computation=q.id ORDER BY generation DESC LIMIT 1)t ON true
      LEFT JOIN entity_compute_results r ON r.computation=q.id AND (r.canon,r.engine,r.accepted_at) IS NOT DISTINCT FROM(q.canon,q.fact_engine,q.fact_at)
        AND EXISTS(SELECT 1 FROM record_compute_result_proofs p WHERE p.result_id=r.id AND p.witnesses=q.witnesses)
    ON CONFLICT(id) DO UPDATE SET ${[...stateColumns,"task_id","result_id"].filter(name=>name!=="id").map(name=>name+"=excluded."+name).join(",")}`);
  await query(tx,"DELETE FROM state_compute_scheduling WHERE id IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) AND id NOT IN(SELECT id FROM migration_final_chunk)",keys);
  for(const scope of ["t.computation","a.question"])await query(tx,`DELETE FROM relation_task_answer_evidence e USING entity_compute_tasks t,public.answers a
    WHERE e.task_id=t.id AND e.answer_id=a.id AND t.computation IS DISTINCT FROM a.question
      AND ${scope} IN(SELECT jsonb_array_elements_text(($1::text)::jsonb))`,keys);
  await query(tx,`INSERT INTO relation_task_answer_evidence(task_id,answer_id,role)
    SELECT s.task_id,a.id,'imported_current_state' FROM public.answers a JOIN state_compute_scheduling s ON s.id=a.question
    WHERE a.question IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ON CONFLICT DO NOTHING`,keys);
  const [bad]=await query(tx,`SELECT k.id FROM jsonb_array_elements_text(($1::text)::jsonb) k(id)
    LEFT JOIN public.questions q ON q.id=k.id LEFT JOIN state_compute_work v ON v.id=k.id
    WHERE (q.id IS NULL AND v.id IS NOT NULL) OR (q.id IS NOT NULL AND (v.id IS NULL OR
      ROW(${qualified("q",sourceColumns)}) IS DISTINCT FROM ROW(${qualified("v",sourceColumns)}))) LIMIT 1`,keys);
  if(bad)throw new Error("final dirty-key field audit differs: "+bad.id);
  const [badEvidence]=await query(tx,`SELECT a.id FROM public.answers a JOIN state_compute_scheduling s ON s.id=a.question
    LEFT JOIN relation_task_answer_evidence e ON e.task_id=s.task_id AND e.answer_id=a.id
    WHERE a.question IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) AND e.answer_id IS NULL LIMIT 1`,keys);
  if(badEvidence)throw new Error("final dirty-key evidence is missing: "+badEvidence.id);
  for(const scope of ["t.computation","a.question"]){
    const [misplaced]=await query(tx,`SELECT e.answer_id FROM relation_task_answer_evidence e JOIN entity_compute_tasks t ON t.id=e.task_id
      JOIN public.answers a ON a.id=e.answer_id WHERE t.computation IS DISTINCT FROM a.question
        AND ${scope} IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) LIMIT 1`,keys);
    if(misplaced)throw new Error("final dirty-key evidence belongs to another input: "+misplaced.answer_id);
  }
  await query(tx,"DROP TABLE migration_final_chunk");
}

async function importReports(tx) {
  await query(tx,`CREATE TABLE IF NOT EXISTS relation_migration_report_imports(question text COLLATE "C",engine text COLLATE "C",
    report_id bigint NOT NULL REFERENCES entity_calculation_reports(id),PRIMARY KEY(question,engine));
    WITH inserted AS(INSERT INTO entity_calculation_reports(question,engine,at,origin,body)
      SELECT r.question,r.engine,r.at,'imported',r.result FROM public.calculation_reports r
      WHERE NOT EXISTS(SELECT 1 FROM relation_migration_report_imports m WHERE m.question=r.question AND m.engine=r.engine)
      RETURNING id,question,engine)
      INSERT INTO relation_migration_report_imports SELECT question,engine,id FROM inserted;
    INSERT INTO entity_calculation_reports(result_id,question,engine,at)
      SELECT o.id,r.question,r.engine,r.at FROM public.calculation_reports r JOIN state_compute_scheduling s ON s.id=r.question
      JOIN entity_compute_results o ON o.id=s.result_id AND o.engine=r.engine AND o.accepted_at=r.at
      WHERE o.canon::jsonb->'report'=r.result::jsonb ON CONFLICT(result_id) DO NOTHING;
    INSERT INTO state_calculation_report_selection(question,engine,report_id,at,retained_from)
      SELECT r.question,r.engine,COALESCE(p.id,m.report_id),r.at,LEAST(r.at,COALESCE((SELECT min(u.completed_at)
        FROM public.user_calculations u WHERE u.question=r.question AND u.engine=r.engine AND u.state='succeeded'),r.at)) FROM public.calculation_reports r
      JOIN relation_migration_report_imports m USING(question,engine)
      LEFT JOIN state_compute_scheduling s ON s.id=r.question LEFT JOIN entity_calculation_reports p
        ON p.result_id=s.result_id AND p.engine=r.engine AND p.at=r.at
      ON CONFLICT(question,engine) DO UPDATE SET report_id=excluded.report_id,at=excluded.at;
    INSERT INTO relation_calculation_report_access(id,owner,question,engine,task,kind,created_at,completed_at,expires_at)
      SELECT id,owner,question,engine,task,kind,created_at,completed_at,expires_at FROM public.user_calculations
      WHERE state='succeeded' AND completed_at IS NOT NULL ON CONFLICT(id) DO NOTHING`);
  const [raw]=await query(tx,`SELECT r.question FROM public.calculation_reports r LEFT JOIN relation_migration_report_imports m USING(question,engine)
    LEFT JOIN entity_calculation_reports p ON p.id=m.report_id
    WHERE p.id IS NULL OR (p.question,p.engine,p.at,p.body) IS DISTINCT FROM(r.question,r.engine,r.at,r.result) LIMIT 1`);
  if(raw)throw new Error("original report bytes or clocks differ: "+raw.question);
  const [access]=await query(tx,`SELECT u.id FROM public.user_calculations u LEFT JOIN relation_calculation_report_access a ON a.id=u.id
    WHERE u.state='succeeded' AND u.completed_at IS NOT NULL AND (a.id IS NULL OR
      (a.owner,a.question,a.engine,a.task,a.kind,a.created_at,a.completed_at,a.expires_at)
      IS DISTINCT FROM(u.owner,u.question,u.engine,u.task,u.kind,u.created_at,u.completed_at,u.expires_at)) LIMIT 1`);
  if(access)throw new Error("original report access or clocks differ: "+access.id);
  return (await query(tx,"SELECT count(*) AS imported_raw_reports FROM relation_migration_report_imports"))[0];
}

async function reconcile() {
  const result=await sql.begin(async tx=>{
    await setup(tx);const p=await prepared(tx);
    if(!["prepared","reconciled"].includes(p.phase))throw new Error("reconcile only the source layout");
    await lockTables(tx,false);await query(tx,`SET LOCAL search_path TO ${stage},public`);
    let cursor=null,keys=0;
    for(;;){
      const batch=await query(tx,"SELECT DISTINCT question FROM record_migration_changes WHERE $1::text IS NULL OR question>$1 ORDER BY question LIMIT 512",[cursor]);
      if(!batch.length)break;
      await reconcileChunk(tx,batch.map(row=>row.question));keys+=batch.length;cursor=batch.at(-1).question;
    }
    const reports=await importReports(tx);
    const [journal]=await query(tx,"SELECT count(*)::text AS rows,COALESCE(max(id),0)::text AS last_id FROM record_migration_changes");
    await query(tx,"UPDATE wfsim_migration_ops.state_library_layout SET baseline=baseline || jsonb_build_object('final_journal',($2::text)::jsonb) WHERE id=$1",[manifest.id,JSON.stringify(journal)]);
    await phase(tx,"reconciled");return {dirty_keys_checked:keys,...reports,journal};
  });emit({ok:true,phase:"reconciled",...result});
}

async function activate() {
  const result=await sql.begin(async tx=>{
    await setup(tx);const p=await prepared(tx);
    if(p.phase!=="reconciled")throw new Error("complete final reconciliation before activation");
    await lockTables(tx,false);
    const [journal]=await query(tx,`SELECT count(*)::text AS rows,COALESCE(max(id),0)::text AS last_id FROM ${stage}.record_migration_changes`);
    if(["rows","last_id"].some(key=>journal[key]!==p.baseline.final_journal[key]))throw new Error("source changed after final reconciliation");
    const [retention]=await query(tx,`SELECT id FROM public.user_calculations WHERE kind IN('optimization','appraisal')
      AND expires_at>to_char(((CASE WHEN state='succeeded' THEN COALESCE(completed_at,created_at) ELSE created_at END)::timestamptz
        + interval '14 days') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') LIMIT 1`);
    if(retention)throw new Error("schema application would rewrite a source retention clock: "+retention.id);
    const [unknownRound]=await query(tx,`SELECT r.id FROM ${stage}.entity_compute_results r
      CROSS JOIN LATERAL(SELECT created_at FROM ${stage}.entity_compute_tasks t WHERE t.id=r.task_id OFFSET 0)t
      WHERE r.accepted_at<t.created_at LIMIT 1`);
    if(unknownRound)throw new Error("accepted output predates its known imported task; preserve it and resolve its provenance before cutover: "+unknownRound.id);
    let count=0;
    for(const item of mapping){
      const target=transformed.has(item.source) ? legacy(item.source) : item.target;
      await query(tx,`ALTER TABLE public.${quote(item.source)} RENAME TO ${quote(target)}`);
      if(transformed.has(item.source))await query(tx,`ALTER TABLE public.${quote(target)} SET SCHEMA wfsim_migration_ops`);
      if(++count===failure)throw new Error("injected activation interruption after "+count+" metadata renames");
      if(count===pauseAfter)await query(tx,"SELECT pg_sleep(30)");
    }
    for(const name of [...core,...views,...sequences]){
      const [relation]=await query(tx,"SELECT to_regclass($1) AS id",[stage+"."+name]);
      if(relation.id)await query(tx,`ALTER ${sequences.includes(name) ? "SEQUENCE" : views.includes(name) ? "VIEW" : "TABLE"} ${stage}.${quote(name)} SET SCHEMA public`);
    }
    const [zoneFunction]=await query(tx,"SELECT to_regprocedure($1) AS id",[stage+".factory_default_zone(text)"]);
    if(zoneFunction.id)await query(tx,`ALTER FUNCTION ${stage}.factory_default_zone(text) SET SCHEMA public`);
    const moved=[];
    for(const fn of p.baseline.functions){
      const signature=fn.signature.replace(/^public\./,"");
      if(!signatures.has(signature) || ["factory_eligible","factory_held"].includes(fn.name)){
        await query(tx,`ALTER ROUTINE public.${signature} SET SCHEMA wfsim_migration_ops`);moved.push(signature);
      }
    }
    await query(tx,"UPDATE wfsim_migration_ops.state_library_layout SET baseline=baseline || jsonb_build_object('moved_functions',($2::text)::jsonb) WHERE id=$1",[manifest.id,JSON.stringify(moved)]);
    await query(tx,`DROP INDEX public.answers_once;ALTER INDEX public.migration_answers_task_once RENAME TO answers_once;
      ALTER TABLE public.record_candidate_answers ALTER COLUMN origin SET DEFAULT 'native'`);
    for(const item of p.baseline.triggers.filter(trigger=>trigger.name.startsWith("migration_capture_"))){
      const table=mapping.find(table=>table.source===item.table_name);
      await query(tx,`DROP TRIGGER ${quote(item.name)} ON ${sourceRelation(table,true)}`);
    }
    for(const source of ddl)await query(tx,source);
    const owner=p.baseline.tables.find(table=>table.name==="questions").owner;
    const [{name:ownerName}]=await query(tx,"SELECT rolname AS name FROM pg_roles WHERE oid=$1::oid",[owner]);
    for(const name of [...core,...views,...sequences,"state_compute_candidate_answers"])await query(tx,`ALTER ${sequences.includes(name) ? "SEQUENCE" : [...views,"state_compute_candidate_answers"].includes(name) ? "VIEW" : "TABLE"} public.${quote(name)} OWNER TO ${quote(ownerName)}`);
    const oldFunctions=new Set(p.baseline.functions.map(fn=>fn.signature));
    for(const fn of await functions(tx))if(!oldFunctions.has(fn.signature))
      await query(tx,`ALTER ROUTINE public.${fn.signature.replace(/^public\./,"")} OWNER TO ${quote(ownerName)}`);
    await query(tx,`CREATE OR REPLACE FUNCTION wfsim_migration_ops.capture_library_write() RETURNS trigger LANGUAGE plpgsql
      SECURITY DEFINER SET search_path=pg_catalog AS $$BEGIN
        INSERT INTO wfsim_migration_ops.record_library_writes(relation_name,operation) VALUES(TG_TABLE_NAME,TG_OP);RETURN NULL;
      END $$;REVOKE ALL ON FUNCTION wfsim_migration_ops.capture_library_write() FROM PUBLIC`);
    const application=[...mapping.filter(table=>!transformed.has(table.source)).map(table=>table.target),...core];
    for(const name of application)await query(tx,`CREATE TRIGGER migration_new_write AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE
      ON public.${quote(name)} FOR EACH STATEMENT EXECUTE FUNCTION wfsim_migration_ops.capture_library_write()`);
    const after=await catalog(tx,names(true));
    for(const before of p.baseline.tables){
      const table=after.find(table=>table.oid===before.oid);
      if(!table || ["storage","owner"].some(key=>table[key]!==before[key]))throw new Error("source object identity/privilege changed: "+before.name);
      // NULL ACL and an explicit owner-only ACL grant the same rights after REVOKE FROM PUBLIC.
      const [permissions]=await query(tx,`SELECT
        (SELECT jsonb_agg(to_jsonb(a) ORDER BY a.grantor,a.grantee,a.privilege_type,a.is_grantable)
          FROM aclexplode(COALESCE($1::aclitem[],acldefault('r',$3::oid)))a)
        IS NOT DISTINCT FROM
        (SELECT jsonb_agg(to_jsonb(a) ORDER BY a.grantor,a.grantee,a.privilege_type,a.is_grantable)
          FROM aclexplode(COALESCE($2::aclitem[],acldefault('r',$3::oid)))a) AS same`,[before.acl,table.acl,before.owner]);
      if(!permissions.same)throw new Error("source object identity/privilege changed: "+before.name);
    }
    const [daemon]=await query(tx,"SELECT rolname FROM pg_roles WHERE rolname='wfsim_factoryd'");
    if(daemon){
      const grants=readFileSync(new URL("deploy/factoryd_grants.sql",root),"utf8");
      for(const match of grants.matchAll(/GRANT (?!CONNECT)[^;]+;/g))await query(tx,match[0]);
    }
    await phase(tx,"activated");return {source_tables:after.length,new_core_tables:core.length,metadata_unchanged:true};
  });emit({ok:true,phase:"activated",...result});
}

async function repairRuntime() {
  await sql.begin(async tx=>{
    await setup(tx);const p=await prepared(tx);
    if(!["activated","live"].includes(p.phase))throw new Error("runtime repair requires the activated layout");
    await lockTables(tx,true);
    for(const name of core)await query(tx,`LOCK TABLE public.${quote(name)} IN ACCESS EXCLUSIVE MODE`);
    const [retention]=await query(tx,`SELECT id FROM public.entity_user_calculation_requests WHERE kind IN('optimization','appraisal')
      AND expires_at>to_char(((CASE WHEN state='succeeded' THEN COALESCE(completed_at,created_at) ELSE created_at END)::timestamptz
        + interval '14 days') AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') LIMIT 1`);
    if(retention)throw new Error("runtime repair would rewrite an existing retention clock");
    // Reinstall the same layout's routines; no source snapshot is restored over current data.
    for(const source of ddl)await query(tx,source);
    const [{name:owner}]=await query(tx,"SELECT pg_get_userbyid(c.relowner) AS name FROM pg_class c WHERE c.oid='public.entity_compute_specs'::regclass");
    for(const fn of await functions(tx))await query(tx,`ALTER ROUTINE public.${fn.signature.replace(/^public\./,"")} OWNER TO ${quote(owner)}`);
    const [daemon]=await query(tx,"SELECT rolname FROM pg_roles WHERE rolname='wfsim_factoryd'");
    if(daemon)for(const match of readFileSync(new URL("deploy/factoryd_grants.sql",root),"utf8").matchAll(/GRANT (?!CONNECT)[^;]+;/g))await query(tx,match[0]);
  });emit({ok:true,phase:"activated",recovery:"runtime_reinstalled_without_data_restore"});
}

async function release() {
  await sql.begin(async tx=>{
    await setup(tx);const p=await prepared(tx);
    if(p.phase!=="activated")throw new Error("release only an activated, verified layout");
    await lockTables(tx,true);
    for(const name of core)await query(tx,`LOCK TABLE public.${quote(name)} IN ACCESS EXCLUSIVE MODE`);
    for(const item of await triggers(tx))if(item.name==="migration_new_write")
      await query(tx,`DROP TRIGGER migration_new_write ON public.${quote(item.table_name)}`);
    await phase(tx,"live");
  });emit({ok:true,phase:"live",backout_permitted:false});
}

async function backout() {
  await sql.begin(async tx=>{
    await setup(tx);const p=await prepared(tx);
    if(p.phase!=="activated")throw new Error("backout is permitted only before releasing the activated layout");
    await lockTables(tx,true);
    for(const name of core)await query(tx,`LOCK TABLE public.${quote(name)} IN ACCESS EXCLUSIVE MODE`);
    const [writes]=await query(tx,"SELECT count(*) AS n FROM wfsim_migration_ops.record_library_writes");
    if(writes.n!=="0")throw new Error("new application writes exist; preserve them and recover forward");
    const oldTables=new Set([...names(true),...core]);
    for(const item of await triggers(tx))if(oldTables.has(item.table_name) || item.name==="migration_new_write")
      await query(tx,`DROP TRIGGER ${quote(item.name)} ON public.${quote(item.table_name)}`);
    for(const item of await triggers(tx,"wfsim_migration_ops"))if(oldTables.has(item.table_name))
      await query(tx,`DROP TRIGGER ${quote(item.name)} ON wfsim_migration_ops.${quote(item.table_name)}`);
    await query(tx,"DROP VIEW public.state_compute_candidate_answers");
    for(const name of [...views,...core,...sequences])await query(tx,`ALTER ${sequences.includes(name) ? "SEQUENCE" : views.includes(name) ? "VIEW" : "TABLE"} public.${quote(name)} SET SCHEMA ${stage}`);
    const [{owner:stageOwner}]=await query(tx,"SELECT pg_get_userbyid(nspowner) AS owner FROM pg_namespace WHERE nspname=$1",[stage]);
    for(const name of [...core,...views,...sequences]){
      const owner=p.baseline.stage_ownership?.find(relation=>relation.name===name)?.owner || stageOwner;
      await query(tx,`ALTER ${sequences.includes(name) ? "SEQUENCE" : views.includes(name) ? "VIEW" : "TABLE"} ${stage}.${quote(name)} OWNER TO ${quote(owner)}`);
    }
    for(const item of [...mapping].reverse()){
      if(transformed.has(item.source))await query(tx,`ALTER TABLE ${sourceRelation(item,true)} SET SCHEMA public`);
      await query(tx,`ALTER TABLE public.${quote(transformed.has(item.source) ? legacy(item.source) : item.target)} RENAME TO ${quote(item.source)}`);
    }
    for(const before of p.baseline.tables){
      const added=await query(tx,`SELECT a.privilege_type,CASE WHEN a.grantee=0 THEN NULL ELSE pg_get_userbyid(a.grantee) END AS grantee
        FROM pg_class c CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl,acldefault('r',c.relowner)))a
        WHERE c.oid=$1::oid AND NOT EXISTS(SELECT 1 FROM aclexplode(COALESCE($2::aclitem[],acldefault('r',$3::oid)))b
          WHERE b.grantee=a.grantee AND b.privilege_type=a.privilege_type)`,[before.oid,before.acl,before.owner]);
      for(const grant of added)await query(tx,`REVOKE ${grant.privilege_type} ON public.${quote(before.name)} FROM ${grant.grantee===null ? "PUBLIC" : quote(grant.grantee)}`);
    }
    for(const signature of p.baseline.moved_functions){
      await query(tx,`DROP ROUTINE IF EXISTS public.${signature}`);
      await query(tx,`ALTER ROUTINE wfsim_migration_ops.${signature} SET SCHEMA public`);
    }
    await query(tx,"ALTER INDEX public.answers_once RENAME TO migration_answers_task_once");
    await query(tx,"SET LOCAL maintenance_work_mem='128MB';SET LOCAL max_parallel_maintenance_workers=0;SET LOCAL temp_file_limit='4GB'");
    await query(tx,p.baseline.answer_index);
    for(const item of p.baseline.functions)await query(tx,item.definition);
    const old=new Set(p.baseline.functions.map(fn=>fn.signature));
    for(const fn of await functions(tx))if(!old.has(fn.signature)){
      await query(tx,`DROP ROUTINE IF EXISTS ${stage}.${fn.signature.replace(/^public\./,"")} `);
      await query(tx,`ALTER ROUTINE public.${fn.signature.replace(/^public\./,"")} SET SCHEMA ${stage}`);
    }
    await query(tx,"ALTER TABLE public.answers ALTER COLUMN origin SET DEFAULT 'imported'");
    for(const item of p.baseline.triggers){
      await query(tx,item.definition);
      if(item.enabled!=="O")await query(tx,`ALTER TABLE public.${quote(item.table_name)} ${item.enabled==="D" ? "DISABLE" : item.enabled==="A" ? "ENABLE ALWAYS" : "ENABLE REPLICA"} TRIGGER ${quote(item.name)}`);
    }
    for(const item of p.baseline.stage_triggers){
      await query(tx,item.definition);
      if(item.enabled!=="O")await query(tx,`ALTER TABLE ${stage}.${quote(item.table_name)} ${item.enabled==="D" ? "DISABLE" : item.enabled==="A" ? "ENABLE ALWAYS" : "ENABLE REPLICA"} TRIGGER ${quote(item.name)}`);
    }
    await phase(tx,"prepared");
  });emit({ok:true,phase:"prepared",backout:"metadata_only_before_new_writes"});
}

try {
  await requireAdministrator(sql);
  if(["reconcile","activate","backout","repair-runtime","release"].includes(command))await requireDatabaseFence(sql,url,mode);
  if(command==="prepare")await prepare();
  else if(command==="reconcile")await reconcile();
  else if(command==="activate")await activate();
  else if(command==="backout")await backout();
  else if(command==="repair-runtime")await repairRuntime();
  else if(command==="release")await release();
  else if(command==="status"){
    const [exists]=await query(sql,"SELECT to_regclass('wfsim_migration_ops.state_library_layout') AS table_name");
    emit({ok:true,...(exists.table_name ? (await query(sql,"SELECT phase,digest,stage,at FROM wfsim_migration_ops.state_library_layout WHERE id=$1",[manifest.id]))[0] : {phase:"source"})});
  } else throw new Error("unknown library rehearsal command: "+command);
} finally {await sql.end();}
