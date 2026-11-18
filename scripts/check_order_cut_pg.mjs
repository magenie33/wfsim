// SPDX-License-Identifier: AGPL-3.0-or-later
// PostgreSQL row locks and snapshots must preserve the cut across an in-flight lease.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {randomBytes,createHash} from 'node:crypto';
import {pgDb,postgresClient,placeholders} from '../worker/db.js';
import {orderCutCases} from './check_order_cut.mjs';
import {orderCutReadCases} from './check_order_cut_reads.mjs';
import {factory,canon} from '../worker/tasks.js';

const url=process.env.WFSIM_ORDER_CUT_TEST_URL;
if(!url)throw Error('set WFSIM_ORDER_CUT_TEST_URL to an isolated local PostgreSQL');
assert(['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname));
const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
const schema='order_cut_'+randomBytes(6).toString('hex');
const options={max:5,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:15000},
  types:{bigint:{to:20,from:[20,1700],parse:Number,serialize:String}}};
const sql=postgres(url,options),ddl=['schema','library','sync','inventory','build_workflows','board_admission']
  .map(name=>readFileSync(new URL(`../worker/pg/${name}.sql`,import.meta.url),'utf8')).join('\n');
const client=postgresClient(sql),db=Object.assign(pgDb(client),{
  exec:text=>sql.unsafe(text),run:async(text,...args)=>(await client.query(placeholders(text),args)).affected,
  get:async(text,...args)=>(await client.query(placeholders(text),args)).rows[0]??null,
  all:async(text,...args)=>(await client.query(placeholders(text),args)).rows,
});
const digest=ids=>createHash('sha256').update([...ids].sort().join('\n')).digest('hex');
const statement=sql=>readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8').match(new RegExp('CREATE OR REPLACE FUNCTION '+sql+'\\([^]*?END \\$\\$;'))[0];
async function reset(){await sql.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE; CREATE SCHEMA ${schema}`);await sql.unsafe(ddl);await sql.unsafe(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));}
async function fixture(n=3) {
  const f=factory({verbs:{simulate:{cap:2,pack:true,lease_ms:60000,canon:(r,w)=>canon({...r,w})}},producers:{sweep:{}},spot:0,recheck:0});
  const now=Date.now(),ids=await f.demandAll({LIBRARY:db},Array.from({length:n},(_,i)=>({producer:'sweep',ref:'race-'+i,trigger_id:'race-'+i,verb:'simulate',zone:3,request:{value:i},due_in_ms:-1})), 'e',now-86400000);
  const cutoff=(await db.get('SELECT max(id) n FROM entity_compute_tasks')).n;
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'race',cut_at:new Date(now-1000).toISOString(),task_cutoff:cutoff,keep_boundary:[1,0,''],source:{fixture:true}}));
  return {ids,now,f};
}
async function waitLock(pid) {
  for(let i=0;i<100;i++) {
    if((await sql.unsafe("SELECT wait_event FROM pg_stat_activity WHERE pid=$1",[pid]))[0]?.wait_event==='advisory')return;
    await new Promise(r=>setTimeout(r,10));
  }
  assert.fail('contending transaction did not wait on the actual activation barrier');
}
try {
  await reset();await orderCutCases(db);
  const effectiveBefore=(await db.get('SELECT count(*) n FROM state_compute_effective_open')).n;
  await sql.unsafe(ddl);await sql.unsafe(ddl);
  assert.equal((await db.get('SELECT count(*) n FROM state_compute_effective_open')).n,effectiveBefore);
  console.log('ok actual PostgreSQL retains activated cut and protected history across two schema reloads');
  await reset();await orderCutReadCases(db);

  await reset();await fixture();
  await db.run('SELECT compute_order_cut_capture(?::text)','race');
  await db.exec('DROP INDEX compute_order_execution_evidence');
  await assert.rejects(db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','race',0,digest([])),/online indexes must be installed and valid/);
  await db.exec('CREATE INDEX compute_order_execution_evidence ON state_compute_scheduling(id) WHERE lease IS NOT NULL');
  await assert.rejects(db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','race',0,digest([])),/online indexes must be installed and valid/);
  assert.equal((await db.get('SELECT count(*) n FROM record_compute_order_cut_activations')).n,0);
  console.log('ok missing online index and same-name incomplete predicate both refuse actual activation');

  await reset();let {ids}=await fixture();
  await db.run('SELECT compute_order_cut_capture(?::text)','race');
  const a=postgres(url,{...options,max:1}),b=postgres(url,{...options,max:1});
  try {
    await a.unsafe('BEGIN');await b.unsafe('BEGIN');
    assert.equal((await b.unsafe('SELECT id FROM state_compute_work WHERE id=$1',[ids[0]])).length,1);
    await a.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('compute-order-cut:'||current_schema(),0))");
    const pid=(await b.unsafe('SELECT pg_backend_pid() pid'))[0].pid;
    const pending=b.unsafe("UPDATE state_compute_work SET lease='stale-selected',leased_to='w',lease_until='2999',leased_at='2026-11-16T23:00:00.000Z' WHERE id=$1 AND state='open' RETURNING id",[ids[0]]).execute();
    await waitLock(pid);
    await a.unsafe('SELECT compute_order_cut_activate($1,$2,$3)',['race',0,digest([])]);await a.unsafe('COMMIT');
    assert.equal((await pending).length,0);await b.unsafe('COMMIT');
    assert.equal((await db.get('SELECT lease FROM state_compute_scheduling WHERE id=?',ids[0])).lease,null);
    assert.equal((await db.get('SELECT count(*) n FROM relation_compute_order_cut_protection')).n,0);
    console.log('ok SELECT before activation / actual UPDATE after activation cannot lease cancelled work or self-protect');
  }finally{await a.unsafe('ROLLBACK');await b.unsafe('ROLLBACK');await a.end();await b.end();}

  await reset();({ids}=await fixture());
  await db.run('SELECT compute_order_cut_capture(?::text)','race');
  const issuer=postgres(url,{...options,max:1}),activation=postgres(url,{...options,max:1});
  try {
    await issuer.unsafe('BEGIN');
    await issuer.unsafe("UPDATE state_compute_work SET lease='genuine-issued',leased_to='w',lease_until='2999',leased_at='2026-11-16T23:00:00.000Z' WHERE id=$1",[ids[1]]);
    await assert.rejects(activation.unsafe('SELECT compute_order_cut_activate($1,$2,$3)',['race',0,digest([])]),/writes in flight; retry activation/);
    await issuer.unsafe('COMMIT');
    await activation.unsafe('SELECT compute_order_cut_activate($1,$2,$3)',['race',0,digest([])]);
    assert.equal((await db.get('SELECT lease FROM state_compute_work WHERE id=?',ids[1])).lease,'genuine-issued');
    assert.equal((await db.get('SELECT count(*) n FROM state_compute_effective_open')).n,1);
    console.log('ok activation refuses in-flight writes promptly; a genuine pre-activation lease remains answerable on retry');
  }finally{await issuer.unsafe('ROLLBACK');await activation.unsafe('ROLLBACK');await issuer.end();await activation.end();}

  // An actual damaged guard must let the forbidden mutation through; restore the same source function.
  await db.exec('CREATE OR REPLACE FUNCTION compute_order_write_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;');
  let failed=false;
  try {await db.run("UPDATE state_compute_scheduling SET lease='broken-guard',leased_to='bad',lease_until='2999' WHERE id=?",ids[0]);
    assert.equal((await db.get('SELECT lease FROM state_compute_scheduling WHERE id=?',ids[0])).lease,null);
  }catch(e){assert(e instanceof assert.AssertionError);failed=true;}
  assert(failed);await db.exec(statement('compute_order_write_guard'));
  await db.run("UPDATE state_compute_scheduling SET lease='restored-guard' WHERE id=?",ids[2]);
  assert.equal((await db.get('SELECT lease FROM state_compute_scheduling WHERE id=?',ids[2])).lease,null);
  console.log('ok real broken write-guard baseline FAIL; identical restored guard PASS');

  await reset();
  await db.exec("INSERT INTO entity_compute_specs(id,verb,request) SELECT md5(i::text),'simulate','{}' FROM generate_series(1,100000) i; INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason) OVERRIDING SYSTEM VALUE SELECT i,md5(i::text),1,'2026-01-01','request' FROM generate_series(1,100000) i; INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position) SELECT md5(i::text),'simulate',i,'open','2026-01-01','2026-01-01','sweep',3,i FROM generate_series(1,100000) i;");
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'perf',cut_at:'2026-11-16',task_cutoff:100000,keep_boundary:[1,0,''],source:{fixture:true}}));
  await db.run('SELECT compute_order_cut_capture(?::text)','perf');
  await db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','perf',0,digest([]));
  await db.exec('ANALYZE state_compute_scheduling; ANALYZE entity_compute_tasks; ANALYZE relation_compute_order_cut_protection; ANALYZE entity_compute_order_cuts; ANALYZE record_compute_order_cut_activations');
  const plans=await db.all("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id FROM state_compute_effective_open ORDER BY coalesce(zone,factory_default_zone(producer)),coalesce(zone_position,task_id),id LIMIT 10");
  const plan=plans[0]['QUERY PLAN'][0],nodes=[];const walk=n=>{nodes.push(n);for(const c of n.Plans||[])walk(c);};walk(plan.Plan);
  console.log(JSON.stringify({empty_tail_plan:plan}));
  assert.equal(plan.Plan['Actual Rows'],0);
  assert(nodes.some(n=>n['Index Name']==='compute_scheduling_zone'&&/ROW\(COALESCE/.test(n['Index Cond']||'')));
  assert(!nodes.some(n=>n['Relation Name']==='state_compute_scheduling'&&n['Actual Rows']>10));
  assert(plan.Plan['Shared Hit Blocks']<1000);
  console.log('ok empty eligible prefix seeks indexed FIFO boundary without visiting 100000 cancelled rows');
  await reset();
  await db.exec("INSERT INTO entity_compute_specs(id,verb,request) SELECT md5(i::text),'simulate','{}' FROM generate_series(1,100000) i; INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason) OVERRIDING SYSTEM VALUE SELECT i,md5(i::text),1,'2026-01-01','request' FROM generate_series(1,100000) i; INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position) SELECT md5(i::text),'simulate',i,'open','2026-01-01','2026-01-01','sweep',3,i FROM generate_series(1,100000) i; ANALYZE state_compute_scheduling; ANALYZE entity_compute_tasks;");
  const source=readFileSync(new URL('../worker/pg/sync.sql',import.meta.url),'utf8');
  const types=['text','bigint','bigint','text','text','text','text','text','bigint','bigint','text','text','bigint','text','text','text','bigint'];
  const inner=source.match(/pick text:=\$pick\$([^]*?)\$pick\$;/)[1].replaceAll('/* lock */','').replaceAll('/* state */',"guard.state='open'").replace('/* source */','factory_eligible($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)').replace(/\$(\d+)\b/g,(_,n)=>`($${n}::${types[Number(n)-1]})`);
  const args=['2026-11-17T00:00:00.000Z',0,1,'e','["w"]','w','n',null,600,0,'["open"]',null,1,null,null,null,100000];
  for(const active of [false,true]) {
    if(active) {
      await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'prefix-perf',cut_at:'2026-11-16',task_cutoff:100000,keep_boundary:[3,50000,'z'],source:{fixture:true}}));
      await db.run('SELECT compute_order_cut_capture(?::text)','prefix-perf');
      await db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','prefix-perf',0,digest([]));
    }
    const actual=active?inner:inner.split('  SELECT chosen.* FROM (')[0]+"SELECT q.* FROM state_compute_scheduling guard CROSS JOIN LATERAL(SELECT e.* FROM eligible e WHERE e.id=guard.id OFFSET 0) q WHERE guard.state='open' AND factory_takeable(guard.lease_until,guard.leased_at,($1::text),($9::bigint)) AND (($12::text) IS NULL OR guard.waits=0) ORDER BY coalesce(guard.zone,factory_default_zone(guard.producer)),coalesce(guard.zone_position,guard.task_id),guard.id LIMIT ($13::bigint)";
    const binds=active?args:args.slice(0,-1);
    const [before]=await sql.unsafe('EXPLAIN (FORMAT JSON) '+actual,binds);console.log(JSON.stringify({head_estimate:{active,plan:before['QUERY PLAN'][0]}}));
    const [item]=await sql.unsafe('EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) '+actual,binds),plan=item['QUERY PLAN'][0];
    console.log(JSON.stringify({head_plan:{active,plan}}));
    assert.equal(plan.Plan['Actual Rows'],1);assert(plan.Plan['Shared Hit Blocks']<500);
    const head=await db.get('SELECT id FROM factory_head(?::text,0,1,?::text,?::text,?::text,?::text,NULL,600,0,?::text)',args[0],'e','["w"]','w','n','[]');
    assert.equal(head.id,createHash('md5').update('1').digest('hex'));
  }
  console.log('ok actual bounded head query uses FIFO index for one of 100000 rows with no cut and a 50000-row prefix after cut');
  const [tailPlan]=await sql.unsafe('EXPLAIN (FORMAT JSON) '+inner.replace('/* prefix enabled */ true','false'),args);
  const planNodes=node=>[node,...(node.Plans??[]).flatMap(planNodes)];
  const tailNodes=planNodes(tailPlan['QUERY PLAN'][0].Plan);
  assert(tailNodes.some(n=>n['Index Name']==='entity_compute_tasks_pkey'&&/id >.*100000/.test(n['Index Cond']??'')));
  assert(!tailNodes.some(n=>n['Relation Name']==='state_compute_scheduling'&&n['Node Type']==='Seq Scan'));
  console.log('ok post-cut dispatch plans the real cutoff and probes scheduling by task identity');
  await db.exec(`INSERT INTO entity_compute_specs(id,verb,request) VALUES ('post-cut-dispatch','simulate','{}');
    INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason) OVERRIDING SYSTEM VALUE
      VALUES (100001,'post-cut-dispatch',1,'2026-11-17','request');
    INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position)
      VALUES ('post-cut-dispatch','simulate',100001,'open','2026-11-17','2026-11-17','post-cut-fixture',3,100001)`);
  const postCutLease=await db.all(`SELECT id FROM factory_lease_pack_zoned(?::text,1,'e','["w"]','w','n',NULL,600,0,
    '["simulate"]',NULL,NULL,'post-cut-fixture',1,'post-cut-lease','2026-11-18','w',1)`,args[0]);
  assert.deepEqual(postCutLease.map(row=>row.id),['post-cut-dispatch']);
  console.log('ok packed dispatch leases an eligible post-cut task across the normal write and lock path');


  await reset();
  await db.exec(`INSERT INTO entity_compute_specs(id,verb,request)
    SELECT md5('history-'||i::text),'simulate','{}' FROM generate_series(1,100000) i;
    INSERT INTO entity_compute_tasks(computation,generation,created_at,reason)
      SELECT id,1,'2026-01-01','request' FROM entity_compute_specs;
    INSERT INTO entity_compute_results(computation,task_id,canon,engine,accepted_at)
      SELECT computation,id,'{}','history','2026-01-01' FROM entity_compute_tasks;
    INSERT INTO record_candidate_answers(question,task_id,engine,device,owner,net,canon,result,work,at,official)
      SELECT computation,id,'history','history','history','','{}','{}',1,'2026-01-01',1 FROM entity_compute_tasks;
    ANALYZE record_candidate_answers;ANALYZE entity_compute_results;ANALYZE entity_compute_tasks;`);
  const historyFixture=await fixture();
  const blocker=postgres(url,{...options,max:1}),capturer=postgres(url,{...options,max:1}),writer=postgres(url,{...options,max:1});
  let pendingCapture,pendingActivation,pendingWrite;
  const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  const ended=promise=>Promise.race([promise.then(value=>({done:true,value})),pause(500).then(()=>({done:false}))]);
  try {
    await blocker.unsafe('BEGIN');await blocker.unsafe('LOCK TABLE record_candidate_answers IN ACCESS EXCLUSIVE MODE');
    const capturePid=(await capturer.unsafe('SELECT pg_backend_pid() pid'))[0].pid;
    pendingCapture=capturer.unsafe('SELECT compute_order_cut_capture($1)',['race']).execute();
    for(let i=0;i<100;i++) {
      if((await sql.unsafe("SELECT wait_event FROM pg_stat_activity WHERE pid=$1",[capturePid]))[0]?.wait_event==='relation')break;
      if(i===99)assert.fail('preparing capture did not reach the actual historical table lock');
      await pause(10);
    }
    const incompleteStart=performance.now();
    await assert.rejects(db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','race',0,digest([])),/completed preparing capture required/);
    const refusedMs=performance.now()-incompleteStart;
    pendingWrite=writer.unsafe("UPDATE state_compute_work SET lease='during-capture',leased_to='w',lease_until='2999',leased_at='2026-11-16T23:00:00.000Z' WHERE id=$1 RETURNING id",[historyFixture.ids[0]]).execute();
    const issued=await ended(pendingWrite);
    assert.equal(issued.done,true,'preparing historical capture blocked an independent valid claim');assert.equal(issued.value.length,1);
    const captureStart=performance.now();await blocker.unsafe('ROLLBACK');await pendingCapture;
    const captureMs=performance.now()-captureStart;
    assert.equal((await db.get('SELECT count(*) n FROM record_compute_order_cut_captures')).n,1);
    const plans=await db.all('EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT count(*) FROM relation_compute_order_cut_protection WHERE cut_id=?','race');
    console.log(JSON.stringify({preparing_history:{answers:100000,results:100000,current_questions:3,capture_ms:captureMs,
      incomplete_activation_refused_ms:refusedMs,independent_claim_unblocked:true,finite_activation_membership_plan:plans[0]['QUERY PLAN'][0]}}));
    if(process.argv.includes('--break-activation-history')) {
      const baseline=readFileSync(new URL('../tests/fixtures/order-cut-activation/f3ffb55e.sql',import.meta.url),'utf8');
      assert.equal(createHash('sha256').update(baseline).digest('hex'),'a7d7357709e0d5771b034159c845a23e125036e2155f9505b656e37bdb40e246');
      await db.exec(baseline);
    }
    await blocker.unsafe('BEGIN');await blocker.unsafe('LOCK TABLE record_candidate_answers IN ACCESS EXCLUSIVE MODE');
    const began=performance.now();
    pendingActivation=db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','race',0,digest([]));
    const activation=await ended(pendingActivation);
    if(!activation.done) {
      const writerPid=(await writer.unsafe('SELECT pg_backend_pid() pid'))[0].pid;
      pendingWrite=writer.unsafe("UPDATE state_compute_work SET lease='blocked-by-old-activation',leased_to='w2',lease_until='2999',leased_at='2026-11-16T23:00:00.000Z' WHERE id=$1 RETURNING id",[historyFixture.ids[1]]).execute();
      await waitLock(writerPid);
      const locks=await db.all("SELECT a.pid,a.wait_event,l.mode,l.granted FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE a.datname=current_database() AND l.locktype='advisory'");
      console.log(JSON.stringify({activation_blocked_by_history:true,new_claim_blocked_on_advisory:true,held_advisory_locks:locks}));
    }
    assert.equal(activation.done,true,'activation acquired its exclusive barrier and waited for historical answers');
    console.log(JSON.stringify({activation_history_isolation:{historical_rows:200000,current_questions:3,activation_ms:performance.now()-began,
      historical_table_still_exclusively_locked:true,protected:(await db.get('SELECT protected_count n FROM record_compute_order_cut_activations')).n}}));
    console.log('ok committed preparing watermark covers large historical sources; activation finishes while history remains locked and never blocks preparing claims');
  } finally {
    await blocker.unsafe('ROLLBACK');
    await Promise.allSettled([pendingCapture,pendingWrite,pendingActivation].filter(Boolean));
    await blocker.end();await capturer.end();await writer.end();
  }

}finally {await sql.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`);await sql.end();}
