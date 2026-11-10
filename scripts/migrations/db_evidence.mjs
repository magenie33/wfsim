// Independently compare candidate membership by bounded source-id ranges, without reading result bodies.
import {readFileSync} from "node:fs";
import {createHash} from "node:crypto";
import postgres from "../pg/node_modules/postgres/src/index.js";

const stage=process.argv.find(arg=>arg.startsWith("--stage="))?.slice(8);
if(!/^wfsim_migrate_[a-f0-9]{12}$/.test(stage || ""))throw new Error("an exact migration stage is required");
const url=process.env.WFSIM_MIGRATION_URL;
if(!url)throw new Error("WFSIM_MIGRATION_URL is required");
if(!["127.0.0.1","localhost","[::1]"].includes(new URL(url).hostname)
    && process.env.WFSIM_MIGRATION_PRODUCTION!=="db-boundaries-2026-11-10")throw new Error("explicit production operation id is required");
const hash=createHash("sha256").update(readFileSync(new URL(import.meta.url))).digest("hex");
const sql=postgres(url,{max:1,fetch_types:false,onnotice:()=>{},connection:{application_name:"wfsim-db-evidence-audit"}});
const id="evidence_audit_"+hash.slice(0,12),budget=2048;
try {
  for(;;) {
    const row=await sql.begin("isolation level repeatable read",async tx=>{
      await tx.unsafe(`SET LOCAL search_path TO ${stage},public;SET LOCAL statement_timeout='60s';SET LOCAL lock_timeout='3s';SET LOCAL work_mem='16MB';SET LOCAL temp_file_limit='128MB'`);
      await tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('wfsim-db-boundaries',0))");
      const [model]=await tx.unsafe("SELECT digest FROM state_migration_progress WHERE id='compute' AND complete");
      const [copied]=await tx.unsafe("SELECT complete FROM state_migration_progress WHERE id='evidence'");
      if(!model || !copied?.complete)throw new Error("finish both initial copies before their evidence audit");
      await tx.unsafe("INSERT INTO state_migration_progress(id,digest) VALUES($1,$2) ON CONFLICT DO NOTHING",[id,model.digest]);
      const [progress]=await tx.unsafe("SELECT * FROM state_migration_progress WHERE id=$1 FOR UPDATE",[id]);
      if(progress.complete)return {complete:true,rows:progress.rows};
      const page=await tx.unsafe(`SELECT id FROM public.answers WHERE id>COALESCE($1::bigint,0) ORDER BY id LIMIT ${budget}`,[progress.cursor]);
      if(!page.length){await tx.unsafe("UPDATE state_migration_progress SET complete=true,updated_at=clock_timestamp() WHERE id=$1",[id]);return {complete:true,rows:progress.rows};}
      const end=String(page.at(-1).id);
      const [counts]=await tx.unsafe(`WITH answers AS MATERIALIZED(
          SELECT a.id,a.question FROM public.answers a WHERE a.id>COALESCE($1::bigint,0) AND a.id<=$2::bigint),
        checked AS(SELECT a.id,a.question,j.changed,q.id AS hot_question,t.id AS task_id,e.answer_id
          FROM answers a LEFT JOIN LATERAL(SELECT true changed FROM record_migration_changes j WHERE j.question=a.question LIMIT 1)j ON true
          LEFT JOIN LATERAL(SELECT id FROM public.questions q WHERE q.id=a.question OFFSET 0)q ON true
          LEFT JOIN LATERAL(SELECT id FROM entity_compute_tasks t WHERE t.computation=a.question AND t.generation=1 OFFSET 0)t ON true
          LEFT JOIN relation_task_answer_evidence e ON e.task_id=t.id AND e.answer_id=a.id)
        SELECT count(*) FILTER(WHERE changed) AS dirty,
          count(*) FILTER(WHERE NOT COALESCE(changed,false) AND hot_question IS NULL) AS orphaned,
          count(*) FILTER(WHERE NOT COALESCE(changed,false) AND hot_question IS NOT NULL AND answer_id IS NULL) AS missing,
          count(*) FILTER(WHERE NOT COALESCE(changed,false) AND hot_question IS NULL AND answer_id IS NOT NULL) AS unexpected
        FROM checked`,[progress.cursor,end]);
      if(counts.missing!=="0" || counts.unexpected!=="0")throw new Error("candidate membership differs within source range ending "+end);
      await tx.unsafe("INSERT INTO record_migration_checks(kind,cursor,rows,dirty_keys) VALUES($1,$2,$3,$4)",[id,end,page.length,counts.dirty]);
      await tx.unsafe("UPDATE state_migration_progress SET cursor=$2,rows=rows+$3,batches=batches+1,updated_at=clock_timestamp() WHERE id=$1",[id,end,page.length]);
      return {complete:false,cursor:end,rows:String(BigInt(progress.rows)+BigInt(page.length)),deferred_dirty:counts.dirty,orphaned:counts.orphaned};
    });
    console.log(JSON.stringify({ok:true,checker:hash,stage,...row}));if(row.complete)break;
  }
} finally {await sql.end();}
