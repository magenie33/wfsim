// SPDX-License-Identifier: AGPL-3.0-or-later
// Provider lifecycle and wire checks use local PostgreSQL, never service endpoints.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import postgres from './pg/node_modules/postgres/src/index.js';
import {POSTGRES_OPTIONS,postgresClient,pgDb,placeholders} from '../worker/db.js';
import {createStatisticsProvider,statisticsStatus,STATISTICS_BUNDLE_FORMAT} from '../worker/statistics_provider.js';
import {demandOf} from '../worker/contribution.js';
import {snapshotTick} from '../worker/settler.js';
import {setPipelineControl} from '../worker/task_pipeline.js';
import {openEnv,run,statisticsLimitsFromEnv} from './factoryd.mjs';
import {statisticsSchemaSql} from './statistics_schema.mjs';
import {seedComputeWork,statisticsCounts} from './pg/fixtures.mjs';
import {statisticsExpiresAt} from '../worker/statistics_clock.js';
import {statisticsClockCases} from './check_statistics_clock.mjs';
import {statisticsStorageCases} from './check_statistics_projection.mjs';
import {createStatisticsProjection,statisticsProjectionIdentity,STATISTICS_PROJECTION_LIMITS,StatisticsStorageCapacityHold,StatisticsProjectionBudget} from '../worker/statistics_projection.js';

