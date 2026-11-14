// SPDX-License-Identifier: AGPL-3.0-or-later
// Recover only proven empty completions; preserve their original request and immutable evidence.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {testDb} from './pg/test_db.mjs';
import {prepareIntakeBuild} from './build_batches.mjs';
import {factory,canon} from '../worker/tasks.js';
import {boardReviewsRoute} from '../worker/board_reviews.js';
import {BUILD_WORKFLOW_PRODUCER} from '../worker/build_workflows.js';

export async function workflowRecoveryCases(database=null) {
  const db=database||await testDb('library'),env={LIBRARY:db,ASSETS:{fetch:async()=>Response.json({engine:'before'})}};
  const sql=readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
  const expansion=sql.match(/CREATE OR REPLACE FUNCTION build_workflow_expansion[^]*?END \$\$;/)[0];
  const legacy=expansion.replace(/  IF jsonb_typeof\(\$3->'scope'\)[^\n]*\n/,'')
    .replace(/  IF \$2 IS NULL AND f.kind='correction'[^]*?  END IF;\n/,'');
  assert.notEqual(legacy,expansion,'the legacy fixture must remove the admission guards');
  if(process.argv.includes('--break-recovery-baseline')) {
    const admission=sql.match(/CREATE OR REPLACE FUNCTION build_intake_batch[^]*?END \$\$;/)[0];
    const broken=admission.replace("AND s.finished_at=observation->>'baseline_finished_at' AND s.score=(observation->>'baseline_score')::double precision AND s.metric=observation->>'metric'",'');
    assert.notEqual(broken,admission);await (db.exec?db.exec(broken):db.run(broken));
  }
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,canon:(r,w)=>canon({...r,work:w})}},producers:{build_workflow:BUILD_WORKFLOW_PRODUCER},spot:0});
  const admit=input=>db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
  const recover=(old,input)=>db.get('SELECT build_workflow_recover(?::text,?::text) AS recovered',old,JSON.stringify(input));
  let ordinal=0;
  async function fixture() {
    env.ASSETS={fetch:async()=>Response.json({engine:'before'})};
    const digit=['e','f','a'][ordinal],id=digit.repeat(32),at=new Date().toISOString();
    const request={weapon:'braton',mods:['serration'],runs:2,seed:9000+ordinal++,duration:2};
    const input=await prepareIntakeBuild({id,at,record:{weapon:'braton',mods:['serration']},initiator:'anonymous',from:['first:'+id],
      flow:{schema:1,scope:{boards:[{id:'standard_single_target'}],modes:{braton:['base']}},builds:[],templates:[request],patches:[{set:{},remove:[]}],
        nodes:[{kind:'measure',identity:id,ruler:'standard_single_target',mode:'base',template:0,patch:0}]}});
    await admit(input);
    const node=await db.get('SELECT * FROM state_build_workflow_nodes WHERE workflow IN(SELECT id FROM entity_build_workflows WHERE build_id=?)',id);
    await f.answerOfficial(env,{device:'official:baseline',owner:'official:baseline'},
      {question:node.computation,engine:'before',result:{score:10,metric:'dps'},work:1});
    const response=await boardReviewsRoute(new Request('https://wfsim.test/api/board/review',{method:'POST',headers:{'content-type':'application/json'},
      body:JSON.stringify({identity:id,ruler:'standard_single_target',mode:'base',engine:'before',observed_score:15,baseline_score:10,metric:'dps',request})}),env,'/api/board/review');
    assert.equal(response.status,200);const review=await response.json();
    const arrival=await db.get('SELECT * FROM record_build_submissions WHERE id=?','review:'+review.id);
    const observation={...JSON.parse(arrival.observation),valid:true};
    await db.run(legacy);
    try {await admit({...input,from:[arrival.id],observation,flow:{...input.flow,scope:null,nodes:[],templates:[],patches:[]}});}
    finally {await db.run(expansion);}
    await db.run('SELECT build_spend_submissions(?::text)',JSON.stringify([arrival.id]));
    const bad=await db.get('SELECT * FROM entity_measurement_reviews WHERE id=?',review.id);
    assert.equal(bad.state,'superseded');assert.equal(bad.task_id,null);
    assert.equal((await db.get('SELECT count(*) n FROM state_build_workflow_nodes WHERE workflow=?',bad.workflow)).n,0);
    return {input:{...input,from:[arrival.id],observation},bad,baselineTask:node.task_id};
  }
  const immutable=old=>db.get(`SELECT md5(w.source::text) source_md5,md5(e.plan::text) plan_md5,md5(s.definition::text) scope_md5
    FROM entity_build_workflows w JOIN entity_build_workflow_expansions e ON e.workflow=w.id
    JOIN entity_build_workflow_scopes s ON s.id=w.scope_id WHERE w.id=?`,old);
  try {
    const first=await fixture(),old=first.bad.workflow,before=await immutable(old);
    const {recovered}=await recover(old,first.input);
    const resumed=await db.get('SELECT * FROM entity_measurement_reviews WHERE id=?',first.bad.id);
    assert.equal(resumed.id,first.bad.id);assert.equal(resumed.state,'waiting');assert.equal(resumed.workflow,recovered.workflow);
    assert.notEqual(resumed.workflow,old);assert.notEqual(resumed.task_id,first.baselineTask);
    assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',old)).state,'failed');
    assert.deepEqual(await immutable(old),before);
    const source=await db.get('SELECT source FROM record_build_workflow_triggers WHERE id=?',recovered.trigger);
    assert.equal(source.source.recovery.workflow,old);assert.equal(source.source.recovery.prior_state,'succeeded');
    assert.equal(source.source.recovery.original_source.observation.review,first.bad.id);
    const count=(await db.get('SELECT count(*) n FROM entity_compute_tasks')).n;
    assert.deepEqual((await recover(old,first.input)).recovered,recovered);
    assert.equal((await db.get('SELECT count(*) n FROM entity_compute_tasks')).n,count);
    await assert.rejects(recover(old,{...first.input,flow:{...first.input.flow,scope:{boards:[]}}}),e=>e.code==='40001');
    await assert.rejects(recover(old,{...first.input,observation:{...first.input.observation,score:99}}),e=>e.code==='40001');
    env.ASSETS={fetch:async()=>Response.json({engine:'after'})};
    for(const n of [1,2]) {
      const witness={device:'recovery-device-'+n,owner:'recovery-owner-'+n,net:'recovery-net-'+n,siblings:['recovery-device-'+n,'cap:fixed-board-input-v1']};
      const work=await f.next(env,witness,'after');assert.ok(work);assert.equal(work.question,resumed.question);
      assert.equal((await f.answer(env,witness,{lease:work.lease,engine:'after',result:{score:15,metric:'dps'},work:1})).taken,true);
    }
    assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',first.bad.id)).state,'succeeded');
    assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',first.bad.identity)).score,15);
    assert.deepEqual((await recover(old,first.input)).recovered,recovered);
    const stale=await fixture();
    await db.run('UPDATE state_board_measurement_selection SET score=11 WHERE identity=?',stale.bad.identity);
    await assert.rejects(recover(stale.bad.workflow,stale.input),e=>e.code==='40001');
    assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',stale.bad.id)).state,'superseded');
    assert.equal(await db.get('SELECT workflow FROM record_build_workflow_triggers WHERE id=?','recover-empty:'+stale.bad.workflow),null);
    const cancelled=await fixture();await db.run("UPDATE state_build_workflows SET state='cancelled' WHERE id=?",cancelled.bad.workflow);
    await assert.rejects(recover(cancelled.bad.workflow,cancelled.input),/not an invalid empty completion/);
    const ordinary=await prepareIntakeBuild({...first.input,id:'b'.repeat(32),from:['ordinary:original'],observation:null,
      flow:{...first.input.flow,nodes:[...first.input.flow.nodes,{...first.input.flow.nodes[0],identity:'b'.repeat(32),template:1}],
        templates:[...first.input.flow.templates,{...first.input.flow.templates[0],seed:9999}]}});
    await db.run(legacy);
    try {await admit({...ordinary,flow:{...ordinary.flow,scope:null,nodes:[]}});}finally{await db.run(expansion);}
    const ordinaryOld=await db.get("SELECT id FROM entity_build_workflows WHERE build_id=?",ordinary.id);
    const ordinaryRecovery=(await recover(ordinaryOld.id,ordinary)).recovered;
    const ordinaryNodes=await db.all('SELECT state,task_id FROM state_build_workflow_nodes WHERE workflow=? ORDER BY id',ordinaryRecovery.workflow);
    assert.deepEqual(ordinaryNodes.map(n=>n.state),['skipped','queued'],'ordinary recovery retains existing coverage and requests only missing work');
    const ordinarySource=await db.get('SELECT requirements FROM relation_compute_task_sources WHERE task_id=?',ordinaryNodes[1].task_id);
    assert.equal(ordinarySource.requirements.force,false);
    if(!database) {
      const {owner,schema}=await db.get('SELECT session_user AS owner,current_schema() AS schema');
      assert.match(owner,/^[a-z_][a-z0-9_]*$/);assert.match(schema,/^[a-z_][a-z0-9_]*$/);
      await db.run('CREATE ROLE recovery_pipeline');
      await db.run('GRANT USAGE ON SCHEMA '+schema+' TO recovery_pipeline');
      await db.run('GRANT EXECUTE ON FUNCTION build_intake_batch(text) TO recovery_pipeline');
      await db.run('SET SESSION AUTHORIZATION recovery_pipeline');
      try {
        await assert.rejects(recover(old,first.input),e=>e.code==='42501');
        await assert.rejects(admit({...first.input,recovery_of:old}),e=>e.code==='42501');
      } finally {await db.run('SET SESSION AUTHORIZATION '+owner);}
    }
    console.log('workflow recovery: original review, immutable captures, idempotency, two fresh witnesses, stale/cancel guards and owner-only entry pass');
  } finally {if(!database)await db.close();}
}

if(process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href)await workflowRecoveryCases();
