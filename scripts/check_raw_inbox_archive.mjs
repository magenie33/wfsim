// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash,randomUUID } from 'node:crypto';
import { mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter,join,resolve } from 'node:path';
import postgres from './pg/node_modules/postgres/src/index.js';
import { consumeArchive } from './raw_inbox_archive_consume.mjs';
import { close,query } from './db.mjs';
import { boardRequestedSchema } from './board_requested_schema.mjs';
import { inboxSource } from './inbox_reader.mjs';
import { placeholders } from '../worker/placeholders.js';

const connection=new URL(process.env.WFSIM_RAW_ROLLOVER_TEST_URL);
const restore=new URL(process.env.WFSIM_RAW_ROLLOVER_RESTORE_URL);
for(const url of [connection,restore]) assert(['127.0.0.1','localhost'].includes(url.hostname)&&!url.password);
assert.notEqual(connection.port,restore.port);
const schema='raw_archive_'+randomUUID().replaceAll('-',''), role=schema+'_reader';
const owner=postgres(connection.toString(),{max:1,onnotice(){}});
const writerRole=schema+'_writer';
const connect=()=>postgres(connection.toString(),{max:1,onnotice(){},connection:{role:writerRole,
  search_path:schema+',pg_catalog,pg_temp',lock_timeout:5000,statement_timeout:10000}});