export async function statisticsGenerationCases(){
 const {PGlite}=await import('./pg/node_modules/@electric-sql/pglite/dist/index.js');
 const pg=new PGlite();let activeTx;
 const q=async(text,args=[])=>(await(activeTx??pg).query(text,args)).rows;
 let create=createStatisticsProjection;
 if(process.argv.includes('--break-generation-guard')){
  const file=new URL('../worker/statistics_projection.js',import.meta.url),guard="    const retained=await q('SELECT id FROM state_statistics_namespaces ORDER BY id LIMIT $1::bigint',[bound.maxNamespaces]);\n    if(retained.length>=bound.maxNamespaces)throw new StatisticsProjectionBudget('statistics retained generation budget');\n";
  let text=readFileSync(file,'utf8');assert.equal(text.split(guard).length,2);text=text.replace(guard,'');
  text=text.replace(/from (["'])(\.[^"']+)\1/g,(_,quote,path)=>'from '+JSON.stringify(new URL(path,file).href));
  ({createStatisticsProjection:create}=await import('data:text/javascript;base64,'+Buffer.from(text).toString('base64')));
 }
 try{
  for(const name of ['schema','library','sync','inventory','build_workflows','board_admission','statistics_projection'])
   await pg.exec(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
  const make=(limits={})=>create({transaction:fn=>pg.transaction(async tx=>{
   activeTx=tx;try{return await fn({query:(text,args)=>tx.query(text,args)});}finally{activeTx=null;}}),
   currentIdentity:()=>statisticsProjectionIdentity(q),limits:{storageReserveBytes:1048576,...limits}});
  assert.equal(STATISTICS_PROJECTION_LIMITS.maxNamespaces,2);
  assert.throws(()=>make({maxNamespaces:16}),/invalid statistics generation budget/,'test fixture budget entered production');
  const first=await make().start();assert.equal((await make({maxNamespaces:1}).start()).namespace,first.namespace,'full budget blocked identical head');
  await q("UPDATE state_statistics_namespaces SET identity=identity||jsonb_build_object('engine','legacy-engine') WHERE id=$1",[first.namespace]);
  assert.equal((await make({maxNamespaces:1}).start()).namespace,first.namespace,'legacy engine in the head forced a new baseline');
  await q("UPDATE state_statistics_namespaces SET phase=1,cursor='[\"retained-failure-cursor\"]',counts_at='2026-11-17T00:00:00.000Z',published='{\"evidence\":\"retained\"}' WHERE id=$1",[first.namespace]);
  await q('UPDATE state_statistics_generation SET revision=1');const second=await make().start();assert.notEqual(first.namespace,second.namespace);
  const retained=()=>q('SELECT to_jsonb(n) body FROM state_statistics_namespaces n ORDER BY id');
  const before=await retained(),revision=(await q('SELECT last_value::text n FROM statistics_revision'))[0].n;
  for(const generation of [2,0]){
   await q('UPDATE state_statistics_generation SET revision=$1::bigint',[generation]);
   await assert.rejects(make().start(),/statistics retained generation budget/);
   assert.deepEqual(await retained(),before,'generation hold changed retained failure/baseline evidence');
   assert.equal((await q('SELECT namespace FROM state_statistics_head'))[0].namespace,second.namespace,'generation hold changed head');
   assert.equal((await q('SELECT last_value::text n FROM statistics_revision'))[0].n,revision,'generation hold allocated a revision');
  }
  await q('UPDATE state_statistics_generation SET revision=1');
  assert.equal((await make().start()).namespace,second.namespace,'restart could not resume at full budget');
  console.log('PASS two-generation boundary: identical/restarted head resumes; third identity and retired reentry HOLD without revision/head/evidence changes');
 }finally{await pg.close();}
}


export async function statisticsProviderCases(){
 const budgets=(max,reserve)=>({FACTORY_STATISTICS_MAX_BYTES:max,FACTORY_STATISTICS_RESERVE_BYTES:reserve});
 assert.deepEqual(statisticsLimitsFromEnv({}),{});
 assert.deepEqual(statisticsLimitsFromEnv(budgets('1073741824','33554432')),
  {maxStorageBytes:1073741824,storageReserveBytes:33554432});
 for(const input of [budgets('1073741824'),budgets('1048576','1048576'),budgets('1e9','4096'),
  budgets('9007199254740992','4096'),budgets('4096','0')])assert.throws(()=>statisticsLimitsFromEnv(input),/set valid FACTORY_STATISTICS/);
 await statisticsClockCases();
 await statisticsGenerationCases();
 await statisticsStorageCases();
 await statisticsStorageProviderCases();
 assert.throws(()=>statisticsSchemaSql('schema'),/tablespace/);assert.throws(()=>statisticsSchemaSql('index'),/tablespace/);
 assert.match(statisticsSchemaSql('index',{tablespace:'pg_default'}),/CREATE INDEX CONCURRENTLY/);
 const unread={LIBRARY:{prepare(){throw Error('historical input read');}}};
 assert.equal(await snapshotTick(unread,{engine:'fixture'}),false,'snapshot without provider reads history');
 await assert.rejects(demandOf(unread),/complete statistics counts/);
 const at='2026-11-17T00:00:00.000Z',key={contract:'statistics-projection-1',epoch:null,generation:'1',blocked:null,inventory:null,fingerprint:'fixture'};
 const complete={format:STATISTICS_BUNDLE_FORMAT,key,namespace:'fixture-head',countsAt:at,snapshot:{at,engine:'fixture'},
  demand:{ok:true,v:1,at,counts_at:at,expires_at:statisticsExpiresAt(at),order_epoch:key.epoch}};
 let paused=false,stopped=false,engine='fixture',installed=true,starts=0,steps=0,seals=0,queries=0,writes=0,kept=null,time=Date.parse(at)+5000;
 const query=async sql=>{queries++;if(sql.includes("kind='statistics_storage_hold'"))return[];if(sql.includes('index_ready'))return[{paused,index_ready:true}];
  if(sql.startsWith('SELECT compute_order_cut_epoch')){const {contract,...identity}=key;return[identity];}
  if(sql.includes('SELECT h.namespace,n.counts_at'))return[{namespace:complete.namespace,counts_at:at}];
  if(sql.includes('SELECT n.published'))return[{published:kept}];throw Error('unexpected provider query: '+sql);};
 const env={LIBRARY:{prepare:()=>({bind:()=>({run:async()=>{writes++;}})})}};
 const make=()=>createStatisticsProvider({env,query,controlQuery:query,transaction:()=>{},release:()=>({engine}),stopped:()=>stopped,
  now:()=>time,dependenciesOf:async()=>({ready:installed,state:'missing_index',index_oid:'7'}),
  projectionFactory:()=>({start:async()=>{starts++;return{namespace:complete.namespace};},
   step:async()=>({baseline:++steps>=3,worked:false}),read:async()=>kept,
   seal:async()=>{seals++;kept=complete;return{counts:complete};}})});
 let provider=make();installed=false;await provider.advance();assert.equal(await provider.get(),null);assert.equal(starts,0);
 installed=true;paused=true;await provider.advance();assert.equal(starts,0);paused=false;
 await Promise.all([provider.advance(),provider.advance()]);assert.equal(starts,1,'duplicate pump');assert.equal(steps,3,'pump advances only one page');assert.equal(seals,1);
 assert.deepEqual(await provider.get(),complete);assert.equal((await provider.get()).countsAt,at,'cached counts re-stamped');
 provider=make();await provider.advance();assert.equal(seals,1,'restart rebuilt a complete sample');
 time=Date.parse(complete.demand.expires_at);assert.equal(await provider.get(),null,'expired provider sample served');time=Date.parse(at)+5000;
 engine='changed';assert.equal(await provider.get(),null,'changed source published cached counts');engine='fixture';
 stopped=true;const before={queries,writes};await provider.advance();assert.equal(await provider.get(),null);assert.deepEqual({queries,writes},before,'stopped owner performed a late lookup/write');
 console.log('PASS provider fail-closed, bounded multi-step/singleflight, cached clock, restart and owner-stop controls');
 await statisticsProviderSourceRaceCases();
}

export async function statisticsStorageProviderCases({create=createStatisticsProvider}={}){
 const {PGlite}=await import('./pg/node_modules/@electric-sql/pglite/dist/index.js');
 const pg=new PGlite();const q=async(text,args=[])=>(await pg.query(text,args)).rows;
 const env={LIBRARY:{prepare:(sql)=>{let args=[];return{bind(...values){args=values;return this;},run:()=>{findingWrites++;return q(placeholders(sql),args);}};}}};
 let starts=0,steps=0,preflights=0,findingWrites=0,fault='storage',capacity=false;
 const capacityError=()=>new StatisticsStorageCapacityHold('retained storage budget',{bytes:'200',limitBytes:100,reserveBytes:10});
 const query=async(sql,args=[])=>sql.includes('index_ready')
  ?[{paused:(await q("SELECT paused FROM state_task_pipeline_controls WHERE name='statistics'"))[0]?.paused??false,index_ready:true}]:q(sql,args);
 const make=()=>create({env,query,controlQuery:query,transaction:()=>{},release:()=>({engine:'fixture'}),
  dependenciesOf:async()=>({ready:true,index_oid:'7'}),projectionFactory:()=>({
   start:async()=>{starts++;if(fault==='storage')throw capacityError();return{namespace:'kept'};},
   checkStorage:async()=>{preflights++;if(!capacity)throw capacityError();},
   step:async()=>{steps++;throw new StatisticsProjectionBudget('statistics '+fault+' budget');},
  })});
 const control=async(paused,stamp)=>q(`INSERT INTO state_task_pipeline_controls(name,paused,changed_at,changed_by) VALUES('statistics',$1,$2,'fixture-operator')
  ON CONFLICT(name) DO UPDATE SET paused=excluded.paused,changed_at=excluded.changed_at,changed_by=excluded.changed_by`,[paused,stamp]);
 const body=async()=>(await q("SELECT body FROM state_factory_findings WHERE kind='statistics_storage_hold'"))[0]?.body;
 try{
  await pg.exec(readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8'));
  let provider=make();await assert.rejects(provider.advance(),/retained storage budget/);
  assert(await body(),'storage hold was not persisted');const held=await body(),before={starts,steps,preflights,findingWrites};
  await provider.advance();provider=make();await provider.advance();
  assert.deepEqual({starts,steps,preflights,findingWrites},before,'held/restarted provider retried writes');assert.equal(await body(),held);
  await control(true,'2026-11-18T01:00:00.000Z');await provider.advance();await provider.get();
  assert.equal(await body(),held,'paused generic state overwrote typed storage hold');
  assert.deepEqual({starts,steps,preflights,findingWrites},before,'pause cleared storage hold');
  await control(false,'2026-11-18T01:00:01.000Z');await assert.rejects(provider.advance(),/retained storage budget/);
  assert.equal(starts,before.starts,'failed resume preflight wrote projection');const failed=await body();
  await provider.advance();provider=make();await provider.advance();assert.equal(preflights,1,'failed resume was blindly retried');assert.equal(await body(),failed);
  capacity=true;fault='time';await control(false,'2026-11-18T01:00:02.000Z');
  await assert.rejects(provider.advance(),/statistics time budget/);assert.equal(JSON.parse(await body()).state,'cleared');
  const retry=starts;await assert.rejects(provider.advance(),/statistics time budget/);assert.equal(starts,retry+1,'ordinary time budget became persistent hold');
  fault='query';await assert.rejects(provider.advance(),/statistics query budget/);const again=starts;
  provider=make();await assert.rejects(provider.advance(),/statistics query budget/);assert.equal(starts,again+1,'ordinary query budget became persistent hold');
  assert.equal(JSON.parse(await body()).state,'cleared');
  console.log('PASS actual persisted storage hold: advance/restart/pause no retry; explicit resume rechecks; time/query remain retryable');
 }finally{await pg.close();}
}

export async function statisticsProviderSourceRaceCases(){
 for(const change of ['engine','notification','both','deadline']){
  const at='2026-11-17T00:00:00.000Z',identity={epoch:null,generation:'1',blocked:null,inventory:null,fingerprint:'fixture'};
  const counts={namespace:'fixture-head',countsAt:at,key:{contract:'statistics-projection-1',...identity},snapshot:{at,engine:'fixture'},
   demand:{ok:true,v:1,at,counts_at:at,expires_at:statisticsExpiresAt(at),order_epoch:identity.epoch}};
  let engine='fixture',checks=0,entered,finish,time=Date.parse(at);
  const waiting=new Promise(resolve=>{entered=resolve;}),held=new Promise(resolve=>{finish=resolve;});
  const query=async sql=>{
   if(sql.includes('index_ready')){if(++checks===2){entered();await held;}return[{paused:false,index_ready:true}];}
   if(sql.startsWith('SELECT compute_order_cut_epoch'))return[identity];
   if(sql.includes('SELECT h.namespace,n.counts_at'))return[{namespace:counts.namespace,counts_at:at}];
   throw Error('unexpected source-race lookup: '+sql);
  };
  const provider=createStatisticsProvider({env:{},query,controlQuery:query,release:()=>({engine}),now:()=>time,
   dependenciesOf:async()=>({ready:true})});
  const asserting=provider.assert(counts),rejected=assert.rejects(asserting,change==='deadline'?/statistics sample expired/:/statistics source changed/);
  await waiting;
  if(change==='deadline')time=Date.parse(counts.demand.expires_at);
  else{if(change!=='notification')engine='changed';if(change!=='engine')provider.changed();}finish();
  await rejected;
 }
 console.log('PASS source change during final control await rejects observed engine and same-engine notification races');
}

export async function nativeStatisticsProviderCases({runDaemon=run}={}){
 const port=Number(process.env.WFSIM_STATISTICS_TEST_PORT||55535);assert(Number.isInteger(port)&&port>=1024&&port<=65535);
 const database=process.env.WFSIM_STATISTICS_TEST_DATABASE||'postgres';assert(/^[a-z_][a-z0-9_]*$/.test(database));
 const localUrl='postgres://127.0.0.1:'+port+'/'+database;
 const schema='statistics_provider_'+Date.now(),sql=postgres(localUrl,{...POSTGRES_OPTIONS,max:1,onnotice:()=>{}});
 const owner=postgresClient(sql),env={LIBRARY:pgDb(owner)},q=async(text,args=[])=>[...await sql.unsafe(text,args)];
 const db={run:async(text,...args)=>(await sql.unsafe(placeholders(text),args)).count,get:async(text,...args)=>(await q(placeholders(text),args))[0],all:(text,...args)=>q(placeholders(text),args)};
 let opened,hook=async()=>{},stopped=false,now=Date.now(),served={engine:'aaaaaaaaaaaa',release:'ui-first'};
 const seen=[],connect=(url,options)=>postgres(url,{...options,connection:{...options.connection,search_path:schema+',pg_catalog'}});
 const root=new URL('../target/statistics-provider/',import.meta.url);mkdirSync(root,{recursive:true});
 try{
  await q('CREATE SCHEMA '+schema);await q('SET search_path TO '+schema+',pg_catalog');
  for(const name of ['schema','library','sync','inventory','build_workflows','board_admission'])await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
  await sql.unsafe(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
  const baseGrants=readFileSync(new URL('../deploy/factoryd_grants.sql',import.meta.url),'utf8').replace('GRANT CONNECT ON DATABASE wfsim TO','GRANT CONNECT ON DATABASE '+database+' TO');
  await sql.unsafe(baseGrants);await q('GRANT USAGE ON SCHEMA '+schema+' TO wfsim_factoryd');
  for(let i=0;i<24;i++){
   const id='provider-'+String(i).padStart(3,'0');await seedComputeWork(db,{id,verb:i===1?'optimize_report':'simulate',producer:i===1?'user_calculation':'sweep',initiator:i%2?'user:fixture':'system',waits:i===2?1:0});
  }
  await q("INSERT INTO entity_compute_batches(id,at,total) VALUES('rescore-provider',$1,12),('new-provider',$1,12)",[new Date(now).toISOString()]);
  await q(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator) SELECT CASE WHEN id<'provider-012' THEN 'rescore-provider' ELSE 'new-provider' END,id,'r','base',initiator FROM state_compute_scheduling`);
  await q("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,payload) SELECT 'sweep',id||'|r|base',id,'live',$1,$1,'{}' FROM state_compute_scheduling",[new Date(now).toISOString()]);
  await q("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('new_build','provider-012|r|base','provider-012','live',$1,$1)",[new Date(now).toISOString()]);
  await q("UPDATE state_compute_scheduling SET lease='held',leased_at=$1,lease_until=$2,leased_to='provider-device' WHERE id='provider-012'",[new Date(now).toISOString(),new Date(now+60000).toISOString()]);
  opened=await openEnv({PG_URL:'postgres://wfsim_factoryd@127.0.0.1:'+port+'/'+database},connect);
  await q('SELECT pg_advisory_lock(hashtextextended($1::text,0))',[schema]);
  const signalWait=opened.signalEnv.LIBRARY.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?::text,0))").bind(schema).first();
  try {
   let waiting=false;
   for(let attempt=0;attempt<50&&!waiting;attempt++){
    [{waiting}]=await q("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted) waiting");
    if(!waiting)await new Promise(resolve=>setTimeout(resolve,10));
   }
   assert(waiting,'fast signal fixture must occupy its connection');
   assert.deepEqual(await opened.statisticsControlQuery("SELECT current_setting('application_name') AS lane"),
    [{lane:'wfsim-factoryd-statistics-control'}]);
  } finally {
   await q('SELECT pg_advisory_unlock(hashtextextended($1::text,0))',[schema]);
   await signalWait;
  }
  const lookup=async(text,args=[],options)=>{seen.push(text);return opened.statisticsQuery(text,args,options);};
  const transaction=fn=>opened.statisticsTransaction(tx=>fn({query:async(text,args)=>{seen.push(text);const out=await tx.query(text,args);await hook(text,args,out);return out;}}));
  const makeProvider=()=>createStatisticsProvider({env:opened.env,controlEnv:opened.signalEnv,query:lookup,controlQuery:opened.statisticsControlQuery,transaction,
   release:()=>served,stopped:()=>stopped,now:()=>now,limits:{pageRows:8,storageReserveBytes:1048576}});
  let provider=makeProvider();
  const noLegacy=()=>assert(!seen.some(text=>/count\(\*\)\s+FROM state_board_effective_pending_work/i.test(text)),'historical pending COUNT fallback');
  await provider.advance();assert.equal(await provider.get(),null);assert.equal((await statisticsStatus(env,now)).state,'not_installed');noLegacy();
  await sql.unsafe(statisticsSchemaSql('schema',{tablespace:'pg_default'}));
  assert.equal((await statisticsStatus(env,now)).state,'missing_index');await provider.advance();noLegacy();
  for(const statement of statisticsSchemaSql('index',{tablespace:'pg_default'}).split(';').map(s=>s.trim()).filter(Boolean))await sql.unsafe(statement);
  assert.equal((await statisticsStatus(env,now)).state,'missing_grants');await provider.advance();noLegacy();
  await sql.unsafe(statisticsSchemaSql('grants'));
  assert.equal((await statisticsStatus(env,now)).dependencies.ready,true);
  const first=await provider.advance();assert(first.steps>1,'one turn advances only one small page');
  let bundle;
  for(let i=0;i<100&&!bundle;i++){await provider.advance();bundle=await provider.get();}
  assert(bundle,'bounded provider did not seal a complete source');
  const expected=await statisticsCounts(env,Date.parse(bundle.countsAt),served),oracle=await demandOf({LIBRARY:env.LIBRARY},Date.parse(bundle.countsAt),undefined,{counts:expected});
  assert.deepEqual(bundle.snapshot,expected.snapshot,'full snapshot parity');assert.deepEqual(bundle.demand,oracle,'complete demand wire parity');noLegacy();
  const applied=await snapshotTick(opened.env,served,now,{counts:bundle,allowed:provider.allowed,
   write:statement=>opened.writeSnapshot(statement,async tx=>{try{return await provider.assert(bundle,{lookup:async(text,args)=>(await tx.query(text,args)).rows});}catch{return false;}})});
  assert(applied);assert.equal((await q('SELECT at FROM state_factory_snapshot WHERE id=1'))[0].at,bundle.countsAt);
  const initial=bundle,ns=initial.namespace;
  served={...served,release:'ui-only-second'};provider=createStatisticsProvider({env:opened.env,controlEnv:opened.signalEnv,query:lookup,controlQuery:opened.statisticsControlQuery,transaction,
   release:()=>served,stopped:()=>stopped,now:()=>now,limits:{pageRows:8,storageReserveBytes:1048576}});
  await provider.advance();assert.equal((await provider.get()).namespace,ns,'UI release rebuilds membership');
  assert.equal((await provider.get()).countsAt,initial.countsAt,'cache re-stamps full data');
  await setPipelineControl(env,'statistics',{paused:true,by:'fixture'},now);
  const held=JSON.stringify(await q('SELECT * FROM state_factory_snapshot')),before=seen.length;await provider.advance();assert.equal(await provider.get(),null);
  assert.equal(JSON.stringify(await q('SELECT * FROM state_factory_snapshot')),held);assert(!seen.slice(before).some(text=>text.includes('state_compute_scheduling')));
  await setPipelineControl(env,'statistics',{paused:false,by:'fixture'},now);
  now+=11000;let paused=false;hook=async text=>{if(!paused&&text.startsWith('UPDATE state_statistics_namespaces SET counts_at=')){paused=true;await setPipelineControl(env,'statistics',{paused:true,by:'last-write'},now);}};
  await assert.rejects(provider.advance(),/paused|stopped/);hook=async()=>{};assert(paused);
  assert.equal((await q('SELECT counts_at FROM state_statistics_namespaces WHERE id=$1',[ns]))[0].counts_at,initial.countsAt,'last-await pause committed a new seal');
  await setPipelineControl(env,'statistics',{paused:false,by:'fixture'},now);provider.changed();
  for(let i=0;i<30;i++){await provider.advance();bundle=await provider.get();if(bundle?.countsAt!==initial.countsAt)break;}
  assert.equal(bundle.namespace,ns);assert.notEqual(bundle.countsAt,initial.countsAt);
  const stoppedAt=bundle.countsAt;now+=11000;
  hook=async text=>{if(text.startsWith('UPDATE state_statistics_namespaces SET counts_at='))stopped=true;};
  await assert.rejects(provider.advance(),/stopped/);hook=async()=>{};
  assert.equal((await q('SELECT counts_at FROM state_statistics_namespaces WHERE id=$1',[ns]))[0].counts_at,stoppedAt,'owner loss committed a new seal');
  const writesAtStop=seen.length;assert.equal(await provider.get(),null);assert.equal(seen.length,writesAtStop,'stopped owner reads or writes');stopped=false;
  const [acl]=await q(`SELECT has_table_privilege('wfsim_factoryd','state_statistics_generation','UPDATE') generation,
   has_function_privilege('wfsim_factoryd','statistics_invalidate(text)','EXECUTE') invalidate,
   has_sequence_privilege('wfsim_factoryd','statistics_revision','SELECT') sequence_read,
   has_table_privilege('wfsim_factoryd','state_statistics_namespaces','DELETE') namespace_delete`);
  assert(Object.values(acl).every(value=>value===false),'installer granted recovery or namespace deletion to runtime');
  await q("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('sweep',$1,'provider-000','live',$2,$2)",['long-'+ 'x'.repeat(1100),new Date(now).toISOString()]);
  assert.equal((await statisticsStatus(env,now)).state,'halted');assert.equal(await provider.get(),null);
  const recovery=statisticsSchemaSql('recover');await assert.rejects(sql.unsafe(recovery),/pause statistics/);await q('ROLLBACK');
  await setPipelineControl(env,'statistics',{paused:true,by:'recovery'},now);
  const oldHead=(await q('SELECT namespace FROM state_statistics_head WHERE id=1'))[0].namespace;
  const oldGeneration=(await q('SELECT revision::text FROM state_statistics_generation'))[0].revision;
  const writer=postgres(localUrl,{...POSTGRES_OPTIONS,max:1,onnotice:()=>{},connection:{search_path:schema+',pg_catalog'}});
  let releaseWriter,writerReady;const writerHeld=new Promise(resolve=>{writerReady=resolve;}),releaseHeld=new Promise(resolve=>{releaseWriter=resolve;});
  const writing=writer.begin(async tx=>{await tx.unsafe("UPDATE state_compute_scheduling SET waits=waits WHERE id='provider-000'");writerReady();await releaseHeld;});
  try{
   await writerHeld;await assert.rejects(sql.unsafe(recovery),error=>error.code==='55P03');await q('ROLLBACK');
   assert.equal((await q('SELECT namespace FROM state_statistics_head'))[0].namespace,oldHead,'failed writer fence cleared head');
   assert((await q('SELECT 1 FROM state_statistics_invalidations LIMIT 1')).length,'failed writer fence cleared halt');
  }finally{releaseWriter();await writing;await writer.end();}
  await sql.unsafe(recovery);assert(BigInt((await q('SELECT revision::text FROM state_statistics_generation'))[0].revision)>BigInt(oldGeneration));
  assert.equal((await q('SELECT 1 FROM state_statistics_head')).length,0);assert.equal((await q('SELECT 1 FROM state_statistics_invalidations')).length,0);
  await setPipelineControl(env,'statistics',{paused:false,by:'recovery'},now);provider.changed();
  for(let i=0;i<100;i++){await provider.advance();bundle=await provider.get();if(bundle)break;}
  assert(bundle&&bundle.namespace!==ns,'halt recovery reused a retired incomplete capture');noLegacy();
  const currentNamespace=bundle.namespace;
  await sql.unsafe(statisticsSchemaSql('schema',{tablespace:'pg_default'}));await sql.unsafe(statisticsSchemaSql('grants'));
  await sql.unsafe(statisticsSchemaSql('schema',{tablespace:'pg_default'}));await sql.unsafe(statisticsSchemaSql('grants'));
  const upgraded=await statisticsStatus(env,now);assert.equal(upgraded.dependencies.ready,true,JSON.stringify(upgraded.dependencies));
  assert.equal((await provider.get()).namespace,currentNamespace,'nonempty repeated upgrade lost state');
  let snapshotWrites=0;const puts=[];
  const code=await runDaemon({once:true,opened:{...opened,close:async()=>{},writeSnapshot:async(...args)=>{
   const applied=await opened.writeSnapshot(...args);
   if(applied&&++snapshotWrites===2)await setPipelineControl(env,'statistics',{paused:true,by:'before-put'},now);return applied;
  }},statisticsLimits:{maxStorageBytes:1073741824,storageReserveBytes:1048576},
  statisticsFactory:({limits})=>{
   assert.deepEqual(limits,{maxStorageBytes:1073741824,storageReserveBytes:1048576});
   return provider;
  },servedNow:async()=>served,target:{bucket:'provider-fixture'},put:async(_,object)=>puts.push(object)});
  assert.equal(code,0);assert.equal(snapshotWrites,2,'real daemon demand path did not write its matching snapshot');
  assert(!puts.some(object=>object.key==='demand.json'),'pause after matching SQL write permitted late R2 publication');
  assert(puts.some(object=>object.key==='work.json'),'statistics pause blocked independent fast signal');
  const [availability]=await q("SELECT body::jsonb body,at FROM state_factory_findings WHERE kind='factory_availability'");
  assert(availability,'combined daemon lost the formal availability writer');
  const [openedFromCache]=await q('SELECT * FROM factory_open($1::text,NULL,$2::text,$3::bigint)',
   [new Date().toISOString(),new Date(Date.now()-120000).toISOString(),availability.body.pack_max]);
  assert.deepEqual(openedFromCache,{open:availability.body.open,asking:availability.body.asking},'factory_open lost the independent cache');
  assert.equal((await q('SELECT at FROM state_factory_snapshot WHERE id=1'))[0].at,bundle.countsAt,'fast cache restamped sealed statistics');
  await setPipelineControl(env,'statistics',{paused:false,by:'generation-boundary'},now);
  provider=makeProvider();
  assert.equal((await q('SELECT count(*)::int n FROM state_statistics_namespaces'))[0].n,2);
  now+=1000;served={...served,engine:'cccccccccccc'};provider.changed();
  await provider.advance();const refreshed=await provider.get();
  assert.equal(refreshed?.namespace,currentNamespace,'served engine switch rebuilt the projection');
  assert.equal(refreshed.snapshot.engine,served.engine,'served switch published an old engine');
  const retained=JSON.stringify(await q('SELECT to_jsonb(n) body FROM state_statistics_namespaces n ORDER BY id'));
  const [generation]=await q('SELECT revision::text FROM state_statistics_generation WHERE id=1');
  await q('SELECT statistics_invalidate()');provider.changed();
  await assert.rejects(provider.advance(),/statistics retained generation budget/);
  assert.equal(provider.state().state,'capacity_hold');assert.equal((await statisticsStatus(env,now)).state,'capacity_hold');
  assert.equal(JSON.stringify(await q('SELECT to_jsonb(n) body FROM state_statistics_namespaces n ORDER BY id')),retained);
  assert.equal((await q('SELECT namespace FROM state_statistics_head'))[0].namespace,currentNamespace);
  assert.equal(await provider.get(),null,'generation hold exposed a stale count source');
  await q('UPDATE state_statistics_generation SET revision=$1::bigint WHERE id=1',[generation.revision]);
  provider.changed();await provider.advance();assert.equal((await provider.get()).namespace,currentNamespace);
  console.log('PASS native provider full wire, missing dependencies, pause/owner last-write, minimal ACL, halted writer fence/new baseline and two upgrades',schema);
  const receipt={schema,port,database,namespace:currentNamespace,firstSteps:first.steps,countsAt:bundle.countsAt,fullWire:true,legacyFallback:false,recoveryWriterFence:true,haltFreshIncarnation:true,twoNonemptyUpgrades:true,beforePutPause:true,availability:{at:availability.at,...openedFromCache},minimalAcl:acl};
  writeFileSync(new URL('native-'+schema+'.json',root),JSON.stringify(receipt,null,2)+'\n');
  return receipt;
 }finally{await opened?.close();await sql.end();}
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 let runDaemon=run;
 if(process.argv.includes('--break-before-put-guard')){
  const file=new URL('./factoryd.mjs',import.meta.url),guard="if(!await allowStatistics('demand'))return false;\n      await statistics.assert(counts);if(stopping)return false;";
  let text=readFileSync(file,'utf8');assert.equal(text.split(guard).length,2);text=text.replace(guard,'');
  text=text.replace(/from (["'])(\.[^"']+)\1/g,(_,quote,path)=>'from '+JSON.stringify(new URL(path,file).href));
  ({run:runDaemon}=await import('data:text/javascript;base64,'+Buffer.from(text).toString('base64')));
 }
 if(process.argv.includes('--native'))await nativeStatisticsProviderCases({runDaemon});else await statisticsProviderCases();
}
