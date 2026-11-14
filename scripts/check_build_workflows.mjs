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
import { captureInbox,inboxSource } from './inbox_reader.mjs';

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
  if(process.argv.includes('--break-merged-engine')) {
    const sql=readFileSync(new URL('../worker/pg/schema.sql',import.meta.url),'utf8');
    await db.exec(sql.match(/CREATE OR REPLACE FUNCTION compute_admit_tasks[^]*?END \$\$;/)[0]
      .replace("SET required_engine=nullif($3,'')",'SET required_engine=target.required_engine'));
  }
  if(process.argv.includes('--break-delivery-round')) {
    const sql=readFileSync(new URL('../worker/pg/sync.sql',import.meta.url),'utf8');
    await db.run(sql.match(/CREATE OR REPLACE FUNCTION factory_delivery_guard[^]*?END \$\$;/)[0]
      .replace('AND ($8 IS NULL OR task_id=$8)','AND true'));
  }
  if(process.argv.includes('--break-scope-gate')) {
    const sql=readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
    await db.exec(sql.match(/CREATE OR REPLACE FUNCTION build_intake_batch[^]*?END \$\$;/)[0]
      .replace('    IF reason IS NULL THEN',"    IF inserted IS NULL AND reason='intake' THEN reason:=NULL; END IF;\n    IF reason IS NULL THEN"));
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
  assert.equal(await count(root.id),2,'a known build checks its current boards and joins shared work for missing coverage');
  const invalid=await fixture('invalid-plan');delete invalid.flow.nodes[0].computation;
  await assert.rejects(intake(invalid),/canonical computation/);
  assert.equal(await db.get('SELECT id FROM entity_weapon_builds WHERE id=?',invalid.id),null,'entity and intent roll back together');
  const untraced=await fixture('untraced-system','',{system_source:{}});
  await assert.rejects(intake(untraced),/explicit source identity/);
  assert.equal(await db.get('SELECT id FROM entity_weapon_builds WHERE id=?',untraced.id),null);
  const absentScope=await fixture('workflow-null-scope');absentScope.flow.scope=null;absentScope.flow.nodes=[];
  await assert.rejects(intake(absentScope),/captured workflow scope/);
  assert.equal(await db.get('SELECT id FROM entity_weapon_builds WHERE id=?',absentScope.id),null,'an invalid scope cannot commit an entity or empty success');
  for(const id of ['system-batch-a','system-batch-b']) {
    const input=await fixture(id,'',{system_source:{id:'one-system-command'},zone:99});await intake(input);
    assert.equal(await count(id),1,'one command can independently target multiple builds');
    assert.equal((await db.get('SELECT zone FROM state_build_workflows WHERE build_id=?',id)).zone,99);
  }
  await f.answerOfficial(env,{device:'official:workflow',owner:'official:workflow'},
    {question:original[0].computation,engine:'e',result:{score:123,metric:'dps'},work:1});
  assert.equal((await nodes(root.id))[0].state,'succeeded');
  assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',original[0].workflow)).state,'active');
  assert.equal((await db.get('SELECT compute_round_occupied(?,?) AS held',original[0].computation,original[0].task_id)).held,true);
  await f.demand(env,{producer:'system',ref:'hold-check',verb:'simulate',request:root.flow.templates[0],payload:{reason:'explicit check'},zone:0,due_in_ms:0},'new-engine');
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
  await db.run('UPDATE state_build_workflows SET zone=1 WHERE id=?',optNode.workflow);
  const continuation=await db.get('SELECT * FROM state_build_inbox_priorities WHERE id=?',inbox.id);
  assert.equal(continuation.zone,1,'a continuation inherits its active parent workflow priority');
  const derived=await fixture('workflow-derived','',{derived:true,flow_source:{workflow:optNode.workflow,node:optNode.id}});
  await assert.rejects(intake({...derived,flow:{...derived.flow,scope:{boards:[]}}}),/captured workflow scope/);
  assert.equal((await nodes(opt.id))[0].state,'awaiting_intake','a mismatched child cannot consume its parent continuation');
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
  await assert.rejects(intake({...corrected,flow:{...corrected.flow,nodes:[]},
    observation:{...JSON.parse(validInbox.observation),valid:true}}),/observed measurement/);
  assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',validReview.id)).state,'waiting');
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
  await currentCoverageCases(db);
  await coalescedRequirementCases(db);
}

