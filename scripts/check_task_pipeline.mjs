// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';
import {factory,canon,CLAIM_STALE_MS} from '../worker/tasks.js';
import {forgetDials} from '../worker/dials.js';

export async function taskPipelineCases(db,{brokenInline=false}={}) {
  const now=Date.parse('2026-11-14T10:00:00.000Z'),at=new Date(now).toISOString();
  await db.exec('CREATE TABLE stage_received(ref text PRIMARY KEY)');
  await db.run("INSERT INTO state_factory_parameters(name,value,set_by,set_at) VALUES('settle_async',1,'check',?) ON CONFLICT(name) DO UPDATE SET value=1",at);
  delete db.settleAtOnce;forgetDials();
  let brokenDelivery=false,releaseSlow,slowStarted;
  const slow=new Promise(resolve=>{releaseSlow=resolve;});
  const started=new Promise(resolve=>{slowStarted=resolve;});
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,pack:true,canon:(result,work)=>canon({...result,work})}},
    producers:{
      sql:{on_fact_writes:(env,d)=>[env.LIBRARY.prepare(`INSERT INTO ${brokenDelivery?'stage_missing':'stage_received'}(ref) VALUES(?) ON CONFLICT DO NOTHING`).bind(d.ref)]},
      callback:{on_fact:async(env,d)=>{slowStarted();await slow;await env.LIBRARY.prepare('INSERT INTO stage_received(ref) VALUES(?) ON CONFLICT DO NOTHING').bind(d.ref).run();}},
    },spot:0,recheck:0,random:()=>1});
  const env={LIBRARY:db};
  const who=id=>({device:id,owner:id,net:id});
  for(const id of ['first','second','third','fourth'])await db.run("INSERT INTO entity_compute_devices(id,seen,work) VALUES(?,? ,0)",id,at);
  const request=async(ref,value=1,producer='sql')=>f.demand(env,{producer,ref,verb:'simulate',request:{value},due_in_ms:1},'e1',now);
  const give=async(id,engine='e1',work=10)=>{
    const task=await f.next(env,who(id),engine,now);
    assert(task,'expected an executable task');
    await f.answerPack(env,who(id),{lease:task.lease,engine,served:'e1',join:engine!=='e1',
      answers:[{question:task.question,result:{score:7,metric:'kpm'},work}]},now);
    return task.question;
  };
  const verify=()=>brokenInline?f.settleQueued(env,now):f.runStage(env,'verification',now);
  const q=await request('original');
  assert.equal(await give('first','e0'),q);
  await f.controlStage(env,'verification',{paused:true,by:'check'},now);
  const paused=await f.runStage(env,'verification',now);
  assert(paused.paused);assert.equal((await db.get('SELECT count(*) n FROM state_pending_verifications')).n,1);
  await f.controlStage(env,'verification',{paused:false,by:'check'},now);
  await verify();assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',q)).state,'open');
  assert.equal(await give('second'),q);
  await db.run("UPDATE state_pending_verifications SET claim='crashed',claimed_at=?",at);
  assert.equal((await f.runStage(env,'verification',now)).processed,0);
  await db.run('UPDATE state_pending_verifications SET claimed_at=?',new Date(now-CLAIM_STALE_MS-1).toISOString());
  await verify();
  assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',q)).state,'fact');
  assert.equal((await db.get('SELECT count(*) n FROM stage_received')).n,0,'verification waited for business delivery');
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,0,'verification ran contribution settlement inline');
  assert.equal((await db.get('SELECT count(*) n FROM record_candidate_answers WHERE credit_work IS NOT NULL')).n,2);
  assert((await db.get("SELECT delivery_at FROM relation_task_consumers WHERE ref='original'")).delivery_at);
  brokenDelivery=true;
  await assert.rejects(f.runStage(env,'delivery',now),/stage_missing/);
  assert.equal((await f.runStage(env,'contribution',now)).processed,2);
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,20);
  assert.equal((await db.get('SELECT count(*) n FROM stage_received')).n,0);
  assert.equal((await f.runStage(env,'contribution',now)).processed,0);
  const later=now+60001;brokenDelivery=false;
  await f.controlStage(env,'delivery',{paused:true,by:'check'},later);
  assert((await f.runStage(env,'delivery',later)).paused);
  assert.equal((await db.get('SELECT count(*) n FROM stage_received')).n,0);
  await f.controlStage(env,'delivery',{paused:false,by:'check'},later);
  assert.equal((await f.runStage(env,'delivery',later)).processed,1);
  assert.equal((await f.runStage(env,'delivery',later)).processed,0);
  assert.equal((await db.get('SELECT count(*) n FROM stage_received')).n,1);
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_results')).n,1);
  const status=await f.pipelineStatus(env,later,1);
  assert.equal(status.find(row=>row.name==='delivery').progress.failures,1);
  assert(status.every(row=>row.pending===0));

  // An external consumer can stay busy while the next computation is verified.
  const callback=await request('slow',2,'callback');
  await give('third');await verify();await give('fourth');await verify();
  const delivering=f.runStage(env,'delivery',now,{runtime:'edge'});
  await started;
  try {
    const next=await request('next',3);
    await give('first');await verify();await give('second');await verify();
    assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',next)).state,'fact');
    assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',callback)).state,'fact');
    assert.equal((await db.get("SELECT state FROM relation_task_consumers WHERE ref='slow'")).state,'live');
  }finally{releaseSlow();await delivering;}
  await f.runStage(env,'delivery',later);
  await f.runStage(env,'contribution',later);
  assert.equal((await db.get('SELECT count(*) n FROM stage_received')).n,3);
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_results')).n,3);
  await assert.rejects(f.controlStage(env,'unknown',{paused:true,by:'check'},now),/unknown/);
  console.log('ok: independent stages preserve rounds, cross-engine witnesses, durable retry, pause, restart and slow-consumer isolation');
  return {f,env,now,give,request};
}

if(process.argv[1] && pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
  const {testDb}=await import('./pg/test_db.mjs');
  const db=await testDb('library');
  try{await taskPipelineCases(db,{brokenInline:process.argv.includes('--broken-inline')});}finally{await db.close();}
}
