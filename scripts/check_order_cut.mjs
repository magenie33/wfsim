import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import worker from '../worker/index.js';
import {officialRoute} from '../worker/factory.js';
import {hmac} from '../worker/accounts.js';
import {factory,canon} from '../worker/tasks.js';
import {prepareIntakeBuild} from './build_batches.mjs';
import {readInventoryState,inventoryWrite} from '../worker/task_inventory.js';

export async function orderCutCases(db) {
  await db.exec(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
  const env={LIBRARY:db},now=Date.parse('2026-11-16T00:00:00.000Z'),old=now-86400000;
  const f=factory({verbs:{simulate:{cap:2,pack:true,lease_ms:60000,canon:(r,work)=>canon({...r,work})}},producers:{sweep:{on_fact:async()=>{}}},spot:0,recheck:0,random:()=>1});
  const asks=['prefix','cancel','replay','known','preparing','late','prestage','prestage-prev-only'].map(ref=>({producer:'sweep',ref,trigger_id:ref,
    verb:'simulate',request:{fixture:'order-cut',value:ref},payload:{fixture:ref},due_in_ms:1000,zone:ref==='prefix'?0:3}));
  const ids=await f.demandAll(env,asks,'e',old),by=Object.fromEntries(asks.map((a,i)=>[a.ref,ids[i]]));
  const raw=id=>db.get('SELECT * FROM state_compute_scheduling WHERE id=?',id);
  await db.run("INSERT INTO entity_compute_batches(id,at,total) VALUES('old-batch',?,8),('new-batch',?,1)",new Date(old).toISOString(),new Date(now+1000).toISOString());
  for(const a of asks) {
    const identity='business-'+a.ref;
    await db.run('INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES(?,?,?,?)','old-batch',identity,'r','base');
    await f.demand(env,{...a,ref:identity+'|r|base',trigger_id:identity,payload:{identity,ruler:'r',mode:'base',batch:'old-batch'}},'e',old);
  }
  await db.run("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES('old-batch','never-registered','r','base'),('new-batch','new-arrival','r','base')");
  await db.run("UPDATE state_compute_scheduling SET lease='prestage-issued',leased_to='before-stage',lease_until='2999',leased_at=?,lease_prev='prestage-prev',leased_to_prev='previous-holder',lease_prev_until='2999' WHERE id=?",new Date(now-1500).toISOString(),by.prestage);
  await db.run("UPDATE state_compute_scheduling SET lease_prev='previous-only',leased_to_prev='previous-only-holder',lease_prev_until='2999' WHERE id=?",by['prestage-prev-only']);
  const original=await raw(by.cancel),cutoff=(await db.get('SELECT max(id) n FROM entity_compute_tasks')).n;
  const rowsBefore=await db.get("SELECT jsonb_agg(to_jsonb(q) ORDER BY id) rows FROM state_compute_scheduling q");
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'fixture-cut',cut_at:new Date(now-1000).toISOString(),task_cutoff:cutoff,keep_boundary:[1,1000,'z'],source:{fixture:true}}));
  await db.run('SELECT compute_order_cut_protect(?::text,(?::text)::jsonb)','fixture-cut',JSON.stringify([by.known]));
  await db.run("UPDATE state_compute_scheduling SET lease='already-issued',leased_to='holder',lease_until='2999',leased_at=? WHERE id=?",new Date(now-500).toISOString(),by.preparing);
  assert.equal((await db.get('SELECT kind FROM relation_compute_order_cut_protection WHERE question=?',by.preparing)).kind,'issued');
  const digest=createHash('sha256').update(by.known).digest('hex');
  await assert.rejects(db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','fixture-cut',1,digest),/completed preparing capture required/);
  await db.run('SELECT compute_order_cut_capture(?::text)','fixture-cut');
  await assert.rejects(db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','fixture-cut',1,'0'.repeat(64)),/manifest changed/);
  assert.equal((await db.get('SELECT count(*) n FROM record_compute_order_cut_activations')).n,0);
  const beforeActivation=await db.all('SELECT to_jsonb(q) data FROM state_compute_scheduling q ORDER BY id');
  await db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','fixture-cut',1,digest);
  assert.deepEqual(await db.all('SELECT to_jsonb(q) data FROM state_compute_scheduling q ORDER BY id'),beforeActivation);
  assert.deepEqual(await raw(by.cancel),original,'activation rewrites a cancelled scheduling row');
  assert.equal(await db.get('SELECT id FROM state_compute_work WHERE id=?',by.cancel),null);
  const live=(await db.all('SELECT id FROM state_compute_effective_open ORDER BY id')).map(q=>q.id);
  assert.deepEqual(live.sort(),[by.prefix,by.known,by.preparing,by.prestage,by['prestage-prev-only']].sort());
  assert.equal((await db.get("SELECT count(*) n FROM state_compute_effective_work WHERE state='open'")).n,5);
  assert.equal((await db.get('SELECT count(*) n FROM state_compute_scheduling')).n,8);
  assert.equal((await db.get('SELECT count(*) n FROM state_board_pending_work')).n,10);
  assert.equal((await db.get('SELECT count(*) n FROM state_board_effective_pending_work')).n,6);
  assert.deepEqual((await db.all('SELECT build_id FROM state_board_effective_pending_work ORDER BY build_id')).map(x=>x.build_id),
    ['business-known','business-prefix','business-preparing','business-prestage','business-prestage-prev-only','new-arrival']);
  console.log('ok cut activation changes only metadata; effective work and business counts preserve prefix/protection and omit unregistered old obligations');
  const witness={device:'dispatch-control',owner:'dispatch-control',net:'dispatch-control',lanes:1};
  const single=await f.next(env,witness,'e',now);assert.equal(single.question,by.prefix);
  await db.run("UPDATE state_compute_work SET lease=NULL,lease_until=NULL,leased_to=NULL WHERE leased_to='dispatch-control'");
  const pack=await f.nextPack(env,witness,'e',{n:10,lease_ms:60000},now);
  assert.deepEqual(pack.questions.map(q=>q.question).sort(),[by.prefix,by.known,by['prestage-prev-only']].sort());
  await db.run("UPDATE state_compute_work SET lease=NULL,lease_until=NULL,leased_to=NULL WHERE leased_to='dispatch-control'");
  const claimed=await f.claim(env,{...witness,official:true},{states:['open'],max:10},'e',now);
  assert.deepEqual(claimed.map(q=>q.question).sort(),[by.prefix,by.known,by['prestage-prev-only']].sort());
  await db.run("UPDATE state_compute_work SET lease=NULL,lease_until=NULL,leased_to=NULL WHERE leased_to='dispatch-control'");
  assert.equal(await f.renew(env,'bad','bad',now),false);
  assert.deepEqual(await f.renewRow(env,'bad','bad',by.cancel,now),{held:false});
  assert.equal(await f.renew(env,'holder','already-issued',now),true);
  assert.equal(await f.renew(env,'before-stage','prestage-issued',now),true);
  assert.deepEqual(await f.renewRow(env,'previous-holder','prestage-prev',by.prestage,now),{held:true});
  assert.equal((await db.get('SELECT kind FROM relation_compute_order_cut_protection WHERE question=?',by.prestage)).kind,'issued');
  assert.deepEqual(await f.renewRow(env,'previous-only-holder','previous-only',by['prestage-prev-only'],now),{held:true});
  assert.equal((await db.get('SELECT kind FROM relation_compute_order_cut_protection WHERE question=?',by['prestage-prev-only'])).kind,'issued');
  console.log('ok pre-stage current and previous-only credentials omitted by the old manifest are independently captured and remain renewable');
  console.log('ok ordinary single/pack/official claim dispatch only effective FIFO work; held renewal remains available');


  const took=await db.run("UPDATE state_compute_work SET lease='bad',leased_to='bad',lease_until='2999',leased_at=? WHERE id=? AND state='open'",new Date(now).toISOString(),by.cancel);
  assert.equal(took.meta?.changes??took.changes??0,0);
  await db.run("UPDATE state_compute_scheduling SET lease='bad-direct',leased_to='bad',lease_until='2999',leased_at=? WHERE id=? AND state='open'",new Date(now).toISOString(),by.cancel);
  assert.equal((await raw(by.cancel)).lease,null,'new lease manufactured a protection exemption');
  assert.equal(await db.get('SELECT question FROM relation_compute_order_cut_protection WHERE question=?',by.cancel),null);
  assert.equal((await db.get('SELECT count(*) n FROM record_compute_order_cancellations WHERE question=?',by.cancel)).n,1);
  console.log('ok view and direct scheduling CAS cannot issue cancelled work or manufacture protection');

  const replayBefore=await raw(by.replay),tasksBefore=(await db.get('SELECT count(*) n FROM entity_compute_tasks')).n;
  await f.demand(env,asks.find(a=>a.ref==='replay'),'e',now);
  assert.equal((await raw(by.replay)).task_id,replayBefore.task_id);
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_tasks')).n,tasksBefore);
  assert.equal(await db.get('SELECT id FROM state_compute_work WHERE id=?',by.replay),null);
  assert.equal((await db.get("SELECT compute_open_task(?::text,'pipeline_reissue','e',?::text,true) value",by.replay,new Date(now).toISOString())).value,null);
  console.log('ok same old producer/ref/trigger replay and explicit round recovery cannot reopen a cut task');

  const user={...asks.find(a=>a.ref==='replay'),ref:'explicit-new-user',trigger_id:'explicit-new-user',zone:0};
  await f.demand(env,user,'e',now+1);
  const reopened=await db.get('SELECT * FROM state_compute_work WHERE id=?',by.replay);
  assert(reopened&&reopened.task_id!==replayBefore.task_id&&reopened.zone===0);
  assert.equal((await db.get("SELECT state FROM relation_task_consumers WHERE producer='sweep' AND ref='replay'")).state,'withdrawn');
  assert.equal((await db.get("SELECT state FROM relation_task_consumers WHERE producer='sweep' AND ref='explicit-new-user'")).state,'live');
  assert.equal((await db.get('SELECT count(*) n FROM relation_compute_task_sources WHERE task_id=?',reopened.task_id)).n,1);
  assert(await db.get('SELECT 1 FROM relation_compute_task_sources WHERE task_id=?',replayBefore.task_id));
  await f.demand(env,{...asks.find(a=>a.ref==='replay'),zone:0},'e',now+3);
  assert.equal((await db.get("SELECT state FROM relation_task_consumers WHERE producer='sweep' AND ref='replay'")).state,'withdrawn');
  console.log('ok a new explicit user source gets a new round without reviving the old consumer or losing its source; old intent replay stays cancelled');

  const late=await raw(by.late);
  await db.run("SELECT factory_answer(?::text,'e','late-input','late-owner','late-net',1,'{\"score\":1,\"work\":1}','{}',1,NULL,NULL,NULL,?::text,?::bigint)",by.late,new Date(now).toISOString(),late.task_id);
  assert.equal((await db.get('SELECT count(*) n FROM record_candidate_answers WHERE question=?',by.late)).n,1);
  assert.equal(await db.get('SELECT id FROM state_compute_work WHERE id=?',by.late),null);
  await f.settle(env,by.late,'e',now+2);
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_results WHERE computation=?',by.late)).n,0);
  assert.equal((await db.get('SELECT coalesce(sum(work),0) n FROM entity_compute_devices')).n,0);
  assert.equal((await db.get('SELECT factory_forget_past(?::text,?::text,?::text,?::bigint) n','2999','[]','[]',100)).n,0);
  assert(await raw(by.late));
  assert.equal((await db.get('SELECT count(*) n FROM state_compute_scheduling')).n,8);
  console.log('ok late imported input stays stored without resurrecting work, accepting a result or creating credit; GC retains physical history');
  const httpEnv={...env,AUTH_SECRET:'order-cut-fixture-only',FACTORY_TOKEN:'fixture-token',ASSETS:{fetch:async()=>Response.json({engine:'e'})}};
  const before=async()=>db.get('SELECT (SELECT count(*) FROM entity_compute_tasks) tasks,(SELECT count(*) FROM record_candidate_answers) answers,(SELECT count(*) FROM entity_compute_results) results,(SELECT count(*) FROM record_build_submissions) inputs,(SELECT coalesce(sum(work),0) FROM entity_compute_devices) credit');
  const prior=await before();
  const imported=await officialRoute(new Request('https://fixture.invalid/api/tasks/official/answer',{method:'POST',headers:{authorization:'Bearer fixture-token','content-type':'application/json'},
    body:JSON.stringify({witness:'official:fixture',engine:'e',producers:['sweep'],answers:[{ref:'late',question:by.late,result:{score:1,metric:'kpm'},work:1}]})}),httpEnv,'/api/tasks/official/answer');
  assert.deepEqual((await imported.json()).states,[null]);
  const carried=Buffer.from(JSON.stringify({scope:'board-builds',question:by.late,weapon:'braton',engine:'e',expires_at:new Date(Date.now()+3600000).toISOString(),initiator:'system',family_source:null})).toString('base64url');
  const receipt=carried+'.'+await hmac(httpEnv.AUTH_SECRET+':board-harvest',carried);
  const harvested=await worker.fetch(new Request('https://fixture.invalid/api/board/submit',{method:'POST',headers:{'content-type':'application/json'},
    body:JSON.stringify({builds:[{weapon:'braton',mode:'base',mods:[],arcanes:[],evolutions:[]}],harvest:{receipt}})}),httpEnv);
  assert.equal(harvested.ok,true);const after=await before();
  assert.deepEqual(after,{...prior,inputs:prior.inputs+1});
  assert.equal(await db.get('SELECT id FROM state_compute_work WHERE id=?',by.late),null);
  console.log('ok actual tokenless official HTTP import is ignored; valid signed harvest stores input without work, result or credit');
  for(const id of ['before-stage','previous-holder'])await db.run('INSERT INTO entity_compute_devices(id,seen,work) VALUES(?,?,0)',id,'fixture');
  for(const [device,lease] of [['before-stage','prestage-issued'],['previous-holder','prestage-prev']])
    assert.deepEqual((await f.answerPack(env,{device,owner:device,net:device},{lease,engine:'e',late:true,answers:[{question:by.prestage,result:{score:2},work:1}]},now+10)).taken,[by.prestage]);
  await f.settle(env,by.prestage,'e',now+11);
  await f.repay(env,now+12);
  assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',by.prestage)).state,'fact');
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,2);
  console.log('ok pre-stage current and previous holders return after activation, verify the original round and receive existing credit');

  const engine='a'.repeat(12),inventoryNow=Date.parse((await db.get('SELECT compute_order_cut_epoch() value')).value)+1000,stamp=new Date(inventoryNow).toISOString();
  await db.run("INSERT INTO state_factory_snapshot(id,at,engine,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit) VALUES(1,?,?,0,0,0,0,0,0,?,'fixture')",stamp,engine,stamp);
  const plan={v:2,started_at:new Date(inventoryNow-2000).toISOString(),source:'fixture',snapshot:'old',engine,rows:27000000,queued:27000000,orders:[{kind:'sweep',total:27000000}],verbs:{simulate:27000000}};
  const write=inventoryWrite(plan,inventoryNow);await db.run(write.sql,...write.params);
  assert.deepEqual(await readInventoryState(db,inventoryNow),{fresh:null,lastComplete:null});
  console.log('ok pre-cut inventory publication cannot reintroduce cached cancelled counts');
  return {by,cutoff,rowsBefore};
}

