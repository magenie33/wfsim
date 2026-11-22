// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash,randomBytes} from 'node:crypto';
import {mkdtempSync,readFileSync,writeFileSync,rmSync,mkdirSync,readdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,delimiter} from 'node:path';
import {spawnSync} from 'node:child_process';
import {gunzipSync} from 'node:zlib';
import {setTimeout as wait} from 'node:timers/promises';
import postgres from './pg/node_modules/postgres/src/index.js';
import {boardRequestedSchema} from './board_requested_schema.mjs';
import {captureInbox,commitInboxCursor,inboxSource} from './inbox_reader.mjs';
import {buildBatches} from './build_batches.mjs';
import {pgDb,placeholders,postgresClient} from '../worker/db.js';
import worker from '../worker/index.js';

const sourceURL=new URL(process.env.WFSIM_RAW_ROLLOVER_TEST_URL);
const restoreURL=new URL(process.env.WFSIM_RAW_ROLLOVER_RESTORE_URL);
for(const url of [sourceURL,restoreURL])assert(['127.0.0.1','localhost','[::1]'].includes(url.hostname)&&!url.password);
assert.notEqual(sourceURL.port,restoreURL.port);
const schema='rollover_'+randomBytes(5).toString('hex');
const native=resolve(process.env.WFSIM_REQUESTED_INTAKE_BIN||'target/debug/wfsim-intake');
const scratch=mkdtempSync(join(tmpdir(),'wfsim-rollover-'));
const runnerRole=schema+'_typed', pipelineRole=schema+'_pipeline', writerRole=schema+'_http';
const connect=(role=null)=>postgres(sourceURL.toString(),{max:1,onnotice(){},connection:{
  search_path:schema,lock_timeout:2000,statement_timeout:30000,...role&&{role},
}});
const sql=connect();
let pipeline,writer,typed;
const read=async (pg,text,args=[])=>(await pg.unsafe(text,args))[0];
const one=async (text,args=[])=>(await read(sql,text,args)).v;
const snapshot=()=>one(`SELECT jsonb_build_object('oid','record_build_submissions'::regclass::oid,
 'path',pg_relation_filepath('record_build_submissions'),'bytes',pg_total_relation_size('record_build_submissions'),
 'index_path',(SELECT pg_relation_filepath(indexrelid) FROM pg_index
   WHERE indrelid='record_build_submissions'::regclass AND indisprimary),
 'positions',(SELECT coalesce(jsonb_object_agg(id,position::text),'{}') FROM state_build_inbox_positions),
 'hot',(SELECT count(*) FROM state_raw_inbox_hot_spool),'claims',(SELECT count(*) FROM state_raw_inbox_archive_claims)) v`);