const native=resolve(process.env.WFSIM_REQUESTED_INTAKE_BIN||'target/debug/wfsim-intake');
const ddl=['schema','library','sync','inventory','build_workflows','raw_inbox_archive']
  .map(name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8')).join('\n');
const generation=randomUUID(), fence='c'.repeat(64), at='2026-11-22T05:22:00.000Z';
const ids=[1,2,3,4].map(n=>'00000000-0000-4000-8000-00000000000'+n);
const rows=ids.map((id,i)=>({position:null,portable:true,input:{id,at,initiator:'anonymous',family_source:null,
  observation:null,record:JSON.stringify({
    weapon:'braton_prime',mods:['hellfire','primary_acuity','galvanized_aptitude','hammer_shot',
      i&1?'vile_acceleration':'speed_trigger',i&2?'point_strike':'vital_sense','vigilante_fervor','magnetic_capacity'],
    benchmark:'standard_single_target',mode:'base',arcanes:['primary_deadhead'],
    evolutions:['braton_prime_evo1_incarnon_form','braton_prime_daring_reverie','braton_prime_voids_guidance','braton_prime_prelude_of_might'],
  })}}));
const parts=[rows.slice(0,2),rows.slice(2)].map((members,i)=>{
  const part=i+1;
  const body=Buffer.from(JSON.stringify({generation,part,after:i?ids[1]:null,rows:members}));
  return {part,body,first:members[0].input.id,last:members.at(-1).input.id,
    digest:createHash('sha256').update(body).digest('hex')};
});
const snapshot=async()=>(await owner.unsafe("SELECT phase,next_part::int,(SELECT count(*)::int FROM entity_weapon_builds) builds,(SELECT count(*)::int FROM entity_build_intake_intents) intents,(SELECT count(*)::int FROM record_build_submissions) hot,(SELECT count(*)::int FROM state_build_inbox_positions) positions FROM state_raw_archive_generation"))[0];
const oldURL=process.env.PG_URL;
const consume=part=>consumeArchive(part.body,part.digest,native,5000);
const scratch=mkdtempSync(join(tmpdir(),'clean-raw-archive-test-'));
const service=join(scratch,'pg_service.conf');
const serviceAddress=url=>'host='+url.hostname+'\nport='+url.port+'\nuser='+url.username+'\ndbname='+url.pathname.slice(1)+'\n';
writeFileSync(service,'[source]\n'+serviceAddress(connection)+'[restore]\n'+serviceAddress(restore));
writeFileSync(join(scratch,'rclone'),[
  '#!/usr/bin/env python3',
  'import json,os,sys',
  'from pathlib import Path',
  'action,source,*rest=sys.argv[1:]',
  'key=rest[0] if action=="copyto" else source',
  'assert key.startswith("r2:wfsim-pg-backup/raw-inbox-rollover-v1/")',
  'path=Path(os.environ["ARCHIVE_TEST_ROOT"])/key.removeprefix("r2:")',
  'fault=os.environ.get("ARCHIVE_TEST_FAULT")',
  'if action=="copyto":',
  ' assert "--immutable" in rest',
  ' path.parent.mkdir(parents=True,exist_ok=True)',
  ' body=Path(source).read_bytes()',
  ' if path.exists(): assert path.read_bytes()==body',
  ' else: path.write_bytes(body)',
  'elif action=="cat":',
  ' if fault=="readback": sys.exit(1)',
  ' sys.stdout.buffer.write(path.read_bytes())',
  'elif action=="lsjson":',
  ' if fault=="stat-denied": sys.exit(1)',
  ' print(json.dumps({"IsDir":not path.is_file(),"Size":path.stat().st_size if path.is_file() else -1}))',
  'elif action=="deletefile": path.unlink()',
  'else: raise RuntimeError("unexpected archive transport")',
].join('\n')+'\n',{mode:0o700});
const realPsql=spawnSync('which',['psql'],{encoding:'utf8'}).stdout.trim();
assert(realPsql);
writeFileSync(join(scratch,'psql'),[
  '#!/usr/bin/env python3',
  'import os,subprocess,sys',
  'body=sys.stdin.buffer.read()',
  'run=subprocess.run([os.environ["ARCHIVE_REAL_PSQL"],*sys.argv[1:]],input=body,capture_output=True)',
  'if os.environ.get("ARCHIVE_TEST_FAULT")=="lost-accept" and b"raw_archive_move_accept(" in body and run.returncode==0:',
  ' sys.exit(1)',
  'sys.stdout.buffer.write(run.stdout)',
  'sys.stderr.buffer.write(run.stderr)',
  'sys.exit(run.returncode)',
].join('\n')+'\n',{mode:0o700});
const budgets=['--work',scratch,'--root-reserve','1','--work-reserve','1'];
function runCli(args,fault=''){
  return spawnSync('python3',['scripts/raw_inbox_rollover.py',...args],{encoding:'utf8',timeout:30000,
    env:{...process.env,PGSERVICE:'source',PGSERVICEFILE:service,PGOPTIONS:'-c search_path='+schema,
      PATH:scratch+delimiter+process.env.PATH,ARCHIVE_TEST_ROOT:scratch,ARCHIVE_REAL_PSQL:realPsql,ARCHIVE_TEST_FAULT:fault}});
}
function cli(...args){
  const result=runCli(args);
  assert.equal(result.status,0,result.stderr||result.error?.message);
  return result.stdout.trim().split('\n').filter(Boolean).map(JSON.parse);
}
function failCli(fault,...args){
  const result=runCli(args,fault);
  assert.notEqual(result.status,0,result.stdout);
  assert.ifError(result.error);
}
let writer, replay;
async function moveFixture(){
  for(const name of ['board_admission','board_mailbox','board_capture','board_pages']){
    await owner.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8')).simple();
  }
  const [{current_user:ownerRole}]=await owner.unsafe('SELECT current_user');
  await owner.unsafe('CREATE ROLE '+writerRole+' NOLOGIN; GRANT USAGE ON SCHEMA '+schema+' TO '+writerRole).simple();
  await owner.unsafe(boardRequestedSchema({httpRole:writerRole,runnerRole:role,readerRole:ownerRole})
    .replace("default_tablespace='wfsim_data'","default_tablespace='pg_default'")).simple();
  await owner.unsafe('DROP TRIGGER forbidden_raw_hydration ON record_build_submissions').simple();
  await owner.unsafe(readFileSync(new URL('../worker/pg/raw_inbox_archive_move.sql',import.meta.url),'utf8')).simple();
  await owner.unsafe('GRANT SELECT,INSERT ON record_build_submissions TO '+writerRole+
    '; GRANT SELECT ON state_build_inbox_inputs,state_build_inbox_positions,state_build_inbox_priorities TO '+writerRole+
    '; GRANT EXECUTE ON FUNCTION build_ack_submissions(text),build_family_owns_inbox(text,jsonb),raw_inbox_library_count() TO '+writerRole).simple();
  writer=connect();replay=connect();
  const source=inboxSource((text,params)=>writer.unsafe(placeholders(text),params));
  const original=randomUUID(), attempt=randomUUID();
  const hot='family:legacy', review='review:kept', arrival='00000000-0000-8000-8000-000000000001', late='workflow:repeat:1';
  const cold='10000000-0000-4000-8000-000000000001', held='10000000-0000-4000-8000-000000000002';
  const objects=gen=>readdirSync(join(scratch,'wfsim-pg-backup','raw-inbox-rollover-v1',gen));
  const transfer=async gen=>[...await owner.unsafe('SELECT part::int,digest FROM record_raw_archive_uploads WHERE generation=$1 ORDER BY part,digest',[gen])];
  const copy=parts=>['copy-clean',...budgets,'--parts',String(parts),'--seconds','20','--restore-service','restore','--rows','2','--bytes','12288'];
  const retire=gen=>['retire-clean',gen,'--work',scratch];
  const submit=(db,id,record='{}')=>db.unsafe('INSERT INTO record_build_submissions(id,at,record) VALUES($1,$2,$3) ON CONFLICT(id) DO NOTHING',[id,at,record]);
  const oldRelation=async()=>(await owner.unsafe("SELECT 'record_build_submissions'::regclass::oid AS oid,pg_relation_filepath('record_build_submissions') AS heap,(SELECT pg_relation_filepath(indexrelid) FROM pg_index WHERE indrelid='record_build_submissions'::regclass AND indisprimary) AS pk"))[0];
  const positions=()=>owner.unsafe('SELECT id,position::text FROM state_build_inbox_positions ORDER BY id');
  const keys=async()=>(await writer.unsafe('SELECT id FROM record_build_submissions ORDER BY id')).map(row=>row.id);
  const fenceFile=join(scratch,'fence.json');
  const fenced=gen=>{
    const consumer={paused:true,inflight:0,capture:'discarded',receipt:fence};
    writeFileSync(fenceFile,JSON.stringify({v:1,generation:gen,source:'a'.repeat(40),live:consumer,hourly:consumer}));
    return createHash('sha256').update(readFileSync(fenceFile)).digest('hex');
  };
  await submit(writer,hot);
  await submit(writer,review);
  await owner.unsafe('ALTER TABLE record_build_submissions DISABLE TRIGGER build_inbox_positioned');
  await submit(writer,cold,rows[0].input.record);
  await submit(writer,held,rows[1].input.record);
  await owner.unsafe('ALTER TABLE record_build_submissions ENABLE TRIGGER build_inbox_positioned');
  await owner.unsafe("INSERT INTO state_build_family_inbox(id,retry_at,input) VALUES($1,$2,'{}')",[held,at]);
  const before=await oldRelation();
  fenced(attempt);
  cli('begin-clean',attempt,'--fence-file',fenceFile);
  assert.equal((await writer.unsafe('SELECT raw_inbox_library_count() AS n'))[0].n,null);
  await submit(writer,arrival);
  failCli('readback',...copy(1));
  const [abandoned]=await transfer(attempt);
  failCli('readback',...copy(1));
  assert.deepEqual(await transfer(attempt),[abandoned]);
  assert.deepEqual(objects(attempt),[abandoned.part+'-'+abandoned.digest+'.json.gz']);
  assert.deepEqual(cli(...retire(attempt)),[]);
  cli('abort-clean',attempt,'--work',scratch);
  assert((await keys()).includes(arrival));
  assert((await keys()).includes(cold));
  assert.equal(cli(...retire(attempt))[0].digest,abandoned.digest);
  assert.deepEqual(objects(attempt),[]);

  writeFileSync(fenceFile,JSON.stringify({v:1,generation:original,source:'a'.repeat(40),live:{paused:true}}));
  assert.throws(()=>cli('begin-clean',original,'--fence-file',fenceFile),/capture receipts must be fenced/);
  const originalFence=fenced(original);
  cli('begin-clean',original,'--fence-file',fenceFile);
  await assert.rejects(owner.unsafe('SELECT raw_archive_move_cut($1,$2)',[original,originalFence]),error=>error.code==='55000');
  await assert.rejects(submit(writer,cold),error=>error.code==='23505');
  await assert.rejects(owner.unsafe('INSERT INTO state_build_inbox_positions(id) VALUES($1)',[cold]),error=>error.code==='55000');
  await assert.rejects(owner.unsafe("INSERT INTO entity_board_requests(id,kind,input,ruler,mode) VALUES($1,'missing','{}','standard_single_target','base')",[cold]),error=>error.code==='55000');
  await assert.rejects(owner.unsafe("INSERT INTO state_build_family_inbox(id,retry_at,input) VALUES($1,$2,'{}')",[cold,at]),error=>error.code==='55000');
  await assert.rejects(owner.unsafe('DELETE FROM record_build_submissions WHERE id=$1',[hot]),error=>error.code==='55000');
  failCli('readback',...copy(1));
  const [superseded]=await transfer(original);
  await owner.unsafe('DELETE FROM state_build_family_inbox WHERE id=$1',[held]);
  failCli('lost-accept',...copy(1));
  const [manifest]=await owner.unsafe('SELECT digest,rows FROM record_raw_archive_parts WHERE generation=$1',[original]);
  assert.equal(manifest.rows,2);
  assert.notEqual(manifest.digest,superseded.digest);
  assert.deepEqual(await transfer(original),[superseded]);
  assert.equal(objects(original).length,2);
  const sealed=cli(...copy(10));
  assert.equal(sealed.reduce((total,item)=>total+item.cold,0),0);
  assert.deepEqual(cli(...retire(original)),[]);
  let wake,release;
  const waiting=new Promise(resolve=>{wake=resolve;});
  const gate=new Promise(resolve=>{release=resolve;});
  const first=writer.begin(async tx=>{await submit(tx,late);wake();await gate;});
  await Promise.race([waiting,first]);
  const again=submit(replay,late);
  try {await assert.rejects(owner.unsafe('SELECT raw_archive_move_cut($1,$2)',[original,originalFence]),error=>error.code==='55P03');}
  finally {release();await first;await again;}
  const beforeCut=await positions();
  const [cut]=cli('cut-clean',original,'--fence-file',fenceFile,...budgets,'--peak-root-bytes','1');
  assert(cut.old_source_bytes>0);
  const [cutStatus]=cli('status-clean');
  assert.deepEqual([cutStatus.phase,cutStatus.generation,cutStatus.sealed_rows,cutStatus.next_part],['complete',original,2,1]);
  assert.equal((await oldRelation()).oid,before.oid);
  for(const name of [before.heap,before.pk]){
    assert.equal((await owner.unsafe('SELECT coalesce((pg_stat_file($1,true)).size,0)::int AS bytes',[name]))[0].bytes,0);
  }
  assert.deepEqual(await positions(),beforeCut);
  assert.equal((await writer.unsafe('SELECT raw_inbox_library_count()::int AS n'))[0].n,11);
  failCli('stat-denied',...retire(original));
  assert.deepEqual(await transfer(original),[superseded]);
  assert.equal(cli(...retire(original))[0].digest,superseded.digest);
  assert.deepEqual(await transfer(original),[]);
  assert.deepEqual(objects(original),['1-'+manifest.digest+'.json.gz']);
  assert.deepEqual(cli(...retire(original)),[]);
  const [received]=cli('consume',...budgets,'--native',native,'--peak-root-bytes','1','--statement-ms','5000');
  assert.deepEqual([received.generation,received.committed,received.rows],[original,true,2]);
  const [retired]=cli(...retire(original));
  assert.deepEqual([retired.part,retired.digest],[1,received.digest]);
  assert.deepEqual(cli(...retire(original)),[]);
  assert.deepEqual(objects(original),[]);
  assert.deepEqual(await positions(),beforeCut);
  assert.deepEqual((await source.byIds([hot,review,arrival,late])).map(row=>row.id),[hot,review,arrival,late]);
  await Promise.all([submit(writer,hot),submit(replay,hot)]);
  assert.equal((await writer.unsafe('SELECT count(*)::int AS n FROM record_build_submissions WHERE id=$1',[hot]))[0].n,1);
  assert.equal((await writer.unsafe('SELECT count(*)::int AS n FROM record_build_submissions WHERE id=$1',[late]))[0].n,1);
  await assert.rejects(writer.unsafe('INSERT INTO record_raw_archive_hot(id,at,record) VALUES($1,$2,$3)',[late,at,'{}']),error=>error.code==='42501');
  await assert.rejects(owner.unsafe('UPDATE record_build_submissions SET id=$1 WHERE id=$2',[arrival,hot]),error=>error.code==='55000');
  assert.equal((await writer.unsafe('SELECT NOT EXISTS(SELECT FROM record_build_submissions WHERE id=$1) AS absent',[review]))[0].absent,false);
  const captured=await source.byIds([hot,late,arrival]);
  await assert.rejects(writer.unsafe('SELECT build_ack_submissions($1)',[JSON.stringify([{...captured[0],record:'{"wrong":1}'}])]),/changed/);
  await writer.unsafe('SELECT build_ack_submissions($1)',[JSON.stringify(captured)]);
  await owner.unsafe('DELETE FROM record_build_submissions WHERE id=$1',[review]);
  const remaining=await positions();
  assert(!remaining.some(row=>[hot,review,late,arrival].includes(row.id)));
  console.log('ok: the same source OID, inherited hot reads, cross-session deduplication and exact ACK preserve both FIFO tiers');
}
try{
  await owner.unsafe('CREATE SCHEMA '+schema+'; CREATE ROLE '+role+' NOLOGIN; SET search_path='+schema+',pg_catalog,pg_temp').simple();
  await owner.unsafe(ddl).simple();
  await owner.unsafe('GRANT USAGE ON SCHEMA '+schema+' TO '+role+'; GRANT EXECUTE ON FUNCTION raw_archive_next(),raw_archive_consume(uuid,bigint,text,text[],text) TO '+role).simple();
  const url=new URL(connection); url.searchParams.set('options','-c search_path='+schema+',pg_catalog,pg_temp -c role='+role+' -c transaction_timeout=10000 -c lock_timeout=250');
  process.env.PG_URL=url.toString();
  await owner.unsafe('SELECT raw_archive_begin($1,$2)',[generation,fence]);
  await assert.rejects(query('SELECT raw_archive_activate(?::uuid,?::text,0)',[generation,fence]),/permission denied/);
  await assert.rejects(owner.unsafe('SELECT raw_archive_seal_part($1,$2,$3,$4,2,$5)',[generation,2,parts[1].first,parts[1].last,parts[1].digest]),error=>error.code==='55000');
  for(const p of parts)await owner.unsafe('SELECT raw_archive_seal_part($1,$2,$3,$4,2,$5)',[generation,p.part,p.first,p.last,p.digest]);
  await owner.unsafe("INSERT INTO record_build_submissions(id,at,record) VALUES('positioned-hot',$1,'{}')",[at]);
  await owner.unsafe("CREATE FUNCTION forbid_raw_archive_hydration() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'raw hydration attempted'; END $$; CREATE TRIGGER forbidden_raw_hydration BEFORE INSERT ON record_build_submissions FOR EACH ROW EXECUTE FUNCTION forbid_raw_archive_hydration()").simple();
  assert.equal((await query('SELECT raw_archive_next() AS part'))[0].part,null);
  await assert.rejects(consume(parts[0]),error=>error.code==='55000');
  await assert.rejects(owner.unsafe('SELECT raw_archive_activate($1,$2,2)',[generation,'d'.repeat(64)]),error=>error.code==='55000');
  await owner.unsafe('SELECT raw_archive_activate($1,$2,2)',[generation,fence]);
  const before=await snapshot();
  await assert.rejects(consumeArchive(parts[0].body,'f'.repeat(64),native,5000),/checksum/);
  await assert.rejects(consume(parts[1]),error=>error.code==='55000');
  await owner.unsafe("CREATE FUNCTION refuse_archive_progress() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'archive progress write refused'; END $$; CREATE TRIGGER refuse_progress BEFORE UPDATE ON state_raw_archive_generation FOR EACH ROW EXECUTE FUNCTION refuse_archive_progress()").simple();
  await assert.rejects(consume(parts[0]),/archive progress write refused/);
  assert.deepEqual(await snapshot(),before);
  await owner.unsafe('DROP TRIGGER refuse_progress ON state_raw_archive_generation');
  assert.deepEqual(await consume(parts[0]),{committed:true,replay:false,rows:2});
  const first=await snapshot();
  assert.deepEqual(await consume(parts[0]),{committed:true,replay:true,rows:2});
  assert.deepEqual(await snapshot(),first);
  assert.deepEqual(await consume(parts[1]),{committed:true,replay:false,rows:2});
  assert.deepEqual(await snapshot(),{phase:'complete',next_part:3,builds:4,intents:8,hot:1,positions:1});
  assert.equal((await query('SELECT raw_archive_next() AS part'))[0].part,null);
  console.log('ok: sealed parts use Native custody and atomic sequential receipts without hydrating raw inputs or changing FIFO positions');
  await moveFixture();
}finally{
  await close();
  if(oldURL===undefined)delete process.env.PG_URL;else process.env.PG_URL=oldURL;
  try{
    await writer?.end({timeout:5});await replay?.end({timeout:5});
    await owner.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE; DROP ROLE IF EXISTS '+role+'; DROP ROLE IF EXISTS '+writerRole).simple();
  }
  finally{await owner.end({timeout:5});rmSync(scratch,{recursive:true,force:true});}
}
