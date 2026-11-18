// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import {seedComputeWork} from './pg/fixtures.mjs';
import {orderCutSchemaSql,orderCutBusinessSql,taskSchemaSql} from './task_schema.mjs';
import {buildAdmissionSchemaSql} from './build_admission_schema.mjs';

const source=name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8');
const view=(sql,name)=>sql.match(new RegExp('CREATE OR REPLACE VIEW '+name+' AS[^]*?;'))?.[0];
const names=['state_compute_effective_open','state_compute_effective_work','state_board_effective_pending_work'];
const oldSource=readFileSync(new URL('../tests/fixtures/order-cut-effective-reads/501b4458.sql',import.meta.url),'utf8');
assert.equal(createHash('sha256').update(oldSource).digest('hex'),'3ef7b857666e12c7894305b4d4e7fbe3318ad8ccae08843fc3b755d26d9e99cc');
const cutFunction=source('schema').match(/CREATE OR REPLACE FUNCTION compute_order_cut\(\)[^]*?\$\$;/)[0];
const current=[cutFunction,...names.map(name=>view(source(name.startsWith('state_board')?'library':'schema'),name))].join('\n');

function measured(plan) {
  const root=plan.Plan,out={scheduling:0,consumers:0,sort_rows:0,temp_blocks:0,blocks:0};
  out.blocks=(root['Shared Hit Blocks']||0)+(root['Shared Read Blocks']||0);
  out.temp_blocks=(root['Temp Read Blocks']||0)+(root['Temp Written Blocks']||0);
  const walk=node=>{
    const read=((node['Actual Rows']||0)+(node['Rows Removed by Filter']||0))*(node['Actual Loops']||0);
    if(node['Relation Name']==='state_compute_scheduling')out.scheduling+=read;
    if(node['Relation Name']==='relation_task_consumers')out.consumers+=read;
    if(node['Node Type']==='Sort')out.sort_rows=Math.max(out.sort_rows,(node.Plans?.[0]?.['Actual Rows']||0)*(node.Plans?.[0]?.['Actual Loops']||0));
    for(const child of node.Plans||[])walk(child);
  };
  walk(root);return out;
}

function bounded(kind,m) {
  if(kind==='pending') {
    assert(m.consumers<=4096,`pending reads unrelated live consumers: ${m.consumers}`);
    assert(m.scheduling<=4096,`pending scheduling reads exceed its live keys: ${m.scheduling}`);
    assert(m.sort_rows<=4096,`pending sorts the physical tail instead of live keys: ${m.sort_rows}`);
  } else assert(m.scheduling<=4096,`${kind} reads the cancelled scheduling tail: ${m.scheduling}`);
  assert.equal(m.temp_blocks,0,`${kind} spills temporary blocks`);
  assert(m.blocks<=100000,`${kind} buffer use exceeds its indexed key work: ${m.blocks}`);
}

