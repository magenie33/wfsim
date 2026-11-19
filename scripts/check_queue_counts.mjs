// Native queue membership, concurrent initialization and bounded balance folding.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {seedComputeWork} from './pg/fixtures.mjs';
import {compareQueueDay,measureQueueCosts,queueHours,recordQueueDay} from '../worker/queue_clearance.js';
import {pgDb,postgresClient,placeholders} from '../worker/db.js';
import {queueDemandOf} from '../worker/contribution.js';
import {seedQueueZones,queueZonesSql} from '../worker/queue_counts.js';

const port=Number(process.env.WFSIM_STATISTICS_TEST_PORT);
assert(Number.isInteger(port)&&port>=1024&&port<=65535);
const open=()=>postgres({host:'127.0.0.1',port,database:'postgres',max:1,onnotice:()=>{}});
const sql=open(), writer=open(), schema='queue_count_test_'+process.pid;
const query=(s,args=[])=>sql.unsafe(s,args);
const seedData={run:(s,...args)=>query(placeholders(s),args),get:async(s,...args)=>(await query(placeholders(s),args))[0]};
const read=async()=>JSON.parse(JSON.stringify((await query(queueZonesSql))[0]?.zones??null));
const oracle=async()=>JSON.parse(JSON.stringify(await query(`SELECT w.zone::int z,count(*)::int n
  FROM state_compute_effective_work w WHERE w.state='open' GROUP BY w.zone ORDER BY w.zone`))).map(r=>[r.z,r.n]);
