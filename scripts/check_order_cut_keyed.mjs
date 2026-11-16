// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {BOARD_PRODUCERS} from '../worker/board_tasks.js';
import {seedComputeWork} from './pg/fixtures.mjs';
import {taskSchemaSql,orderCutSchemaSql,orderCutBusinessSql} from './task_schema.mjs';
import {buildAdmissionSchemaSql} from './build_admission_schema.mjs';

const source=name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8');
const view=(sql,name)=>sql.match(new RegExp('CREATE OR REPLACE VIEW '+name+' AS[^]*?;'))[0];
const owed=sql=>sql.match(/CREATE OR REPLACE FUNCTION board_owed\([^]*?\$\$;/)[0];
const frozen=readFileSync(new URL('../tests/fixtures/order-cut-effective-reads/ef61dd588.sql',import.meta.url),'utf8');
assert.equal(createHash('sha256').update(frozen).digest('hex'),'ab6e9c10a016ebfeba4970c890beb78f811aa92fc5be491964e91cab07ac00cf');
const names=['state_compute_effective_open','state_compute_effective_work','state_board_effective_pending_work'];
const current=names.map(name=>view(source(name.startsWith('state_board')?'library':'schema'),name)).join('\n')+'\n'+owed(source('sync'));
const literal=value=>value===null?'NULL':"'"+String(value).replaceAll("'","''")+"'";

function measured(plan) {
  const root=plan.Plan,m={scheduling:0,consumers:0,sort_rows:0,temp_blocks:(root['Temp Read Blocks']||0)+(root['Temp Written Blocks']||0),
    blocks:(root['Shared Hit Blocks']||0)+(root['Shared Read Blocks']||0),indexes:[],consumer_scans:[]};
  const walk=n=>{
    const count=((n['Actual Rows']||0)+(n['Rows Removed by Filter']||0))*(n['Actual Loops']||0);
    if(n['Relation Name']==='state_compute_scheduling')m.scheduling+=count;
    if(n['Relation Name']==='relation_task_consumers') {
      m.consumers+=count;
      if(n['Actual Loops'])m.consumer_scans.push(n['Node Type']);
    }
    if(n['Index Name']&&n['Actual Loops'])m.indexes.push(n['Index Name']);
    if(n['Node Type']==='Sort')m.sort_rows=Math.max(m.sort_rows,(n.Plans?.[0]?.['Actual Rows']||0)*(n.Plans?.[0]?.['Actual Loops']||0));
    for(const c of n.Plans||[])walk(c);
  };walk(root);return m;
}
function bounded(m,n) {
  assert(m.scheduling<=64*n,`key scheduling reads ${m.scheduling}`);
  assert(m.consumers<=64*n,`key consumer reads ${m.consumers}`);
  assert(m.blocks<=4096*n,`key buffer reads ${m.blocks}`);
  assert(m.sort_rows<=64*n,`key sort rows ${m.sort_rows}`);
  assert.equal(m.temp_blocks,0,'key spills temporary blocks');
  assert(!m.consumer_scans.includes('Seq Scan'),'key scans unrelated consumers');
  assert(!m.indexes.includes('demands_live_due'),'key combines the whole live-due index');
}

export async function orderCutKeyCases(db,{prefix=512,cancelled=100001,protectedCount=1024,idPrefix='key-',seed=true,
  receiptFile=process.env.WFSIM_ORDER_CUT_KEY_RECEIPT||null}={}) {
  assert(prefix>=512&&cancelled>=100001&&protectedCount>=1024);
  const at='2026-11-15T00:00:00.000Z',cutAt='2026-11-16T00:00:00.000Z',later='2026-11-16T00:00:01.000Z';
  const total=prefix+cancelled+protectedCount,id=i=>idPrefix+String(i).padStart(7,'0');
  const schema=(await db.get('SELECT current_schema() value')).value;
  const install=async text=>{await db.run("SELECT set_config('search_path',?::text,false)",schema);await db.exec(text);};
  if(seed) {
    await db.exec(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
    await db.exec(`INSERT INTO entity_compute_specs(id,verb,request)
      SELECT '${idPrefix}'||lpad(i::text,7,'0'),'simulate','{"fixture":"keyed-cut"}' FROM generate_series(1,${total})i;
      INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason)
        SELECT i,'${idPrefix}'||lpad(i::text,7,'0'),1,'${at}','fixture' FROM generate_series(1,${total})i;
      INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position)
        SELECT computation,'simulate',id,'open','${at}','${at}','sweep',3,id FROM entity_compute_tasks;
      INSERT INTO entity_compute_batches(id,at,total) VALUES('before','${at}',${total}),('after','${later}',1);
      INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
        SELECT 'sweep',id||'|r|base',id,'live','${at}','${at}' FROM state_compute_scheduling
        UNION ALL SELECT 'sweep','unrelated-ref-'||i,'unrelated-question-'||i,'live','${at}','${at}' FROM generate_series(1,100001)i;`);
    for(const [count,build] of [[total,"'"+idPrefix+"'||lpad(i::text,7,'0')"],[100001,"'unrelated-pending-'||i"]])
      for(let from=1;from<=count;from+=128)await db.exec(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode)
        SELECT 'before',${build},'r','base' FROM generate_series(${from},${Math.min(from+127,count)})i`);
    const protectedIds=Array.from({length:protectedCount},(_,i)=>id(prefix+cancelled+i+1));
    await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'keyed',cut_at:cutAt,task_cutoff:total,keep_boundary:[3,prefix,'z'],source:{fixture:'keyed'}}));
    await db.run('SELECT compute_order_cut_protect(?::text,(?::text)::jsonb)','keyed',JSON.stringify(protectedIds));
    await db.run("SELECT compute_order_cut_capture('keyed')");
    await db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','keyed',protectedCount,createHash('sha256').update(protectedIds.join('\n')).digest('hex'));
  }
  await db.exec("SELECT setval(pg_get_serial_sequence('entity_compute_tasks','id'),(SELECT max(id) FROM entity_compute_tasks));");
  for(const state of ['fact','spot','withdrawn','dispute']) {
    await seedComputeWork(db,{id:'keyed-'+state,state,request:{fixture:state},opened_at:later,due_at:later,canon:'{"value":7}',fact_engine:'fixture',fact_at:later});
    await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('before',?,'r','base')",'keyed-'+state);
    await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,delivery_at) VALUES('sweep',?,?,'live',?,?,?)",'keyed-'+state+'|r|base','keyed-'+state,later,later,later);
  }
  await seedComputeWork(db,{id:'keyed-new-round',state:'open',request:{fixture:'new-round'},opened_at:later,due_at:later});
  await db.exec(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('after','keyed-new-round','r','base'),('after','keyed-post|pipe','r','base');
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      VALUES('sweep','keyed-new-round|r|base','keyed-new-round','live','${later}','${later}'),
        ('refresh','keyed-post|pipe|r|base','${id(prefix)}','live','${later}','${later}');`);
  const special=[['keyed-suffix','r','base','keyed-suffix|r|base|more'],['keyed-missing-mode','r','','keyed-missing-mode|r'],
    ['keyed-missing-two','','','keyed-missing-two'],['keyed-empty','r','','keyed-empty|r|'],
    ['keyed-pipe-mode','r','a|b','keyed-pipe-mode|r|a|b'],['keyed|pipe-build','r','base','keyed|pipe-build|r|base'],
    ['keyed-汉_%[]','r','汉_%[]','keyed-汉_%[]|r|汉_%[]|suffix'],['keyed-state','r','base','keyed-state|r|base']];
  for(const [build,ruler,mode,ref] of special) {
    await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('before',?,?,?)",build,ruler,mode);
    await db.run("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('refresh',?,?,?, ?,?)",ref,id(prefix),build==='keyed-state'?'withdrawn':'live',at,at);
  }
  await db.exec(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('after','keyed-unregistered','r','base');
    INSERT INTO record_compute_order_cancellations(question,task_id,cut_id,at)
      SELECT s.id,s.task_id,c.id,'${later}' FROM state_compute_scheduling s CROSS JOIN compute_order_cut()c WHERE s.id='${id(total-1)}';
    ANALYZE state_compute_scheduling;ANALYZE entity_compute_tasks;ANALYZE relation_task_consumers;ANALYZE state_board_pending_work;
    ANALYZE entity_compute_batches;ANALYZE relation_compute_order_cut_protection;`);
  await install(frozen);
  await db.exec('CREATE TEMP TABLE keyed_expected_open AS SELECT * FROM state_compute_effective_open; CREATE TEMP TABLE keyed_expected_work AS SELECT * FROM state_compute_effective_work; CREATE TEMP TABLE keyed_expected_pending AS SELECT * FROM state_board_effective_pending_work;');
  const shape=()=>db.all("SELECT c.relname,a.attnum,a.attname,a.atttypid,a.atttypmod,a.attcollation FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid WHERE c.relnamespace=current_schema()::regnamespace AND c.relname IN('state_compute_effective_open','state_compute_effective_work','state_board_effective_pending_work') AND a.attnum>0 AND NOT a.attisdropped ORDER BY c.relname,a.attnum");
  const routine=()=>db.get("SELECT prorettype,proargtypes::text args,provolatile,prosecdef,proconfig,proowner::text owner,proacl::text acl FROM pg_proc WHERE oid='board_owed(text,text,text)'::regprocedure");
  const beforeShape=await shape(),beforeRoutine=await routine();
  const columns=()=>db.all("SELECT c.relname,c.relowner::text owner,c.relacl::text acl,a.attname,a.attacl::text column_acl FROM pg_class c JOIN pg_attribute a ON a.attrelid=c.oid WHERE c.oid IN('entity_compute_batches'::regclass,'state_compute_scheduling'::regclass,'state_board_pending_work'::regclass,'relation_task_consumers'::regclass) AND a.attnum>0 ORDER BY c.relname,a.attnum");
  const guards=()=>db.all("SELECT proname,prosrc,prosecdef,proconfig FROM pg_proc WHERE pronamespace=current_schema()::regnamespace AND proname IN('compute_order_cancelled','compute_order_write_guard','compute_order_existing_evidence') ORDER BY proname");
  const beforeGuards=await guards(),observations=[];
  const save=()=>{if(receiptFile)writeFileSync(receiptFile,JSON.stringify({schema,prefix,cancelled,protectedCount,observations},null,2)+'\n');};
  const keys=[[id(prefix),'r','base'],[id(total),'r','base'],['not-present','r','base'],[id(prefix+1),'r','base'],['keyed-unregistered','r','base'],['keyed-new-round','r','base']];
  const ds=keys.slice(0,3).map(([identity,ruler,mode])=>({payload:{identity,ruler,mode},ref:[identity,ruler,mode].join('|')}));
  const expected=async args=>(await db.get('SELECT EXISTS(SELECT 1 FROM keyed_expected_pending WHERE (build_id,ruler,mode)=(?::text,?::text,?::text)) wanted',...args)).wanted;
  assert.equal(await expected([id(total-1),'r','base']),false,'cancellation ledger overrides protected membership');
  for(const state of ['fact','spot','withdrawn','dispute'])assert.equal(await expected(['keyed-'+state,'r','base']),true,'pending delivery of '+state+' remains owed');
  for(const [i,args] of keys.entries())assert.equal(await expected(args),[true,true,false,false,true,true][i]);
  assert.equal(await expected(['keyed-post|pipe','r','base']),false,'post-cut registration keeps the exact raw ref exclusion');
  const run=async(label,negative)=>{
    let manySql;
    const env={LIBRARY:{prepare:text=>{manySql=text;return db.prepare(text);}}};
    assert.deepEqual([...(await BOARD_PRODUCERS.sweep.wanted_many(env,ds))].sort(),ds.filter((_,i)=>i<2).map(d=>d.ref).sort());
    await db.exec('PREPARE keyed_one(text,text,text) AS SELECT board_owed($1,$2,$3) wanted; PREPARE keyed_body(text,text,text) AS '+owed(negative?frozen:source('sync')).split('$$')[1]+'; PREPARE keyed_many(text) AS '+manySql.replace('?::text','$1::text'));
    for(const mode of ['force_generic_plan','force_custom_plan']) {
      await db.exec('SET plan_cache_mode='+mode);
      const calls=keys.flatMap((args,i)=>[{kind:'one-'+i,prepared:'keyed_one',args,count:1},{kind:'body-'+i,prepared:'keyed_body',args,count:1}]);
      calls.push({kind:'wanted_many',prepared:'keyed_many',args:[JSON.stringify(ds.map(d=>[d.payload.identity,d.payload.ruler,d.payload.mode,d.ref]))],count:ds.length});
      for(const {kind,prepared,args,count} of calls) {
        const execute=`EXECUTE ${prepared}(${args.map(literal).join(',')})`;
        if(prepared==='keyed_one')assert.equal((await db.get(execute)).wanted,await expected(args));
        if(prepared==='keyed_many')assert.deepEqual((await db.all(execute)).map(r=>r.ref).sort(),ds.slice(0,2).map(d=>d.ref).sort());
        const plan=(await db.get('EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON,TIMING OFF) '+execute))['QUERY PLAN'][0],metrics=measured(plan);
        if(negative)assert.throws(()=>bounded(metrics,count),/key/,'frozen ef61 keyed path must fail its actual read budget');else bounded(metrics,count);
        observations.push({label,mode,kind,metrics,plan});save();console.log(JSON.stringify({label,mode,kind,metrics}));
      }
    }
    await db.exec('DEALLOCATE keyed_one;DEALLOCATE keyed_body;DEALLOCATE keyed_many;RESET plan_cache_mode');
  };
  const timeout=(await db.get('SHOW statement_timeout')).statement_timeout;
  await db.exec("SET statement_timeout='5s'");
  try {await run('old',true);await install(current);await run('new',false);}finally{await db.run("SELECT set_config('statement_timeout',?::text,false)",timeout);}
  const semanticKeys=[...keys,[id(total-1),'r','base'],['keyed-post|pipe','r','base'],...special.map(x=>x.slice(0,3)),...['fact','spot','withdrawn','dispute'].map(s=>['keyed-'+s,'r','base']),[null,'r','base']];
  const equal=async()=>{
    assert.deepEqual(await shape(),beforeShape);assert.deepEqual(await routine(),beforeRoutine);assert.deepEqual(await guards(),beforeGuards);
    for(const name of names)assert.equal((await db.get(`SELECT count(*) n FROM ((SELECT * FROM ${name} EXCEPT ALL SELECT * FROM keyed_expected_${name.endsWith('open')?'open':name.startsWith('state_board')?'pending':'work'}) UNION ALL (SELECT * FROM keyed_expected_${name.endsWith('open')?'open':name.startsWith('state_board')?'pending':'work'} EXCEPT ALL SELECT * FROM ${name}))d`)).n,0,name+' changes full canonical rows');
    for(const args of semanticKeys)assert.equal((await db.get('SELECT board_owed(?::text,?::text,?::text) wanted',...args)).wanted,await expected(args),JSON.stringify(args));
  };
  await equal();
  const reader='keyed_reader_'+schema;
  const grant=readFileSync(new URL('../deploy/factoryd_grants.sql',import.meta.url),'utf8').match(/GRANT SELECT \(id,at\) ON entity_compute_batches TO wfsim_factoryd;/)?.[0];
  assert(grant,'deployment must grant precisely the id and cut timestamp columns');
  await db.exec(`CREATE ROLE ${reader};GRANT USAGE ON SCHEMA ${schema} TO ${reader};
    GRANT SELECT ON state_board_pending_work,relation_task_consumers,state_compute_effective_work,state_compute_work TO ${reader};
    GRANT SELECT(id) ON entity_compute_batches TO ${reader};`);
  try {
    await db.exec('SET ROLE '+reader);
    await assert.rejects(db.get("SELECT board_owed('keyed-unregistered','r','base')"),error=>error.code==='42501');
    await db.exec('RESET ROLE');await db.exec(grant.replaceAll('wfsim_factoryd',reader));await db.exec('SET ROLE '+reader);
    assert.equal((await db.get("SELECT board_owed('keyed-unregistered','r','base') wanted")).wanted,true);
    await db.exec('RESET ROLE');
    const permissions=await db.get(`SELECT has_column_privilege('${reader}','entity_compute_batches','at','SELECT') at,
      has_column_privilege('${reader}','entity_compute_batches','why','SELECT') why,
      has_column_privilege('${reader}','entity_compute_batches','total','SELECT') total,
      has_table_privilege('${reader}','entity_compute_batches','INSERT,UPDATE,DELETE') writes`);
    assert.deepEqual(permissions,{at:true,why:false,total:false,writes:false});
  } finally {await db.exec(`RESET ROLE;DROP OWNED BY ${reader};DROP ROLE ${reader}`);}
  const grants=await columns();
  for(const exported of [taskSchemaSql(),buildAdmissionSchemaSql({role:null})]) {
    assert(exported.includes(owed(source('sync'))),'installer must export authoritative board_owed');
    assert(exported.includes('GRANT SELECT(at) ON entity_compute_batches TO wfsim_factoryd;'),'installer must grant the exact timestamp read');
    for(const name of names)assert(exported.includes(view(current,name)),'installer must export authoritative '+name);
  }
  for(let pass=0;pass<2;pass++) {
    await install(taskSchemaSql().replace('SET ROLE wfsim;','').replaceAll('INDEX CONCURRENTLY','INDEX'));
    await install(buildAdmissionSchemaSql({role:null}));await equal();assert.deepEqual(await columns(),grants);
  }
  assert(orderCutSchemaSql().includes(view(current,'state_compute_effective_open')));
  assert(orderCutBusinessSql().includes(owed(source('sync'))));
  save();console.log('ok keyed board_owed and real wanted_many fail on frozen ef61, use bounded indexed keys, retain canonical fact/delivery/ref/ledger semantics and types/grants after two nonempty upgrades');
  return observations;
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url) {
  const {testDb}=await import('./pg/test_db.mjs');const db=await testDb('library');
  try {await orderCutKeyCases(db);}finally{await db.close();}
}
