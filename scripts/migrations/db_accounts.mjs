// Rehearse atomic account-table renames and a pre-write backout on a restored local database.
import {readFileSync} from "node:fs";
import {createHash} from "node:crypto";
import postgres from "../pg/node_modules/postgres/src/index.js";

const command=process.argv[2] || "status",url=process.env.WFSIM_MIGRATION_URL;
if(!url)throw new Error("WFSIM_MIGRATION_URL is required");
const parsed=new URL(url);
if(!["127.0.0.1","localhost","[::1]"].includes(parsed.hostname)
    || !/^\/wfsim_accounts_migration_[a-z0-9_]+$/.test(parsed.pathname))
  throw new Error("account layout rehearsal requires an isolated local restored database");
const manifest=JSON.parse(readFileSync(new URL("db-boundaries-2026-11-10.json",import.meta.url),"utf8"));
const mapping=manifest.databases.find(database=>database.name==="wfsim_accounts").tables;
if(mapping.some(table=>table.mode!=="rename"))throw new Error("account layout contains a non-metadata transformation");
const ddl=readFileSync(new URL("../../worker/pg/accounts.sql",import.meta.url),"utf8");
const digest=createHash("sha256").update(readFileSync(new URL(import.meta.url))).update(ddl).update(JSON.stringify(mapping)).digest("hex");
const functions=[...ddl.matchAll(/CREATE OR REPLACE FUNCTION ([a-z_]+)\(/g)].map(match=>match[1]);
const sql=postgres(url,{max:1,fetch_types:false,onnotice:()=>{},connection:{application_name:"wfsim-accounts-layout-rehearsal"}});
const quote=value=>'"'+value.replaceAll('"','""')+'"';
const failAfter=Number(process.argv.find(arg=>arg.startsWith("--fail-after="))?.slice(13) || 0);
const catalog=async(tx,names)=>{
  return [...await tx.unsafe(`SELECT c.oid::text AS oid,c.relname AS name,c.relfilenode::text AS storage,
      c.relowner::text AS owner,c.relacl::text AS acl,c.reloptions,
      (SELECT jsonb_agg(jsonb_build_object('name',a.attname,'type',a.atttypid::text,'mod',a.atttypmod,
        'null',a.attnotnull,'collation',a.attcollation::text,'identity',a.attidentity,'generated',a.attgenerated,
        'acl',a.attacl::text) ORDER BY a.attnum) FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0 AND NOT a.attisdropped) AS columns,
      (SELECT jsonb_agg(jsonb_build_object('oid',i.indexrelid::text,'unique',i.indisunique,'valid',i.indisvalid,
        'key',i.indkey::text) ORDER BY i.indexrelid) FROM pg_index i WHERE i.indrelid=c.oid) AS indexes,
      (SELECT jsonb_agg(jsonb_build_object('oid',k.oid::text,'kind',k.contype,'key',k.conkey,'foreign_relation',k.confrelid::text,
        'foreign_key',k.confkey,'valid',k.convalidated) ORDER BY k.oid) FROM pg_constraint k WHERE k.conrelid=c.oid) AS constraints
    FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'
      AND c.relkind='r' AND c.relname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ORDER BY c.oid`,[JSON.stringify(names)])];
};
const hashes=async(tx,tables)=>{
  const output=[];
  for(const table of tables) {
    const keys=await tx.unsafe(`SELECT a.attname FROM pg_index i CROSS JOIN unnest(i.indkey) WITH ORDINALITY k(attnum,position)
      JOIN pg_attribute a ON a.attrelid=i.indrelid AND a.attnum=k.attnum WHERE i.indrelid=$1::regclass AND i.indisprimary ORDER BY k.position`,["public."+table.name]);
    const order=keys.length ? keys.map(row=>quote(row.attname)).join(",") : 'to_jsonb(t)::text COLLATE "C"';
    const hash=createHash("sha256");let rows=0;
    for await(const page of tx.unsafe(`SELECT row_to_json(t)::text AS body FROM public.${quote(table.name)} t ORDER BY ${order}`).cursor(256)) {
      for(const row of page) {hash.update(row.body).update("\n");rows++;}
    }
    output.push({oid:table.oid,rows,sha256:hash.digest("hex")});
  }
  return output;
};
const same=(left,right)=>JSON.stringify(left)===JSON.stringify(right);

try {
  if(command==="status") {
    const exists=(await sql.unsafe("SELECT to_regclass('wfsim_migration_ops.state_account_layout') AS relation"))[0].relation;
    console.log(JSON.stringify({digest,phase:exists ? (await sql.unsafe("SELECT phase FROM wfsim_migration_ops.state_account_layout WHERE id=$1",[manifest.id]))[0]?.phase : "source"}));
  } else if(command==="rename" || command==="backout") {
    const result=await sql.begin(async tx=>{
      await tx.unsafe("SET LOCAL lock_timeout='3s';SET LOCAL statement_timeout='120s';SET LOCAL idle_in_transaction_session_timeout='10s';SET LOCAL work_mem='16MB';SET LOCAL temp_file_limit='256MB'");
      await tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('wfsim-db-boundaries',0))");
      await tx.unsafe(`CREATE SCHEMA IF NOT EXISTS wfsim_migration_ops;
        REVOKE ALL ON SCHEMA wfsim_migration_ops FROM PUBLIC;
        CREATE TABLE IF NOT EXISTS wfsim_migration_ops.state_account_layout(
          id text PRIMARY KEY,digest text NOT NULL,phase text NOT NULL,baseline jsonb NOT NULL,
          row_hashes jsonb NOT NULL,functions jsonb NOT NULL,at timestamptz NOT NULL DEFAULT clock_timestamp());`);
      const [held]=await tx.unsafe("SELECT * FROM wfsim_migration_ops.state_account_layout WHERE id=$1 FOR UPDATE",[manifest.id]);
      if(held && held.digest!==digest)throw new Error("account rehearsal checkpoint belongs to different source");
      const from=command==="rename" ? "source" : "target",to=from==="source" ? "target" : "source";
      if(held?.phase===(command==="rename" ? "renamed" : "source"))return {already:true,phase:held.phase};
      if(command==="backout" && !held)throw new Error("no account layout baseline exists");
      const names=mapping.map(table=>table[from]);
      const before=await catalog(tx,names);
      if(before.length!==mapping.length)throw new Error("account source table census differs from the complete manifest");
      const [collision]=await tx.unsafe("SELECT relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND relname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) LIMIT 1",[JSON.stringify(mapping.map(table=>table[to]))]);
      if(collision)throw new Error("refusing to overwrite an existing relation: "+collision.relname);
      await tx.unsafe("LOCK TABLE "+names.map(name=>"public."+quote(name)).join(",")+" IN ACCESS EXCLUSIVE MODE");
      const content=await hashes(tx,before);
      if(command==="backout" && !same(content,held.row_hashes))throw new Error("account rows changed after activation; preserve them and recover forward");
      if(command==="rename" && !held) {
        const definitions=await tx.unsafe("SELECT pg_get_functiondef(p.oid) AS definition FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' AND p.proname IN(SELECT jsonb_array_elements_text(($1::text)::jsonb)) ORDER BY p.oid",[JSON.stringify(functions)]);
        if(definitions.length!==functions.length)throw new Error("account function census differs from source");
        await tx.unsafe("INSERT INTO wfsim_migration_ops.state_account_layout(id,digest,phase,baseline,row_hashes,functions) VALUES($1,$2,'source',($3::text)::jsonb,($4::text)::jsonb,($5::text)::jsonb)",
          [manifest.id,digest,JSON.stringify(before),JSON.stringify(content),JSON.stringify(definitions)]);
      }
      for(const [index,table] of mapping.entries()) {
        await tx.unsafe(`ALTER TABLE public.${quote(table[from])} RENAME TO ${quote(table[to])}`);
        if(failAfter===index+1)throw new Error("injected interruption after "+failAfter+" metadata renames");
      }
      if(command==="rename")await tx.unsafe(ddl);
      else for(const fn of held.functions)await tx.unsafe(fn.definition);
      const after=await catalog(tx,mapping.map(table=>table[to]));
      const expected=before.map(table=>({...table,name:mapping.find(item=>item[from]===table.name)[to]}));
      if(!same(after,expected))throw new Error("metadata rename changed an OID, storage, owner, permission, column, index or constraint");
      const contentAfter=await hashes(tx,after);
      if(!same(content,contentAfter))throw new Error("account content hash or row count changed across metadata rename");
      const phase=command==="rename" ? "renamed" : "source";
      await tx.unsafe("UPDATE wfsim_migration_ops.state_account_layout SET phase=$2,at=clock_timestamp() WHERE id=$1",[manifest.id,phase]);
      return {phase,tables:after.length,rows:content.reduce((sum,table)=>sum+table.rows,0),row_hashes:contentAfter,metadata_unchanged:true};
    });
    console.log(JSON.stringify({ok:true,command,digest,rehearsal_only:true,production_changes:false,...result}));
  } else throw new Error("unknown account rehearsal command: "+command);
} finally {await sql.end();}
