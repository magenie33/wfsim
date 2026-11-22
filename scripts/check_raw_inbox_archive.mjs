// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash,randomUUID } from 'node:crypto';
import { mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter,join,resolve } from 'node:path';
import { gunzipSync } from 'node:zlib';
import postgres from './pg/node_modules/postgres/src/index.js';
import { consumeArchive } from './raw_inbox_archive_consume.mjs';
import { close,query } from './db.mjs';
import { boardRequestedSchema } from './board_requested_schema.mjs';
import { buildBatches } from './build_batches.mjs';
import { inboxSource } from './inbox_reader.mjs';
import { placeholders } from '../worker/placeholders.js';

const connection=new URL(process.env.WFSIM_RAW_ROLLOVER_TEST_URL);
const restore=new URL(process.env.WFSIM_RAW_ROLLOVER_RESTORE_URL);
const precopy=process.argv[2]==='--precopy';
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
  'elif action=="deletefile":',
  ' path.unlink()',
  ' if fault=="lost-delete": sys.exit(1)',
  'else: raise RuntimeError("unexpected archive transport")',
].join('\n')+'\n',{mode:0o700});
const realPsql=spawnSync('which',['psql'],{encoding:'utf8'}).stdout.trim();
assert(realPsql);
writeFileSync(join(scratch,'psql'),[
  '#!/usr/bin/env python3',
  'import os,subprocess,sys',
  'body=sys.stdin.buffer.read()',
  'fault=os.environ.get("ARCHIVE_TEST_FAULT")',
  'if fault=="before-accept" and b"raw_archive_move_accept(" in body: sys.exit(1)',
  'run=subprocess.run([os.environ["ARCHIVE_REAL_PSQL"],*sys.argv[1:]],input=body,capture_output=True)',
  'if fault=="lost-accept" and b"raw_archive_move_accept(" in body and run.returncode==0:',
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
  const objectDir=gen=>join(scratch,'wfsim-pg-backup','raw-inbox-rollover-v1',gen);
  const objects=gen=>readdirSync(objectDir(gen));
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
  if(precopy) {
    const ack=(tx,captured)=>tx.unsafe('SELECT build_ack_submissions($1)',[JSON.stringify(captured)]);
    const warm='08000000-0000-4000-8000-000000000001';
    const freshArrival='90000000-0000-8000-8000-000000000001';
    const raced='07000000-0000-4000-8000-000000000001', gained='07500000-0000-4000-8000-000000000001';
    const [settled,claimed,survivor,settledPart,tail]=[1,2,3,4,5].map(n=>'10000000-0000-4000-8000-00000000000'+n);
    const content=i=>{
      const input=JSON.parse(rows[0].input.record);
      input.mods.splice(i,1);
      return JSON.stringify(input);
    };
    const pending=async queryIds=>(await source.byIds(queryIds)).map(row=>row.id);
    const skips=async(gen=original)=>(await owner.unsafe('SELECT id::text FROM record_raw_archive_skips WHERE generation=$1 ORDER BY id',[gen])).map(row=>row.id);
    const hotIds=async()=>(await owner.unsafe('SELECT id FROM ONLY record_raw_archive_hot ORDER BY id')).map(row=>row.id);
    async function ready(captured){
      const done=join(scratch,'live.done'), builds=join(scratch,'live.ndjson');
      const result=spawnSync(native,['--deadline','15','--done',done],{
        input:captured.map(row=>JSON.stringify(row)).join('\n')+'\n',encoding:'utf8',timeout:20000,maxBuffer:8388608,
      });
      assert.equal(result.status,0,result.stderr);
      assert.deepEqual(readFileSync(done,'utf8').trim().split('\n'),captured.map(row=>row.id));
      writeFileSync(builds,result.stdout);
      const batch=[];
      for await(const statement of buildBatches(builds))batch.push(statement);
      return async(tx,proofs=captured)=>{
        for(const statement of batch) await tx.unsafe(placeholders(statement.sql),statement.params);
        await ack(tx,proofs);
      };
    }
    await owner.unsafe('GRANT EXECUTE ON FUNCTION build_intake_batch(text) TO '+writerRole+
      '; GRANT INSERT ON state_build_inbox_positions TO '+writerRole+
      '; GRANT USAGE ON SEQUENCE compute_queue_position TO '+writerRole).simple();
    await ack(writer,await source.byIds(['positioned-hot']));
    await submit(writer,warm,content(0));
    await owner.unsafe('ALTER TABLE record_build_submissions DISABLE TRIGGER build_inbox_positioned');
    for(const [i,id] of [settled,claimed,survivor,settledPart,tail].entries()) await submit(writer,id,content(i+1));
    await submit(writer,raced,content(7));
    await submit(writer,gained,content(0));
    await owner.unsafe('ALTER TABLE record_build_submissions ENABLE TRIGGER build_inbox_positioned');
    const parent=await oldRelation();
    const writerFile=join(scratch,'writers.json');
    const writers={wfsim:fence,factoryd:fence,pipeline:fence};
    function writerProof(gen, verified=writers){
      const body=JSON.stringify({v:1,generation:gen,source:'a'.repeat(40),legacy_raw_writers:false,writers:verified});
      writeFileSync(writerFile,body);
      return createHash('sha256').update(body).digest('hex');
    }
    const start=gen=>['begin-precopy-clean',gen,'--writer-file',writerFile];
    writerProof(original,{wfsim:fence,factoryd:fence});
    assert.throws(()=>cli(...start(original)),/verify the deployed writers/);
    writerProof(original);
    assert.throws(()=>cli(...start(original)),/55000/);
    await owner.unsafe('SELECT raw_archive_move_attach_arrivals()');
    await submit(replay,freshArrival,content(6));
    const copyOnline=copy(1);
    copyOnline[copyOnline.indexOf('--rows')+1]='4';
    writerProof(attempt);
    cli(...start(attempt));
    failCli('before-accept',...copyOnline);
    const [prepared]=await transfer(attempt);
    const verified=gunzipSync(readFileSync(join(objectDir(attempt),prepared.part+'-'+prepared.digest+'.json.gz'))).toString('utf8');
    const racingAck=await ready(await source.byIds([raced]));
    const pids=await Promise.all([owner,writer,replay].map(async db=>(await db.unsafe('SELECT pg_backend_pid() AS pid'))[0].pid));
    const observer=postgres(connection.toString(),{max:1,onnotice(){}});
    let acceptStarted,finishAccept;
    const acceptingFirst=new Promise(resolve=>{acceptStarted=resolve;});
    const holdAccept=new Promise(resolve=>{finishAccept=resolve;});
    const publication=owner.begin(async tx=>{
      await tx.unsafe('SELECT raw_archive_move_accept($1,$2,4,12288)',[verified,prepared.digest]);
      acceptStarted();
      await holdAccept;
    });
    await Promise.race([acceptingFirst,publication]);
    const operations=Promise.allSettled([
      writer.begin(tx=>racingAck(tx)),
      replay.unsafe('INSERT INTO state_build_inbox_positions(id) VALUES($1)',[gained]),
    ]);
    try {
      let blocked=false;
      for(let i=0;i<80;i++){
        const [state]=await observer.unsafe('SELECT $1::int=ANY(pg_blocking_pids($2::int)) AND $1::int=ANY(pg_blocking_pids($3::int)) AS blocked',pids);
        if(state.blocked){blocked=true;break;}
        await new Promise(resolve=>setTimeout(resolve,20));
      }
      assert(blocked,'legacy ACK and new owner wait for the accepted range');
    } finally {
      finishAccept();
      await observer.end({timeout:5});
      await publication;
      for(const result of await operations) assert.equal(result.status,'fulfilled',result.reason?.message);
    }
    assert.deepEqual(await skips(attempt),[raced,gained]);
    assert.deepEqual(await hotIds(),[gained,warm]);
    assert.deepEqual(await pending([raced,gained]),[gained]);
    const visible=[gained,warm,settled,claimed,survivor,settledPart,tail,freshArrival];
    const beforeAbort=await source.byIds(visible), fifoBeforeAbort=await positions();
    const grants=()=>writer.unsafe("SELECT has_table_privilege('state_build_inbox_inputs','SELECT') AS reads,"+
      "has_function_privilege('build_ack_submissions(text)','EXECUTE') AS acknowledges,"+
      "has_table_privilege('record_raw_archive_arrivals','INSERT') AS child_write");
    const aclBefore=await grants();
    assert.deepEqual([...aclBefore],[{reads:true,acknowledges:true,child_write:false}]);
    cli('abort-clean',attempt,'--work',scratch);
    assert.deepEqual(await oldRelation(),parent);
    assert.deepEqual(await source.byIds(visible),beforeAbort);
    assert.deepEqual(await positions(),fifoBeforeAbort);
    assert.deepEqual(await grants(),aclBefore);
    assert.deepEqual(await hotIds(),[]);
    console.log('ok: the accepted range fences waiting ACK and ownership writes, and abort preserves the live views, ACL and FIFO');
    failCli('lost-delete',...retire(attempt));
    assert.deepEqual(await skips(attempt),[raced,gained]);
    assert.deepEqual(objects(attempt),[]);
    assert.equal(cli(...retire(attempt)).length,1);
    assert.deepEqual(await skips(attempt),[]);
    assert.equal((await owner.unsafe("SELECT pg_relation_size('record_raw_archive_skips')::int AS n"))[0].n,0);
    assert.deepEqual(objects(attempt),[]);
    await ack(writer,await source.byIds([gained]));
    const writerDigest=writerProof(original);
    cli(...start(original));
    assert.equal(cli('status-clean')[0].writer_receipt,writerDigest);
    await assert.rejects(submit(writer,cold),error=>error.code==='23505');
    await assert.rejects(owner.unsafe('UPDATE record_build_submissions SET record=$1 WHERE id=$2',['{}',survivor]),error=>error.code==='55000');
    assert.throws(()=>cli('install-clean','--tablespace','pg_default','--http-role',writerRole,'--consume-role',role),/P0001/);
    const [first]=cli(...copyOnline);
    assert.deepEqual([first.after,first.hot,first.cold],[survivor,1,3]);
    assert.deepEqual(await hotIds(),[warm]);
    assert.deepEqual(await pending([warm,settled,claimed,survivor,tail]),[warm,settled,claimed,survivor,tail]);
    await submit(replay,freshArrival,content(6));
    const captured=await source.byIds([warm,settled,freshArrival]);
    const settle=await ready(captured);
    const fifo=await positions();
    await assert.rejects(writer.begin(tx=>settle(tx,[...captured.slice(0,2),{...captured[2],record:'{}'}])),/changed/);
    assert.deepEqual(await hotIds(),[warm]);
    assert.deepEqual(await skips(),[]);
    assert.deepEqual(await pending([warm,settled,freshArrival]),[warm,settled,freshArrival]);
    assert.deepEqual(await positions(),fifo);
    await writer.begin(tx=>settle(tx));
    assert.deepEqual(await hotIds(),[]);
    assert.deepEqual(await skips(),[settled]);
    assert.deepEqual(await pending([warm,settled,freshArrival]),[]);
    await owner.unsafe('INSERT INTO state_build_inbox_positions(id) VALUES($1)',[claimed]);
    assert.deepEqual(await hotIds(),[claimed]);
    assert.deepEqual(await skips(),[settled,claimed]);
    const copyOne=[...copyOnline];
    copyOne[copyOne.indexOf('--rows')+1]='1';
    const [next]=cli(...copyOne);
    assert.deepEqual([next.after,next.cold],[settledPart,1]);
    const finishPart=await ready(await source.byIds([settledPart]));
    await writer.begin(tx=>finishPart(tx));
    const positionsBefore=await positions();
    failCli('readback',...copyOnline);
    const [outstanding]=await transfer(original);
    const [{body}]=await owner.unsafe('SELECT raw_archive_move_page(4,12288)::text AS body');
    const last=await ready(await source.byIds([tail]));
    let notify,release;
    const holding=new Promise(resolve=>{notify=resolve;});
    const wait=new Promise(resolve=>{release=resolve;});
    const live=writer.begin(async tx=>{await last(tx);notify();await wait;});
    await Promise.race([live,holding]);
    const accepting=owner.unsafe('SELECT raw_archive_move_accept($1,$2,4,12288)',[body,outstanding.digest]).then(()=>null,error=>error);
    try {
      assert.equal(await Promise.race([accepting,new Promise(resolve=>setTimeout(()=>resolve('waiting'),70))]),'waiting');
    } finally {release();await live;}
    assert.equal((await accepting).code,'55000');
    assert.equal(cli('status-clean')[0].after,settledPart);
    assert.deepEqual(await transfer(original),[outstanding]);
    assert.deepEqual(cli(...copyOnline),[]);
    const fenceDigest=fenced(original);
    await assert.rejects(owner.unsafe('SELECT raw_archive_move_cut($1,$2,$3)',[original,fenceDigest,'b'.repeat(40)]),error=>error.code==='55000');
    assert.deepEqual(await oldRelation(),parent);
    const [cut]=cli('cut-clean',original,'--fence-file',fenceFile,...budgets,'--peak-root-bytes','1');
    assert(cut.old_source_bytes>0);
    assert.equal((await oldRelation()).oid,parent.oid);
    for(const name of [parent.heap,parent.pk]){
      assert.equal((await owner.unsafe('SELECT coalesce((pg_stat_file($1,true)).size,0)::int AS bytes',[name]))[0].bytes,0);
    }
    assert.deepEqual(await positions(),positionsBefore);
    assert.deepEqual(await pending([claimed,survivor]),[claimed]);
    assert.equal(cli(...retire(original))[0].digest,outstanding.digest);
    assert.deepEqual(cli(...retire(original)),[]);
    const [{digest},{digest:emptyDigest}]=await owner.unsafe('SELECT digest FROM record_raw_archive_parts WHERE generation=$1 ORDER BY part',[original]);
    const sealed=gunzipSync(readFileSync(join(objectDir(original),'1-'+digest+'.json.gz'))).toString('utf8');
    await assert.rejects(owner.unsafe('SELECT raw_archive_consume($1,1,$2,$3::text[],\'[]\')',[original,sealed,[settled,claimed,survivor]]),
      error=>error.code==='22023');
    assert.deepEqual(await skips(),[settled,claimed,settledPart]);
    const [received]=cli('consume',...budgets,'--native',native,'--peak-root-bytes','1','--statement-ms','5000');
    assert.deepEqual([received.generation,received.committed,received.rows],[original,true,3]);
    assert.deepEqual(await skips(),[settledPart]);
    assert.equal(cli(...retire(original))[0].digest,digest);
    const [empty]=cli('consume',...budgets,'--native',process.execPath,'--peak-root-bytes','1','--statement-ms','5000');
    assert.deepEqual([empty.generation,empty.committed,empty.rows],[original,true,1]);
    assert.deepEqual(await skips(),[]);
    assert.equal((await owner.unsafe("SELECT pg_relation_size('record_raw_archive_skips')::int AS n"))[0].n,0);
    const consumed=await owner.unsafe('SELECT DISTINCT source_id FROM relation_build_intake_sources WHERE source_id IN ($1,$2,$3,$4) ORDER BY source_id',[settled,claimed,survivor,tail]);
    assert.deepEqual(consumed.map(row=>row.source_id),[settled,survivor,tail]);
    assert.equal(cli(...retire(original))[0].digest,emptyDigest);
    assert.deepEqual(objects(original),[]);
    assert.deepEqual(await pending([claimed,survivor]),[claimed]);
    console.log('ok: online pre-copy preserves live ACK and FIFO, protects a new owner, and retires exact parts after atomic custody');
    return;
  }
  await submit(writer,hot);
  await submit(writer,review);
  await owner.unsafe('ALTER TABLE record_build_submissions DISABLE TRIGGER build_inbox_positioned');
  await submit(writer,cold,rows[0].input.record);
  await submit(writer,held,rows[1].input.record);
  await owner.unsafe('ALTER TABLE record_build_submissions ENABLE TRIGGER build_inbox_positioned');
  await owner.unsafe("INSERT INTO state_build_family_inbox(id,retry_at,input) VALUES($1,$2,'{}')",[held,at]);
  const before=await oldRelation();
  const stagedLegacy='00000000-0000-4000-8000-000000000009';
  const stagedArrival='00000000-0000-8000-8000-000000000009';
  await owner.unsafe('SELECT raw_archive_move_attach_arrivals()');
  for (const id of [stagedLegacy,stagedArrival])
    await Promise.all([submit(writer,id),submit(replay,id)]);
  assert.deepEqual([...await owner.unsafe(`SELECT id,tableoid::regclass::text AS storage
    FROM record_build_submissions WHERE id IN($1,$2) ORDER BY id`,[stagedLegacy,stagedArrival])],[
    {id:stagedLegacy,storage:'record_build_submissions'},{id:stagedArrival,storage:'record_raw_archive_arrivals'},
  ]);
  const staged=await source.byIds([stagedLegacy,stagedArrival]);
  assert.equal(staged.length,2);
  const stagedPositions=async()=>(await positions()).filter(row=>[stagedLegacy,stagedArrival].includes(row.id));
  assert.equal((await stagedPositions()).length,2);
  await writer.unsafe('SELECT build_ack_submissions($1::text)',[JSON.stringify(staged)]);
  assert.equal((await source.byIds([stagedLegacy,stagedArrival])).length,0);
  assert.deepEqual(await stagedPositions(),[]);
  assert.deepEqual(await oldRelation(),before);
  console.log('ok: idle attachment routes new arrivals while both writer generations still intake and acknowledge');
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
if(!precopy){
  const online=spawnSync(process.execPath,[process.argv[1],'--precopy'],{stdio:'inherit',timeout:90000});
  assert.equal(online.status,0,online.error?.message);
}
try{
  await owner.unsafe('CREATE SCHEMA '+schema+'; CREATE ROLE '+role+' NOLOGIN; SET search_path='+schema+',pg_catalog,pg_temp').simple();
  await owner.unsafe(ddl).simple();
  await owner.unsafe('GRANT USAGE ON SCHEMA '+schema+' TO '+role+'; GRANT EXECUTE ON FUNCTION raw_archive_next(),raw_archive_pending(uuid,bigint,text),raw_archive_consume(uuid,bigint,text,text[],text) TO '+role).simple();
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
