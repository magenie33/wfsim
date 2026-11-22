// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { randomUUID,createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdirSync,mkdtempSync,readFileSync,writeFileSync,rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join,resolve } from 'node:path';
import postgres from './pg/node_modules/postgres/src/index.js';
import { buildInboxSchemaSql } from './build_inbox_schema.mjs';
import { buildBatches } from './build_batches.mjs';
import { captureInbox,inboxSource } from './inbox_reader.mjs';
import { boardRequestedSchema } from './board_requested_schema.mjs';
import { placeholders,pgDb,postgresClient } from '../worker/db.js';
import { inboxSourceId,INBOX_GENERATION_MS } from '../worker/inbox_source.js';
import worker from '../worker/index.js';
import { FAMILY_PRODUCERS } from '../worker/board_family.js';
import { NEXT_INBOX_GENERATION_SQL } from './inbox_storage.mjs';
import { requestedBoardPass } from './board_requested_intake.mjs';

const address=new URL(process.env.WFSIM_INTAKE_PG_URL||process.env.WFSIM_RAW_ROLLOVER_TEST_URL);
assert(['127.0.0.1','localhost'].includes(address.hostname)&&!address.password,'use an isolated local PostgreSQL');
const native=resolve(process.env.WFSIM_INTAKE_BIN||'target/debug/wfsim-intake');
const schema='inbox_'+randomUUID().replaceAll('-','');
const http=schema+'_http', pipeline=schema+'_pipeline', typed=schema+'_typed';
const scratch=mkdtempSync(join(tmpdir(),'wfsim-inbox-storage-'));
const baseline=JSON.parse(readFileSync(new URL('../tests/fixtures/inbox-c41/source.json',import.meta.url),'utf8'));
const frozen=name=>{
  const body=readFileSync(new URL('../tests/fixtures/inbox-c41/'+name+'.sql',import.meta.url),'utf8');
  assert.equal(createHash('sha256').update(body).digest('hex'),baseline.files[name+'.sql']);return body;
};
const connect=role=>postgres(address.toString(),{max:1,onnotice(){},connection:{search_path:schema+',pg_catalog,pg_temp',
  lock_timeout:1000,statement_timeout:20000,...role&&{role}}});
const owner=connect(), writer=connect(http), competitor=connect(http), reader=connect(pipeline), typedRunner=connect(typed);
const source=inboxSource((text,args)=>reader.unsafe(placeholders(text),args));
const record=JSON.stringify(JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url),'utf8'))[0].record);
const at='2026-11-22';
const query=(text,args=[])=>owner.unsafe(text,args);
const one=async(text,args=[])=>(await query(text,args))[0];
const positions=()=>query('SELECT id,position::text,generation::text,payload_generation::text FROM state_build_inbox_positions ORDER BY id');
const generation=Math.floor(Date.now()/INBOX_GENERATION_MS);
const sourceGenerations=new Map();
const key=(offset=0)=>{const id=inboxSourceId(randomUUID());sourceGenerations.set(id,generation+offset);return id;};
const input=id=>({id,at,record,initiator:'anonymous',family_source:null,observation:null});
const submit=(pg,row,birth=sourceGenerations.get(row.id)||generation)=>pg.unsafe('SELECT build_inbox_submit($1,$2,$3,$4,$5,$6,$7)',
  [row.id,row.at,row.record,row.initiator,row.family_source,row.observation,birth]);
