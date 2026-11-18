// Native queue membership, concurrent initialization and bounded balance folding.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {seedComputeWork} from './pg/fixtures.mjs';
import {placeholders} from '../worker/db.js';
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
const check=async label=>{assert.deepEqual(await read(),await oracle(),label);console.log('ok',label);};
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
  for(const id of ['a','b','c','d','e'])await seedComputeWork(seedData,{id,producer:'sweep'});
  await query(readFileSync(new URL('../worker/pg/queue_counts.sql',import.meta.url),'utf8'));
  assert.equal(await read(),null);
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
  await writer.end();await query('SELECT queue_count_fold()');
  await check('folding a disconnected writer preserves the complete read');
  const again=await seed();assert.equal(again.installed,false);await check('retry cannot overwrite a seed');
  await query('TRUNCATE record_compute_order_cancellations');assert.equal(await read(),null);
  await seed();await check('truncation requires a new seed');
}finally{
  await writer.end().catch(()=>{});await query('DROP SCHEMA '+schema+' CASCADE').catch(()=>{});await sql.end();
}
