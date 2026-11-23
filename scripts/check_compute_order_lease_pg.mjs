// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import postgres from './pg/node_modules/postgres/src/index.js';

const url=process.env.WFSIM_ORDER_CUT_TEST_URL;
assert(url&&['127.0.0.1','localhost'].includes(new URL(url).hostname),'use an isolated PostgreSQL database');
const schema='order_lease_'+randomBytes(6).toString('hex');
const connect=()=>postgres(url,{max:1,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:15000}});
const owner=connect(), devices=Array.from({length:7},connect);
const read=name=>readFileSync(new URL(`../worker/pg/${name}.sql`,import.meta.url),'utf8');
const at='2026-11-23T04:00:00.000Z', past='2026-11-23T01:55:00.000Z', expiry='2026-11-23T04:05:00.000Z';
const cap='["cap:fixed-board-input-v1"]';

function lease(sql,device,{size=128,token=device,capabilities=cap}={}) {
  return sql.unsafe(`SELECT id,zone,zone_position,verb FROM factory_lease_pack_zoned($1,4,'fixture',
    $2,'fixture-user','',NULL,7200,0,'["simulate"]',NULL,NULL,NULL,$3,$4,$5,$6,16)`,
  [at,capabilities,size,token,expiry,device]);
}

try {
  await owner.unsafe(`CREATE SCHEMA ${schema}`);
  for(const name of ['schema','library','sync','inventory','build_workflows','board_admission','order_cut_indexes','order_dispatch_open_index']){
    const source=name==='sync'&&process.env.WFSIM_ORDER_LEASE_SYNC_SOURCE
      ?readFileSync(process.env.WFSIM_ORDER_LEASE_SYNC_SOURCE,'utf8'):read(name);
    await owner.unsafe(source.replaceAll('INDEX CONCURRENTLY','INDEX')).simple();
  }
  await owner.unsafe(`INSERT INTO entity_compute_specs(id,verb,request)
    SELECT 'round-'||n,'simulate','{}' FROM generate_series(1,1200) n;
    INSERT INTO entity_compute_tasks(id,computation,generation,created_at,reason) OVERRIDING SYSTEM VALUE
    SELECT n,'round-'||n,1,'2026-11-01T00:00:00.000Z','fixture' FROM generate_series(1,1200) n;
    INSERT INTO state_compute_scheduling(id,verb,task_id,state,due_at,opened_at,producer,zone,zone_position,required_capability)
    SELECT 'round-'||n,'simulate',n,'open','2026-11-01T00:00:00.000Z','2026-11-01T00:00:00.000Z',
      'sweep',CASE WHEN n IN(1,3) THEN 0 ELSE 3 END,n,'fixed-board-input-v1' FROM generate_series(1,1200) n;
    SELECT compute_order_cut_stage('{"id":"lease-cut","cut_at":"2026-11-22T00:00:00.000Z","task_cutoff":3,
      "keep_boundary":[0,1,"round-1"],"source":{}}');
    SELECT compute_order_cut_capture('lease-cut');`).simple();
  const empty=createHash('sha256').update('').digest('hex');
  await owner.unsafe('SELECT compute_order_cut_activate($1,0,$2)',['lease-cut',empty]);
  assert.equal((await lease(devices[0],'no-capability',{size:1,capabilities:'[]'})).length,0);
  const [first]=await lease(devices[0],'prefix',{size:1});
  assert.equal(first.id,'round-1');

  const signature='factory_take(text,bigint,text,text,text,text,text,bigint,bigint,text,text,text,text,bigint)';
  const [{body}]=await owner.unsafe('SELECT pg_get_functiondef($1::regprocedure) body',[signature]);
  await owner.unsafe(body.replace(/AS \$function\$[\s\S]*\$function\$/,"AS $function$ SELECT 'round-1'::text $function$")).simple();
  try {
    const duplicate=await lease(devices[1],'selected-before-previous-commit',{size:1});
    assert.equal(duplicate.length,0,'a stale selection cannot replace a still-valid lease');
    assert.equal((await owner.unsafe("SELECT lease FROM state_compute_scheduling WHERE id='round-1'"))[0].lease,'prefix');
    await owner.unsafe("UPDATE state_compute_scheduling SET leased_at=$1 WHERE id='round-1'",[past]);
    const [reopened]=await lease(devices[1],'past-ceiling',{size:1});
    assert.equal(reopened.id,'round-1');
    const [previous]=await owner.unsafe("SELECT lease,lease_prev FROM state_compute_scheduling WHERE id='round-1'");
    assert.equal(previous.lease,'past-ceiling');assert.equal(previous.lease_prev,'prefix');
  } finally {await owner.unsafe(body).simple();}

  const actual=await Promise.all(devices.map((sql,i)=>lease(sql,'fleet-'+i)));
  const taken=actual.flat().map(row=>row.id);
  assert.equal(new Set(taken).size,taken.length,'concurrent lease replies must not duplicate a physical round');
  assert(taken.length>=128);
  assert(!taken.some(id=>['round-1','round-2','round-3'].includes(id)));
  const remaining=await lease(devices[0],'next',{size:1});
  assert(!remaining.some(row=>taken.includes(row.id)));
  const [index]=await owner.unsafe(`SELECT i.indisvalid,pg_get_expr(i.indpred,i.indrelid) predicate FROM pg_index i
    WHERE i.indexrelid='compute_scheduling_open_task'::regclass`);
  assert(index.indisvalid&&index.predicate.includes("state = 'open'"));
  console.log(JSON.stringify({leased:taken.length,unique:taken.length,rechecked:true,oldLeaseRecovery:true,
    cancelledExcluded:true,positionPrefix:first.id,capabilityVerified:true,indexValid:true}));
} finally {
  await Promise.all(devices.map(sql=>sql.end()));
  try{await owner.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`);}finally{await owner.end();}
}
