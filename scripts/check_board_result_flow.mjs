// SPDX-License-Identifier: AGPL-3.0-or-later
// An accepted measurement reaches the native projection, real query route and page reader.
import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {execFileSync,spawn} from 'node:child_process';
import {randomBytes} from 'node:crypto';
import {createServer} from 'node:http';
import vm from 'node:vm';
import postgres from './pg/node_modules/postgres/src/index.js';
import {pgDb,postgresClient} from '../worker/db.js';
import worker from '../worker/index.js';
import {openBoardResultFlow,captureBoardPartition} from './board_result_flow.mjs';
import {clientSource} from './ci_client_inputs.mjs';
import {recommendationsRoute} from '../worker/recommendations.js';
import {boardReadSchema} from './board_read_schema.mjs';

const url=process.env.WFSIM_BOARD_MAILBOX_TEST_URL;
assert(url&&['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname),'use an isolated local PostgreSQL');
const binary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const schema='board_flow_'+randomBytes(6).toString('hex'),root=mkdtempSync(join(tmpdir(),'wfsim-board-result-'));
const role=schema+'_reader';
const sql=postgres(url,{max:5,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000,lock_timeout:3000}});
let flow,server,reader;
const priorCaches=globalThis.caches,cacheEntries=new Map();
let cacheTime=0;
globalThis.caches={default:{
 async match(request){const hit=cacheEntries.get(request.url);if(hit&&cacheTime<hit.expires)return hit.response.clone();},
 async put(request,response){
  const maxAge=response.headers.get('cache-control')?.match(/max-age=(\d+)/);
  if(maxAge)cacheEntries.set(request.url,{response:response.clone(),expires:cacheTime+Number(maxAge[1])});
 }
}};
const browser=process.env.WFSIM_BOARD_FLOW_BROWSER==='1';

try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 const database=new URL(url).pathname.slice(1);assert(/^[a-zA-Z0-9_]+$/.test(database));
 const setup=await sql.reserve();
 try{
  for(const name of ['schema','library',...(browser?['accounts']:[])])
   await setup.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
  const install=httpRole=>boardReadSchema(httpRole).replaceAll('wfsim_board_read',role)
   .replace("default_tablespace='wfsim_data'","default_tablespace='pg_default'").replace('ON DATABASE wfsim','ON DATABASE "'+database+'"');
  const run=async text=>{try{return await setup.unsafe(text);}catch(error){await setup.unsafe('ROLLBACK');throw error;}};
  await assert.rejects(run(install(schema+'_missing_http_role')),error=>error.code==='42704');
  assert.equal((await setup.unsafe("SELECT to_regclass('state_board_input_mailbox') name"))[0].name,null,
   'a failed role grant must roll back every schema and capture change');
  await run(install(role));
  const before=(await setup.unsafe('SELECT source FROM state_board_input_source'))[0].source;
  await assert.rejects(run(install(role)),/schema exists; an explicit upgrade is required/);
  assert.equal((await setup.unsafe('SELECT source FROM state_board_input_source'))[0].source,before,'reinstall must preserve the installed source');
 }finally{setup.release();}
 await sql.unsafe('GRANT USAGE ON SCHEMA '+schema+' TO '+role);
 reader=postgres(url,{max:3,fetch_types:false,onnotice(){},connection:{search_path:schema,role,statement_timeout:10000,lock_timeout:3000}});
 await assert.rejects(reader.unsafe("UPDATE entity_weapon_builds SET at='2026-11-16' WHERE false"),error=>error.code==='42501');
 await assert.rejects(reader.unsafe('UPDATE state_board_read_partitions SET enabled=false WHERE false'),error=>error.code==='42501');
 const at='2026-11-16T00:00:00.000Z', nextEngine='111111111111';
 let fixture;
 if(process.env.WFSIM_BOARD_FLOW_INPUT)fixture=JSON.parse(readFileSync(process.env.WFSIM_BOARD_FLOW_INPUT,'utf8'));
 else{
  const record=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url),'utf8'))[0].record;
  const identityOf=build=>JSON.parse(execFileSync(binary,['--inventory-builds'],{input:JSON.stringify([build]),encoding:'utf8',timeout:30000}).trim()).id;
  const identity=identityOf(record);
  const other={...record,mods:record.mods.map(mod=>mod==='serration'?'serration@8':mod)};
  const retained=identityOf(other);
  const unmeasured={...record,mods:record.mods.map(mod=>mod==='serration'?'serration@7':mod)};
  const unmeasuredId=identityOf(unmeasured);
  assert.notEqual(identity,retained,'the old-engine control must be a distinct legal build');
  const fact={identity,ruler:'standard_single_target',mode:'base',score:10,metric:'kpm',cost_seconds:1,
   measured_by:'verified:000000000000',started_at:at,finished_at:at,review_id:null};
  fixture={builds:[{id:identity,at:'2026-11-16',record:JSON.stringify(record)},
   {id:retained,at:'2026-11-16',record:JSON.stringify(other)},
   {id:unmeasuredId,at:'2026-11-16',record:JSON.stringify(unmeasured)}],
   facts:[fact,{...fact,identity:retained,score:9}],
   target:{identity,weapon:record.weapon,ruler:fact.ruler,mode:'base',score:10},retainedScore:9,unmeasuredId};
  const review='4d1de37e-c95d-482f-a7b2-6bb01a846885';
  fixture.facts[0].review_id=review;
  fixture.reviews=[{id:review,ref:JSON.stringify([identity,'standard_single_target','base']),round_at:at,
   identity,ruler:'standard_single_target',mode:'base',engine:'000000000000',question:'fixture',
   record:JSON.stringify(record),state:'succeeded',baseline_score:9,baseline_metric:'kpm',baseline_cost_seconds:0,
   baseline_by:'old',baseline_started_at:at,baseline_finished_at:at,created_at:at,completed_at:at}];
 }
 const target=fixture.target;
 const [{source}]=await sql.unsafe('SELECT source FROM state_board_input_source WHERE id=1');
 flow=await openBoardResultFlow({sql:reader,source,engine:target.engine||'000000000000',binary,work:root});
 const scene=flow.contract.rulers.find(row=>row.id===target.ruler)?.scene;
 assert.equal(typeof scene,'string','native ruler must exist');
 await sql.unsafe('INSERT INTO state_board_read_partitions(weapon,ruler,scene,enabled,canonical_sources) VALUES($1,$2,$3,true,true)',[target.weapon,target.ruler,scene]);
 const env={LIBRARY:pgDb(postgresClient(sql)),ACCOUNTS:browser?pgDb(postgresClient(sql)):undefined},requests=[];
 server=createServer(async(req,res)=>{
  try{
   requests.push(req.url);
   const path=new URL(req.url,'http://127.0.0.1').pathname;
   if(browser&&path.startsWith('/api/recommendations/')){
     const response=await recommendationsRoute(new Request('http://127.0.0.1'+req.url),env,path,{waitUntil:p=>p.catch(()=>{})});
     if(response){res.writeHead(response.status,Object.fromEntries(response.headers));res.end(await response.text());return;}
   }
   const pending=[],response=await worker.fetch(new Request('http://127.0.0.1'+req.url,{headers:req.headers}),env,{waitUntil:p=>pending.push(p)});
   res.writeHead(response.status,Object.fromEntries(response.headers));res.end(await response.text());await Promise.all(pending);
  }catch(error){res.writeHead(500);res.end(String(error));}
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const base='http://127.0.0.1:'+server.address().port;
 const before=await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler}));
 assert.equal(before.status,404,'an unfinished partition must retain the previous served view');
 await sql.begin(async tx=>{
  const sources=['entity_weapon_builds','state_board_measurement_selection','entity_measurement_reviews'];
  if(fixture.complete)for(const relation of sources)await tx.unsafe('ALTER TABLE '+relation+' DISABLE TRIGGER board_input_changed');
  await tx.unsafe(`INSERT INTO entity_weapon_builds(id,at,record)
   SELECT id,at,record FROM jsonb_to_recordset(($1::text)::jsonb)x(id text,at text,record text)`,[JSON.stringify(fixture.builds)]);
  if(fixture.reviews?.length)await tx.unsafe(`INSERT INTO entity_measurement_reviews
    SELECT * FROM jsonb_populate_recordset(NULL::entity_measurement_reviews,($1::text)::jsonb)`,[JSON.stringify(fixture.reviews)]);
  await tx.unsafe(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at,review_id)
   SELECT identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at,review_id FROM
    jsonb_to_recordset(($1::text)::jsonb)x(identity text,ruler text,mode text,measured_by text,score double precision,
     metric text,cost_seconds double precision,started_at text,finished_at text,review_id text)`,[JSON.stringify(fixture.facts)]);
  await tx.unsafe(`INSERT INTO entity_weapon_builds(id,at,record)
    SELECT 'uncomputed_'||n,$1,jsonb_build_object('weapon',$2::text,'unused',repeat('x',131072))::text
    FROM generate_series(1,128)n`,[at.slice(0,10),target.weapon]);
  if(fixture.complete){
   for(const relation of sources)await tx.unsafe('ALTER TABLE '+relation+' ENABLE TRIGGER board_input_changed');
   const current=fixture.recent_target||target;
   await tx.unsafe('UPDATE entity_weapon_builds SET at=at WHERE id=$1',[current.identity]);
   await tx.unsafe('UPDATE state_board_measurement_selection SET score=score WHERE identity=$1 AND ruler=$2 AND mode=$3',
    [current.identity,current.ruler,current.mode]);
  }
 });
 let routed=0,published=0;
 for(let n=0;n<100000;n++){
  const step=await flow.step();if(step.status==='idle')break;
  if(step.stage==='routing'&&step.status==='applied')routed++;
  if(step.stage==='publication'&&step.status==='applied')published++;
  assert(n<99999,'flow did not drain');
 }
 assert(routed>=2&&published>=1,'the actual source and partition handoffs must run');
 const after=await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler}));
 assert.equal(after.status,200);const page=await after.json();assert(page.ok&&page.rows.length>0,'native publication must contain an eligible score');
 const completeRows=async first=>{
  const rows=[...first.rows];let next=first.next;
  while(next!==null){
   const response=await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler,revision:first.revision,after:String(next)}));
   assert.equal(response.status,200);const more=await response.json();
   assert(more.ok&&more.revision===first.revision&&more.after===next);
   rows.push(...more.rows);next=more.next;
  }
  assert.equal(rows.length,first.count);return rows;
 };
 const complete=await completeRows(page);
 if(fixture.unmeasuredId)assert.equal(complete.length,2,'a canonical build without a measured fact stays outside the ranking');
 const readLibrary=()=>sql.unsafe('SELECT id,record FROM entity_weapon_builds ORDER BY id');
 const libraryBefore=await readLibrary();
 if(fixture.unmeasuredId)assert(libraryBefore.some(row=>row.id===fixture.unmeasuredId),
  "the library holds the canonical build before its first measurement");
 const currentUrl=base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler});
 const pinnedUrl=currentUrl+'&revision='+page.revision;
 const queryCalls=env.LIBRARY.cost.calls;
 assert.equal((await fetch(currentUrl)).status,200);
 assert.equal(env.LIBRARY.cost.calls,queryCalls+1,'an unversioned page must confirm the current selection');
 assert.equal((await fetch(pinnedUrl)).status,200);
 const pinnedCalls=env.LIBRARY.cost.calls;
 assert.equal((await fetch(pinnedUrl)).status,200);
 assert.equal(env.LIBRARY.cost.calls,pinnedCalls,'an immutable hot page must avoid another database query');
 const visible=complete.find(row=>row.mode===target.mode&&row.score===target.score);
 const retainedRow=fixture.retainedScore==null?null:complete.find(row=>row.score===fixture.retainedScore);
 if(fixture.retainedScore!=null)assert.equal(retainedRow?.measured_by,"000000000000");
 assert(visible,'the accepted score must retain its exact mode and value among '+page.count+' native rows');
 if(fixture.recent_target)assert(complete.some(row=>row.mode===fixture.recent_target.mode&&row.score===fixture.recent_target.score),
  'the current-engine community result must remain visible alongside historical scores');
 assert.equal(await (async()=>{const [{n}]=await sql.unsafe('SELECT count(*)::text n FROM state_board_input_mailbox WHERE version>applied_version');return n;})(),'0');

 const ui=clientSource('scripts/check_board_result_flow.mjs');
 const part=ui.slice(ui.indexOf('const pageBoardReader'),ui.indexOf('/// `res.ok`'));
 const shared=ui.slice(ui.indexOf('const publishedBoardRevisions'),ui.indexOf('function headlessNonaHelp(say'));
 const context=vm.createContext({URLSearchParams,AbortController,AbortSignal,window:{},target,scene,
  fetchJsonPatient:async(path,tries,signal)=>{const r=await fetch(new URL(path,base),{signal});if(!r.ok)throw Error('HTTP '+r.status);return r.json();}});
 vm.runInContext(shared+'\n'+part,context);
 const {rows,rulers}=await vm.runInContext('readCurrentBoardRows(target.weapon,scene)',context);
 assert(rulers.includes(target.ruler));
 assert.deepEqual(JSON.parse(JSON.stringify(rows.filter(row=>row.benchmark===target.ruler))),complete);
 const index=await vm.runInContext('readCurrentBoardIndex(scene)',context);
 assert(index[target.weapon].some(row=>row.score===target.score&&row.mode===target.mode));
 const catalogUrl=base+'/api/board/pages/catalog?'+new URLSearchParams({weapon:target.weapon,scene});
 const catalogTag=(await fetch(catalogUrl)).headers.get('etag');
 assert(catalogTag,'a current catalog has a conditional refresh validator');
 const refreshCatalog=()=>fetch(catalogUrl,{headers:{'if-none-match':catalogTag}});
 const sameCatalog=await refreshCatalog();
 assert.equal(sameCatalog.status,304);assert.equal(await sameCatalog.text(),'');
 const indexed=await captureBoardPartition(sql,target);
 assert.equal(indexed.bootstrap,false,'subsequent publication must use the measured-source index');
 assert.equal(indexed.facts.length,fixture.facts.length,'the indexed cut retains every historical fact');
 assert.equal(indexed.builds.length,fixture.builds.length-Number(Boolean(fixture.unmeasuredId)),
  'unmeasured records stay in the library and do not enter the publication inputs');
 if(browser)await new Promise((resolveChild,reject)=>{
  const child=spawn(process.execPath,[resolve('scripts/board_result_browser.mjs'),base,JSON.stringify({target,scene,shown:visible.shown,libraryTarget:fixture.recent_target||target})],{stdio:'inherit'});
  child.on('error',reject);child.once('close',code=>code===0?resolveChild():reject(Error('board browser failed: '+code)));
 });
 const drain=async()=>{
  for(let n=0;n<1000;n++){const step=await flow.step();if(step.status==='idle')return;}
  assert.fail('updated input did not drain');
 };
 if(retainedRow){
  await flow.close();
  flow=await openBoardResultFlow({sql:reader,source,engine:nextEngine,binary,work:root});
  await drain();
  const unchanged=await(await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler}))).json();
  assert.equal(unchanged.revision,page.revision,'an engine release does not republish or clear the board');
  assert.deepEqual(await completeRows(unchanged),complete);
 }
 await sql.unsafe(`UPDATE state_board_measurement_selection SET score=$4,finished_at=$5,measured_by=coalesce($6,measured_by)
   WHERE identity=$1 AND ruler=$2 AND mode=$3`,[target.identity,target.ruler,target.mode,target.score+1,'2026-11-17T00:00:00.000Z',retainedRow?'verified:'+nextEngine:null]);
 await drain();
 const updated=await(await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler}))).json();
 const newCatalog=await refreshCatalog();
 assert.equal(newCatalog.status,200);assert.notEqual(newCatalog.headers.get('etag'),catalogTag);
 assert((await newCatalog.json()).partitions.some(p=>p.revision===updated.revision));
 const currentRows=await completeRows(updated);
 const replacement=currentRows.find(row=>row.score===target.score+1&&row.mode===target.mode);
 assert(replacement,'a subsequent accepted result must use the indexed path');
 if(retainedRow){
  assert.equal(replacement.measured_by,nextEngine);
  assert.deepEqual(currentRows.find(row=>row.score===fixture.retainedScore),retainedRow,
   'a different valid build retains its score, provenance and public row when the engine changes');
  assert(!currentRows.some(row=>row.score===target.score),'the replacement supersedes only its own former selected fact');
  console.log('ok: an engine release preserves the published board, and new and old measured results rank together');
 }
 const immutable=await(await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler,revision:page.revision}))).json();
 assert((await completeRows(immutable)).some(row=>row.score===target.score&&row.mode===target.mode),'a pinned page must retain its immutable score');
 await sql.unsafe('DELETE FROM state_board_measurement_selection WHERE identity=$1 AND ruler=$2 AND mode=$3',[target.identity,target.ruler,target.mode]);
 await drain();
 const removed=await(await fetch(base+'/api/board/pages?'+new URLSearchParams({weapon:target.weapon,ruler:target.ruler}))).json();
 assert(!(await completeRows(removed)).some(row=>row.score===target.score+1&&row.mode===target.mode),'a removed fact must disappear from the current ranking');
 assert.equal((await fetch(pinnedUrl)).status,200,'a cached immutable response retains its own revision');
 cacheTime+=11;
 assert.equal((await fetch(pinnedUrl)).status,409,'after cache expiry a retired revision requires a fresh complete partition');
 assert.deepEqual(await readLibrary(),libraryBefore,'engine changes, new scores and removals preserve the same canonical build records');
 console.log('ok: review-backed accepted facts publish on the production driver; indexed updates, immutable pages and removals reach the query route');
 console.log(JSON.stringify({status:'RESULT_TO_PAGE_READER_PASSED',input:process.env.WFSIM_BOARD_FLOW_INPUT?'captured authoritative result':'fixture',
  target,routed,published,revision:page.revision,rows:page.count,requests:requests.length,production_writes:false}));
}finally{
 await flow?.close();await reader?.end();if(server)await new Promise(resolve=>server.close(resolve));
 try{
  await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');
  const [held]=await sql.unsafe('SELECT 1 FROM pg_roles WHERE rolname=$1',[role]);
  if(held){await sql.unsafe('DROP OWNED BY '+role);await sql.unsafe('DROP ROLE '+role);}
 }finally{await sql.end();rmSync(root,{recursive:true,force:true});}
 globalThis.caches=priorCaches;
}