const ack=(tx,rows)=>tx.unsafe('SELECT build_ack_submissions($1)',[JSON.stringify(rows)]);
async function normalized(rows) {
  const done=join(scratch,'done'), file=join(scratch,'builds.ndjson');
  const result=spawnSync(native,['--deadline','30','--done',done],{
    input:rows.map(row=>JSON.stringify(row)).join('\n')+'\n',encoding:'utf8',timeout:40000,maxBuffer:16*1024*1024});
  assert.equal(result.status,0,result.stderr||result.error?.message);
  assert.deepEqual(readFileSync(done,'utf8').split('\n').filter(Boolean),rows.map(row=>row.id));
  writeFileSync(file,result.stdout);
  const canonical=result.stdout.split('\n').filter(Boolean).map(JSON.parse);
  const accepted=new Set(canonical.flatMap(row=>row.from));
  const proof=rows.map(row=>accepted.has(row.id)?row:{...row,rejected:true});
  const operations=[];
  for await(const operation of buildBatches(file))operations.push(operation);
  return async(tx,captured=proof)=>{
    for(const operation of operations)await tx.unsafe(placeholders(operation.sql),operation.params);
    await ack(tx,captured);
  };
}
let space='pg_default',customSpace=false,adapter;
try {
  await query('CREATE SCHEMA '+schema);
  for(const name of ['schema','library','sync','inventory','build_workflows','board_admission','board_mailbox','board_capture','board_pages'])
    await owner.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8')).simple();
  for(const role of [http,pipeline,typed])await query('CREATE ROLE '+role+' NOLOGIN; GRANT USAGE ON SCHEMA '+schema+' TO '+role);
  const [{current_user:ownerRole}]=await query('SELECT current_user');
  await owner.unsafe(boardRequestedSchema({httpRole:http,runnerRole:typed,readerRole:ownerRole})
    .replace("default_tablespace='wfsim_data'","default_tablespace='pg_default'")).simple();
  await query('GRANT SELECT,INSERT ON record_build_submissions,record_equipment_submissions TO '+http+
    '; GRANT SELECT ON state_build_inbox_inputs,state_build_inbox_positions,state_build_inbox_priorities TO '+pipeline+
    '; GRANT SELECT,INSERT,UPDATE ON entity_equipment_builds TO '+pipeline+
    '; GRANT EXECUTE ON FUNCTION build_intake_batch(text),build_ack_submissions(text),build_family_owns_inbox(text,jsonb) TO '+pipeline);
  if(process.env.WFSIM_INBOX_TEST_TABLESPACE_PATH) {
    const directory=resolve(process.env.WFSIM_INBOX_TEST_TABLESPACE_PATH);
    mkdirSync(directory,{recursive:true});space=schema+'_data';
    await query("CREATE TABLESPACE "+space+" LOCATION '"+directory.replaceAll("'","''")+"'");customSpace=true;
  }
  await query('SET default_tablespace='+space);
  for(const name of ['state_build_inbox_inputs','build_ack_submissions'])await owner.unsafe(frozen(name)).simple();
  await query('DROP TABLE record_build_inbox_payloads; ALTER TABLE state_build_inbox_positions DROP COLUMN generation,DROP COLUMN payload_generation');
  for(const name of ['raw_inbox_archive','raw_inbox_archive_move'])await owner.unsafe(frozen(name)).simple();
  await query('SELECT raw_archive_move_attach_arrivals()');
  const legacy='20000000-0000-8000-8000-000000000001';
  await writer.unsafe('INSERT INTO record_build_submissions(id,at,record) VALUES($1,$2,$3)',[legacy,at,record]);
  await query('ALTER TABLE record_raw_archive_hot INHERIT record_build_submissions');
  await query("INSERT INTO record_raw_archive_hot(id,at,record) VALUES('offline:protected',$1,$2)",[at,record]);
  await query("INSERT INTO state_build_inbox_positions(id) VALUES('offline:protected')");
  const protectedInput=await one("SELECT to_jsonb(b) AS input,pg_relation_filepath('record_raw_archive_hot') AS file FROM ONLY record_raw_archive_hot b WHERE id='offline:protected'");
  const oldArrivals=await one("SELECT pg_relation_filepath('record_raw_archive_arrivals') AS file");
  const originalParent=await one("SELECT 'record_build_submissions'::regclass::oid AS oid,pg_relation_filepath('record_build_submissions') AS path");
  const originalFIFO=Array.from(await query('SELECT id,position::text FROM state_build_inbox_positions ORDER BY id'));
  const attrs=await one("SELECT to_jsonb(p)-'prosrc' AS value FROM pg_proc p WHERE oid='build_ack_submissions(text)'::regprocedure");
  const install=buildInboxSchemaSql({tablespace:space,schema,writerRole:http,pipelineRole:pipeline});
  await owner.unsafe(install).simple();
  await owner.unsafe(install).simple();
  assert.deepEqual(await one("SELECT to_jsonb(p)-'prosrc' AS value FROM pg_proc p WHERE oid='build_ack_submissions(text)'::regprocedure"),attrs);
  assert.deepEqual((await positions()).map(({id,position})=>({id,position})),originalFIFO);
  assert.equal((await one('SELECT enabled FROM state_build_inbox_storage')).enabled,false);
  await assert.rejects(submit(writer,input(key())),error=>error.code==='55000');
  await query('SET default_tablespace=pg_default');
  for(const part of [generation-1,generation,generation+1])await query('SELECT build_inbox_prepare($1)',[part]);
  if(customSpace)for(const row of await query(`SELECT pg_relation_filepath(c.oid) AS file FROM pg_class c WHERE c.oid IN(
    SELECT original FROM state_build_inbox_generations UNION ALL SELECT i.indexrelid FROM pg_index i
    JOIN state_build_inbox_generations g ON g.original=i.indrelid)`))assert(row.file.startsWith('pg_tblspc/'),row.file);
  await query('SELECT build_inbox_enable($1,false)',['c'.repeat(64)]);
  assert.match(readFileSync(new URL('../deploy/wfsim-inbox-storage.service',import.meta.url),'utf8'),
    /ExecStart=\/usr\/bin\/node scripts\/inbox_storage\.mjs tick --capacity \/etc\/wfsim-inbox-storage\.capacity\.json/);
  assert.match(readFileSync(new URL('../deploy/wfsim-inbox-storage.timer',import.meta.url),'utf8'),/OnUnitActiveSec=1min/);
  if(customSpace) {
    const ownerURL=new URL(address);ownerURL.searchParams.set('options','-c search_path='+schema+',pg_catalog,pg_temp');
    const capacityFile=join(scratch,'capacity.json'),readinessFile=join(scratch,'readiness.json');
    writeFileSync(capacityFile,JSON.stringify({v:1,work:resolve(process.env.WFSIM_INBOX_TEST_TABLESPACE_PATH),root_reserve_bytes:1,data_reserve_bytes:1,rows:64,seconds:10}));
    writeFileSync(readinessFile,JSON.stringify({v:1,source:'a'.repeat(40),...Object.fromEntries(['schema','worker','live','hourly','typed','native'].map(key=>[key,'c'.repeat(64)]))}));
    const storage=(...args)=>{
      const result=spawnSync(process.execPath,['scripts/inbox_storage.mjs',...args],{encoding:'utf8',timeout:15000,
        env:{...process.env,PG_URL:ownerURL.toString()}});
      assert.equal(result.status,0,result.stderr);return JSON.parse(result.stdout);
    };
    assert.equal(storage('tick','--capacity',capacityFile).paused,'automatic maintenance is disabled');
    assert.equal(storage('enable','--readiness',readinessFile,'--automatic').automatic,true);
    assert.deepEqual(storage('tick','--capacity',capacityFile),{receipts:[]});
  }
  await assert.rejects(writer.unsafe('INSERT INTO record_build_submissions(id,at,record) VALUES($1,$2,$3)',
    ['30000000-0000-8000-8000-000000000001',at,record]),error=>error.code==='55000');
  const incoming=Array.from({length:257},()=>input(key()));
  for(const row of incoming)await submit(writer,row);
  const tail=input(key()), refused={...input(key()),record:'{}'};
  await submit(writer,tail);await submit(writer,refused);
  const before=await positions();
  await Promise.all([submit(writer,incoming[0]),submit(competitor,incoming[0])]);
  await assert.rejects(submit(writer,{...incoming[0],record:'{}'}),error=>error.code==='23505');
  await assert.rejects(submit(competitor,incoming[0],generation+1),error=>error.code==='23505');
  assert.deepEqual(await positions(),before);
  assert.deepEqual((await source.byIds([legacy,...incoming.slice(0,3).map(row=>row.id)])).map(row=>row.id),[legacy,...incoming.slice(0,3).map(row=>row.id)]);
  await assert.rejects(ack(reader,[tail]),error=>error.code==='55000');
  const captured=await source.byIds(incoming.map(row=>row.id));
  const settle=await normalized(captured);
  const buildsBefore=(await one('SELECT count(*)::int AS n FROM entity_weapon_builds')).n;
  await assert.rejects(reader.begin(tx=>settle(tx,[...captured.slice(0,-1),{...captured.at(-1),record:'{}'}])),error=>error.code==='22023');
  assert.deepEqual(await positions(),before);
  assert.equal((await one('SELECT count(*)::int AS n FROM entity_weapon_builds')).n,buildsBefore);
  await reader.begin(tx=>settle(tx));
  await reader.begin(tx=>settle(tx));
  assert.equal((await one("SELECT count(*)::int AS n FROM record_build_inbox_payloads WHERE generation=$1 AND state='accepted'",[generation])).n,257);
  assert.equal((await one('SELECT count(DISTINCT source_id)::int AS n FROM relation_build_intake_sources WHERE source_id=ANY($1::text[])',[incoming.map(row=>row.id)])).n,257);
  await submit(writer,incoming[0]);
  await assert.rejects(submit(writer,{...incoming[0],initiator:'system'}),error=>error.code==='23505');
  const reject=await normalized([refused]);
  await reader.begin(tx=>reject(tx));
  assert.deepEqual(await one('SELECT state,record,at FROM record_build_inbox_payloads WHERE generation=$1 AND id=$2',[generation,refused.id]),
    {state:'refused',record:null,at:null});
  await submit(writer,refused);
  await assert.rejects(submit(competitor,refused,generation+1),error=>error.code==='23505');
  const gear={id:'equipment:storage-fixture',at,record:JSON.stringify({kind:'companion',build:{companion:'sentinel',
    mods:['calculated_redirection','accelerated_deflection','animal_instinct','vacuum','guardian',
      'regen','repair_kit','spare_parts','shield_charger','coolant_leak'].map(id=>({id}))}})};
  const liveRows=[input(key()),{...input(key()),record:'{}'},gear];
  for(const row of liveRows.slice(0,2))await submit(writer,row);
  await writer.unsafe('INSERT INTO record_equipment_submissions(id,at,record) VALUES($1,$2,$3)',[gear.id,gear.at,gear.record]);
  const capture=join(scratch,'live.ndjson');writeFileSync(capture,liveRows.map(row=>JSON.stringify(row)).join('\n')+'\n');
  const liveURL=new URL(address);liveURL.searchParams.set('options','-c search_path='+schema+',pg_catalog,pg_temp -c role='+pipeline);
  const oldAck=(await one("SELECT pg_get_functiondef('build_ack_submissions(text)'::regprocedure) AS body")).body;
  await owner.unsafe(frozen('build_ack_submissions')).simple();
  const live=()=>spawnSync(process.execPath,['scripts/intake_batches.mjs',capture],{encoding:'utf8',timeout:40000,
    env:{...process.env,PG_URL:liveURL.toString(),WFSIM_INTAKE_BIN:native}});
  const downgraded=live();assert.notEqual(downgraded.status,0);
  assert.match(downgraded.stderr,/installed acknowledgement did not release/);
  assert.equal((await source.byIds(liveRows.map(row=>row.id))).length,3);
  assert.equal((await one('SELECT count(*)::int AS n FROM entity_equipment_builds')).n,0);
  await owner.unsafe(oldAck).simple();
  const resumed=live();assert.equal(resumed.status,0,resumed.stderr);
  assert.equal((await source.byIds(liveRows.map(row=>row.id))).length,0);
  assert.equal((await one('SELECT count(*)::int AS n FROM entity_equipment_builds')).n,1);
  assert.equal((await one('SELECT state FROM record_build_inbox_payloads WHERE generation=$1 AND id=$2',[generation,liveRows[1].id])).state,'refused');
  const beforeBridge=await positions();
  await assert.rejects(owner.begin(async tx=>{await tx.unsafe('SELECT build_inbox_adopt_arrivals(64)');throw Error('bridge rollback');}),/bridge rollback/);
  assert.deepEqual(await positions(),beforeBridge);
  assert.equal((await one('SELECT build_inbox_adopt_arrivals(64) AS n')).n,1);
  assert.equal((await one('SELECT count(*)::int AS n FROM ONLY record_raw_archive_arrivals')).n,0);
  assert.equal((await positions()).find(row=>row.id===legacy).position,originalFIFO[0].position);
  await query('SELECT build_inbox_finish_bridge()');
  assert.equal((await one('SELECT coalesce((pg_stat_file($1,true)).size,0)::int AS n',[oldArrivals.file])).n,0);
  assert.deepEqual(await one("SELECT to_jsonb(b) AS input,pg_relation_filepath('record_raw_archive_hot') AS file FROM ONLY record_raw_archive_hot b WHERE id='offline:protected'"),protectedInput);
  const publicDb=pgDb(postgresClient(writer));
  const response=await worker.fetch(new Request('https://wfsim.app/api/board/submit',{method:'POST',
    headers:{'content-type':'application/json'},body:record}),{LIBRARY:publicDb,ASSETS:{fetch:async()=>new Response('fixture')}});
  assert.equal(response.status,200,await response.text());
  const batchResponse=await worker.fetch(new Request('https://wfsim.app/api/board/submit',{method:'POST',
    headers:{'content-type':'application/json'},body:JSON.stringify({builds:[JSON.parse(record),JSON.parse(record)]})}),
    {LIBRARY:publicDb,ASSETS:{fetch:async()=>new Response('fixture')}});
  assert.equal(batchResponse.status,200,await batchResponse.text());
  await assert.rejects(writer.unsafe('SELECT * FROM record_build_inbox_payloads'),error=>error.code==='42501');
  await assert.rejects(reader.unsafe('SELECT build_inbox_retire($1,false)',[generation]),error=>error.code==='42501');
  await query('UPDATE state_board_request_control SET enabled=true');
  const requested=await worker.fetch(new Request('https://wfsim.app/api/board/submit',{method:'POST',
    headers:{'content-type':'application/json',origin:'https://wfsim.app'},body:JSON.stringify({...JSON.parse(record),
      benchmark:'standard_single_target',mode:'base',requested:true})}),{LIBRARY:publicDb,ASSETS:{fetch:async()=>new Response('fixture')}});
  assert.equal(requested.status,200,await requested.clone().text());
  const requestedId=(await requested.json()).request;
  const requestSource=(await one('SELECT source FROM state_board_request_control')).source;
  const requestedStage=await requestedBoardPass(typedRunner,{source:requestSource,binary:native});
  assert.equal(requestedStage.staged,true,JSON.stringify(requestedStage));
  assert.equal((await one('SELECT count(*)::int AS n FROM record_build_inbox_payloads WHERE id=$1',[requestedId])).n,0);
  assert.equal((await one("SELECT phase FROM state_board_requests WHERE id=$1",[requestedId])).phase,'staged');
  const family='f'.repeat(32),familyKey='family:'+family;
  await query("INSERT INTO entity_family_search_requests(id,source,weapon,ruler,request,context,engine,state,created_at) VALUES($1,'fixture',$2,'standard_single_target','{}',$4,'fixture','waiting',$3)",
    [family,JSON.parse(record).weapon,at,JSON.stringify({weapon:JSON.parse(record).weapon,mode:'base',benchmark:'standard_single_target'})]);
  await query('GRANT SELECT,UPDATE ON entity_family_search_requests TO '+http+'; GRANT EXECUTE ON FUNCTION compute_order_family_wanted(text) TO '+http);
  const best={mods:JSON.parse(record).mods,evolutions:JSON.parse(record).evolutions,arcane:JSON.parse(record).arcanes,
    mode:'base',replay:{weapon:JSON.parse(record).weapon},forma:{}};
  await FAMILY_PRODUCERS.family_build.on_fact({LIBRARY:publicDb},{ref:family},{result:{report:{results:[best]}}});
  assert.equal((await one('SELECT state FROM entity_family_search_requests WHERE id=$1',[family])).state,'succeeded');
  assert.equal((await source.byIds([familyKey])).length,1);
  const familyCapture=(await source.byIds([familyKey]))[0];
  await writer.unsafe('SELECT build_inbox_submit_family($1,$2,$3,$4,$5)',
    [familyKey,'2099-01-01',familyCapture.record,'system',family]);
  assert.equal((await source.byIds([familyKey]))[0].at,familyCapture.at);
  await assert.rejects(writer.unsafe('SELECT build_inbox_submit_family($1,$2,$3,$4,$5)',
    [familyKey,familyCapture.at,'{}','system',family]),error=>['22023','23505'].includes(error.code));
  const heldKey=input(key());let staged,release;
  const staging=new Promise(resolve=>{staged=resolve;}),wait=new Promise(resolve=>{release=resolve;});
  const writing=writer.begin(async tx=>{await submit(tx,heldKey);staged();await wait;});
  await Promise.race([writing,staging]);
  try{await assert.rejects(query('SELECT build_inbox_close_generations()'),error=>error.code==='55P03');}
  finally{release();await writing;}
  await assert.rejects(competitor.begin('isolation level repeatable read',async tx=>{
    await tx.unsafe('SELECT id FROM record_build_submissions LIMIT 1');
    await query("UPDATE state_build_inbox_storage SET readiness=repeat('d',64) WHERE id=1");
    await submit(tx,input(key()));
  }),error=>error.code==='40001');
  const carryBefore=await positions();
  await assert.rejects(writer.unsafe("SELECT id FROM build_inbox_scan_ids('',1)"),error=>error.code==='42501');
  await assert.rejects(reader.unsafe('SELECT id FROM record_build_inbox_payloads LIMIT 1'),error=>error.code==='42501');
  const indexedCapture=[];
  const fullSource=await query('SELECT * FROM state_build_inbox_inputs ORDER BY id');
  const scanCursor=fullSource[0].id;
  const withoutOwnership=({owned_family,...row})=>row;
  assert.deepEqual((await source.after(scanCursor,2)).map(withoutOwnership),fullSource.slice(1,3));
  const indexedReceipt=await captureInbox({source,maxRows:5,pageRows:2,emit:(_line,row)=>indexedCapture.push(row.id)});
  assert.equal(indexedReceipt.rows,Math.min(5,fullSource.length));
  assert.equal(new Set(indexedCapture).size,indexedReceipt.rows);
  assert(!indexedReceipt.read_error);
  await query('CREATE OR REPLACE FUNCTION build_inbox_current_generation() RETURNS bigint LANGUAGE sql VOLATILE AS $$ SELECT '+(generation+2)+'::bigint $$');
  await query('SELECT build_inbox_close_generations()');
  await assert.rejects(submit(writer,refused),error=>error.code==='55000');
  await assert.rejects(submit(writer,incoming[0]),error=>error.code==='55000');
  await assert.rejects(query('SELECT build_inbox_retire($1,false)',[generation]),error=>error.code==='55000');
  await query('SELECT build_inbox_prepare_retained($1)',[generation]);
  await assert.rejects(owner.begin(async tx=>{await tx.unsafe('SELECT build_inbox_carry($1,64)',[generation]);throw Error('carry rollback');}),/carry rollback/);
  assert.deepEqual(await positions(),carryBefore);
  const moved=(await one('SELECT build_inbox_carry($1,64) AS n',[generation])).n;
  assert(moved>=2);
  assert.equal((await one('SELECT build_inbox_carry($1,64) AS n',[generation])).n,0);
  assert.deepEqual((await positions()).map(({payload_generation,...row})=>row),carryBefore.map(({payload_generation,...row})=>row));
  assert((await source.after(tail.id.slice(0,-1),10)).some(row=>row.id===tail.id),'carried generation stays in the indexed scan');
  await assert.rejects(owner.begin(async tx=>{
    await tx.unsafe('UPDATE state_build_inbox_generations SET original=(SELECT original FROM state_build_inbox_generations WHERE generation=$2) WHERE generation=$1',[generation,generation+1]);
    await tx.unsafe('SELECT build_inbox_retire($1,false)',[generation]);
  }),error=>error.code==='55000');
  const released=await one('SELECT build_inbox_retire($1,false) AS receipt',[generation]);
  assert(released.receipt.bytes>0);
  for(const path of released.receipt.files)assert.equal((await one('SELECT coalesce((pg_stat_file($1,true)).size,0)::int AS n',[path])).n,0);
  assert.deepEqual(await one("SELECT 'record_build_submissions'::regclass::oid AS oid,pg_relation_filepath('record_build_submissions') AS path"),originalParent);
  const newer=generation+2;
  await query('SELECT build_inbox_prepare($1)',[newer]);
  const newerRow=input(key(2));await submit(writer,newerRow);
  const newerFinish=await normalized([newerRow]);await reader.begin(tx=>newerFinish(tx));
  await query('CREATE OR REPLACE FUNCTION build_inbox_current_generation() RETURNS bigint LANGUAGE sql VOLATILE AS $$ SELECT '+(generation+4)+'::bigint $$');
  await query('SELECT build_inbox_close_generations()');
  for(const row of await query('SELECT generation::text FROM state_build_inbox_generations WHERE generation<$1 AND generation<>$2 ORDER BY generation',[newer,generation]))
    await query('SELECT build_inbox_retire($1,false)',[Number(row.generation)]);
  assert.equal((await one(NEXT_INBOX_GENERATION_SQL)).generation,String(newer));
  const newerRetired=(await one('SELECT build_inbox_retire($1,false) AS receipt',[newer])).receipt;
  assert(newerRetired.bytes>0);
  for(const path of newerRetired.files)assert.equal((await one('SELECT coalesce((pg_stat_file($1,true)).size,0)::int AS n',[path])).n,0);
  assert((await source.byIds([tail.id])).length===1,'an older live tail cannot starve later reclamation');
  const remaining=await source.byIds((await positions()).filter(row=>row.generation===String(generation)).map(row=>row.id));
  const finish=await normalized(remaining);
  await reader.begin(tx=>finish(tx));
  const retained=(await one('SELECT build_inbox_retire($1,true) AS receipt',[generation])).receipt;
  assert(retained.bytes>0);
  for(const path of retained.files)assert.equal((await one('SELECT coalesce((pg_stat_file($1,true)).size,0)::int AS n',[path])).n,0);
  assert.equal((await one('SELECT count(*)::int AS n FROM state_build_inbox_generations WHERE generation=$1',[generation])).n,0);
  assert.equal((await one('SELECT count(*)::int AS n FROM record_build_inbox_payloads WHERE generation IN($1::bigint,-$1::bigint)',[generation])).n,0);
  await query('SELECT build_inbox_prepare($1)',[generation+4]);
  await assert.rejects(submit(writer,incoming[0],generation+4),error=>error.code==='23505');
  await assert.rejects(submit(writer,refused),error=>error.code==='55000');
  await assert.rejects(submit(writer,{...refused,record}),error=>error.code==='55000');
  await query('CREATE OR REPLACE FUNCTION build_inbox_current_generation() RETURNS bigint LANGUAGE sql VOLATILE AS $$ SELECT '+generation+'::bigint $$');
  await assert.rejects(submit(writer,refused),error=>error.code==='55000');
  await assert.rejects(writer.unsafe('SELECT build_inbox_submit_family($1,$2,$3,$4,$5)',
    [familyKey,familyCapture.at,familyCapture.record,'system',family]),error=>error.code==='23505');
  await query('UPDATE state_build_inbox_storage SET enabled=false WHERE id=1');
  await assert.rejects(writer.unsafe('INSERT INTO record_build_submissions(id,at,record) VALUES($1,$2,$3)',
    ['30000000-0000-8000-8000-000000000002',at,record]),error=>error.code==='55000');
  assert.equal((await one('SELECT (build_inbox_retire($1,false)->>\'bytes\')::int AS n',[generation])).n,0);
  console.log(JSON.stringify({postgres:'18',native:257,publicWorker:true,legacyFIFO:true,acceptedAndRefusedReplay:true,
    staleWriterBlocked:true,carryOnce:true,rollback:true,temporaryReceiptsGone:true,legacyFamily:true,atomicNativeRefusal:true,
    downgradedAckBlocked:true,noGenerationStarvation:true,typedRequest:true,equipment:true,protectedLegacyUnchanged:true,
    originalBytes:released.receipt.bytes,retainedBytes:retained.bytes}));
} finally {
  await adapter?.close?.();
  await writer.end({timeout:2});await competitor.end({timeout:2});await reader.end({timeout:2});await typedRunner.end({timeout:2});
  await owner.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE').simple();
  for(const role of [http,pipeline,typed])await owner.unsafe('DROP ROLE IF EXISTS '+role).simple();
  if(customSpace)await query('DROP TABLESPACE '+space);
  await owner.end({timeout:2});rmSync(scratch,{recursive:true,force:true});
}