async function currentCoverageCases(db) {
  const env={LIBRARY:db},id='coverage-known',at='2026-11-11T00:00:00.000Z',record={weapon:'braton',mods:['serration']};
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,canon:(r,w)=>canon({...r,work:w})}},producers:{build_workflow:BUILD_WORKFLOW_PRODUCER}});
  await db.run('INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?)',id,at,JSON.stringify(record));
  for(const ruler of ['coverage_old_a','coverage_old_hidden']) await db.run(`INSERT INTO state_board_measurement_selection
    (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES(?,?,'base','prior',0.00001,'dps',0,?,?)`,id,ruler,at,at);
  const make=async(count,source)=>prepareIntakeBuild({id,at,record,from:[source],initiator:'anonymous',flow:{schema:1,
    scope:{boards:['coverage_old_a','coverage_old_hidden','coverage_new_a','coverage_new_b'].slice(0,count).map(id=>({id}))},
    builds:[],templates:[{weapon:'braton',runs:2}],patches:Array.from({length:count},(_,i)=>({set:{seed:987650+i},remove:[]})),
    nodes:['coverage_old_a','coverage_old_hidden','coverage_new_a','coverage_new_b'].slice(0,count)
      .map((ruler,i)=>({kind:'measure',identity:id,ruler,mode:'base',template:0,patch:i}))}});
  const intake=async input=>db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
  const tasksBefore=(await db.get('SELECT count(*) n FROM entity_compute_tasks')).n;
  await intake(await make(2,'already-covered'));
  assert.equal((await db.get('SELECT count(*) n FROM entity_build_workflows WHERE build_id=?',id)).n,0,'all stored scores count, independently of visible rank');
  await intake(await make(4,'from-old-board'));
  const flow=await db.get('SELECT id FROM state_build_workflows WHERE build_id=?',id);
  await Promise.all(Array.from({length:24},async(_,i)=>intake(await make(4,'concurrent-upload-'+i))));
  const rows=await db.all('SELECT * FROM state_build_workflow_nodes WHERE workflow=? ORDER BY position',flow.id);
  assert.deepEqual(rows.map(n=>n.state),['skipped','skipped','queued','queued']);
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_tasks')).n,tasksBefore+2,'only missing boards create computation work');
  assert.equal((await db.get('SELECT count(*) n FROM record_build_workflow_triggers WHERE workflow=?',flow.id)).n,25,'concurrent upload sources are retained');
  for(const n of rows.filter(n=>n.state==='queued')) await f.answerOfficial(env,{device:'official:coverage',owner:'official:coverage'},
    {question:n.computation,engine:'e',result:{score:42,metric:'dps'},work:1});
  assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',flow.id)).state,'succeeded');
  const tasksAfter=(await db.get('SELECT count(*) n FROM entity_compute_tasks')).n;
  await intake(await make(4,'from-new-board'));
  assert.equal((await db.get('SELECT count(*) n FROM entity_compute_tasks')).n,tasksAfter);
  assert.equal((await db.get('SELECT count(*) n FROM state_board_measurement_selection WHERE identity=?',id)).n,4);
  console.log('build coverage: known and hidden rows, current new boards, 24 concurrent uploads, complete delivery and repeat no-growth pass');
}

