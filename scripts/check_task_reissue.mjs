// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {randomBytes} from 'node:crypto';
import {factory,canon} from '../worker/tasks.js';
import {officialRoute} from '../worker/factory.js';
import {pgDb,placeholders,postgresClient} from '../worker/db.js';
import {taskReissuePage,applyTaskReissue,taskReissueSchemaSql} from './task_reissue.mjs';

async function openDatabase() {
  const url=process.env.WFSIM_TASK_REISSUE_TEST_URL;
  if(!url)return (await import('./pg/test_db.mjs')).testDb('library');
  assert(['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname));
  const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
  const schema='task_reissue_'+randomBytes(6).toString('hex');
  const sql=postgres(url,{max:4,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000,lock_timeout:2000},
    types:{bigint:{to:20,from:[20,1700],parse:Number,serialize:String}}});
  await sql.unsafe(`CREATE SCHEMA ${schema}`);
  try {
    const ddl=['schema','library','sync','inventory','build_workflows','board_admission']
      .map(name=>readFileSync(new URL(`../worker/pg/${name}.sql`,import.meta.url),'utf8')).join('\n');
    await sql.unsafe(ddl);
    const client=postgresClient(sql);
    return Object.assign(pgDb(client),{
      exec:text=>sql.unsafe(text),
      run:async(text,...args)=>(await client.query(placeholders(text),args)).affected,
      get:async(text,...args)=>(await client.query(placeholders(text),args)).rows[0] || null,
      all:async(text,...args)=>(await client.query(placeholders(text),args)).rows,
      close:async()=>{try{await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`);}finally{await sql.end();}},
    });
  }catch(error){await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`);await sql.end();throw error;}
}
const db=await openDatabase(),env={LIBRARY:db,FACTORY_TOKEN:'fixture'};
const now=Date.parse('2026-11-15T01:00:00.000Z'),at=new Date(now).toISOString();
const who=device=>({device,owner:device,net:device});
const f=factory({verbs:{simulate:{cap:2,pack:true,lease_ms:60000,canon:(r,work)=>canon({...r,work})}},
  producers:{user_calculation:{on_fact_writes:(env,d)=>[env.LIBRARY.prepare('INSERT INTO received VALUES(?) ON CONFLICT DO NOTHING').bind(d.ref)]}},spot:0,recheck:0,random:()=>1});
const make=async(ref,value)=>{
  const id=await f.demand(env,{producer:'user_calculation',ref,verb:'simulate',request:{value},due_in_ms:60000},'e',now);
  await db.run(`INSERT INTO entity_user_calculation_requests(id,task,owner,kind,engine,question,state,created_at,expires_at)
    VALUES(?,?,'fixture','simulation','e',?,'waiting',?,'2026-12-01')`,ref,ref,id,at);
  return id;
};
const give=async(device)=>{
  const task=await f.next(env,who(device),'e',now);
  assert(task);
  await f.answerPack(env,who(device),{lease:task.lease,engine:'e',answers:[{question:task.question,result:{score:7,metric:'kpm'},work:10}]},now);
  await f.runStage(env,'verification',now);
  return task;
};
const reissue=(id,task,op='cutover')=>db.prepare('SELECT compute_reissue_task(?1::text,?2::bigint,?3::text,?4::text,?5::text) AS task_id')
  .bind(id,task,op,'e',at).first();
const official=async(answer)=>{
  const response=await officialRoute(new Request('https://example/api/tasks/official/answer',{method:'POST',
    headers:{authorization:'Bearer fixture','content-type':'application/json'},
    body:JSON.stringify({witness:'official:old',engine:'e',producers:['user_calculation'],answers:[answer]})}),env,'/api/tasks/official/answer');
  return response.json();
};
try {
  await db.exec('CREATE TABLE received(ref text PRIMARY KEY)');
  await db.run("INSERT INTO state_factory_parameters(name,value,set_by,set_at) VALUES('settle_async',1,'fixture',?)",at);
  delete db.settleAtOnce;
  for(const id of ['a','b','c','d'])await db.run('INSERT INTO entity_compute_devices(id,seen,work) VALUES(?,?,0)',id,at);
  const accepted=await make('accepted',1);
  await give('a');await give('b');
  await f.runStage(env,'delivery',now);await f.runStage(env,'contribution',now);
  const acceptedBefore=await db.get('SELECT * FROM state_compute_work WHERE id=?',accepted);
  const resultsBefore=await db.all('SELECT * FROM entity_compute_results ORDER BY id');
  const paidBefore=(await db.get('SELECT sum(work) n FROM entity_compute_devices')).n;
  await db.exec('DROP FUNCTION compute_reissue_task(text,bigint,text,text,text); DROP TABLE record_compute_task_reissues;');
  await db.exec(taskReissueSchemaSql());await db.exec(taskReissueSchemaSql());
  assert.deepEqual(await db.get('SELECT * FROM state_compute_work WHERE id=?',accepted),acceptedBefore);
  assert.deepEqual(await db.all('SELECT * FROM entity_compute_results ORDER BY id'),resultsBefore);
  await assert.rejects(reissue(accepted,acceptedBefore.task_id),/no longer unfinished/);
  assert.deepEqual(await db.get('SELECT * FROM state_compute_work WHERE id=?',accepted),acceptedBefore);
  assert.deepEqual(await db.all('SELECT * FROM entity_compute_results ORDER BY id'),resultsBefore);
  console.log('ok: accepted results, proof selection and paid work survive retirement refusal');

  const id=await make('unfinished',2);
  await give('a');
  const old=await f.next(env,who('b'),'e',now);
  assert.equal(old.question,id);
  await db.run("UPDATE state_compute_scheduling SET lease_prev='older',leased_to_prev='a',lease_prev_until='2026-12-01',required_engine='e',zone=NULL,zone_position=NULL WHERE id=?",id);
  const before=await db.get('SELECT * FROM state_compute_work WHERE id=?',id);
  await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES('wf-build',?,'{}')",at);
  await db.exec(`INSERT INTO entity_build_workflow_scopes VALUES('scope','{}');
    INSERT INTO entity_build_workflows VALUES('flow','wf-build','scope-key','intake','scope','{}','${at}');
    INSERT INTO state_build_workflows(id,build_id,scope_key,state,zone,due_at) VALUES('flow','wf-build','scope-key','active',0,'${at}');
    INSERT INTO entity_build_workflow_expansions(id,workflow,plan,created_at) VALUES('expansion','flow','{}','${at}');`);
  await db.run(`INSERT INTO state_build_workflow_nodes(workflow,id,expansion,definition,state,computation,task_id,retry_at)
    VALUES('flow','node','expansion','{}','queued',?,?,?)`,id,before.task_id,at);
  await db.run(`INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
    baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at,task_id)
    VALUES('review','review',?1,'wf-build','ruler','base','e',?2,'{}','waiting',1,'kpm',0,'old',?1,?1,?1,?3)`,at,id,before.task_id);
  const consumersBefore=await db.all('SELECT * FROM relation_task_consumers WHERE question=? ORDER BY producer,ref',id);
  const answersBefore=await db.all('SELECT * FROM record_candidate_answers WHERE question=? ORDER BY id',id);
  await db.run('DELETE FROM relation_compute_task_sources WHERE task_id=?',before.task_id);
  await db.run("INSERT INTO state_pending_verifications(question,engine,queued_at) VALUES(?,'e',?) ON CONFLICT DO NOTHING",id,at);
  await db.exec(`CREATE OR REPLACE FUNCTION check_reissue_rollback() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN IF NEW.operation='fail' THEN RAISE EXCEPTION 'injected reissue interruption'; END IF; RETURN NEW; END $$;
    CREATE TRIGGER check_reissue_rollback BEFORE INSERT ON record_compute_task_reissues FOR EACH ROW EXECUTE FUNCTION check_reissue_rollback();`);
  await assert.rejects(reissue(id,before.task_id,'fail'),/injected reissue interruption/);
  assert.deepEqual(await db.get('SELECT * FROM state_compute_work WHERE id=?',id),before);
  assert.equal((await db.get('SELECT count(*) n FROM relation_compute_task_sources WHERE task_id=?',before.task_id)).n,0);
  assert.equal((await db.get("SELECT task_id FROM state_build_workflow_nodes WHERE workflow='flow'")).task_id,before.task_id);
  console.log('ok: a failure after round replacement rolls back sources, scheduling and workflow bindings');
  if(process.argv.includes('--broken-lease-fence'))await db.exec(`CREATE OR REPLACE FUNCTION check_broken_reissue() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN UPDATE state_compute_scheduling SET lease=NEW.snapshot->'scheduling'->>'lease',
      leased_to=NEW.snapshot->'scheduling'->>'leased_to',lease_until=NEW.snapshot->'scheduling'->>'lease_until' WHERE id=NEW.computation;
      RETURN NEW; END $$; CREATE TRIGGER check_broken_reissue AFTER INSERT ON record_compute_task_reissues FOR EACH ROW EXECUTE FUNCTION check_broken_reissue();`);
  const {task_id:replacement}=await reissue(id,before.task_id);
  assert.notEqual(replacement,before.task_id);
  const current=await db.get('SELECT * FROM state_compute_work WHERE id=?',id);
  for(const field of ['zone','zone_position','due_at','waits','producer','initiator','required_capability','required_engine','request'])assert.deepEqual(current[field],before[field],field);
  for(const field of ['lease','leased_to','lease_until','lease_prev','leased_to_prev','lease_prev_until'])assert.equal(current[field],null,field);
  assert.deepEqual(await db.all('SELECT * FROM relation_task_consumers WHERE question=? ORDER BY producer,ref',id),consumersBefore);
  assert.deepEqual(await db.all('SELECT * FROM record_candidate_answers WHERE question=? ORDER BY id',id),answersBefore);
  assert.equal((await db.get('SELECT count(*) n FROM relation_compute_task_sources WHERE task_id=?',replacement)).n,1);
  assert.equal((await db.get('SELECT count(*) n FROM state_pending_verifications WHERE question=?',id)).n,0);
  assert.equal((await db.get("SELECT task_id FROM state_build_workflow_nodes WHERE workflow='flow'")).task_id,replacement);
  assert.equal((await db.get("SELECT task_id FROM entity_measurement_reviews WHERE id='review'")).task_id,replacement);
  assert.equal((await reissue(id,before.task_id)).task_id,replacement);
  await assert.rejects(reissue(id,before.task_id,'another'),/different task reissue/);
  assert.equal((await db.get('SELECT count(*) n FROM record_compute_task_reissues')).n,1);
  console.log('ok: immutable source capture, order, old evidence and idempotent old-to-new mapping are retained');

  const stale={question:id,result:{score:999,metric:'kpm'},work:999};
  assert.deepEqual((await f.answerPack(env,who('b'),{lease:old.lease,engine:'e',late:true,answers:[stale]},now)).taken,[]);
  assert.deepEqual((await f.answerPack(env,who('a'),{lease:'older',engine:'e',late:true,answers:[stale]},now)).taken,[]);
  assert.deepEqual(await f.renewRow(env,'b',old.lease,id,now),{held:false});
  assert.equal(await f.answerOfficial(env,{device:'official:old',official:true},{...stale,engine:'e'},now),null);
  const body={ref:'unfinished',result:stale.result,work:999};
  assert.deepEqual((await official(body)).states,[null]);
  assert.deepEqual((await official({...body,question:id,task_id:before.task_id,lease:old.lease})).states,[null]);
  await assert.rejects(db.prepare('SELECT factory_answer(?1::text,?2::text,?3::text,?3::text,?3::text,0,?4::text,?4::text,999,NULL,NULL,NULL,?5::text,?6::bigint)')
    .bind(id,'e','old-racer',canon(stale.result),at,before.task_id).run(),/changed before candidate reception/);
  await assert.rejects(db.prepare(`INSERT INTO record_candidate_answers(question,engine,device,owner,net,canon,result,work,at)
    VALUES(?1::text,'e','legacy-adopter','legacy-adopter','',?2::text,?2::text,999,?3::text)`)
    .bind(id,canon(stale.result),at).run(),/requires a captured task/);
  await assert.rejects(db.prepare("SELECT compute_accept_result(?1::text,?2::bigint,'open','fact',?3::text,'e',?4::text,0,'[]')")
    .bind(id,before.task_id,canon(stale.result),at).run(),/changed before result acceptance/);
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,paidBefore);
  console.log('ok: old current/previous leases, renewal, HTTP, internal official and in-flight SQL cannot settle the new round');

  await give('c');await give('d');
  assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',id)).state,'fact');
  assert.equal((await db.get("SELECT count(*) n FROM received WHERE ref='unfinished'")).n,0);
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,paidBefore);
  await f.runStage(env,'contribution',now);await f.runStage(env,'contribution',now);
  await f.runStage(env,'delivery',now);await f.runStage(env,'delivery',now);
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,paidBefore+20);
  assert.equal((await db.get("SELECT count(*) n FROM received WHERE ref='unfinished'")).n,1);
  assert.equal((await db.get('SELECT count(*) n FROM record_candidate_answers WHERE question=?',id)).n,3);
  assert.deepEqual(await db.get('SELECT * FROM state_compute_work WHERE id=?',accepted),acceptedBefore);
  console.log('ok: reissued work uses independent verification, delivery and exactly-once credit');

  const release=(body)=>officialRoute(new Request('https://example/api/tasks/official/release',{method:'POST',
    headers:{authorization:'Bearer fixture','content-type':'application/json'},body:JSON.stringify({witness:'official:reused',...body})}),env,'/api/tasks/official/release');
  const released=await make('release',8);
  const releaseBefore=await db.get('SELECT task_id FROM state_compute_work WHERE id=?',released);
  await db.run("UPDATE state_compute_scheduling SET lease='old-release',leased_to='official:reused',lease_until='2026-12-01' WHERE id=?",released);
  const releaseNew=(await reissue(released,releaseBefore.task_id,'release')).task_id;
  await db.run("UPDATE state_compute_scheduling SET lease='new-release',leased_to='official:reused',lease_until='2026-12-01' WHERE id=?",released);
  const unrelated=await make('release-other',9);
  await db.run("UPDATE state_compute_scheduling SET lease='other-release',leased_to='official:reused',lease_until='2026-12-01' WHERE id=?",unrelated);
  const oldClaim={question:released,task_id:releaseBefore.task_id,lease:'old-release'};
  assert.equal((await release({})).status,400);
  assert.equal((await release({claims:[oldClaim]})).status,200);
  assert.equal((await db.get('SELECT lease FROM state_compute_work WHERE id=?',released)).lease,'new-release');
  assert.equal((await release({claims:[{question:released,task_id:releaseNew,lease:'new-release'},{}]})).status,400);
  assert.equal((await db.get('SELECT lease FROM state_compute_work WHERE id=?',released)).lease,'new-release');
  assert.equal((await release({claims:[{...oldClaim,task_id:releaseNew}]})).status,200);
  assert.equal((await db.get('SELECT lease FROM state_compute_work WHERE id=?',released)).lease,'new-release');
  assert.equal((await release({claims:[{question:released,task_id:releaseNew,lease:'new-release'}]})).status,200);
  assert.equal((await db.get('SELECT lease FROM state_compute_work WHERE id=?',released)).lease,null);
  assert.equal((await db.get('SELECT lease FROM state_compute_work WHERE id=?',unrelated)).lease,'other-release');
  assert.equal((await release({claims:[]})).status,200);
  console.log('ok: stale cleanup cannot release a replacement lease even with the same official witness');
  await db.run("UPDATE relation_task_consumers SET state='withdrawn' WHERE question=?",released);
  await db.run("UPDATE state_compute_scheduling SET state='withdrawn' WHERE id=?",released);
  await db.run("UPDATE relation_task_consumers SET state='withdrawn' WHERE question=?",unrelated);
  await db.run("UPDATE state_compute_scheduling SET state='withdrawn' WHERE id=?",unrelated);

  const orphan=await make('orphan',3);
  await db.run("DELETE FROM entity_user_calculation_requests WHERE id='orphan'");
  const orphanTask=(await db.get('SELECT task_id FROM state_compute_work WHERE id=?',orphan)).task_id;
  await assert.rejects(reissue(orphan,orphanTask),/traceable live sources/);
  assert.equal((await db.get('SELECT task_id FROM state_compute_work WHERE id=?',orphan)).task_id,orphanTask);
  const one=await make('page-one',4),two=await make('page-two',5),three=await make('page-three',6);
  const cutoff=(await db.get('SELECT max(id) n FROM entity_compute_tasks')).n;
  await make('outside-cut',7);
  const seen=[];let after=null;
  do {
    const page=await taskReissuePage(db,{cutoff,after,limit:1});
    if(!page.entries.length)break;
    seen.push(page.entries[0].id);after=page.after;
    if(page.entries[0].traced)await applyTaskReissue(db,page.entries[0],{operation:'pages',engine:'e',at});
    if(!page.has_more)break;
  }while(true);
  assert.deepEqual(new Set(seen),new Set([orphan,one,two,three]));
  assert.equal(seen.length,4);
  assert.equal((await db.get('SELECT sum(work) n FROM entity_compute_devices')).n,paidBefore+20);
  await assert.rejects(taskReissuePage(db,{cutoff,limit:0}),/invalid/);
  console.log('ok: bounded pages preserve order across replacements and exclude new arrivals; untraceable tasks remain explicit');
} finally {await db.close();}