const check=async label=>{
  assert.deepEqual(await read(),await oracle(),label);
  const [{verbs}]=await query(queueZonesSql);
  const expected=await query("SELECT verb,count(*)::int n FROM state_compute_effective_work WHERE state='open' GROUP BY verb");
  assert.deepEqual(verbs,Object.fromEntries(expected.map(r=>[r.verb,r.n])),label);
  console.log('ok',label);
};
const seed=async(during=async()=>{})=>sql.begin('isolation level repeatable read',async tx=>{
  let changed=false;
  return seedQueueZones(async(s,args=[])=>{
    const rows=await tx.unsafe(s,args);
    if(!changed && s.includes('WITH page AS MATERIALIZED')){changed=true;await during();}
    return rows;
  },{pageRows:2});
});
try{
  await query('CREATE SCHEMA '+schema);
  for(const c of [sql,writer])await c.unsafe('SET search_path TO '+schema+',pg_catalog');
  for(const name of ['schema','library','sync','inventory','build_workflows','board_admission'])
    await query(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
  await query(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
  for(const id of ['a','b','c','d','e'])await seedComputeWork(seedData,{id,verb:id==='b'?'optimize':'simulate',producer:'sweep'});
  const db=pgDb(postgresClient(sql));
  assert.equal(await queueDemandOf({LIBRARY:db}),undefined);
  assert.equal(await measureQueueCosts(db,null),null);
  await query(`CREATE TABLE state_queue_zone_changes(epoch text NOT NULL,writer int NOT NULL,zone smallint NOT NULL,n bigint NOT NULL,PRIMARY KEY(epoch,writer,zone));
    CREATE TABLE state_queue_zone_seed(epoch text PRIMARY KEY,zones jsonb NOT NULL,sampled_at timestamptz NOT NULL);
    INSERT INTO state_queue_zone_changes VALUES('',0,3,2);
    INSERT INTO state_queue_zone_seed VALUES('','[[3,3]]',now());`);
  await query("DELETE FROM state_queue_zone_seed WHERE epoch=''");
  const legacySeed=await sql.begin('isolation level repeatable read',tx=>
    seedQueueZones((s,args=[])=>tx.unsafe(s,args),{typed:false,pageRows:2}));
  assert(legacySeed.installed);
  const legacy=await queueDemandOf({LIBRARY:db});
  assert.deepEqual(legacy.queue.zones,[[3,5]]);
  assert.equal(legacy.clear_hours,null);
  assert.equal(await measureQueueCosts(db,null),null);
  console.log('ok total orders survive a staged queue statistics upgrade');
  await query(readFileSync(new URL('../worker/pg/queue_counts.sql',import.meta.url),'utf8'));
  assert.deepEqual(await read(),[[3,5]]);
  assert.equal((await query(queueZonesSql))[0].verbs,null);
  await seed(()=>writer.unsafe("UPDATE state_compute_scheduling SET zone=0 WHERE id='c'"));
  await check('zone promotion committed during bootstrap appears exactly once');
  await query("UPDATE state_compute_scheduling SET lease='held',waits=2 WHERE id='a'");
  await query("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('sweep','one','a','live','2026-01-01','2026-01-01'),('new_build','two','a','live','2026-01-01','2026-01-01')");
  await check('held work and shared requests keep one queue entry');
  await query("UPDATE state_compute_scheduling SET state='withdrawn' WHERE id='e'");
  await query("UPDATE state_compute_scheduling SET state='fact' WHERE id='d'");
  await check('completion and withdrawal remove one each');
  await query("SELECT compute_open_task('d','fixture_recheck','fixture','2026-01-02'),compute_open_task('e','new_user_request','fixture','2026-01-02')");
  await query("UPDATE state_compute_scheduling SET zone_position=task_id");
  await query("SELECT compute_order_cut_stage($1::text::jsonb)",[JSON.stringify({id:'queue-cut',cut_at:'2026-02-01',task_cutoff:Number((await query('SELECT max(id) id FROM entity_compute_tasks'))[0].id),keep_boundary:[3,1,'a'],source:{fixture:true}})]);
  await query("SELECT compute_order_cut_protect('queue-cut',$1::text::jsonb)",[JSON.stringify(['d'])]);
  await query("SELECT compute_order_cut_capture('queue-cut')");
  const [capture]=await query("SELECT protected_count,protected_sha256 FROM record_compute_order_cut_captures WHERE cut_id='queue-cut' ORDER BY id DESC LIMIT 1");
  await query("SELECT compute_order_cut_activate('queue-cut',$1,$2)",[1,createHash('sha256').update('d').digest('hex')]);
  assert(capture);assert.equal(await read(),null);
  await seed();await check('cut keeps prefix and protected work with a new epoch');
  await seedComputeWork(seedData,{id:'post',producer:'sweep'});
  await check('post-cut intake is counted');
  await query("INSERT INTO record_compute_order_cancellations SELECT id,task_id,'queue-cut','2026-02-02' FROM state_compute_scheduling WHERE id='post'");
  await check('explicit cancellation removes its current round');
  await query("DELETE FROM record_compute_order_cancellations WHERE question='post'");
  await check('removing a cancellation restores current eligibility');
  await seedComputeWork(seedData,{id:'post-opt',verb:'optimize',producer:'sweep'});
  await check('mixed verbs in one zone retain their own workload');
  const now=Date.now();
  await query(readFileSync(new URL('../deploy/queue_cost_index.sql',import.meta.url),'utf8').replace('INDEX CONCURRENTLY','INDEX'));
  for(const [verb,ms] of [['simulate',100],['optimize',900]]) {
    const id='measured-'+verb, at=new Date(now-(verb==='simulate'?2:48)*3600000).toISOString();
    await seedComputeWork(seedData,{id,verb,opened_at:at});
    await query(`INSERT INTO record_candidate_answers(question,task_id,engine,device,owner,official,canon,result,work,compute_ms,at)
      SELECT id,task_id,'fixture',device,device,0,'{}','{}',1,$2::bigint,$3 FROM state_compute_scheduling
      CROSS JOIN (VALUES('first'),('second')) a(device) WHERE id=$1`,[id,ms,at]);
    await query("SELECT compute_accept_result(id,task_id,'open','fact','{}','fixture',$2,0,'[]') FROM state_compute_scheduling WHERE id=$1",[id,at]);
  }
  await query("INSERT INTO record_hourly_compute_totals(verifier,hour,ms) VALUES('first',$1,2400)",
    [new Date(now-3600000).toISOString().slice(0,13)]);
  const prices=await measureQueueCosts(db,null,now);
  const [row]=await query(queueZonesSql);
  assert.deepEqual(row.completed,{simulate:1,optimize:1});
  assert.equal(prices.msOf.simulate,200);
  const count=Object.values(row.verbs).reduce((a,b)=>a+b,0);
  assert.equal(queueHours(row,count,prices,now),2*row.verbs.simulate+18*(row.verbs.optimize||0));
  assert.equal(prices.msOf.optimize,1800);
  assert.equal(queueHours(row,count,{...prices,factsOf:{simulate:1}},now),null);
  assert.equal(queueHours(row,count+1,prices,now),null);
  assert.equal(queueHours(row,count,prices,prices.expires),null);
  assert.equal(queueHours(row,count,{...prices,perHour:0},now),null);
  await recordQueueDay(db,prices,now);await recordQueueDay(db,prices,now);
  const days=await query('SELECT forecast FROM state_queue_clearance_days');
  assert.equal(days.length,1);
  const prior={...days[0].forecast,at:new Date(Date.parse(row.counts_at)-3600000).toISOString()};
  assert.equal(compareQueueDay(prior,{...row,completed:{...row.completed,simulate:2}}).completed_ms,200);
  const newVerb={...row,completed:{...row.completed,simulate:2,followup:2}};
  const partial=compareQueueDay(prior,newVerb);
  assert.equal(partial?.completed_ms,null);
  assert.equal(partial.priced_ms,200);
  assert.equal(partial.priced_rounds,1);
  assert.deepEqual(partial.unpriced_rounds,{followup:2});
  assert.equal(compareQueueDay(prior,{...row,completed:{simulate:2}}),null);
  assert.equal(compareQueueDay(prior,{...row,completed:{...row.completed,followup:0}}).completed_ms,0);
  console.log('ok measured confirmed rounds, complete-hour capacity, daily checkpoint and cancellation-safe comparison');
  await writer.end();await query('SELECT queue_count_fold()');
  await check('folding a disconnected writer preserves the complete read');
  const again=await seed();assert.equal(again.installed,false);await check('retry cannot overwrite a seed');
  await query('TRUNCATE record_compute_order_cancellations');assert.equal(await read(),null);
  await seed();await check('truncation requires a new seed');
}finally{
  await writer.end().catch(()=>{});await query('DROP SCHEMA '+schema+' CASCADE').catch(()=>{});await sql.end();
}