const source=(pg)=>inboxSource((text,args)=>pg.unsafe(placeholders(text),args));
const generation='11440000-0000-4000-8000-000000000001';
const abandoned='11440000-0000-4000-8000-000000000002';
const spent='00000000-0000-4000-8000-000000000001';
const raced='00000000-0000-4000-8000-000000000002';
const retained='00000000-0000-4000-8000-000000000003';
const late='00000000-0000-4000-8000-000000000004';
const gear='equipment:protected';
const at='2026-11-22';
const instant=/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/;
const record=JSON.stringify(JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url),'utf8'))[0].record);
const captureFile=join(scratch,'incoming.ndjson'),receiptFile=join(scratch,'capture.json');
const done=join(scratch,'done.txt'),cursor=join(scratch,'cursor.json'),builds=join(scratch,'derived.ndjson');
const objectRoot=join(scratch,'objects');
mkdirSync(objectRoot);
const service=join(scratch,'pg_service.conf');
const pair=(url)=>`host=${url.hostname}\nport=${url.port}\nuser=${url.username}\ndbname=${url.pathname.slice(1)}\n`;
writeFileSync(service,`[source]\n${pair(sourceURL)}[restore]\n${pair(restoreURL)}`);
writeFileSync(join(scratch,'rclone'),`#!/usr/bin/env python3
import os,sys
from pathlib import Path
args=sys.argv
base=Path(os.environ['R2_FIXTURE'])
def target(key):
  assert key.startswith('r2:wfsim-pg-backup/raw-inbox-rollover-v1/')
  return base.joinpath(*key.split('/')[2:])
if args[1]=='copyto':
  assert '--immutable' in args
  data=Path(args[2]).read_bytes()
  out=target(args[3]);out.parent.mkdir(exist_ok=True)
  if out.exists(): assert out.read_bytes()==data
  else: out.write_bytes(data)
elif args[1]=='cat':
  sys.stdout.buffer.write(target(args[2]).read_bytes())
else: raise ValueError('unexpected storage verb')
`,{mode:0o700});
const cli=(args,{reject=false,sourceAs='source',env={}}={})=>{
  const reply=spawnSync('python3',['scripts/raw_inbox_rollover.py',...args],{encoding:'utf8',timeout:120000,
    env:{...process.env,PATH:scratch+delimiter+process.env.PATH,PGHOST:sourceURL.hostname,
      PGPORT:sourceURL.port,PGUSER:sourceURL.username,PGDATABASE:sourceURL.pathname.slice(1),
      PGSERVICE:sourceAs,PGOPTIONS:'-c search_path='+schema,PGSERVICEFILE:service,R2_FIXTURE:objectRoot,...env}});
  if(reject){assert.notEqual(reply.status,0,reply.stdout);return reply.stderr;}
  if(reply.status!==0)throw Error(reply.stderr||String(reply.error));
  return reply.stdout.trim().split('\n').filter(Boolean).map(JSON.parse);
};
const shortLock=async args=>{
 for(let attempt=0;;attempt++){
  try{return cli(args);}
  catch(error){
   if(attempt>=3||!String(error.message).includes('SQLSTATE 55P03'))throw error;
   await wait(125*(attempt+1));
  }
 }
};
const budgets=['--work',scratch,'--root-reserve','1','--work-reserve','1'];
const copy=['copy',...budgets,'--parts','128','--seconds','100','--restore-service','restore'];
const drain=['drain',...budgets,'--parts','128','--seconds','100','--max-hot-rows','10000','--part-peak-bytes','16777216'];
const cut=['cut',generation,...budgets,'--peak-root-bytes','1048576','--hot-rows','100','--hot-bytes','1048576','--seconds','5'];
const put=(pg,id,raw=record)=>pg.unsafe('INSERT INTO record_build_submissions(id,at,record,initiator) VALUES($1,$2,$3,\'anonymous\')',[id,at,raw]);