export async function orderCutReadCases(db,{n=100001,receiptFile=process.env.WFSIM_ORDER_CUT_READ_RECEIPT||null}={}) {
  assert(Number.isSafeInteger(n)&&n>=100001);
  const at='2026-11-15T00:00:00.000Z',now='2026-11-16T00:00:00.000Z';
  const schemaPath=(await db.get('SELECT current_schema() value')).value;
  const install=async text=>{await db.run("SELECT set_config('search_path',?::text,false)",schemaPath+',pg_catalog,pg_temp');await db.exec(text);};
  await db.exec(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
  await db.exec(`
    INSERT INTO entity_compute_specs(id,verb,request)
      SELECT 'tail-'||lpad(i::text,7,'0'),'simulate','{"fixture":"old-tail"}' FROM generate_series(1,${n}) i
      UNION ALL SELECT 'prefix-'||lpad(i::text,3,'0'),'simulate','{"fixture":"kept-prefix"}' FROM generate_series(1,64) i;
    INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason)
      SELECT i+1000,'tail-'||lpad(i::text,7,'0'),1,'${at}','fixture' FROM generate_series(1,${n}) i
      UNION ALL SELECT i,'prefix-'||lpad(i::text,3,'0'),1,'${at}','fixture' FROM generate_series(1,64) i;
    INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position,waits)
      SELECT computation,'simulate',id,'open','${at}','${at}','sweep',3,id,CASE WHEN id IN(1,2) THEN 1 ELSE 0 END FROM entity_compute_tasks;
    SELECT setval(pg_get_serial_sequence('entity_compute_tasks','id'),(SELECT max(id) FROM entity_compute_tasks));
    INSERT INTO entity_compute_batches(id,at,total) VALUES('old-owner','${at}',${n}),('other-owner','${at}',1),('new-owner','2026-11-16T00:00:01.000Z',1);
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      SELECT 'sweep',id||'|r|base',id,'live','${at}','${at}' FROM state_compute_scheduling
      UNION ALL SELECT 'sweep','unrelated-consumer-'||i,'absent-'||i,'live','${at}','${at}' FROM generate_series(1,${n}) i;
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      SELECT 'new_build',id||'|r|base',id,'live','${at}','${at}' FROM state_compute_scheduling WHERE id LIKE 'prefix-%';
    UPDATE state_compute_scheduling SET lease='held',lease_until='2999',leased_to='fixture',leased_at='${at}' WHERE id='prefix-003';
  `);
  await db.exec("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator) SELECT 'old-owner',id,'r','base','kept owner' FROM state_compute_scheduling WHERE id LIKE 'prefix-%'");
  for(let from=1;from<=n;from+=128) {
    const to=Math.min(from+127,n);
    await db.exec(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator)
      SELECT 'old-owner','tail-'||lpad(i::text,7,'0'),'r','base','kept owner' FROM generate_series(${from},${to}) i
      UNION ALL SELECT 'old-owner','unrelated-pending-'||i,'r','base',NULL FROM generate_series(${from},${to}) i;`);
  }
  for(const state of ['fact','spot','withdrawn','dispute']) {
    await seedComputeWork(db,{id:'nonopen-'+state,state,request:JSON.stringify({fixture:state}),opened_at:at,due_at:at,canon:'{"value":7}',fact_engine:'fixture',fact_at:at});
    await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator) VALUES('old-owner',?,'r','base','nonopen')",'nonopen-'+state);
    await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,delivery_at) VALUES('sweep',?,?,'live',?,?,?)",'nonopen-'+state+'|r|base','nonopen-'+state,at,at,now);
  }
  const refCases=[
    ['suffix','r','base','suffix|r|base|more'],
    ['missing-mode','r','','missing-mode|r'],
    ['missing-two','','','missing-two'],
    ['empty-mode','r','','empty-mode|r|'],
    ['pipe-mode','r','a|b','pipe-mode|r|a|b'],
    ['pipe|build','r','base','pipe|build|r|base'],
    ['unicode_%[]','r','汉_%[]','unicode_%[]|r|汉_%[]|suffix'],
  ];
  for(const [build,ruler,mode,ref] of refCases) {
    await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('other-owner',?,?,?)",build,ruler,mode);
    await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('refresh',?,'prefix-004','live',?,?)",ref,at,at);
  }
  await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode,initiator) VALUES('other-owner','prefix-004','r','base','wrong owner')");
  assert.equal((await db.get("SELECT batch FROM state_board_pending_work WHERE build_id='prefix-004' AND ruler='r' AND mode='base'")).batch,'old-owner');
  await db.exec("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('old-owner','old-unregistered','r','base'),('new-owner','new-unregistered','r','base')");
  const protectedIds=(await db.all("SELECT id FROM state_compute_scheduling WHERE id LIKE 'tail-%' ORDER BY id LIMIT 128")).map(row=>row.id);
  await db.run("UPDATE state_compute_scheduling SET lease='held-prefix',lease_until='2999',leased_to='fixture',leased_at=? WHERE id LIKE 'prefix-%' OR id IN(SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))",at,JSON.stringify(protectedIds));
  const cutoff=(await db.get('SELECT max(id) n FROM entity_compute_tasks')).n;
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'read-cut',cut_at:now,task_cutoff:cutoff,keep_boundary:[3,64,'z'],source:{fixture:'effective-reads'}}));
  await db.run('SELECT compute_order_cut_protect(?::text,(?::text)::jsonb)','read-cut',JSON.stringify(protectedIds));
  await db.run("SELECT compute_order_cut_capture('read-cut')");
  await db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','read-cut',protectedIds.length,createHash('sha256').update(protectedIds.join('\n')).digest('hex'));
  await db.exec(`INSERT INTO entity_compute_specs(id,verb,request) VALUES('new-round','simulate','{"fixture":"after-cut"}');
    INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason) VALUES(${cutoff+1},'new-round',1,'2026-11-16T00:00:01.000Z','fixture');
    INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,zone,zone_position,lease,lease_until,leased_to,leased_at)
      VALUES('new-round','simulate',${cutoff+1},'open','${at}','${at}',3,${cutoff+1},'new-held','2999','fixture','${at}');
    INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('new-owner','new-round','r','base');
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      VALUES('sweep','new-round|r|base','new-round','live','2026-11-16T00:00:01.000Z','${at}');`);
  await db.exec('ANALYZE state_compute_scheduling; ANALYZE entity_compute_tasks; ANALYZE relation_task_consumers; ANALYZE state_board_pending_work; ANALYZE entity_compute_batches; ANALYZE relation_compute_order_cut_protection; SET work_mem=\'1MB\';');
  await install(oldSource);
  await db.exec('CREATE TEMP TABLE expected_open AS SELECT * FROM state_compute_effective_open; CREATE TEMP TABLE expected_work AS SELECT * FROM state_compute_effective_work; CREATE TEMP TABLE expected_pending AS SELECT * FROM state_board_effective_pending_work;');
  assert.equal((await db.get("SELECT count(*) n FROM expected_pending WHERE build_id LIKE 'nonopen-%'")).n,4,'fact/spot/withdrawn/dispute pending deliveries must survive');
  assert.equal((await db.get("SELECT count(*) n FROM expected_pending WHERE build_id IN('suffix','missing-mode','missing-two','empty-mode','unicode_%[]')")).n,5);
  assert.equal((await db.get("SELECT count(*) n FROM expected_pending WHERE build_id IN('pipe-mode','pipe|build','old-unregistered')")).n,0);
  assert.equal((await db.get("SELECT count(*) n FROM expected_pending WHERE build_id='new-unregistered'")).n,1);
  const definitions=await db.all("SELECT viewname,definition FROM pg_views WHERE schemaname=current_schema() AND viewname='state_compute_work'");
  const cutDefinition=await db.get("SELECT prosrc,prosecdef,provolatile,proconfig FROM pg_proc WHERE oid='compute_order_cut()'::regprocedure");
  const storageDefinition=()=>db.all("SELECT relname,relfilenode::text storage,relowner::text owner,relacl::text acl FROM pg_class WHERE oid IN('state_compute_scheduling'::regclass,'entity_compute_tasks'::regclass,'entity_compute_results'::regclass,'state_board_pending_work'::regclass,'relation_compute_order_cut_protection'::regclass) ORDER BY relname");
  const heaps=await storageDefinition();
  const guardDefinitions=()=>db.all("SELECT proname,prosrc,prosecdef,provolatile,proconfig,proowner::text owner FROM pg_proc WHERE pronamespace=current_schema()::regnamespace AND proname IN('compute_order_cancelled','compute_order_write_guard','compute_order_existing_evidence') ORDER BY proname");
  const guards=await guardDefinitions();
  const columns=()=>db.all("SELECT c.relname,a.attnum,a.attname,a.atttypid,a.atttypmod,a.attcollation FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid WHERE c.relnamespace=current_schema()::regnamespace AND c.relname IN('state_compute_effective_open','state_compute_effective_work','state_board_effective_pending_work') AND a.attnum>0 AND NOT a.attisdropped ORDER BY c.relname,a.attnum");
  const shape=await columns();
  const storage=()=>db.get("SELECT (SELECT count(*) FROM state_compute_scheduling) scheduling,(SELECT count(*) FROM entity_compute_tasks) tasks,(SELECT count(*) FROM record_candidate_answers) answers,(SELECT count(*) FROM entity_compute_results) results,(SELECT count(*) FROM relation_compute_order_cut_protection) protections,(SELECT count(*) FROM record_compute_order_cut_activations) activations");
  const before=await storage();
  const assertEqual=async()=>{
    for(const [actual,expected] of names.map((name,i)=>[name,['expected_open','expected_work','expected_pending'][i]])) {
      assert.equal((await db.get(`SELECT count(*) n FROM ((SELECT * FROM ${actual} EXCEPT ALL SELECT * FROM ${expected}) UNION ALL (SELECT * FROM ${expected} EXCEPT ALL SELECT * FROM ${actual})) differences`)).n,0,actual+' changed complete rows');
    }
    assert.deepEqual(await storage(),before);
    assert.deepEqual(await db.all("SELECT viewname,definition FROM pg_views WHERE schemaname=current_schema() AND viewname='state_compute_work'"),definitions);
    assert.deepEqual(await db.get("SELECT prosrc,prosecdef,provolatile,proconfig FROM pg_proc WHERE oid='compute_order_cut()'::regprocedure"),cutDefinition);
    assert.equal((await db.get("SELECT prorows FROM pg_proc WHERE oid='compute_order_cut()'::regprocedure")).prorows,1);
    assert.deepEqual(await storageDefinition(),heaps,'upgrade rewrites physical heaps, ownership or grants');
    assert.deepEqual(await guardDefinitions(),guards,'upgrade changes a cancellation or credential guard');
    assert.deepEqual(await columns(),shape,'effective view output schema changed');
  };
  const queries={
    open:"SELECT count(*) n FROM (SELECT 1 FROM state_compute_effective_work WHERE state='open' AND waits=0 AND (lease_until IS NULL OR lease_until<'2026-11-16T00:00:00.000Z') LIMIT 7) limited",
    urgent:"SELECT count(*) n FROM state_compute_effective_work WHERE state='open' AND waits>0",
    pending:'SELECT count(*) n FROM state_board_effective_pending_work',
  };
  const observations=[];
  const runPlans=async(label,negative)=>{
    const failures=[];
    for(const [kind,query] of Object.entries(queries)) {
      await db.exec(`PREPARE read_${kind} AS ${query}`);
      for(const mode of ['force_generic_plan','force_custom_plan']) {
        await db.exec(`SET plan_cache_mode=${mode}`);
        let plan;
        try {plan=(await db.get(`EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON,TIMING OFF) EXECUTE read_${kind}`))['QUERY PLAN'][0];}
        catch(error) {
          if(!negative||error.code!=='57014'||!/statement timeout/.test(error.message))throw error;
          observations.push({label,kind,mode,timeout:true,code:error.code,message:error.message});
          if(receiptFile)writeFileSync(receiptFile,JSON.stringify({n,before,observations,complete:false},null,2)+'\n');
          console.log(JSON.stringify({label,kind,mode,timeout:true,code:error.code}));continue;
        }
        const metrics=measured(plan),entry={label,kind,mode,metrics,plan};observations.push(entry);
        if(receiptFile)writeFileSync(receiptFile,JSON.stringify({n,before,observations,complete:false},null,2)+'\n');
        console.log(JSON.stringify({label,kind,mode,metrics}));
        if(negative)assert.throws(()=>bounded(kind,metrics),/tail|unrelated|spills|buffer|sorts|live keys/,'frozen old '+kind+' must fail on actual executed input');
        else {try{bounded(kind,metrics);}catch(error){failures.push(kind+'/'+mode+': '+error.message);}}
      }
      await db.exec(`DEALLOCATE read_${kind}`);
    }
    assert.deepEqual(failures,[],label+' actual read bounds failed');
  };
  await runPlans('old',true);
  await install(current);await assertEqual();
  await runPlans('new',false);
  for(const exported of [taskSchemaSql(),buildAdmissionSchemaSql({role:null})]) {
    assert(exported.includes(cutFunction),'installer must export the singleton row estimate without changing the cut function');
    for(const name of names)assert.equal(view(exported,name),view(current,name),'installer must export the authoritative '+name);
  }
  for(let pass=0;pass<2;pass++) {
    await install(orderCutSchemaSql());await install(orderCutBusinessSql());await assertEqual();
  }
  await db.exec('RESET plan_cache_mode; RESET work_mem');
  if(receiptFile)writeFileSync(receiptFile,JSON.stringify({n,before,observations,complete_rows_equal:true,nonempty_upgrades:2},null,2)+'\n');
  console.log('ok effective reads preserve full fact/delivery/pending ownership and source exports; old read bounds fail, indexed reads pass after two nonempty upgrades');
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url) {
  const url=process.env.WFSIM_ORDER_CUT_TEST_URL;
  if(url) {
    assert(['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname));
    const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
    const {pgDb,postgresClient,placeholders}=await import('../worker/db.js');
    const schema='cut_reads_'+Date.now(),sql=postgres(url,{max:1,fetch_types:false,onnotice(){},connection:{search_path:schema},types:{bigint:{to:20,from:[20,1700],parse:Number,serialize:String}}});
    const client=postgresClient(sql),db=Object.assign(pgDb(client),{exec:text=>sql.unsafe(text),run:async(text,...args)=>(await client.query(placeholders(text),args)).affected,
      get:async(text,...args)=>(await client.query(placeholders(text),args)).rows[0]??null,all:async(text,...args)=>(await client.query(placeholders(text),args)).rows});
    try {await sql.unsafe(`CREATE SCHEMA ${schema}`);await db.exec(['schema','library','sync','inventory','build_workflows','board_admission'].map(source).join('\n'));await orderCutReadCases(db,{receiptFile:process.env.WFSIM_ORDER_CUT_READ_RECEIPT});}
    finally {await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`);await sql.end();}
  } else {
    const {testDb}=await import('./pg/test_db.mjs');const db=await testDb('library');
    try {await orderCutReadCases(db,{receiptFile:process.env.WFSIM_ORDER_CUT_READ_RECEIPT});}finally{await db.close();}
  }
}
