// Exact canonical bags, resumable fanout and publication under concurrent writes.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import postgres from './pg/node_modules/postgres/src/index.js';
import {postgresClient,POSTGRES_OPTIONS,placeholders} from '../worker/db.js';
import {seedComputeWork} from './pg/fixtures.mjs';
import {statisticsProjectionIdentity,statisticsSourceSql,statisticsConsumerPageSql,createStatisticsProjection as productionProjection} from '../worker/statistics_projection.js';
import {STATISTICS_WORK} from './fixtures/statistics_work_421.mjs';

export async function statisticsStorageCases({create=productionProjection,only=null}={}){
 const {PGlite}=await import('./pg/node_modules/@electric-sql/pglite/dist/index.js');
 const pg=new PGlite();let activeTx;const writes=[];
 const q=async(text,args=[])=>(await(activeTx??pg).query(text,args)).rows;
 const make=(limits={})=>create({transaction:fn=>pg.transaction(async tx=>{
  activeTx=tx;try{return await fn({query:async(text,args)=>{
   if(/INSERT INTO|UPDATE state_statistics|DELETE FROM|nextval\(|SELECT statistics_replace/.test(text))writes.push(text);
   return tx.query(text,args);
  }});}finally{activeTx=null;}}),currentIdentity:()=>statisticsProjectionIdentity(q),
  limits:{storageReserveBytes:1048576,...limits}});
 try{
  for(const name of ['schema','library','sync','inventory','build_workflows','board_admission','statistics_projection'])
   await pg.exec(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
  const kept=await make().start();
  const db={run:(text,...args)=>q(placeholders(text),args),get:async(text,...args)=>(await q(placeholders(text),args))[0]};
  await seedComputeWork(db,{id:'storage-question',producer:'sweep'});
  await q(`UPDATE state_statistics_namespaces SET phase=2,cursor='["kept"]',counts_at='2026-11-17T00:00:00.000Z',published='{"old":"clock"}' WHERE id=$1`,[kept.namespace]);
  const snapshot=async()=>JSON.stringify(await q('SELECT to_jsonb(n) body FROM state_statistics_namespaces n ORDER BY id'));
  const before=await snapshot(),revision=(await q('SELECT last_value::text n FROM statistics_revision'))[0].n;
  for(const operation of ['start','step','seal']){
   if(only&&only!==operation)continue;
   writes.length=0;await assert.rejects(make({maxStorageBytes:1,storageReserveBytes:1})[operation](),/retained storage budget/);
   assert.deepEqual(writes,[],operation+' wrote before admission');assert.equal(await snapshot(),before,operation+' changed cursor/head/counts clock');
  }
  if(!only||only==='new-head'){await q('UPDATE state_statistics_generation SET revision=1');writes.length=0;
  await assert.rejects(make({maxStorageBytes:1,storageReserveBytes:1}).start(),/retained storage budget/);
  assert.deepEqual(writes,[],'new head allocated before admission');assert.equal(await snapshot(),before);
  assert.equal((await q('SELECT last_value::text n FROM statistics_revision'))[0].n,revision,'storage hold consumed incarnation');}
  if(only)return;
  await q('UPDATE state_statistics_generation SET revision=0');writes.length=0;
  await assert.rejects(make({storageReserveBytes:0}).start(),/storage reservation unqualified/);
  assert.deepEqual(writes,[],'unqualified reserve wrote');
  const [size]=await q(`SELECT sum(pg_total_relation_size(c.oid))::text bytes FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind='r' AND c.relname LIKE 'state_statistics_%'`);
  await assert.rejects(make({maxStorageBytes:Number(size.bytes)+1023,storageReserveBytes:1024}).start(),/retained storage budget/,'reserve was not included');
  await make().start();assert.equal(await snapshot(),before,'admitted same head restart changed evidence');
  console.log('PASS actual PGlite same/new-head, step/seal prewrite admission, qualified reserve, unchanged revision/cursor/old clock');
 }finally{await pg.close();}
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 if(process.argv.includes('--storage-only'))await statisticsStorageCases();else{
const projectionFile=new URL('../worker/statistics_projection.js',import.meta.url),generationBudget='export const STATISTICS_RETAINED_GENERATIONS=2;';
let projectionSource=readFileSync(projectionFile,'utf8');assert.equal(projectionSource.split(generationBudget).length,2);
projectionSource=projectionSource.replace(generationBudget,'export const STATISTICS_RETAINED_GENERATIONS=16;')
 .replace(/from (["'])(\.[^"']+)\1/g,(_,quote,path)=>'from '+JSON.stringify(new URL(path,projectionFile).href));
const fixtureRoot=new URL('../target/statistics-projection/',import.meta.url);mkdirSync(fixtureRoot,{recursive:true});
const fixtureModule=new URL('membership-'+process.pid+'.mjs',fixtureRoot);writeFileSync(fixtureModule,projectionSource);
const {createStatisticsProjection,StatisticsProjectionBudget}=await import(fixtureModule.href);

const port=Number(process.env.WFSIM_STATISTICS_TEST_PORT||55535);
assert(Number.isSafeInteger(port)&&port>=1024&&port<=65535,'explicit local test port');
const schema='statistics_check_'+Date.now(),options={...POSTGRES_OPTIONS,max:1,idle_timeout:0,onnotice:()=>{}};
const open=()=>postgres('postgres://127.0.0.1:'+port+'/postgres',options);
const sql=open(),writer=open(),peek=open(),client=postgresClient(sql);
const exec=async(text,args=[])=>[...await sql.unsafe(text,args)],write=async(text,args=[])=>[...await writer.unsafe(text,args)];
const query=async(text,args=[])=>[...await peek.unsafe(text,args)];
const db={exec:text=>sql.unsafe(text),run:async(text,...args)=>(await sql.unsafe(placeholders(text),args)).count,
 get:async(text,...args)=>(await exec(placeholders(text),args))[0]??null,all:(text,...args)=>exec(placeholders(text),args)};
const ddl=readFileSync(new URL('../worker/pg/statistics_projection.sql',import.meta.url),'utf8');
const consumerIndex=ddl.match(/^-- (CREATE INDEX CONCURRENTLY[^\n]+);$/m)[1].replace('INDEX CONCURRENTLY','INDEX');
let at=Date.parse('2026-11-17T00:00:00.000Z'),stopped=false,hook=async()=>{};
const stamp=()=>new Date(at).toISOString(),currentIdentity=()=>statisticsProjectionIdentity(query);
// Membership keeps its fixture incarnations in a source copy; production refuses this budget.
const limits={pageRows:8,maxMs:10000,maxQueries:2048,maxRows:20000,maxNamespaces:16,storageReserveBytes:1048576};
const projection=(extra={})=>createStatisticsProjection({transaction:client.transaction,currentIdentity,stopped:()=>stopped,now:()=>at,
 limits,...extra,onQuery:async event=>{await hook(event);await extra.onQuery?.(event);}});
let unit=projection();
const sort=rows=>rows.map(row=>JSON.stringify(row)).sort();
const kind=`CASE WHEN EXISTS(SELECT 1 FROM relation_task_consumers d WHERE d.producer='new_build' AND d.state='live' AND d.ref=q.build_id||'|'||q.ruler||'|'||q.mode) THEN 0 WHEN q.batch LIKE 'rescore%' THEN 1 WHEN q.batch LIKE 'refresh-%' THEN 3 ELSE 2 END`;
const allTables=['namespaces','obligations','checkpoints','contributions','rollup','expiry'];
const derived=async()=>Object.fromEntries(await Promise.all(allTables.map(async name=>[name,sort(await exec('SELECT to_jsonb(x) body FROM state_statistics_'+name+' x'))])));
async function oracle(){
 const pending=await exec('SELECT batch,build_id,ruler,mode,count(*)::text n FROM state_board_effective_pending_work GROUP BY batch,build_id,ruler,mode');
 const frozen=await exec(`WITH ${STATISTICS_WORK} SELECT batch,build_id,ruler,mode,count(*)::text n FROM stats_pending GROUP BY batch,build_id,ruler,mode`);
 assert.deepEqual(sort(frozen),sort(pending),'frozen421 and current canonical complete pending bag');
 const open=await exec(`SELECT verb,producer,${statisticsSourceSql('q')} source,count(*)::text n,count(*) FILTER(WHERE leased_at IS NOT NULL OR lease IS NOT NULL)::text dispatched FROM state_compute_effective_work q WHERE state='open' GROUP BY verb,producer,source`);
 const taken=await exec(`SELECT k,count(*)::text n FROM(SELECT x.id,min(CASE WHEN d.producer='new_build' THEN 0 WHEN d.payload::jsonb->>'batch' LIKE 'rescore%' THEN 1 WHEN d.producer='refresh' THEN 3 ELSE 2 END) k
  FROM state_compute_effective_work x JOIN relation_task_consumers d ON d.question=x.id AND d.state='live' AND d.producer IN('new_build','survey_build','refresh','sweep') WHERE x.state='open' AND x.verb='simulate' AND(x.leased_at IS NOT NULL OR x.lease IS NOT NULL) GROUP BY x.id)y GROUP BY k`);
 const owed=await exec(`SELECT ${kind} k,${statisticsSourceSql('q')} source,count(*)::text n FROM state_board_effective_pending_work q JOIN entity_compute_batches b ON b.id=q.batch GROUP BY k,source`);
 const [snapshot]=await exec(`SELECT(SELECT count(*) FROM state_board_effective_pending_work)::text queue_rows,
  count(*) FILTER(WHERE waits=0 AND(lease_until IS NULL OR lease_until<$1))::text open,count(*) FILTER(WHERE waits>0)::text urgent FROM state_compute_effective_work WHERE state='open'`,[stamp()]);
 const result={by:{},sources:{},takenBy:Object.fromEntries(taken.map(x=>[x.k,Number(x.n)])),openOf:open.map(x=>({...x,n:Number(x.n),dispatched:Number(x.dispatched)})),snapshot:Object.fromEntries(Object.entries(snapshot).map(([k,v])=>[k,Number(v)]))};
 for(const row of owed){result.by[row.k]=(result.by[row.k]??0)+Number(row.n);(result.sources[row.k]??={requested:0,system:0,unattributed:0})[row.source]+=Number(row.n);}
 return{...result,bag:pending.map(({batch,build_id,ruler,mode,n})=>({key:JSON.stringify([batch,build_id,ruler,mode]),n:Number(n)}))};
}
async function equivalent(counts,expected=undefined){
 expected??=await oracle();const {bag,...wanted}=expected;
 assert.deepEqual({...counts.snapshot,at:undefined},{...wanted.snapshot,at:undefined},'snapshot exact membership');
 for(const name of ['by','sources','takenBy'])assert.deepEqual(counts[name],wanted[name],name);
 assert.deepEqual(sort(counts.openOf),sort(wanted.openOf),'openOf');
 const actual=await exec(`SELECT key,(SELECT value->>1 FROM jsonb_array_elements(body) WHERE value->0='["queue_rows"]'::jsonb)::bigint n FROM state_statistics_contributions WHERE namespace=$1 AND kind='p'`,[counts.namespace]);
 assert.deepEqual(sort(actual.map(x=>({...x,key:JSON.stringify(JSON.parse(x.key)),n:Number(x.n)}))),sort(bag),'per-physical bag multiplicity');
}
async function converge(max=10000){
 for(let i=0;i<max;i++){
  await unit.step();const [head]=await exec(`SELECT n.phase::text,(SELECT count(*) FROM state_statistics_obligations o WHERE o.namespace=n.id)::text work,
   (SELECT count(*) FROM state_statistics_journal j WHERE NOT EXISTS(SELECT 1 FROM state_statistics_checkpoints c WHERE c.namespace=n.id AND(c.owner,c.kind,c.key,c.revision)=(j.owner,j.kind,j.key,j.revision)))::text dirty FROM state_statistics_head h JOIN state_statistics_namespaces n ON n.id=h.namespace`);
  if(head.phase==='2'&&head.work==='0'&&head.dirty==='0')return i+1;
 }
 throw Error('bounded convergence fixture failed');
}
async function qualify(label){const steps=await converge(),{counts}=await unit.seal();await equivalent(counts);console.log('ok',label,steps,'steps');return counts;}
const ref=async(producer,key,question,state='live',payload='{}')=>db.run(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,payload) VALUES(?,?,?,?,?,?,?)`,producer,key,question,state,'2026-11-16T00:00:00.000Z','2026-11-16T00:00:00.000Z',payload);
const pending=async(batch,build,ruler='r',mode='base',initiator=null)=>db.run('INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator) VALUES(?,?,?,?,?)',batch,build,ruler,mode,initiator);
async function sourceLockCase(capture,reference='shared|r|base'){
 await exec('ALTER TABLE relation_task_consumers '+(capture?'ENABLE':'DISABLE')+' TRIGGER statistics_capture');
 await exec("UPDATE relation_task_consumers SET state='live' WHERE ref=$1",[reference]);
 const guard=(q,id,producer,reference)=>q('SELECT factory_delivery_guard($1,$2,$3,$4,$5,$6,$7)',[id,'{}','fixture',producer,reference,'2026-11-16T00:00:00.000Z','{}']);
 await exec("BEGIN; SET LOCAL deadlock_timeout='50ms'; SET LOCAL statement_timeout='2000ms'");
 await write("BEGIN; SET LOCAL deadlock_timeout='50ms'; SET LOCAL statement_timeout='2000ms'");
 try{
  await guard(exec,'lock-a','new_build',reference);await exec("UPDATE relation_task_consumers SET state='served' WHERE producer='new_build' AND ref=$1",[reference]);
  await guard(write,'lock-b','sweep',reference);const began=performance.now();
  const b=write("UPDATE relation_task_consumers SET state='served' WHERE producer='sweep' AND ref=$1",[reference]).then(()=>({ok:true}),error=>({error:error.code}));
  const first=await Promise.race([b,new Promise(resolve=>setTimeout(()=>resolve({blocked:true}),30))]);let a;
  if(first.blocked){const both=await Promise.all([b,guard(exec,'lock-b','refresh','other|r|base').then(()=>({ok:true}),error=>({error:error.code}))]);a=both;}
  else{await write('COMMIT');a=[first,await guard(exec,'lock-b','refresh','other|r|base').then(()=>({ok:true}),error=>({error:error.code}))];}
  return{capture,waitMs:performance.now()-began,blocked:!!first.blocked,results:a};
 }finally{await exec('ROLLBACK').catch(()=>{});await write('ROLLBACK').catch(()=>{});}
}
try{
 await sql.unsafe('CREATE SCHEMA '+schema);for(const c of [sql,writer,peek])await c.unsafe('SET search_path TO '+schema+',pg_catalog');
 for(const name of ['schema','library','sync','inventory','build_workflows','board_admission'])await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 await sql.unsafe(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
 await sql.unsafe(ddl);await sql.unsafe(consumerIndex);
 if(process.argv.includes('--baseline-only')){
  const captureTables=['state_compute_scheduling','relation_task_consumers','state_board_pending_work','entity_compute_batches','entity_compute_tasks','entity_compute_results','record_candidate_answers','state_pending_verifications','record_compute_order_cancellations','relation_compute_order_cut_protection','entity_compute_order_cuts','record_compute_order_cut_activations','state_factory_findings'];
  for(const name of captureTables)await exec('ALTER TABLE '+name+' DISABLE TRIGGER statistics_capture');
  await exec(`INSERT INTO entity_compute_specs(id,verb,request) SELECT lpad(i::text,32,'0'),'simulate',jsonb_build_object('fixture',i)::text FROM generate_series(1,100000)i;
   INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason) SELECT i,lpad(i::text,32,'0'),1,'2026-11-15','fixture' FROM generate_series(1,100000)i;
   SELECT setval(pg_get_serial_sequence('entity_compute_tasks','id'),100000);
   INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,zone,zone_position) SELECT lpad(i::text,32,'0'),'simulate',i,'open','2026-11-15','2026-11-15',3,i FROM generate_series(1,100000)i;
   INSERT INTO entity_compute_batches(id,at,total) VALUES('old','2026-11-15',100000),('new','2026-11-17',256);
   INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) SELECT 'sweep',id||'|r|base',id,'live','2026-11-15','2026-11-15' FROM state_compute_scheduling`);
  for(let from=1;from<=100000;from+=256)await exec("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) SELECT 'old',id,'r','base' FROM state_compute_scheduling WHERE task_id>=$1::bigint AND task_id<$2::bigint",[from,from+256]);
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'benchmark-cut',cut_at:'2026-11-16',task_cutoff:'100000',keep_boundary:[3,0,''],source:{fixture:'cold-baseline'}}));
  await db.run("SELECT compute_order_cut_capture('benchmark-cut')");await db.run("SELECT compute_order_cut_activate('benchmark-cut',0,?::text)",createHash('sha256').update('').digest('hex'));
  for(const name of captureTables)await exec('ALTER TABLE '+name+' ENABLE TRIGGER statistics_capture');
  await exec('ANALYZE state_compute_scheduling; ANALYZE relation_task_consumers; ANALYZE entity_compute_tasks; ANALYZE state_board_pending_work');
  const bench=async label=>{
   const began=performance.now(),metrics={steps:0,logicalQueries:0,returnedRows:0,rawQuestionRows:0,questionPages:0,restarted:false};
   const make=()=>projection({limits:{...limits,pageRows:128,maxMs:3000,maxRows:8192,maxQueries:1024},onQuery:async event=>{
    metrics.logicalQueries++;metrics.returnedRows+=event.rows.length;
    if(event.sql.startsWith('WITH page AS MATERIALIZED(SELECT id,state FROM state_compute_scheduling')){metrics.rawQuestionRows+=event.rows.length;metrics.questionPages++;}
   }});
   unit=make();const start=await unit.start();assert.equal(await unit.read(),null);
   for(;;){await unit.step();metrics.steps++;
    if(!metrics.restarted&&metrics.rawQuestionRows>=50000){const [saved]=await exec('SELECT phase::text,cursor,counts_at FROM state_statistics_namespaces WHERE id=$1',[start.namespace]);
     assert.equal(saved.counts_at,null);unit=make();assert.equal((await unit.start()).namespace,start.namespace);const [after]=await exec('SELECT phase::text,cursor FROM state_statistics_namespaces WHERE id=$1',[start.namespace]);
     assert.deepEqual(after,{phase:saved.phase,cursor:saved.cursor});metrics.restarted=true;metrics.restartCursor=saved.cursor;}
    const [state]=await exec(`SELECT n.phase::text,(SELECT count(*) FROM state_statistics_obligations WHERE namespace=n.id)::text work,
     EXISTS(SELECT 1 FROM state_statistics_journal j WHERE NOT EXISTS(SELECT 1 FROM state_statistics_checkpoints c WHERE c.namespace=n.id AND(c.owner,c.kind,c.key,c.revision)=(j.owner,j.kind,j.key,j.revision))) dirty FROM state_statistics_namespaces n WHERE n.id=$1`,[start.namespace]);
    if(state.phase==='2'&&state.work==='0'&&!state.dirty)break;
   }
   const sealed=await unit.seal();metrics.ms=performance.now()-began;metrics.rawRowsPerSecond=metrics.rawQuestionRows/(metrics.ms/1000);await equivalent(sealed.counts);
   const [stored]=await exec('SELECT count(*)::text n FROM state_statistics_contributions WHERE namespace=$1',[start.namespace]);metrics.contributions=Number(stored.n);metrics.queueRows=sealed.counts.snapshot.queue_rows;
   console.log('BASELINE',label,JSON.stringify(metrics));return metrics;
  };
  const zero=await bench('100k-zero-eligible');assert.equal(zero.queueRows,0);assert.equal(zero.contributions,0);
  assert(zero.questionPages<=150,'sparse baseline must advance beyond the per-page enqueue capacity');
  for(let i=0;i<256;i++){const id='new-'+String(i).padStart(5,'0');await seedComputeWork(db,{id});await pending('new',id);await ref('sweep',id+'|r|base',id);}
  await exec('SELECT statistics_invalidate()');const positive=await bench('100k-tail-plus-256-positive');assert.equal(positive.queueRows,256);
  mkdirSync(new URL('../target/statistics-projection/',import.meta.url),{recursive:true});writeFileSync(new URL('../target/statistics-projection/baseline-'+schema+'.json',import.meta.url),JSON.stringify({schema,port,zero,positive,noProductionEta:true,limits:{pageRows:128,maxMs:3000}},null,2)+'\n');
  console.log('PASS cold baseline restart and sparse positive contributions');
 }else if(process.argv.includes('--locking-only')){
  for(const id of ['lock-a','lock-b'])await seedComputeWork(db,{id,state:'fact'});
  await ref('new_build','shared|r|base','lock-a');await ref('sweep','shared|r|base','lock-b');await ref('refresh','other|r|base','lock-b');
  const baseline=await sourceLockCase(false);
  const [touch]=await exec("SELECT pg_get_functiondef('statistics_touch(text,jsonb,text)'::regprocedure) def");
  await sql.unsafe(touch.def.replace("VALUES($3,$1,$2::text,nextval('statistics_revision'))","VALUES('shared',$1,$2::text,nextval('statistics_revision'))"));
  const captured=await sourceLockCase(true);await sql.unsafe(touch.def);const fixed=await sourceLockCase(true);
  console.log(JSON.stringify({schema,baseline,captured,fixed},null,2));
  assert(!baseline.blocked&&baseline.results.every(x=>x.ok),'canonical baseline blocked');
  assert(captured.blocked&&captured.results.some(x=>x.error==='40P01'),'old shared capture did not exercise the actual cycle');
  assert(!fixed.blocked&&fixed.results.every(x=>x.ok),'owner capture still blocks a distinct source row');
  await exec('DELETE FROM state_statistics_invalidations');
  const long='long-'+('x'.repeat(1100))+'|r|base';await exec('UPDATE relation_task_consumers SET ref=$1 WHERE ref=$2',[long,'shared|r|base']);
  await exec('DELETE FROM state_statistics_invalidations');
  const longResult=await sourceLockCase(true,long);assert(!longResult.blocked&&longResult.results.every(x=>x.ok),'oversized fallback blocks source delivery');
  assert((await exec('SELECT 1 FROM state_statistics_invalidations')).length,'oversized source did not stop derived capture');
  await assert.rejects(unit.start(),/capture storage budget/);console.log('PASS source canonical-row ownership and independent oversized halt marker',JSON.stringify(longResult));
 }else{
 if(!process.argv.includes('--performance-only')){
 await unit.start();assert.equal(await unit.read(),null);await assert.rejects(unit.seal(),/baseline incomplete/);
 await qualify('empty baseline');
 await db.exec(`INSERT INTO entity_compute_batches(id,at,total) VALUES('old','2026-11-15T00:00:00.000Z',100),('new','2026-11-17T00:00:00.000Z',100),('rescore-old','2026-11-15T00:00:00.000Z',100),('refresh-new','2026-11-17T00:00:00.000Z',100)`);
 for(let i=0;i<16;i++){
  const id='q'+String(i).padStart(2,'0');await seedComputeWork(db,{id,state:i===4?'fact':i===5?'withdrawn':i===6?'dispute':'open',producer:i%2?'user_calculation':'sweep',initiator:i%3===0?'system':i%3===1?'user:x':null,waits:i===2?2:0});
  await db.run('UPDATE state_compute_scheduling SET zone=3,zone_position=? WHERE id=?',i+1,id);
  await pending(i%2?'new':'old',id,'r','base',i%3===0?'system':i%3===1?'user:x':null);await ref('sweep',id+'|r|base',id);
 }
 await db.run("UPDATE state_compute_scheduling SET lease='held',leased_at=?,lease_until=? WHERE id='q01'",stamp(),new Date(at+1000).toISOString());
 await ref('new_build','q01|r|base','q00');await ref('refresh','q01|r|base|suffix','q02');
 for(const [build,ruler,mode,reference] of [['suffix','r','base','suffix|r|base|more'],['missing','r','','missing|r'],['two','','','two'],['empty','','','empty||'],['pipe|build','r','base','pipe|build|r|base'],['modepipe','r','a|b','modepipe|r|a|b'],['unicode_%[]','汉','','unicode_%[]|汉|']]){
  await pending('new',build,ruler,mode);await ref('refresh',reference,'q00');
 }
 await pending('orphan','orphan');await ref('sweep','orphan|r|base','q00');await pending('old','move-old');await pending('old','move-new');await ref('sweep','move-old|r|base','q15');
 await unit.start();const noCut=await qualify('no-cut physical queue including orphan');
 await sql.unsafe(ddl);await sql.unsafe(ddl);await equivalent((await unit.seal()).counts);
 console.log('ok nonempty double additive upgrade retains contributions and clocks');
 const cutoff=String((await exec('SELECT max(id)::text n FROM entity_compute_tasks'))[0].n),cut='2026-11-16T12:00:00.000Z';
 await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'projection-cut',cut_at:cut,task_cutoff:cutoff,keep_boundary:[3,4,'z'],source:{fixture:'statistics-projection'}}));
 const protect=['q00','q15'];await db.run('SELECT compute_order_cut_protect(?::text,(?::text)::jsonb)','projection-cut',JSON.stringify(protect));
 await db.run("SELECT compute_order_cut_capture('projection-cut')");
 await db.run("SELECT compute_order_cut_activate('projection-cut',?::bigint,?::text)",protect.length,createHash('sha256').update(protect.join('\n')).digest('hex'));
 await assert.rejects(unit.step(),/target changed|baseline not started/);await unit.start();assert.equal(await unit.read(),null);
 await qualify('cut exact bag, suffix double, missing/empty/pipe, nonopen and orphan');
 const [suffix]=await exec("SELECT count(*)::text n FROM state_board_effective_pending_work WHERE build_id='suffix'");assert.equal(suffix.n,'2','suffix retains independent postcut arm');
 const cancelled=await exec("SELECT id FROM state_compute_scheduling WHERE state='open' AND NOT EXISTS(SELECT 1 FROM state_compute_effective_work w WHERE w.id=state_compute_scheduling.id) ORDER BY id");assert(cancelled.length>0,'fixture has an actually cancelled tail');
 await db.run("UPDATE relation_task_consumers SET asked_at='2026-11-17T00:00:00.000Z' WHERE question='q05'");await qualify('withdrawn late consumer evidence requalifies original guard');
 await db.run("INSERT INTO record_compute_order_cancellations(question,task_id,cut_id,at) SELECT id,task_id,'projection-cut',? FROM state_compute_scheduling WHERE id='q00'",stamp());await qualify('protected same-cut exact question/task ledger');
 await db.run("DELETE FROM record_compute_order_cancellations WHERE question='q00'");await qualify('ledger deletion requalifies membership');
 await db.run("UPDATE relation_task_consumers SET ref='move-new|r|base' WHERE producer='sweep' AND ref='move-old|r|base'");await qualify('consumer old/new ref removes old key');
 await db.run("DELETE FROM entity_compute_batches WHERE id='new'");await qualify('batch deletion keeps live orphan queue but removes owed');
 await db.run("INSERT INTO entity_compute_batches(id,at,total) VALUES('new','2026-11-17T00:00:00.000Z',100)");await qualify('batch reinsertion recovers postcut and owed');
 for(let i=0;i<19;i++)await ref('refresh','q15|r|base|fan-'+String(i).padStart(3,'0'),'q15');
 await pending('old','fan-remove');await ref('sweep','fan-remove|r|base','q15');await qualify('cross-page global aliases deduplicate once');
 let middle=false;hook=async event=>{if(!event.rr&&!middle&&event.sql.startsWith('UPDATE state_statistics_obligations SET cursor=')&&event.args[1]==='q'&&event.args[2]==='["q15"]'){
  middle=true;await write("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('old','fan-added','r','base')");
  await write("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('new_build','fan-added|r|base','q15','live','2026-11-17','2026-11-17')");
  await write("DELETE FROM state_board_pending_work WHERE build_id='fan-remove'");
 }};
 await db.run("UPDATE state_compute_scheduling SET waits=1 WHERE id='q15'");await converge();hook=async()=>{};assert(middle);await equivalent((await unit.seal()).counts);
 console.log('ok fanout page insertion behind cursor and pending deletion');
 const beforeCrash=await derived();let crash=true;
 hook=async event=>{if(crash&&event.sql.startsWith('UPDATE state_statistics_namespaces SET phase=')){crash=false;throw Error('injected baseline crash');}};
 await exec('SELECT statistics_invalidate()');await unit.start();await assert.rejects(unit.step(),/injected baseline crash/);hook=async()=>{};
 const afterCrash=await derived();for(const name of ['obligations','checkpoints','contributions','rollup','expiry'])assert.deepEqual(afterCrash[name],beforeCrash[name],'atomic crash '+name);
 const adaptive=projection({limits:{...limits,pageRows:128}}),widths=[];
 hook=async event=>{
  if(!event.sql.startsWith('WITH page AS MATERIALIZED(SELECT id,state FROM state_compute_scheduling'))return;
  widths.push(Number(event.args.at(-1)));
  if(widths.length===1)throw Object.assign(new Error('fixture statement timeout'),{code:'57014'});
 };
 await assert.rejects(adaptive.step(),{code:'57014'});
 assert.deepEqual(await derived(),afterCrash,'timed-out page committed');
 await adaptive.step();hook=async()=>{};
 assert.deepEqual(widths,[128,64],'timed-out page did not retry at a smaller width');
 unit=projection();await qualify('new process resumes interrupted baseline');
 const stable=await unit.read();await new Promise(r=>setTimeout(r,5));assert.equal((await unit.read()).countsAt,stable.countsAt,'cache never restamps');
 at+=2000;await equivalent((await unit.seal()).counts);console.log('ok lease expiry obligation without source update');
 await db.run("UPDATE relation_task_consumers SET payload='{\"batch\":\"rescore-hot\"}' WHERE producer='sweep' AND ref='q01|r|base'");
 const beforeHot=await oracle();let hot=false,newRevision;
 hook=async event=>{if(event.rr&&!hot&&event.sql.includes('INSERT INTO state_statistics_checkpoints')){
  hot=true;const began=performance.now();await write("UPDATE relation_task_consumers SET payload='{}' WHERE producer='sweep' AND ref='q01|r|base'");assert(performance.now()-began<1000,'RR reader blocked producer dirty');
  newRevision=(await write("SELECT revision::text FROM state_statistics_journal WHERE kind='q' AND key='[\"q01\"]' ORDER BY revision DESC LIMIT 1"))[0]?.revision;
 }};
 const sealed=(await unit.seal()).counts;hook=async()=>{};assert(hot);await equivalent(sealed,beforeHot);
 assert((await exec('SELECT 1 FROM state_statistics_journal WHERE revision=$1::bigint',[newRevision])).length,'RR checkpoint overwrote newer journal token');
 await unit.step();if(newRevision)assert((await exec("SELECT revision::text FROM state_statistics_journal WHERE kind='q' AND key='[\"q01\"]'"))[0]?.revision===newRevision||!(await exec("SELECT 1 FROM state_statistics_checkpoints WHERE kind='q' AND key='[\"q01\"]'" )).length,'CAS removed a newer source revision');
 await qualify('hot dirty during RR checkpoint retained for next sample');
 // Reproduce the retired-identity reuse policy against an actual cleaned journal.
 const originalIdentity=await currentIdentity(),originalHead=(await exec('SELECT namespace FROM state_statistics_head'))[0].namespace;
 await exec('SELECT statistics_invalidate()');await unit.start();await qualify('second generation baseline');
 await db.run("UPDATE state_compute_scheduling SET waits=1 WHERE id='q03'");await qualify('second generation consumes changed input');
 for(let i=0;i<20&&(await exec('SELECT 1 FROM state_statistics_journal LIMIT 1')).length;i++)await unit.step();
 assert.equal((await exec('SELECT 1 FROM state_statistics_journal LIMIT 1')).length,0,'generation fixture journal not cleaned');
 const secondHead=(await exec('SELECT namespace FROM state_statistics_head'))[0].namespace;
 await exec('UPDATE state_statistics_generation SET revision=$1::bigint',[originalIdentity.generation]);at+=1000;
 await exec('UPDATE state_statistics_head SET namespace=$1',[originalHead]);
 const badReuse=(await unit.seal()).counts;await assert.rejects(equivalent(badReuse),/snapshot exact|openOf/);
 await exec('UPDATE state_statistics_head SET namespace=$1',[secondHead]);const restarted=await unit.start();assert.notEqual(restarted.namespace,originalHead);
 assert.equal(await unit.read(),null);await assert.rejects(unit.seal(),/baseline incomplete/);await qualify('retired identity actual reuse FAIL fresh incarnation PASS');
 let changed=false;const oldAt=(await unit.read()).countsAt;
 hook=async event=>{if(event.rr&&!changed&&event.sql.startsWith('UPDATE state_statistics_namespaces SET counts_at=')){
  changed=true;await write("INSERT INTO state_factory_findings(kind,body,at) VALUES('task_inventory','{\"engine\":\"fixture\",\"source\":\"new-source\",\"snapshot\":\"new-snapshot\"}', $1) ON CONFLICT(kind) DO UPDATE SET body=excluded.body",[stamp()]);
 }};
 await assert.rejects(unit.seal(),/namespace changed/);hook=async()=>{};
 assert((await exec('SELECT counts_at FROM state_statistics_namespaces WHERE counts_at=$1',[oldAt])).length,'failed source seal removed old publication');
 await unit.start();await qualify('source switch uses independent resumable namespace');
 const identityBefore=await currentIdentity(),headBefore=(await exec('SELECT namespace FROM state_statistics_head'))[0].namespace;
 await db.run("UPDATE state_factory_findings SET at=?,body=jsonb_set(body::jsonb,'{progress}','7'::jsonb)::text WHERE kind='task_inventory'",stamp());
 assert.deepEqual(await currentIdentity(),identityBefore,'unchanged inventory identity invalidates baseline');assert.equal((await unit.start()).namespace,headBefore);
 const [definition]=await exec("SELECT pg_get_functiondef('factory_default_zone(text)'::regprocedure) def");
 const [zoneBody]=await exec("SELECT prosrc FROM pg_proc WHERE oid='factory_default_zone(text)'::regprocedure");
 await sql.unsafe(definition.def.replace(zoneBody.prosrc,zoneBody.prosrc+'\n'));
 await assert.rejects(unit.start(),/contract requires upgrade/);await sql.unsafe(definition.def);assert.equal((await unit.start()).namespace,headBefore);
 const hung=createStatisticsProjection({transaction:client.transaction,currentIdentity:()=>new Promise(()=>{}),limits:{...limits,maxMs:40}}),hungAt=performance.now();
 await assert.rejects(hung.start(),StatisticsProjectionBudget);assert(performance.now()-hungAt<500,'identity await escapes total deadline');
 const beforeStop=await derived();hook=async event=>{if(event.rr&&event.sql.startsWith('UPDATE state_statistics_namespaces SET counts_at='))stopped=true;};
 await assert.rejects(unit.seal(),/stopped/);stopped=false;hook=async()=>{};assert.deepEqual(await derived(),beforeStop,'last-write stop rolls back publication');
 const small=projection({limits:{...limits,maxRows:1}});const beforeBudget=await derived();await assert.rejects(small.seal(),StatisticsProjectionBudget);assert.deepEqual(await derived(),beforeBudget,'input limit rollback retains obligation');
 console.log('ok source/owner-stop/input-limit rollback, no duplicate replay');
 // The broken capture is installed and exercised, then restored before retry.
 const functionText=(await exec("SELECT pg_get_functiondef('statistics_capture()'::regprocedure) def"))[0].def;
 await sql.unsafe(functionText.replace('PERFORM statistics_ref(OLD.ref,old_owner);',''));
 await db.run("UPDATE relation_task_consumers SET ref='move-old|r|base' WHERE producer='sweep' AND ref='move-new|r|base'");
 await converge();const broken=(await unit.seal()).counts;await assert.rejects(equivalent(broken),/per-physical|snapshot exact|by|sources/);
 await sql.unsafe(functionText);await db.run("SELECT statistics_ref('move-new|r|base')");await qualify('actual missing-old-ref capture FAIL restored PASS');
 console.log('ok original no-cut sample timestamp retained',noCut.countsAt);
 }
 const performanceReceipt={};
 await sql.begin(async tx=>{
  await tx.unsafe('ALTER TABLE relation_task_consumers DISABLE TRIGGER statistics_capture');
  await tx.unsafe(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
   SELECT 'sweep',md5(i::text)||'|fixture-ruler|base','00000000000000000000000000000001','live','x','x' FROM generate_series(1,100000)i
   UNION ALL SELECT 'sweep','unrelated-'||md5(i::text),'unrelated-'||i,'live','x','x' FROM generate_series(1,100000)i`);
  await tx.unsafe("SET LOCAL work_mem='1MB'; SET LOCAL maintenance_work_mem='1MB'; SET LOCAL log_temp_files=0; ANALYZE relation_task_consumers; DROP INDEX statistics_consumer_pages");
  const request=statisticsConsumerPageSql(true);
  const metrics=plan=>{const out={consumer_rows:0,sort_rows:0,blocks:(plan['Shared Hit Blocks']||0)+(plan['Shared Read Blocks']||0),temp:(plan['Temp Read Blocks']||0)+(plan['Temp Written Blocks']||0)};
   const walk=p=>{if(p['Relation Name']==='relation_task_consumers')out.consumer_rows+=((p['Actual Rows']||0)+(p['Rows Removed by Filter']||0))*(p['Actual Loops']||0);if(p['Node Type']==='Sort')out.sort_rows=Math.max(out.sort_rows,p.Plans?.[0]?.['Actual Rows']||0);for(const child of p.Plans||[])walk(child);};walk(plan);return out;};
  for(const mode of ['force_generic_plan','force_custom_plan']){
   await tx.unsafe('SET LOCAL plan_cache_mode='+mode);await tx.unsafe('PREPARE statistics_fanout(text,text,text,bigint) AS '+request);
   const [old]=await tx.unsafe("EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON) EXECUTE statistics_fanout('00000000000000000000000000000001','sweep','ffffffffffffffffffffffffffffffff|fixture-ruler|base',128)");
   const oldPlan=old['QUERY PLAN'][0];await tx.unsafe('DEALLOCATE statistics_fanout');
   const [wal]=await tx.unsafe('SELECT pg_current_wal_insert_lsn() lsn');const began=performance.now();
   await tx.unsafe(consumerIndex);
   const [size]=await tx.unsafe("SELECT pg_relation_size('statistics_consumer_pages')::text bytes,pg_wal_lsn_diff(pg_current_wal_insert_lsn(),$1::pg_lsn)::text wal",[wal.lsn]);
   await tx.unsafe('PREPARE statistics_fanout(text,text,text,bigint) AS '+request);
   const [fresh]=await tx.unsafe("EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON) EXECUTE statistics_fanout('00000000000000000000000000000001','sweep','ffffffffffffffffffffffffffffffff|fixture-ruler|base',128)");
   const freshPlan=fresh['QUERY PLAN'][0],before=metrics(oldPlan.Plan),after=metrics(freshPlan.Plan);
   mkdirSync(new URL('../target/statistics-projection/',import.meta.url),{recursive:true});writeFileSync(new URL('../target/statistics-projection/fanout-'+schema+'-'+mode+'.json',import.meta.url),JSON.stringify({before,after,oldPlan,freshPlan},null,2)+'\n');
   assert(before.consumer_rows>50000,'old fanout did not exercise unbounded input: '+JSON.stringify(before));
   assert(after.consumer_rows<=128&&after.sort_rows===0&&after.temp===0&&after.blocks<=256,'new fanout exceeded real bounds: '+JSON.stringify(after));
   const [filled]=await tx.unsafe("EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON) EXECUTE statistics_fanout('00000000000000000000000000000001','sweep','80000000000000000000000000000000|fixture-ruler|base',128)");
   const filledMetrics=metrics(filled['QUERY PLAN'][0].Plan);assert.equal(filled['QUERY PLAN'][0].Plan['Actual Rows'],128);assert(filledMetrics.consumer_rows<=128&&filledMetrics.sort_rows===0&&filledMetrics.blocks<=256,'nonempty indexed page not bounded');
   performanceReceipt[mode]={before,after,filled:filledMetrics,index:{bytes:Number(size.bytes),rows:200000,bytesPerRow:Number(size.bytes)/200000,wal:Number(size.wal),localBuildMs:performance.now()-began},oldPlan,freshPlan};
   await tx.unsafe('DEALLOCATE statistics_fanout; DROP INDEX statistics_consumer_pages');
  }
  await tx.unsafe(`INSERT INTO state_statistics_contributions(namespace,kind,key,body)
   SELECT repeat('f',64),'p',jsonb_build_array('rescore-fixture',md5(i::text),'fixture-ruler','base')::text,
    '[[["queue_rows"],1],[["owed",2,"unattributed"],1]]'::jsonb FROM generate_series(1,100000)i`);
  const [size]=await tx.unsafe("SELECT pg_total_relation_size('state_statistics_contributions')::text bytes,(SELECT count(*) FROM state_statistics_contributions)::text rows,(SELECT avg(pg_column_size(x)) FROM state_statistics_contributions x WHERE namespace=repeat('f',64))::text row_bytes");
  performanceReceipt.contributions={bytes:Number(size.bytes),rows:Number(size.rows),tableAndIndexBytesPerRow:Number(size.bytes)/Number(size.rows),heapTupleBytes:Number(size.row_bytes)};
  await tx.unsafe('ROLLBACK');
 });
 console.log('ok 100k fanout generic/custom old scan/sort FAIL new bounded PASS; local capacity measured');
 mkdirSync(new URL('../target/statistics-projection/',import.meta.url),{recursive:true});
 writeFileSync(new URL('../target/statistics-projection/receipt-'+schema+'.json',import.meta.url),JSON.stringify({schema,port,canonicalBag:!process.argv.includes('--performance-only'),frozen421:'d88861f2dfe2f1069f61e392a6310307ef15d0d448acd5730eb5ef3a5635b30f',at:stamp(),performance:performanceReceipt},null,2)+'\n');
 console.log('PASS statistics projection native PostgreSQL',process.argv.includes('--performance-only')?'performance-only':'complete',schema);
 }
}finally{await Promise.all([sql.end(),writer.end(),peek.end()]);}

}}
