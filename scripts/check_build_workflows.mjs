// SPDX-License-Identifier: AGPL-3.0-or-later
// Exercise captured intent, shared physical work and the complete-flow hold.
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
import { readFileSync } from 'node:fs';
import { testDb } from './pg/test_db.mjs';
import { prepareIntakeBuild } from './build_batches.mjs';
import { factory, canon } from '../worker/tasks.js';
import { boardReviewsRoute } from '../worker/board_reviews.js';
import { BUILD_WORKFLOW_PRODUCER } from '../worker/build_workflows.js';
import { syncRoute } from '../worker/sync.js';
import { setDial } from '../worker/dials.js';

export async function workflowProtocolCases() {
  for(const proc of [0,1]) {
    const db=await testDb('library'),env={LIBRARY:db,ASSETS:{fetch:async()=>Response.json({engine:'e'})}};
    try {
      await setDial(env,'sync_proc',proc,'check');
      const request={weapon:'braton',mods:['serration'],runs:2,seed:123,duration:2};
      const input=await prepareIntakeBuild({id:'b'.repeat(32),at:new Date().toISOString(),record:{weapon:'braton',mods:['serration']},
        system_source:{id:'protocol-fixture',reason:'explicit protocol fixture'},zone:0,
        flow:{schema:1,scope:{boards:[{id:'standard_single_target'}]},builds:[],templates:[request],patches:[{set:{},remove:[]}],
          nodes:[{kind:'measure',identity:'b'.repeat(32),ruler:'standard_single_target',mode:'base',template:0,patch:0}]}});
      await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
      assert.equal((await db.get('SELECT zone FROM state_build_workflows')).zone,0,'system authority applies to a newly registered build too');
      const sync=async(capabilities,digit)=>{
        const response=await syncRoute(new Request('https://wfsim.test/api/board/sync',{method:'POST',headers:{'content-type':'application/json'},
          body:JSON.stringify({verifier:digit.repeat(24),engine:'e',protocol:8,capabilities,lanes:2,want_ms:1000,
            consent:{v:3,at:new Date().toISOString()}})}),env);
        assert.equal(response.status,200);return response.json();
      };
      const old=await sync([],'1');assert.ok(!old.work && !old.pack);
      const capable=await sync(['fixed-board-input-v1'],'2');
      const task=capable.pack?.tasks?.[0] || capable.work;assert.ok(task);
      assert.deepEqual(task.fight,request);assert.equal(task.identity,input.id);
    } finally {await db.close();}
  }
  console.log('build workflows: both protocol 8 paths gate capabilities and deliver the frozen fight');
}