export async function orderCutWorkflowCases(db) {
  await db.exec(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
  const now=Date.now(),old=now-86400000,at=new Date(old).toISOString(),env={LIBRARY:db};
  const input=async key=>prepareIntakeBuild({id:key.repeat(32),at,initiator:'anonymous',from:['cut-workflow-'+key],record:{weapon:'braton',mods:['serration']},
    flow:{schema:1,scope:{boards:[{id:'standard_single_target'}]},builds:[],templates:[{weapon:'braton',record:{weapon:'braton',mods:['serration']},runs:1,seed:key==='a'?1:2,duration:2}],
      patches:[{set:{},remove:[]}],nodes:Array.from({length:4},(_,i)=>({kind:'measure',identity:key.repeat(32),ruler:'standard_single_target',mode:'mode-'+i,template:0,patch:0}))}});
  for(const key of ['a','b']) {
    const item=await input(key);await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([item]));
    const job=(await db.get("SELECT build_admission_claim('intake',?::text) job",new Date(now+60000).toISOString())).job;
    await db.run('SELECT build_admission_run(?::text,?::text,?::text,2,?::text)',job.kind,job.key,job.lease,new Date(now+60000).toISOString());
  }
  const flows=await db.all('SELECT id FROM entity_build_workflows ORDER BY build_id');
  const expansion=(await db.get('SELECT id FROM entity_build_workflow_expansions WHERE workflow=?',flows[0].id)).id;
  await db.run('SELECT build_workflow_admit_nodes(?::text,1,?::text)',expansion,new Date(now).toISOString());
  const nodes=await db.all('SELECT * FROM state_build_workflow_nodes WHERE workflow=? ORDER BY position',flows[0].id),issued=nodes[0].computation;
  await db.run("UPDATE state_compute_work SET lease='kept-child',leased_to='holder',lease_until='2999',leased_at=? WHERE id=?",at,issued);
  const cutoff=(await db.get('SELECT max(id) n FROM entity_compute_tasks')).n;
  await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'workflow-cut',cut_at:new Date(now+1000).toISOString(),task_cutoff:cutoff,keep_boundary:[3,nodes[0].queue_position,''],source:{fixture:true}}));
  await db.run('SELECT compute_order_cut_protect(?::text,(?::text)::jsonb)','workflow-cut',JSON.stringify([issued]));
  await db.run('SELECT compute_order_cut_capture(?::text)','workflow-cut');
  await db.run('SELECT compute_order_cut_activate(?::text,1,?::text)','workflow-cut',createHash('sha256').update(issued).digest('hex'));
  assert.equal((await db.get('SELECT compute_order_workflow_cancelled(?::text) value',flows[0].id)).value,false);
  assert.equal((await db.get('SELECT compute_order_workflow_cancelled(?::text) value',flows[1].id)).value,true);
  assert.equal((await db.get('SELECT count(*) n FROM state_build_effective_admission_work')).n,0);
  assert.equal((await db.get('SELECT build_workflow_admit_nodes(?::text,128,?::text) n',expansion,new Date(now).toISOString())).n,0);
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_tasks')).n,1);
  assert.equal((await db.get('SELECT build_workflow_wanted(?::text,?::text) value',flows[0].id+'|'+nodes[0].id,issued)).value,true);
  await db.run('SELECT build_workflow_finish(?::text,?::text)',flows[1].id,new Date(now).toISOString());
  assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',flows[1].id)).state,'cancelled');
  assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',flows[0].id)).state,'active');
  console.log('ok captured pending tail cannot register or revive; a genuine issued child preserves its active parent while the cancelled parent reports cancelled');
}

if(process.argv[1]===new URL(import.meta.url).pathname) {
  const {testDb}=await import('./pg/test_db.mjs');const db=await testDb('library');
  try {
    if(process.argv.includes('--break-prestage-capture')) {
      const source=readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8');
      const original=source.match(/CREATE OR REPLACE FUNCTION compute_order_cut_capture\([^]*?END \$\$;/)[0];
      const broken=original.replace(/  INSERT INTO relation_compute_order_cut_protection[^]*?ON CONFLICT DO NOTHING;/,'  PERFORM 1;');
      assert.notEqual(broken,original);await db.exec(broken);
    }
    await orderCutCases(db);
  } finally {await db.close();}
  const flows=await testDb('library');try {await orderCutWorkflowCases(flows);}finally{await flows.close();}
}