try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 for(const name of ['schema','library','sync','inventory','build_workflows','board_admission','board_mailbox','board_capture','board_pages'])
  await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 for(const role of [runnerRole,pipelineRole,writerRole])await sql.unsafe(`CREATE ROLE ${role} NOLOGIN; GRANT USAGE ON SCHEMA ${schema} TO ${role};`);
 const [{current_user:owner}]=await sql.unsafe('SELECT current_user');
 await sql.unsafe(boardRequestedSchema({httpRole:writerRole,runnerRole,readerRole:owner}).replace("default_tablespace='wfsim_data'","default_tablespace='pg_default'"));
 await sql.unsafe(`GRANT SELECT,INSERT ON record_build_submissions,record_equipment_submissions TO ${writerRole};
  GRANT SELECT ON state_build_inbox_inputs,state_build_inbox_positions,state_build_inbox_priorities TO ${pipelineRole};
  GRANT EXECUTE ON FUNCTION build_family_owns_inbox(text,jsonb),build_intake_batch(text),build_ack_submissions(text) TO ${pipelineRole};`);
 writer=connect(writerRole);pipeline=connect(pipelineRole);typed=connect(runnerRole);
 cli(['install','--tablespace','pg_default']);
 await sql.unsafe(`GRANT EXECUTE ON FUNCTION raw_inbox_library_count() TO ${writerRole};
  GRANT SELECT ON state_board_effective_pending_work TO ${writerRole};`);
 const pending=async ()=>(await worker.fetch(new Request('https://wfsim.app/api/board/pending'),
  {LIBRARY:pgDb(postgresClient(writer)),ASSETS:{fetch:async()=>new Response('site')}})).json();
 assert.equal((await pending()).count,0);
 await assert.rejects(pipeline.unsafe('SELECT raw_inbox_rollover_begin($1)',[generation]),error=>error.code==='42501');
 await assert.rejects(writer.unsafe('SELECT * FROM state_raw_inbox_archive_claims'),error=>error.code==='42501');

 await sql.unsafe('ALTER TABLE record_build_submissions DISABLE TRIGGER build_inbox_positioned');
 for(const id of [spent,raced,retained])await put(writer,id);
 await sql.unsafe(`INSERT INTO record_build_submissions(id,at,record)
   SELECT '7'||substr(h,2,7)||'-'||substr(h,9,4)||'-4'||substr(h,14,3)||'-8'||substr(h,18,3)||'-'||substr(h,21,12),
     '${at}',jsonb_build_object('fixture',repeat(h,34))::text FROM (SELECT md5(n::text) h FROM generate_series(1,4096)n) values;`);
 await sql.unsafe('ALTER TABLE record_build_submissions ENABLE TRIGGER build_inbox_positioned');
 let captured='';
 const statement=await captureInbox({source:source(pipeline),maxRows:1,emit:line=>{captured+=line;}});
 assert.deepEqual(statement.ids,[spent]);
 writeFileSync(captureFile,captured);writeFileSync(receiptFile,JSON.stringify(statement));
 const nativeResult=spawnSync(native,['--done',done],{input:captured,encoding:'utf8',timeout:30000});
 assert.equal(nativeResult.status,0,nativeResult.stderr);
 assert.equal(readFileSync(done,'utf8').trim(),spent);
 assert(nativeResult.stdout.trim(),'the native capture produced a canonical build');
 writeFileSync(builds,nativeResult.stdout);
 for await(const batch of buildBatches(builds))await pipeline.unsafe(placeholders(batch.sql),batch.params);

 await writer.unsafe('INSERT INTO record_equipment_submissions(id,at,record) VALUES($1,$2,$3)',[gear,at,'{}']);
 await put(writer,'named-position');
 await sql.unsafe("UPDATE state_board_request_control SET enabled=true");
 const typedId='11440000-0000-4000-8000-000000000009';
 assert.equal((await read(writer,"SELECT board_request_submit($1,$2,$3,'anonymous','standard_single_target') v",[typedId,at,record])).v,typedId);
 assert.equal(await one("SELECT EXISTS(SELECT FROM state_board_requests WHERE id=$1) v",[typedId]),true);
 const pre=await snapshot();
 cli(['begin',generation]);

 const review='rollover-review', family='rollover-family';
 const canonical=await one('SELECT id v FROM entity_weapon_builds LIMIT 1');
 await sql.unsafe(`INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
  baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at)
  VALUES('rollover-review',$1,$2,$3,'standard_single_target','base','test','test',$4,'waiting',1,'kpm',0,'test',$2,$2,$2)`,[review,at,canonical,record]);
 await writer.unsafe('INSERT INTO record_build_submissions(id,at,record,observation) VALUES($1,$2,$3,$4)',
  ['review:'+review,at,record,JSON.stringify({review})]);
 await sql.unsafe("INSERT INTO entity_build_workflow_scopes VALUES('rollover-scope','{}')");
 await sql.unsafe(`INSERT INTO entity_build_workflows(id,build_id,scope_key,kind,scope_id,source,created_at)
  VALUES($1,$2,'rollover-scope','intake','rollover-scope','{"family":true}',$3)`,[family,canonical,at]);
 const derivedId='workflow:'+family+':node';
 await writer.unsafe('INSERT INTO record_build_submissions(id,at,record,family_source) VALUES($1,$2,$3,$4)',
  [derivedId,at,record,'workflow|'+family+'|node']);
 assert.equal((await source(pipeline).byIds(['review:'+review,derivedId])).length,0);
 assert.equal(await one('SELECT EXISTS(SELECT FROM entity_board_requests WHERE id=$1) v',['review:'+review]),true);
 assert.equal(await one('SELECT EXISTS(SELECT FROM state_build_family_inbox WHERE id=$1) v',[derivedId]),true);

 const stale=await one('SELECT raw_inbox_archive_page(512,1048576) v');
 const staleBytes=await one('SELECT raw_inbox_archive_page(512,1048576)::text v');
 await assert.rejects(sql.unsafe('SELECT raw_inbox_archive_accept(($1::text)::jsonb,$2)',[staleBytes,'0'.repeat(64)]),/digest or bounds/);
 cli(cut,{reject:true});
 await sql.unsafe('DELETE FROM record_build_submissions WHERE id=$1',[raced]);
 await assert.rejects(sql.unsafe('SELECT raw_inbox_archive_accept(($1::text)::jsonb,$2)',[staleBytes,createHash('sha256').update(staleBytes).digest('hex')]),/source changed/);
 assert.equal(stale.rows[1].input.id,raced);
 await put(writer,late);
 await assert.rejects(sql.unsafe("UPDATE record_build_submissions SET record='{}' WHERE id=$1",[retained]),/active archive/);
 await assert.rejects(sql.unsafe('TRUNCATE record_build_submissions'),/active archive/);

 const first=cli(copy);
 const packets=first.filter(row=>row.stage==='copied');
 assert(packets.length>2);
 assert.equal(cli(['status'])[0].next_part,packets.at(-1).part+1);
 const afterCopy=await snapshot();
 assert(afterCopy.claims>4000 && afterCopy.hot===2);
 await assert.rejects(sql.unsafe('INSERT INTO state_build_inbox_positions(id) VALUES($1)',[retained]),/new FIFO position/);
 assert.equal((await source(pipeline).byIds([late]))[0].id,late);

 await assert.rejects(sql.begin(async(tx)=>{
  await tx.unsafe('SELECT raw_inbox_rollover_cut($1,100,1048576)',[generation]);
  throw Error('injected failure after replacing heap');
 }),/injected failure/);
 assert.deepEqual(await snapshot(),afterCopy);
 let release,ready;
 const waiting=new Promise(resolve=>{ready=resolve;});
 const hold=new Promise(resolve=>{release=resolve;});
 const concurrent=writer.begin(async(tx)=>{
  await put(tx,'arriving-at-cut');
  ready();
  await hold;
 });
 await Promise.race([waiting,concurrent]);
 try{cli(cut,{reject:true});}finally{release();await concurrent;}
 const lastBeforeCut=await snapshot();
 const [cutReceipt,phase]=await shortLock(cut);
 assert(cutReceipt.old_bytes>4*1024*1024 && cutReceipt.new_bytes<cutReceipt.old_bytes/50);
 assert.equal(phase.phase,'draining');
 assert.match(phase.cut_at,instant);
 assert.equal(await one('SELECT raw_inbox_library_count() v'),null);
 assert.equal((await pending()).count,null);
 const postCut=await snapshot();
 assert.equal(postCut.oid,pre.oid);
 assert.notEqual(postCut.path,pre.path);
 assert.notEqual(postCut.index_path,pre.index_path);
 for(const location of [pre.path,pre.index_path])
  assert.equal(await one('SELECT coalesce((pg_stat_file($1,true)).size,0)::int v',[location]),0);
 assert.deepEqual(postCut.positions,lastBeforeCut.positions);
 assert.equal(cli(cut)[0].stage,'already_cut');
 assert.equal(await one('SELECT EXISTS(SELECT FROM state_board_requests WHERE id=$1) v',[typedId]),true);
 assert.notEqual(await read(typed,'SELECT current_user'),null);

 await assert.rejects(put(writer,retained,'{"different":true}'),error=>error.code==='23505');
 await put(writer,retained);
 assert.equal((await source(pipeline).byIds([retained])).length,0,'an exact duplicate remains under the same archive ownership');
 assert.equal((await source(pipeline).byIds(['arriving-at-cut']))[0].id,'arriving-at-cut');
 const tampered=JSON.parse(captured);
 tampered.record='{"different":true}';
 await assert.rejects(pipeline.unsafe('SELECT build_ack_submissions($1)',[JSON.stringify([tampered])]),/captured archive input changed/);
 for await(const batch of buildBatches(captureFile,{deleting:true}))await pipeline.unsafe(placeholders(batch.sql),batch.params);
 commitInboxCursor({cursorFile:cursor,receiptFile,inputFile:captureFile,doneFile:done});
 assert.equal(JSON.parse(readFileSync(cursor,'utf8')).after,spent);
 assert.equal(await one('SELECT EXISTS(SELECT FROM state_raw_inbox_archive_claims WHERE id=$1) v',[spent]),false);

 const directory=join(objectRoot,generation);
 const key=readdirSync(directory).find(name=>name.startsWith('1-'));
 const object=join(directory,key);
 const bytes=readFileSync(object);
 writeFileSync(object,Buffer.from('damaged'));
 cli(drain,{reject:true});
 assert.equal(await one('SELECT loaded_at AS v FROM record_raw_inbox_archive_chunks WHERE part=1'),null);
 writeFileSync(object,bytes);
 const loaded=cli(drain).filter(row=>row.stage==='drained');
 assert.equal(loaded.length,packets.length);
 assert.match(await one('SELECT loaded_at v FROM record_raw_inbox_archive_chunks WHERE part=1'),instant);
 assert.equal((await source(pipeline).byIds([raced,spent])).length,0);
 const recovered=(await source(pipeline).byIds([retained]))[0];
 for(const key of ['id','at','record','initiator','family_source','observation'])
  assert.deepEqual(recovered[key],stale.rows.find(row=>row.input.id===retained).input[key]);
 assert.deepEqual((await snapshot()).positions,lastBeforeCut.positions);
 const proof=JSON.stringify([recovered]);
 await pipeline.unsafe('SELECT build_ack_submissions($1)',[proof]);
 const replay=await one('SELECT raw_inbox_archive_load(($1::text)::jsonb) v',[gunzipSync(bytes).toString('utf8')]);
 assert.deepEqual(replay,{loaded:0,replay:true});
 assert.equal(cli(drain).filter(row=>row.stage==='drained').length,0);
 assert.equal((await source(pipeline).byIds([retained])).length,0,'a committed drain cannot resurrect an ACKed input');
 const [finished]=await shortLock(['finish',generation]);
 assert.match(finished.completed_at,instant);
 assert(Number(await one('SELECT raw_inbox_library_count() v'))>4000);
 assert((await pending()).count>4000);
 await shortLock(['begin',abandoned]);
 await put(writer,'new-on-abort');
 const [aborted]=await shortLock(['abort',abandoned]);
 assert.match(aborted.aborted_at,instant);
 assert.equal((await source(pipeline).byIds(['new-on-abort']))[0].id,'new-on-abort');
 assert.equal(await one("SELECT count(*)::int v FROM pg_trigger WHERE tgname LIKE '%raw_inbox_rollover%' AND NOT tgisinternal"),0);
 console.log(JSON.stringify({native:true,restored_parts:packets.length,old_bytes:cutReceipt.old_bytes,new_bytes:cutReceipt.new_bytes,
   unchanged_source_oid:true,old_heap_and_index_files_zero:true,positions_preserved:true,typed_owner_preserved:true,cut_rollback:true,
   corrupt_remote_refused:true,stale_ack_exact:true,no_consumed_revival:true}));
} finally {
 for(const connection of [typed,writer,pipeline])if(connection)await connection.end();
 try{
  await sql.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`);
  for(const role of [runnerRole,pipelineRole,writerRole])await sql.unsafe('DROP ROLE IF EXISTS '+role);
 }finally{await sql.end();rmSync(scratch,{recursive:true,force:true});}
}