async function coalescedRequirementCases(db) {
  const env={LIBRARY:db},request={weapon:'braton',runs:2,seed:887766},at=Date.now();let fail=true;
  await db.run('CREATE TABLE coalesced_deliveries(ref text PRIMARY KEY,task_id bigint,result_id bigint)');
  const producer={on_fact_writes(env,d,fact){
    if(['blocked-delivery','later-force'].includes(d.ref) && fail)throw Error('delivery fixture temporarily unavailable');
    return [env.LIBRARY.prepare('INSERT INTO coalesced_deliveries VALUES(?,?,?) ON CONFLICT DO NOTHING').bind(d.ref,fact.task_id,fact.result_id)];
  }};
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,canon:(r,w)=>canon({...r,work:w})}},producers:{merge:producer}});
  const ask=(ref,extra={})=>({producer:'merge',ref,verb:'simulate',request,payload:{ref},due_in_ms:60000,zone:3,...extra});
  const id=await f.demand(env,ask('first-intake'),'old-engine',at);
  await db.run("UPDATE state_compute_scheduling SET lease='retained-lease',leased_to='running-machine',lease_until='2999-01-01' WHERE id=?",id);
  const initial=await db.get('SELECT task_id,lease FROM state_compute_work WHERE id=?',id);
  await Promise.all(Array.from({length:24},(_,i)=>f.demand(env,ask(i===23?'blocked-delivery':'source-'+i,
    {force:i%3!==0,zone:[3,1,0][i%3]}),'new-engine',at+1)));
  const merged=await db.get('SELECT * FROM state_compute_work WHERE id=?',id);
  assert.equal(merged.task_id,initial.task_id);assert.equal(merged.lease,initial.lease);
  assert.equal(merged.zone,0);assert.equal(merged.required_engine,'new-engine');
  assert.equal((await db.get('SELECT count(*) n FROM relation_compute_task_sources WHERE task_id=?',initial.task_id)).n,25);
  assert.equal((await db.get('SELECT count(*) n FROM relation_compute_task_sources WHERE task_id=? AND requirements->>\'force\'=\'true\'',initial.task_id)).n,16);
  await f.answerOfficial(env,{device:'official:old',owner:'official:old'},{question:id,engine:'old-engine',result:{score:10},work:1});
  assert.equal((await db.get('SELECT state FROM state_compute_work WHERE id=?',id)).state,'open','old engine cannot satisfy a newer merged requirement');
  assert.equal((await db.get('SELECT count(*) n FROM coalesced_deliveries')).n,0);
  await assert.rejects(f.answerOfficial(env,{device:'official:new',owner:'official:new'},
    {question:id,engine:'new-engine',result:{score:20},work:1}),/temporarily unavailable/);
  const accepted=await db.get('SELECT * FROM state_compute_work WHERE id=?',id);
  assert.equal((await db.get('SELECT count(*) n FROM coalesced_deliveries')).n,24);
  assert.equal((await db.get('SELECT compute_round_occupied(?,?) held',id,accepted.task_id)).held,true,'failed delivery retains active ID');
  fail=false;await f.redeliver(env,Date.now()+120000);
  assert.equal((await db.get('SELECT count(*) n FROM coalesced_deliveries')).n,25);
  assert.equal((await db.get('SELECT count(DISTINCT task_id) tasks,count(DISTINCT result_id) results FROM coalesced_deliveries')).results,1);
  assert.equal((await db.get('SELECT compute_round_occupied(?,?) held',id,accepted.task_id)).held,false);
  await f.demand(env,ask('later-force',{force:true,zone:0}),'new-engine',at+2);
  const next=await db.get('SELECT * FROM state_compute_work WHERE id=?',id);
  assert.notEqual(next.task_id,accepted.task_id);
  assert.equal((await db.get('SELECT count(*) n FROM state_compute_scheduling WHERE id=?',id)).n,1);
  await assert.rejects(db.run('SELECT compute_accept_result(?,?,\'open\',\'fact\',?,\'old-engine\',?,0,\'[]\')',id,next.task_id,accepted.canon,new Date().toISOString()),e=>e.code==='40001');
  fail=true;
  await assert.rejects(f.answerOfficial(env,{device:'official:next',owner:'official:next'},
    {question:id,engine:'new-engine',result:{score:20},work:1}),/temporarily unavailable/);
  const late=await db.get("SELECT * FROM relation_task_consumers WHERE producer='merge' AND ref='later-force'");
  await assert.rejects(db.run('SELECT factory_delivery_guard(?,?,?,?,?,?,?,?)',id,accepted.canon,'new-engine',late.producer,late.ref,late.asked_at,late.payload,accepted.task_id),
    e=>e.code==='WS002','equal output from another round cannot acknowledge a newer request');
  fail=false;await f.redeliver(env,Date.now()+120000);
  assert.equal((await db.get('SELECT compute_round_occupied(?,?) held',id,next.task_id)).held,false);
  const batchRequest={...request,seed:887767};
  const batchIds=await f.demandAll(env,[ask('batch-intake',{request:batchRequest}),ask('batch-review',{request:batchRequest,force:true,zone:1})],'new-engine',at+3);
  assert.equal(batchIds[0],batchIds[1]);
  const snapshots=await db.all('SELECT ref,requirements FROM relation_compute_task_sources WHERE task_id=(SELECT task_id FROM state_compute_scheduling WHERE id=?) ORDER BY ref',batchIds[0]);
  assert.deepEqual(snapshots.map(s=>s.requirements.force),[false,true]);
  console.log('compute coalescing: 24 mixed sources, highest zone, held lease, latest engine, all-source delivery/retry and ID release pass');
}
export async function inboxReadPlanCases(database=null) {
  const db=database||await testDb('library');
  try {
    await db.run("INSERT INTO entity_weapon_builds(id,at,record) VALUES('inbox-build','2026-11-14','{}')");
    await db.run("INSERT INTO entity_build_workflow_scopes(id,definition) VALUES('inbox-scope','{\"boards\":[]}'::jsonb)");
    await db.run(`INSERT INTO entity_build_workflows(id,build_id,scope_key,kind,scope_id,source,created_at)
      VALUES('inbox-flow','inbox-build','inbox-key','intake','inbox-scope','{}'::jsonb,'2026-11-14')`);
    await db.exec(`INSERT INTO record_build_submissions(id,at,record,initiator,family_source,observation)
      SELECT 'inbox-plan-'||lpad(n::text,6,'0'),'2026-11-14','{"weapon":"braton"}','anonymous',
        CASE n%4 WHEN 0 THEN 'workflow|inbox-flow|0' WHEN 1 THEN 'workflow|missing-flow|0' WHEN 2 THEN 'legacy-family' END,
        CASE WHEN n%7=0 THEN '{"review":"retained-observation"}' END FROM generate_series(1,10000) n;
      INSERT INTO record_equipment_submissions(id,at,record) VALUES('equipment:inbox-plan','2026-11-14','{"kind":"warframe"}');
      ANALYZE record_build_submissions;ANALYZE record_equipment_submissions;
      ANALYZE entity_build_workflows;ANALYZE entity_build_workflow_scopes;`);
    if(process.argv.includes('--break-inbox-plan')) await db.exec(`CREATE OR REPLACE VIEW state_build_inbox_inputs AS
      SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,s.definition::text AS workflow_scope
      FROM record_build_submissions b LEFT JOIN entity_build_workflows w
        ON split_part(b.family_source,'|',1)='workflow' AND w.id=split_part(b.family_source,'|',2)
      LEFT JOIN entity_build_workflow_scopes s ON s.id=w.scope_id
      UNION ALL SELECT id,at,record,NULL,NULL,NULL,NULL FROM record_equipment_submissions;`);
    const actual=await db.all('SELECT * FROM state_build_inbox_inputs ORDER BY id LIMIT 100 OFFSET 50');
    const expected=await db.all(`WITH prefix AS MATERIALIZED (
      SELECT * FROM (SELECT id,at,record,initiator,family_source,observation FROM record_build_submissions
        UNION ALL SELECT id,at,record,NULL,NULL,NULL FROM record_equipment_submissions) arrivals ORDER BY id LIMIT 100 OFFSET 50
    ) SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,s.definition::text AS workflow_scope
      FROM prefix b LEFT JOIN entity_build_workflows w
        ON split_part(b.family_source,'|',1)='workflow' AND w.id=split_part(b.family_source,'|',2)
      LEFT JOIN entity_build_workflow_scopes s ON s.id=w.scope_id ORDER BY b.id`);
    assert.deepEqual(actual,expected,'scope enrichment preserves missing parents, observations and exact page membership');
    assert.equal((await db.get('SELECT id FROM state_build_inbox_inputs ORDER BY id LIMIT 1')).id,'equipment:inbox-plan');
    const [{['QUERY PLAN']:plan}]=await db.all('EXPLAIN (FORMAT JSON) SELECT * FROM state_build_inbox_inputs ORDER BY id LIMIT 100');
    const walk=node=>[node,...(node.Plans||[]).flatMap(walk)];
    const source=walk(plan[0].Plan).find(node=>node['Relation Name']==='record_build_submissions');
    assert.ok(source?.['Node Type'].includes('Index'),'a bounded inbox read cannot scan the full submission table before its limit');
    console.log('build workflows: indexed inbox pages preserve scope, equipment and source rows before bounded enrichment');
  } finally {if(!database)await db.close();}
}

