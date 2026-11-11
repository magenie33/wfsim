// SPDX-License-Identifier: AGPL-3.0-or-later
// The same zone order governs single, packed and official claims.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { factory, canon } from '../worker/tasks.js';

export async function queueZoneCases(db) {
  const at=Date.parse('1900-01-01T00:00:00Z'),env={LIBRARY:db},registered=[];
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,pack:true,canon}},
    producers:{zone_p:{on_fact:async()=>{}},zone_q:{on_fact:async()=>{}}}});
  const row=(ref,zone,extra={})=>({producer:'zone_p',ref,zone,verb:'simulate',request:{zone_fixture:ref},
    due_in_ms:-1000,payload:{},...extra});
  const admit=async asks=>{
    registered.push(...asks);return f.demandAll(env,asks,'e',at);
  };
  const clean=async()=>{for(const a of registered.splice(0))await f.withdraw(env,a.producer,a.ref);};
  const witness=name=>({device:'zone-'+name,owner:'zone-'+name,net:'zone-'+name,lanes:128,before:'1901-01-01'});
  if(process.argv.includes('--break-zone-order')) {
    const source=readFileSync(new URL('../worker/pg/sync.sql',import.meta.url),'utf8');
    const definition=source.match(/CREATE OR REPLACE FUNCTION factory_head\([^]*?\$\$;/)[0];
    await db.run(definition.replace('ORDER BY q.zone,q.zone_position,q.id','ORDER BY q.due_at,q.id'));
  }
  try {
    const [low,daily,review,manual]=await admit([row('zone-low',99,{due_in_ms:-40000}),row('zone-daily',3),
      row('zone-review',1),row('zone-manual',0)]);
    for(const [n,id]of [manual,review,daily,low].entries())
      assert.equal((await f.next(env,witness('single'+n),'e',at)).question,id);
    await clean();
    const fifo=await admit([row('zone-first',3),row('zone-second',3,{producer:'zone_q',due_in_ms:-900000,waits:true})]);
    assert.equal((await f.next(env,witness('fifo0'),'e',at)).question,fifo[0]);
    assert.equal((await f.next(env,witness('fifo1'),'e',at)).question,fifo[1]);
    await clean();
    const [ahead,promoted]=await admit([row('zone-existing-manual',0),row('zone-promoted',3)]);
    const held=await f.next(env,witness('held'),'e',at);
    assert.equal(held.question,ahead);
    const running=await f.next(env,witness('running'),'e',at);
    assert.equal(running.question,promoted);
    const original=await db.get('SELECT task_id,lease FROM state_compute_work WHERE id=?',promoted);
    await admit([row('zone-promote-source',0,{request:{zone_fixture:'zone-promoted'}})]);
    const moved=await db.get('SELECT zone,task_id,lease FROM state_compute_work WHERE id=?',promoted);
    assert.equal(moved.zone,0);assert.equal(moved.task_id,original.task_id);assert.equal(moved.lease,original.lease);
    await clean();
    const packIds=await admit([row('zone-pack-daily',3),row('zone-pack-review',1,{producer:'zone_q'}),row('zone-pack-manual',0)]);
    const pack=await f.nextPack(env,witness('pack'),'e',{n:3,lease_ms:60000},at);
    assert.deepEqual(pack.questions.map(q=>q.question),[packIds[2],packIds[1],packIds[0]]);
    await clean();
    const officialIds=await admit([row('zone-official-low',99),row('zone-official-review',1),row('zone-official-manual',0)]);
    const official=await f.claim(env,{device:'official:zone',owner:'official:zone',official:true},{states:['open'],max:3},'e',at);
    assert.deepEqual(official.map(q=>q.question),[officialIds[2],officialIds[1],officialIds[0]]);
    await assert.rejects(f.demand(env,row('zone-invalid',100),'e',at),/zone/);
    await assert.rejects(db.run('UPDATE state_compute_scheduling SET zone=-1 WHERE id=?',officialIds[0]),/zone_bounds/);
    assert.equal((await db.get("SELECT factory_default_zone('user_calculation') zone")).zone,0);
    assert.equal((await db.get("SELECT factory_default_zone('board_review') zone")).zone,1);
    assert.equal((await db.get("SELECT factory_default_zone('new_build') zone")).zone,3);
    console.log('queue zones: priority, FIFO, held-source promotion, mixed packs, official claims and bounds pass');
  } finally { await clean(); }
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href) {
  const {testDb}=await import('./pg/test_db.mjs');const db=await testDb('library');
  try{await queueZoneCases(db);}finally{await db.close();}
}