export async function buildWorkflowCases(db) {
  const env={LIBRARY:db},at='2026-11-11T00:00:00.000Z';
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,canon:(r,w)=>canon({...r,work:w})},
    optimize_report:{cap:3,lease_ms:60000,canon}},producers:{build_workflow:BUILD_WORKFLOW_PRODUCER,system:{on_fact:async()=>{}}}});
  if(process.argv.includes('--break-completion-hold')) await db.run(`CREATE OR REPLACE FUNCTION compute_round_occupied(text,bigint)
    RETURNS boolean LANGUAGE sql STABLE AS $$ SELECT EXISTS(SELECT 1 FROM relation_task_consumers WHERE question=$1 AND state='live') $$`);
  if(process.argv.includes('--break-scope-gate')) {
    const sql=readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
    await db.exec(sql.match(/CREATE OR REPLACE FUNCTION build_intake_batch[^]*?END \$\$;/)[0].replace('IF inserted IS NOT NULL THEN','IF true THEN'));
  }
  const fixture=async(id,suffix='',extra={})=>prepareIntakeBuild({id,at,record:{weapon:'braton',mods:['serration']},from:['inbox:'+id],initiator:'anonymous',
    flow:{schema:1,scope:{boards:[{id:'one'+suffix},{id:'two'}],modes:{braton:['base']}},builds:[],
      templates:[{weapon:'braton',runs:2,seed:100+id.length}],patches:[{set:{},remove:[]},{set:{seed:200+id.length},remove:[]}],
      nodes:[{kind:'measure',identity:id,ruler:'one'+suffix,mode:'base',template:0,patch:0},
        {kind:'measure',identity:id,ruler:'two',mode:'base',template:0,patch:1}]},...extra});
  const intake=async input=>db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
  const nodes=async id=>db.all(`SELECT n.* FROM state_build_workflow_nodes n JOIN entity_build_workflows f ON f.id=n.workflow WHERE f.build_id=? ORDER BY n.position`,id);
  const count=async id=>(await db.get('SELECT count(*) n FROM entity_build_workflows WHERE build_id=?',id)).n;
  const root=await fixture('workflow-root');
  await Promise.all(Array.from({length:8},()=>intake(root)));
  assert.equal(await count(root.id),1);
  const original=await nodes(root.id);assert.equal(original.length,2);
  assert.equal((await db.get('SELECT zone FROM state_compute_work WHERE id=?',original[0].computation)).zone,3);
  await intake(await fixture(root.id,'-future'));
  assert.equal(await count(root.id),1,'an already stored build does not backfill a new board');
  const invalid=await fixture('invalid-plan');delete invalid.flow.nodes[0].computation;
  await assert.rejects(intake(invalid),/canonical computation/);
  assert.equal(await db.get('SELECT id FROM entity_weapon_builds WHERE id=?',invalid.id),null,'entity and intent roll back together');
  const untraced=await fixture('untraced-system','',{system_source:{}});
  await assert.rejects(intake(untraced),/explicit source identity/);
  assert.equal(await db.get('SELECT id FROM entity_weapon_builds WHERE id=?',untraced.id),null);
  for(const id of ['system-batch-a','system-batch-b']) {
    const input=await fixture(id,'',{system_source:{id:'one-system-command'},zone:99});await intake(input);
    assert.equal(await count(id),1,'one command can independently target multiple builds');
    assert.equal((await db.get('SELECT zone FROM state_build_workflows WHERE build_id=?',id)).zone,99);
  }
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:original[0].computation,engine:'e',result:{score:123,metric:'dps'},work:1});
  assert.equal((await nodes(root.id))[0].state,'succeeded');
  assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',original[0].workflow)).state,'active');
  await f.demand(env,{producer:'system',ref:'hold-check',verb:'simulate',request:root.flow.templates[0],payload:{reason:'explicit check'},force:true,zone:0,due_in_ms:0},'new-engine');
  assert.equal((await db.get('SELECT task_id FROM state_compute_work WHERE id=?',original[0].computation)).task_id,original[0].task_id,
    'a completed child stays occupied until its entire workflow finishes');
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:original[1].computation,engine:'e',result:{score:456,metric:'dps'},work:1});
  assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',original[0].workflow)).state,'succeeded');
  await f.demand(env,{producer:'system',ref:'later-round',verb:'simulate',request:root.flow.templates[0],payload:{reason:'later explicit check'},force:true,zone:0,due_in_ms:0},'new-engine');
  assert.notEqual((await db.get('SELECT task_id FROM state_compute_work WHERE id=?',original[0].computation)).task_id,original[0].task_id);
  const aliases=await fixture('workflow-aliases');aliases.flow.nodes[1].patch=0;await prepareIntakeBuild(aliases);await intake(aliases);
  const aliasNodes=await nodes(aliases.id);assert.equal(aliasNodes[0].computation,aliasNodes[1].computation);
  assert.equal(aliasNodes[0].task_id,aliasNodes[1].task_id);
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:aliasNodes[0].computation,engine:'e',result:{score:12,metric:'dps'},work:1});
  assert.ok((await nodes(aliases.id)).every(n=>n.state==='succeeded'));
  const opt=await fixture('workflow-optimizer');opt.flow.nodes=[{kind:'optimize',weapon:'braton',request:{weapon:'braton',runs:1,starts:[]},context:{weapon:'braton'}}];
  await prepareIntakeBuild(opt);await intake(opt);const [optNode]=await nodes(opt.id);
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:optNode.computation,engine:'e',result:{report:{results:[{mods:['serration'],mode:'base'}]}},work:1});
  assert.equal((await nodes(opt.id))[0].state,'awaiting_intake');
  const inbox=await db.get('SELECT * FROM state_build_inbox_inputs WHERE family_source=?','workflow|'+optNode.workflow+'|'+optNode.id);
  assert.deepEqual(JSON.parse(inbox.workflow_scope),opt.flow.scope);
  const derived=await fixture('workflow-derived','',{derived:true,flow_source:{workflow:optNode.workflow,node:optNode.id}});
  derived.flow.scope=opt.flow.scope;await intake(derived);
  assert.equal(await count(derived.id),0,'derived output advances its parent instead of restarting fanout');
  assert.equal((await nodes(opt.id))[0].state,'succeeded');
  const continued=await db.get('SELECT count(*) n FROM entity_build_workflow_expansions WHERE workflow=?',optNode.workflow);assert.equal(continued.n,2);
  await db.run('DELETE FROM record_build_submissions WHERE id=?',inbox.id);await f.redeliver(env);
  assert.equal(await db.get('SELECT id FROM record_build_submissions WHERE id=?',inbox.id),null,'retry cannot recreate a consumed derived inbox row');
  const corrected=await fixture('c'.repeat(32));corrected.flow.nodes=[{...corrected.flow.nodes[0],ruler:'standard_single_target'}];
  await prepareIntakeBuild(corrected);await intake(corrected);const [correctionNode]=await nodes(corrected.id);
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:correctionNode.computation,engine:'e',result:{score:10,metric:'dps'},work:1});
  env.ASSETS={fetch:async()=>Response.json({engine:'e'})};
  const observe=async()=>{
    const response=await boardReviewsRoute(new Request('https://wfsim.test/api/board/review',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({identity:corrected.id,
      ruler:'standard_single_target',mode:'base',engine:'e',observed_score:15,baseline_score:10,metric:'dps',request:corrected.flow.templates[0]})}),env,'/api/board/review');
    assert.equal(response.status,200);return response.json();
  };
  const review=await observe();const observationInbox=await db.get('SELECT * FROM record_build_submissions WHERE id=?','review:'+review.id);
  assert.ok(observationInbox);assert.equal(await count(corrected.id),1,'HTTP only captures the observation, not a second queue ingress');
  const invalidObservation={...JSON.parse(observationInbox.observation),valid:false};
  await intake({...corrected,observation:invalidObservation});assert.equal(await count(corrected.id),1,'an unmatching environment cannot trigger correction');
  await db.run("UPDATE entity_measurement_reviews SET created_at='2020-01-01',round_at='2020-01-01' WHERE id=?",review.id);
  const validReview=await observe();const validInbox=await db.get('SELECT * FROM record_build_submissions WHERE id=?','review:'+validReview.id);
  await intake({...corrected,observation:{...JSON.parse(validInbox.observation),valid:true}});
  assert.equal(await count(corrected.id),2);const latest=(await nodes(corrected.id)).at(-1);
  assert.equal((await db.get('SELECT zone FROM state_compute_work WHERE id=?',latest.computation)).zone,1);
  assert.notEqual(latest.task_id,correctionNode.task_id,'a completed flow releases its ID for sourced correction');
  assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',corrected.id)).score,10);
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:latest.computation,engine:'e',result:{score:15,metric:'dps'},work:1});
  assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',corrected.id)).score,15);
  assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',validReview.id)).state,'succeeded');
  const aliasNode=aliasNodes[0];await BUILD_WORKFLOW_PRODUCER.on_retract(env,{question:aliasNode.computation,payload:{workflow:aliasNode.workflow,node:aliasNode.id}});
  assert.ok((await db.get('SELECT retired_at FROM board_facts WHERE identity=? AND ruler=?',aliases.id,'one')).retired_at);
  assert.equal((await db.get("SELECT count(*) n FROM record_build_workflow_triggers WHERE source#>>'{system,reason}'='retracted measurement proof'")).n,1);
  console.log('build workflows: exact-environment correction, guarded replacement and explicitly sourced proof repair pass');
  console.log('build workflows: atomic registration, global existence, shared work, full-flow hold and captured continuation pass');
}
if(process.argv[1] && import.meta.url===pathToFileURL(process.argv[1]).href) {
 const db=await testDb('library');try{await buildWorkflowCases(db);}finally{await db.close();}
 await workflowProtocolCases();
}