export async function queuedReviewAcrossReleaseCases(database=null) {
  const db=database||await testDb('library'),oldEngine='0c3ee2cd6e0a',newEngine='7bd3c7f00f18';
  const env={LIBRARY:db,ASSETS:{fetch:async()=>Response.json({engine:oldEngine})}};
  const f=factory({verbs:{simulate:{cap:2,lease_ms:60000,canon:(result,work)=>canon({...result,work})}},
    producers:{build_workflow:BUILD_WORKFLOW_PRODUCER},spot:0});
  try {
    const id='d'.repeat(32),at=new Date().toISOString(),request={weapon:'braton',mods:['serration'],runs:2,seed:9191,duration:2};
    const input=await prepareIntakeBuild({id,at,record:{weapon:'braton',mods:['serration']},from:['release-review-fixture'],initiator:'anonymous',
      flow:{schema:1,scope:{boards:[{id:'standard_single_target'}]},builds:[],templates:[request],patches:[{set:{},remove:[]}],
        nodes:[{kind:'measure',identity:id,ruler:'standard_single_target',mode:'base',template:0,patch:0}]}});
    await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
    const first=await db.get('SELECT * FROM state_build_workflow_nodes');
    await f.answerOfficial(env,{device:'official:before-release',owner:'official:before-release'},
      {question:first.computation,engine:oldEngine,result:{score:10,metric:'dps'},work:1});
    const oldResult=await db.get('SELECT id,task_id,canon,engine FROM entity_compute_results WHERE computation=?',first.computation);
    const response=await boardReviewsRoute(new Request('https://wfsim.test/api/board/review',{method:'POST',headers:{'content-type':'application/json'},
      body:JSON.stringify({identity:id,ruler:'standard_single_target',mode:'base',engine:oldEngine,observed_score:15,
        baseline_score:10,metric:'dps',request})}),env,'/api/board/review');
    assert.equal(response.status,200);const review=await response.json();
    const arrival=await db.get('SELECT * FROM record_build_submissions WHERE id=?','review:'+review.id);
    assert.ok(arrival);
    await db.exec(`INSERT INTO record_build_submissions(id,at,record)
      SELECT '000-ordinary-'||lpad(n::text,4,'0'),'2026-11-14','{}' FROM generate_series(1,1000) n;`);
    await db.run(`INSERT INTO entity_measurement_reviews
      SELECT (jsonb_populate_record(NULL::entity_measurement_reviews,to_jsonb(r)||jsonb_build_object(
        'id','missing-review-'||n,'ref','missing-ref-'||n,'created_at','2020-01-01'))).*
      FROM entity_measurement_reviews r CROSS JOIN generate_series(1,200) n WHERE r.id=?`,review.id);
    const source=inboxSource((sql,params)=>db.all(sql,...params));
    if(process.argv.includes('--break-inbox-prefilter')) source.priority=max=>db.all(`SELECT p.* FROM (
      SELECT * FROM state_build_inbox_priorities ORDER BY zone,position,id LIMIT ?::bigint) p
      WHERE EXISTS(SELECT 1 FROM state_build_inbox_inputs i WHERE i.id=p.id)`,max);
    const captured=[];
    const capture=await captureInbox({source,maxRows:1,emit:(_line,row)=>captured.push(row)});
    assert.deepEqual(captured.map(row=>row.id),[arrival.id],'eligible review must pass missing earlier sources and the ordinary UUID backlog');
    assert.equal(capture.next_after,'','priority work does not move the ordinary capture cursor');
    assert.deepEqual(captured[0],await db.get('SELECT * FROM state_build_inbox_inputs WHERE id=?',arrival.id));
    await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([{...input,from:[arrival.id],
      observation:{...JSON.parse(arrival.observation),valid:true}}]));
    await db.run('SELECT build_spend_submissions(?::text)',JSON.stringify([arrival.id]));
    const queued=await db.get('SELECT * FROM state_compute_work WHERE id=?',first.computation);
    assert.notEqual(queued.task_id,oldResult.task_id);
    env.ASSETS={fetch:async()=>Response.json({engine:newEngine})};
    for(const suffix of ['one','two']) {
      const witness={device:'release-review-'+suffix,owner:'release-owner-'+suffix,net:'release-net-'+suffix,
        siblings:['release-review-'+suffix,'cap:fixed-board-input-v1']};
      const leased=await f.next(env,witness,newEngine);
      assert.ok(leased,'an observation version cannot strand a queued review when the served engine advances');
      assert.equal(leased.question,first.computation);
      const answered=await f.answer(env,witness,{lease:leased.lease,engine:newEngine,result:{score:15,metric:'dps'},work:1});
      assert.equal(answered.taken,true);
    }
    assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',review.id)).state,'succeeded');
    assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',id)).score,15);
    assert.deepEqual(await db.get('SELECT id,task_id,canon,engine FROM entity_compute_results WHERE id=?',oldResult.id),oldResult);
    const origin=await db.get("SELECT source FROM record_build_workflow_triggers WHERE id=?",review.id);
    assert.equal(origin.source.observation.engine,oldEngine,'the source observation retains its original version');
    assert.equal((await db.get('SELECT fact_engine FROM state_compute_work WHERE id=?',first.computation)).fact_engine,newEngine);
    console.log('build workflows: a queued observation survives a release, two fresh witnesses and guarded score delivery');
  } finally {if(!database)await db.close();}
}

if(process.argv[1] && import.meta.url===pathToFileURL(process.argv[1]).href) {
 if(process.argv.includes('--queued-review-release')) {await queuedReviewAcrossReleaseCases();process.exit(0);}
 if(process.argv.includes('--inbox-read-plan') || process.argv.includes('--break-inbox-plan')) {await inboxReadPlanCases();process.exit(0);}
 const db=await testDb('library');try{await buildWorkflowCases(db);}finally{await db.close();}
 await workflowProtocolCases();
 await queuedReviewAcrossReleaseCases();
 await inboxReadPlanCases();
}
