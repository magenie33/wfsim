// Captured keys must bound source reads without changing canonical pending multiplicity.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {seedComputeWork} from './pg/fixtures.mjs';

const ddl=name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8');
const current=ddl('inventory').match(/CREATE OR REPLACE FUNCTION inventory_inputs\(text,text,text\)[^]*?END \$\$;/)[0];
const broad=readFileSync(new URL('../tests/fixtures/inventory-inputs-keyed/broad-inputs.sql',import.meta.url),'utf8');
assert.equal(createHash('sha256').update(broad).digest('hex'),'1ecb2797db28ac96c27c726a3af557947b7e19db58384ccb3d8cc5fe614cb085');

export async function inventoryInputKeyCases(db,{noise=2048,receiptFile=null}={}) {
  assert(Number.isSafeInteger(noise)&&noise>=2048);
  const engine='0ae1bf8edb0f',covers=['new_build','survey_build','refresh','sweep'];
  const at='2026-11-15T00:00:00.000Z',cutAt='2026-11-16T00:00:00.000Z',later='2026-11-16T00:00:01.000Z';
  const id=i=>'input-'+String(i).padStart(7,'0'),total=noise+2,observations=[];
  const path=(await db.get("SELECT substring(x FROM 13) value FROM pg_proc p,unnest(p.proconfig)x WHERE p.oid='inventory_inputs(text,text,text)'::regprocedure AND x LIKE 'search_path=%'")).value;
  const install=async sql=>{await db.run("SELECT set_config('search_path',?::text,false)",path);await db.exec(sql);};
  await db.exec(ddl('order_cut_indexes').replaceAll('INDEX CONCURRENTLY','INDEX'));
  await db.exec(`INSERT INTO entity_compute_specs(id,verb,request)
      SELECT 'input-'||lpad(i::text,7,'0'),'simulate','{"fixture":"incremental-inputs"}' FROM generate_series(1,${total})i;
    INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason)
      SELECT i,'input-'||lpad(i::text,7,'0'),1,'${at}','fixture' FROM generate_series(1,${total})i;
    INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position)
      SELECT computation,'simulate',id,'open','${at}','${at}','sweep',3,id FROM entity_compute_tasks;
    INSERT INTO entity_compute_batches(id,at,total) VALUES('before','${at}',${total}),('after','${later}',4);
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      SELECT 'sweep',id||'|r|base',id,'live','${at}','${at}' FROM state_compute_scheduling;
    SELECT setval(pg_get_serial_sequence('entity_compute_tasks','id'),${total});`);
  for(let from=1;from<=total;from+=128)await db.exec(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode)
    SELECT 'before','input-'||lpad(i::text,7,'0'),'r','base' FROM generate_series(${from},${Math.min(from+127,total)})i`);
  const special=[['suffix','r','base','suffix|r|base|more'],['missing-mode','r','','missing-mode|r'],
    ['missing-two','','','missing-two'],['empty-mode','r','','empty-mode|r|'],
    ['pipe-mode','r','a|b','pipe-mode|r|a|b'],['pipe|build','r','base','pipe|build|r|base'],
    ['汉_%[]','r','汉_%[]','汉_%[]|r|汉_%[]|suffix'],['withdrawn','r','base','withdrawn|r|base'],
    ['fresh-suffix','r','base','fresh-suffix|r|base|more']];
  for(const [build,ruler,mode,ref] of special) {
    await db.run('INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES(?,?,?,?)',build==='fresh-suffix'?'after':'before',build,ruler,mode);
    await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('refresh',?,?,?,?,?)",ref,id(1),build==='withdrawn'?'withdrawn':'live',at,at);
  }
  await db.exec("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('after','fresh','r','base'),('absent-batch','orphan','r','base')");
  const selected=[id(1),id(noise+1),id(total),...special.map(x=>x[0]),'fresh','orphan','missing-build'];
  for(const build of selected.slice(0,-1))await db.run('INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?)',build,at,'{"weapon":"torid","mods":[]}');
  await db.run(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
    VALUES(?,'r','base',?,12,'kpm',1,?,?)`,id(1),engine,at,at);
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'inputs',cut_at:cutAt,task_cutoff:total,keep_boundary:[3,noise,'z'],source:{fixture:'incremental-inputs'}}));
  await db.run("SELECT compute_order_cut_protect('inputs',(?::text)::jsonb)",JSON.stringify([id(total)]));
  await db.exec("SELECT compute_order_cut_capture('inputs')");
  const digest=createHash('sha256').update(id(total)).digest('hex');
  for(const predicate of ['FROM state_board_pending_work b WHERE b.build_id=k.id OFFSET 0','FROM state_compute_work q WHERE q.id=k.id OFFSET 0']) {
    const unchecked=current.replace(predicate,predicate.replace(/ WHERE [^]* OFFSET 0/,' OFFSET 0'));
    assert.notEqual(unchecked,current);await install(unchecked);
    await assert.rejects(db.run("SELECT compute_order_cut_activate('inputs',1,?::text)",digest),/complete order cut runtime upgrade required/);
  }
  await install(current);
  await db.run("SELECT compute_order_cut_activate('inputs',1,?::text)",digest);
  for(const state of ['fact','spot','withdrawn','disputed','open'])await seedComputeWork(db,{id:'lifecycle-'+state,state,opened_at:later,due_at:later,fact_engine:engine});
  await db.exec(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      VALUES('other','other','lifecycle-open','live','${later}','${later}');
    INSERT INTO record_refusal_withdrawals(question,engine,owners,at) VALUES('refused','${engine}',3,'${later}');
    ANALYZE state_compute_scheduling;ANALYZE entity_compute_tasks;ANALYZE relation_task_consumers;
    ANALYZE state_board_pending_work;ANALYZE entity_compute_batches;ANALYZE relation_compute_order_cut_protection;`);
  await db.run('SELECT inventory_begin(?::text,?::text,NULL::text)',engine,JSON.stringify({v:1,covers,engine_rules:'fixture'}));
  const keys=[...selected.map(key=>({scope:'build',key,revision:'1'})),
    ...[id(1),id(noise+1),id(total),'lifecycle-fact','lifecycle-spot','lifecycle-withdrawn','lifecycle-disputed','lifecycle-open','refused','missing-question'].map(key=>({scope:'question',key,revision:'1'}))];
  keys.push({...keys[0]});
  const routine=()=>db.get("SELECT oid::text,prorettype::text,proargtypes::text,provolatile,prosecdef,proconfig,proowner::text,proacl::text FROM pg_proc WHERE oid='inventory_inputs(text,text,text)'::regprocedure");
  const before=await routine();
  const read=async changes=>{
    await db.exec('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
    try {
      const {value}=await db.get('SELECT inventory_inputs(?::text,?::text,?::text) value',engine,JSON.stringify(covers),JSON.stringify(changes));
      await db.exec('COMMIT');
      const {snapshot,started_at,...stable}=value;assert(snapshot&&started_at);return stable;
    } catch(error){await db.exec('ROLLBACK');throw error;}
  };
  const budget=metrics=>{
    assert(metrics.blocks<=4096,`incremental input buffer reads ${metrics.blocks}`);
    assert.equal(metrics.temp,0,'incremental input spills temporary blocks');
  };
  const measured=async(mode,label)=>{
    await db.exec(`BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;SET LOCAL plan_cache_mode='${mode}';SET LOCAL jit=off;`);
    try {
      const result=await db.get('EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON,TIMING OFF) SELECT inventory_inputs(?::text,?::text,?::text)',engine,JSON.stringify(covers),JSON.stringify([{scope:'build',key:id(1),revision:'1'}]));
      const plan=result['QUERY PLAN'][0],root=plan.Plan;
      const metrics={label,mode,blocks:(root['Shared Hit Blocks']||0)+(root['Shared Read Blocks']||0),temp:(root['Temp Read Blocks']||0)+(root['Temp Written Blocks']||0),ms:plan['Execution Time']};
      observations.push(metrics);return metrics;
    } finally {await db.exec('ROLLBACK');}
  };
  await install(broad);
  const expected=await read(keys),empty=await read([]);
  assert.equal(expected.queue.filter(x=>x.build_id==='fresh-suffix').length,2,'canonical UNION ALL retains both fresh suffix rows');
  assert.equal(expected.queue.filter(x=>x.build_id===id(noise+1)).length,0,'cancelled prefix tail stays absent');
  assert.equal(expected.queue.filter(x=>x.build_id===id(total)).length,1,'manifest protection survives');
  for(const key of ['pipe|build','pipe-mode','withdrawn','orphan'])assert(!expected.queue.some(x=>x.build_id===key),key);
  for(const key of ['suffix','missing-mode','missing-two','empty-mode','汉_%[]','fresh'])assert(expected.queue.some(x=>x.build_id===key),key);
  for(const mode of ['force_generic_plan','force_custom_plan']) {
    const old=await measured(mode,'broad');
    assert.throws(()=>budget(old),/incremental input buffer reads/);
  }
  await install(current);
  assert.deepEqual(await routine(),before,'replacement preserves function identity, ownership, ACL and security configuration');
  assert.deepEqual(await read(keys),expected,'all canonical source values and pending duplicates stay exact');
  assert.deepEqual(await read([]),empty,'empty changes retain empty sources');
  for(const mode of ['force_generic_plan','force_custom_plan'])budget(await measured(mode,'keyed'));
  if(receiptFile)writeFileSync(receiptFile,JSON.stringify({noise,observations,equivalent:true,routine:before},null,2)+'\n');
  console.log('ok: keyed incremental inputs retain cut semantics, duplicates and function permissions with bounded reads');
  return {noise,observations,equivalent:true,routine:before};
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href) {
  const {testDb}=await import('./pg/test_db.mjs'),db=await testDb('library');
  try {await inventoryInputKeyCases(db,{receiptFile:process.env.WFSIM_INPUTS_KEY_RECEIPT||null});}
  finally {await db.close();}
}
