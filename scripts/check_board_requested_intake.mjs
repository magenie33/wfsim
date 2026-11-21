// SPDX-License-Identifier: AGPL-3.0-or-later
// Fresh requests cross native normalization, common community verification and website publication.
import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {randomBytes,createHash} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {pgDb,postgresClient,placeholders} from '../worker/db.js';
import worker from '../worker/index.js';
import {familyBoardPass,normalizeBoardRequest,requestedBoardPass} from './board_requested_intake.mjs';
import {factory,canon} from '../worker/tasks.js';
import {BUILD_WORKFLOW_PRODUCER} from '../worker/build_workflows.js';
import {openBoardResultFlow} from './board_result_flow.mjs';
import {boardRequestedSchema,boardRequestedSubmitUpgrade} from './board_requested_schema.mjs';
import {rawInboxRetentionPage} from './raw_inbox_retention.mjs';
import {captureInbox,inboxSource} from './inbox_reader.mjs';
import {calculationId} from '../worker/board_tasks.js';

const url=process.env.WFSIM_REQUESTED_INTAKE_TEST_URL||process.env.WFSIM_BOARD_MAILBOX_TEST_URL;
assert(url&&['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname),'use an isolated local PostgreSQL');
const schema='board_requested_'+randomBytes(6).toString('hex');
const sql=postgres(url,{max:4,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000,lock_timeout:2000},
 types:{bigint:{to:20,from:[20,1700],parse:Number,serialize:String}}});
const binary=resolve(process.env.WFSIM_REQUESTED_INTAKE_BIN||'target/debug/wfsim-intake');
const boardBinary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const root=mkdtempSync(join(tmpdir(),'wfsim-board-requested-'));
const engine='8ce96e275dc2',at=new Date().toISOString();
const realRandom=Math.random;Math.random=()=>0.5;
let flow,runner;
const runnerRole=schema+'_runner';
const pipelineRole=schema+'_pipeline';
const client=postgresClient(sql),db=Object.assign(pgDb(client),{
 get:async(text,...args)=>(await client.query(placeholders(text),args)).rows[0],
 all:async(text,...args)=>(await client.query(placeholders(text),args)).rows,
 run:async(text,...args)=>(await client.query(placeholders(text),args)).affected,
});
const env={LIBRARY:db,ASSETS:{fetch:async()=>Response.json({engine,release:'requested-test',accepted:[]})}};
const post=(path,body,headers={})=>worker.fetch(new Request('https://wfsim.app'+path,{method:'POST',headers:{'content-type':'application/json',...headers},body:JSON.stringify(body)}),env,{waitUntil(){}});
const requestRow=id=>db.get(`SELECT r.*,s.* FROM entity_board_requests r JOIN state_board_requests s USING(id) WHERE r.id=?`,id);
const count=async table=>Number((await db.get('SELECT count(*) n FROM '+table)).n);

try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 for(const name of ['schema','library','sync','inventory','build_workflows','board_admission'])
  await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 await db.run("INSERT INTO record_build_submissions(id,at,record,initiator) VALUES('old-inbox',?,'{}','anonymous')",at);
 for(const name of ['board_mailbox','board_capture','board_pages'])
  await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 await sql.unsafe('CREATE ROLE '+runnerRole+' NOLOGIN');
 const [{current_user:owner}]=await sql.unsafe('SELECT current_user');
 const install=httpRole=>boardRequestedSchema({httpRole,runnerRole,readerRole:owner}).replace("default_tablespace='wfsim_data'","default_tablespace='pg_default'");
 const setup=await sql.reserve();
 const runInstall=async text=>{try{return await setup.unsafe(text);}catch(error){await setup.unsafe('ROLLBACK');throw error;}};
 try{
  await assert.rejects(runInstall(install(schema+'_absent_http_role')),error=>error.code==='42704');
  assert.equal((await setup.unsafe("SELECT to_regclass('entity_board_requests') name"))[0].name,null,'late grants roll back source capture and schema');
  await runInstall(install(owner));
  await runInstall(boardRequestedSubmitUpgrade({httpRole:owner}));
  await assert.rejects(runInstall(install(owner)),/explicit upgrade is required/);
 }finally{setup.release();}
 await sql.unsafe('GRANT USAGE ON SCHEMA '+schema+' TO '+runnerRole);
 runner=postgres(url,{max:2,fetch_types:false,onnotice(){},connection:{search_path:schema,role:runnerRole,statement_timeout:10000,lock_timeout:2000}});
 await assert.rejects(runner.unsafe('UPDATE state_board_request_control SET enabled=true'),error=>error.code==='42501');
 await assert.rejects(runner.unsafe('DELETE FROM entity_weapon_builds WHERE false'),error=>error.code==='42501');
 await assert.rejects(runner.unsafe("SELECT board_request_submit('bad','date','{}','anonymous')"),error=>error.code==='42501');
 const configs=await sql.unsafe(`SELECT proname,array_to_string(proconfig,E'\n') config FROM pg_proc
   WHERE pronamespace=$1::regnamespace AND proname LIKE 'board_request%' AND prosecdef`,[schema]);
 assert(configs.length>=10);
 assert(configs.every(row=>row.config.includes('search_path='+schema+',pg_catalog,pg_temp')),'every requested definer function pins its own trusted namespace');
 const probe=await runner.reserve();
 try{
  await probe.unsafe(`CREATE TEMP TABLE state_board_request_control(id integer,source uuid,enabled boolean);
   CREATE TEMP TABLE state_board_requests(id text,position bigint,phase text,intent text,workflow text,lease uuid,
     lease_until text,retry_at text,attempts bigint,error text);
   CREATE TEMP TABLE entity_board_requests(id text,kind text,input jsonb,ruler text,mode text,created_at text);`);
  const [{source:realSource}]=await sql.unsafe('SELECT source FROM state_board_request_control');
  await probe.unsafe('INSERT INTO pg_temp.state_board_request_control VALUES(1,$1::uuid,true)',[realSource]);
  await probe.unsafe(`INSERT INTO pg_temp.state_board_requests VALUES('namespace-probe',1,'pending',NULL,NULL,NULL,NULL,'2026-01-01T00:00:00.000Z',0,NULL);
   INSERT INTO pg_temp.entity_board_requests VALUES('namespace-probe','missing','{}','r','base','2026-01-01T00:00:00.000Z');`);
  const invoke=async()=>(await probe.unsafe('SELECT board_request_claim($1::uuid) claim',[realSource]))[0].claim;
  assert.equal(await invoke(),null,'temporary shadows cannot enable the real paused consumer');
  try{
   await sql.unsafe('ALTER FUNCTION board_request_claim(uuid) SET search_path TO '+schema);
   assert.equal((await invoke())?.id,'namespace-probe','the negative control must actually exploit an implicit temporary namespace');
  }finally{await sql.unsafe('ALTER FUNCTION board_request_claim(uuid) SET search_path TO '+schema+',pg_catalog,pg_temp');}
  assert.equal(await invoke(),null,'the restored pin refuses the same temporary-table bypass');
 }finally{probe.release();}
 await db.run("INSERT INTO state_task_pipeline_controls(name,paused,changed_at,changed_by) VALUES('admission',true,?,'check') ON CONFLICT(name) DO UPDATE SET paused=true",at);
 await db.run("INSERT INTO state_factory_parameters(name,value,set_at,set_by) VALUES('board_admission',0,?,'check') ON CONFLICT(name) DO UPDATE SET value=0",at);
 const fences=await db.all("SELECT * FROM state_task_pipeline_controls WHERE name='admission'");
 await sql.unsafe(readFileSync(new URL('../worker/pg/order_cut_indexes.sql',import.meta.url),'utf8').replaceAll('INDEX CONCURRENTLY','INDEX'));
 const cancelled='c'.repeat(32),old='2026-01-01T00:00:00.000Z';
 const record=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url),'utf8'))[0].record;
 const ruler='standard_single_target',mode='base';
 const body={...record,benchmark:ruler,mode,requested:true};
 const retained=[];
 for(const benchmark of ['demolisher',ruler]){
  const input=await normalizeBoardRequest(binary,{kind:'missing',ruler:benchmark,mode,
   input:{id:'retained-'+benchmark,at:old,record:JSON.stringify({...record,benchmark,mode}),initiator:'anonymous'}});
  delete input.family_flow;
  delete input.family_root;
  await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
  for(const kind of ['intake','expansion']){
   const {job}=await db.get('SELECT build_admission_claim(?::text,board_input_timestamp(clock_timestamp())) job',kind);assert(job);
   await db.run('SELECT build_admission_run(?::text,?::text,?::text,1,board_input_timestamp(clock_timestamp()))',job.kind,job.key,job.lease);
  }
  retained.push(input.flow.nodes[0].computation);
 }
 const heldResponse=await post('/api/board/work',{verifier:'1'.repeat(24),engine,protocol:7,capabilities:['fixed-board-input-v1'],consent:{v:3,at},lanes:8},{'cf-connecting-ip':'192.0.2.1'});
 const held=await heldResponse.json();assert.equal(held.pack?.tasks.length,2);
 assert.equal(held.pack.tasks[0].question,retained[0],'ordinary retained work is really ahead before promotion');
 const sharedBefore=await db.get('SELECT * FROM state_compute_scheduling WHERE id=?',retained[1]);
 assert.equal(sharedBefore.zone,3);assert.equal(sharedBefore.lease,held.pack.lease);
 await db.run('SELECT compute_register_tasks(?::text,?::text,?::text)',JSON.stringify([
  {id:cancelled,verb:'simulate',request:'{}',due_at:old,producer:'new_build',waits:0,zone:99}]),engine,old);
 const cutoff=(await db.get('SELECT max(id) n FROM entity_compute_tasks')).n;
 const cutAt=(await db.get('SELECT board_input_timestamp(clock_timestamp()) at')).at;
 await db.run('SELECT compute_order_cut_stage((?::text)::jsonb)',JSON.stringify({id:'requested-active-cut',
  cut_at:cutAt,task_cutoff:cutoff,keep_boundary:[3,sharedBefore.zone_position,sharedBefore.id],source:{fixture:true}}));
 await db.run('SELECT compute_order_cut_capture(?::text)','requested-active-cut');
 await db.run('SELECT compute_order_cut_activate(?::text,?::bigint,?::text)','requested-active-cut',0,createHash('sha256').update('').digest('hex'));
 const cut=await db.all('SELECT * FROM entity_compute_order_cuts');
 const protectedRows=await db.all('SELECT * FROM relation_compute_order_cut_protection');
 assert.equal(await count('state_compute_work'),2,'the durable cut retains the old prefix and excludes the cancelled task');
 assert(!(await db.get('SELECT 1 FROM state_compute_work WHERE id=?',cancelled)));
 assert.equal((await post('/api/board/submit',body)).status,503,'disabled requested admission must not report acceptance');
 assert.equal(await count('entity_board_requests'),0,'installation never captures the old inbox');
 await db.run('UPDATE state_board_request_control SET enabled=true');
 const {source}=await db.get('SELECT source FROM state_board_request_control');
 const pass=()=>requestedBoardPass(runner,{source,binary});
 assert.deepEqual(await requestedBoardPass(sql,{source:'00000000-0000-4000-8000-000000000000',binary}),{idle:true});
 await post('/api/board/submit',{builds:[{...record,benchmark:ruler,mode}]});
 assert.equal(await count('entity_board_requests'),0,'ordinary bulk uploads remain outside requested admission');
 const response=await post('/api/board/submit',{...body,benchmark:undefined,requested_ruler:ruler});assert.equal(response.status,200);
 const accepted=await response.json();assert.match(accepted.request,/^[a-f0-9-]{36}$/);
 const id=accepted.request;
 const custom=await requestRow(id);
 assert.equal(custom.ruler,ruler);
 assert.equal(custom.input.record.benchmark,undefined,'the requested score target is independent of the custom fight');
 assert(!(await db.get('SELECT 1 FROM record_build_submissions WHERE id=?',id)),'the requested work owns its input from submission');
 const [{claim}]=await sql.unsafe('SELECT board_request_claim($1::uuid) claim',[source]);
 const instant=/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/;
 for(const value of [claim.created_at,claim.retry_at,claim.lease_until])assert.match(value,instant);
 assert.equal(claim.id,id);assert((await pass()).idle,'an active claim cannot be stolen');
 const normalized=await normalizeBoardRequest(binary,claim);
 assert.equal(normalized.flow.nodes.length,1,'a player request has no family or optimizer fanout');
 assert.equal(normalized.flow.nodes[0].identity,normalized.id);
 const bad=structuredClone(normalized);bad.flow.nodes.push(bad.flow.nodes[0]);
 await assert.rejects(db.run('SELECT board_request_stage(?::text,?::uuid,?::text)',id,claim.lease,JSON.stringify(bad)),/changed its captured scope/);
 assert.equal(await count('entity_build_intake_intents'),2,'a rejected stage cannot add an intent');
 assert.deepEqual((await requestRow(id)).input,custom.input,'a rejected stage retains the independently owned input');
 await db.run("UPDATE state_board_requests SET lease_until=board_input_timestamp(clock_timestamp()-interval '1second') WHERE id=?",id);
 await assert.rejects(db.run('SELECT board_request_stage(?::text,?::uuid,?::text)',id,claim.lease,JSON.stringify(normalized)),/lease changed/);
 const otherRecord=JSON.stringify({weapon:'braton'});
 assert.notDeepEqual(JSON.parse(otherRecord),custom.input.record);
 await db.run('INSERT INTO record_build_submissions(id,at,record,initiator) VALUES(?,?,?,?)',
   id,custom.input.at,otherRecord,custom.input.initiator);
 assert((await pass()).staged,'expired claims recover through the same source');
 assert.equal(await count('state_compute_work'),2,'canonical intake does not create or replace the held computation');
 assert.equal((await db.get('SELECT record FROM record_build_submissions WHERE id=?',id))?.record,
   otherRecord,'staging preserves a different payload with the same source ID');
 assert(await db.get("SELECT 1 FROM record_build_submissions WHERE id='old-inbox'"));
 const sharedNode=await db.get('SELECT workflow FROM state_build_workflow_nodes WHERE computation=? AND task_id=?',retained[1],sharedBefore.task_id);
 await db.run(`INSERT INTO state_build_workflow_nodes(workflow,id,expansion,definition,state,retry_at,queue_position)
   SELECT workflow,'unrelated-fixture',expansion,definition||'{"kind":"optimize"}'::jsonb,'pending',retry_at,nextval('compute_queue_position')
   FROM state_build_workflow_nodes WHERE workflow=? LIMIT 1`,sharedNode.workflow);
 await assert.rejects(pass(),/requested workflow includes unrelated computations/);
 assert.equal((await db.get('SELECT zone FROM state_compute_scheduling WHERE id=?',retained[1])).zone,3,'an ambiguous legacy scope cannot promote existing work');
 assert.equal((await requestRow(id)).phase,'staged');
 await db.run("DELETE FROM state_build_workflow_nodes WHERE workflow=? AND id='unrelated-fixture'",sharedNode.workflow);
 await db.run('UPDATE state_board_requests SET retry_at=board_input_timestamp(clock_timestamp()) WHERE id=?',id);
 assert(!(await pass()).pending);assert.equal((await requestRow(id)).phase,'admitted');
 assert.equal(await count('state_compute_work'),2,'the existing shared calculation is reused');
 const sharedAfter=await db.get('SELECT * FROM state_compute_scheduling WHERE id=?',retained[1]);
 assert.equal(sharedAfter.zone,0,'explicit missing measurement requests use the human calculation zone');
 assert(sharedAfter.zone_position>sharedBefore.zone_position,'promotion joins the new zone tail');
 for(const key of ['task_id','lease','lease_until','leased_to','leased_at','lease_prev','leased_to_prev','lease_prev_until'])
  assert.equal(sharedAfter[key],sharedBefore[key],'priority resolution preserves the held '+key);
 assert.equal((await db.get('SELECT zone FROM state_compute_scheduling WHERE id=?',retained[0])).zone,3,'the older ordinary prefix keeps its priority');
 assert.deepEqual(await db.all("SELECT * FROM state_task_pipeline_controls WHERE name='admission'"),fences);
 assert.equal((await db.get("SELECT value FROM state_factory_parameters WHERE name='board_admission'")).value,0);
 const inputNode=normalized.flow.nodes[0],patch=normalized.flow.patches[inputNode.patch];
 const calculation={...normalized.flow.templates[inputNode.template],...patch.set};
 for(const key of patch.remove)delete calculation[key];
 async function answer(d,score,identity=normalized.id){
  const verifier=d.repeat(24),headers={'cf-connecting-ip':'192.0.2.'+d};
  const got=d==='1'?null:await post('/api/board/work',{verifier,engine,protocol:7,capabilities:['fixed-board-input-v1'],consent:{v:3,at},lanes:8},headers);
  if(got)assert.equal(got.status,200);const dispatched=got?await got.json():held;
  assert(dispatched.pack?.tasks?.length,'a fresh request must reach actual public work dispatch under the active cut: '+JSON.stringify(dispatched));
  const work=dispatched.pack.tasks.find(task=>task.identity===identity&&task.ruler===ruler);assert(work);
  if(got)assert.equal(dispatched.pack.tasks[0].question,work.question,'the new requested task advances ahead of retained zone03 work');
  assert(dispatched.pack.tasks.every(task=>task.question!==cancelled));
  const replied=await post('/api/board/answers',{verifier,engine,lease:dispatched.pack.lease,done:true,
   answers:[{question:work.question,score,metric:'kpm',work:100}]},headers);
  assert.equal(replied.status,200);assert((await replied.json()).ok);
 }
 await answer('1',10);
 assert.equal(await count('state_board_measurement_selection'),0,'one witness never publishes a fact');
 await answer('2',10);
 const selected=await db.get('SELECT * FROM state_board_measurement_selection WHERE identity=?',normalized.id);
 assert.equal(selected.score,10);assert.equal(selected.measured_by,'verified:'+engine);
 assert.equal(Number(selected.publication_zone),0,'the workflow must carry the accepted calculation zone to publication');
 assert.notEqual(selected.publication_zone,null);
 assert.equal((await db.get('SELECT state FROM state_build_workflows WHERE id=?',(await requestRow(id)).workflow)).state,'succeeded');
 const legacy='0fd706f8-7da9-4bca-8f93-3dbf7eaee322';
 const legacyInput={id:'review:'+legacy,at:old,record:JSON.stringify({...normalized.record,benchmark:ruler,mode}),initiator:'anonymous',
  observation:JSON.stringify({score:12,metric:'kpm',engine,baseline_score:10,baseline_finished_at:selected.finished_at,
   request:calculation,ruler,mode,review:legacy})};
 await sql.begin(async tx=>{
  await tx.unsafe(`INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
   baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at)
   VALUES($1,$2,$3,$4,$5,$6,$7,'legacy-question',$8,'waiting',10,'kpm',0,$9,$10,$10,$3)`,
   [calculationId(normalized.id,ruler,mode),legacy,old,normalized.id,ruler,mode,engine,JSON.stringify(normalized.record),selected.measured_by,selected.finished_at]);
  await tx.unsafe('ALTER TABLE record_build_submissions DISABLE TRIGGER board_requested_review');
  await tx.unsafe('INSERT INTO record_build_submissions(id,at,record,initiator,observation) VALUES($1,$2,$3,$4,$5)',Object.values(legacyInput));
  await tx.unsafe('ALTER TABLE record_build_submissions ENABLE TRIGGER board_requested_review');
 });
 const legacyNormalized=await normalizeBoardRequest(binary,{kind:'correction',input:legacyInput,ruler,mode});
 await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([legacyNormalized]));
 const legacyIntent=await db.get('SELECT to_jsonb(i) body FROM entity_build_intake_intents i WHERE id=?',legacy);
 const reviewResponse=await post('/api/board/review',{identity:normalized.id,ruler,mode,engine,
  observed_score:12,baseline_score:10,metric:'kpm',request:calculation});
 assert.equal(reviewResponse.status,200,await reviewResponse.clone().text());
 const review=await reviewResponse.json();assert(review.accepted);
 assert.notEqual(review.id,legacy,'a new click replaces the stranded legacy review with a fresh source');
 assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',legacy)).state,'superseded');
 assert(await db.get('SELECT 1 FROM record_build_submissions WHERE id=?',legacyInput.id),'the old inbox is retained');
 const correction='review:'+review.id;assert.equal((await requestRow(correction)).kind,'correction');
 assert(!(await db.get('SELECT 1 FROM record_build_submissions WHERE id=?',correction)),'the enabled review queue owns its observation');
 const ownedReview=(await requestRow(correction)).input;
 const retryReview=record=>db.run(`INSERT INTO record_build_submissions(id,at,record,initiator,observation)
    VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING`,correction,old,record,ownedReview.initiator,ownedReview.observation);
 await retryReview(ownedReview.record);
 await assert.rejects(retryReview('{"weapon":"braton"}'),/review source already owns a different input/);
 assert.deepEqual((await requestRow(correction)).input,ownedReview);
 await db.run('UPDATE state_board_request_control SET enabled=false');
 await db.run('INSERT INTO record_build_submissions(id,at,record,initiator,observation) VALUES(?,?,?,?,?)',
   correction,ownedReview.at,ownedReview.record,ownedReview.initiator,ownedReview.observation);
 assert(await db.get('SELECT 1 FROM record_build_submissions WHERE id=?',correction));
 await db.run('UPDATE state_board_request_control SET enabled=true');
 assert((await pass()).staged);
 assert(!(await db.get('SELECT 1 FROM record_build_submissions WHERE id=?',correction)),'staging removes the exact legacy copy');
 assert(!(await pass()).pending);
 assert.deepEqual(await db.get('SELECT to_jsonb(i) body FROM entity_build_intake_intents i WHERE id=?',legacy),legacyIntent,'fresh admission never changes the old intent');
 assert(await db.get("SELECT 1 FROM state_build_admission_work WHERE kind='intake' AND key=?",legacy),'the old staged work is not admitted');
 assert.equal(await count('state_compute_work'),2,'correction retains the computation identity and opens its execution round');
 await answer('3',12);
 assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',normalized.id)).score,10);
 await answer('4',12);
 const reviewed=await db.get('SELECT * FROM entity_measurement_reviews WHERE id=?',review.id);
 assert.equal(reviewed.state,'succeeded',JSON.stringify({reviewed,source:await requestRow(correction),queue:await db.all('SELECT id,state,task_id,fact_engine,canon FROM state_compute_work')}));
 assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',normalized.id)).score,12);
 const duplicate=await (await post('/api/board/submit',body)).json();
 assert((await pass()).staged);await pass();
 assert.equal((await requestRow(duplicate.request)).phase,'admitted');
 assert.equal(await count('state_compute_work'),2,'existing coverage never creates another task');
 const status=await (await post('/api/board/requests/status',{ids:[id,duplicate.request]})).json();
 assert(status.ok);assert.equal(status.requests[0].current_score,12);
 const [{source:publicationSource}]=await sql.unsafe('SELECT source FROM state_board_input_source');
 flow=await openBoardResultFlow({sql,source:publicationSource,engine,binary:boardBinary,work:root,requestedPartitions:true});
 const scene=flow.contract.rulers.find(row=>row.id===ruler).scene;
 assert.equal((await db.get('SELECT count(*) n FROM state_board_read_partitions')).n,0,'a source cannot activate its own partition');
 for(let i=0;i<100;i++){const step=await flow.step();if(step.status==='idle')break;assert(i<99);}
 const page=await worker.fetch(new Request('https://wfsim.app/api/board/pages?'+new URLSearchParams({weapon:record.weapon,ruler})),env,{waitUntil(){}});
 assert.equal(page.status,200);const published=await page.json();
 assert(published.rows.some(row=>row.score===12),'the website query must contain the independently accepted correction');
 const partition=await db.get('SELECT * FROM state_board_read_partitions WHERE weapon=? AND ruler=?',record.weapon,ruler);
 assert(partition.enabled&&partition.canonical_sources&&partition.revision,'complete native preparation atomically selects the new requested partition');
 assert.match((await db.get('SELECT activated_at FROM state_board_requested_partitions WHERE weapon=? AND ruler=?',record.weapon,ruler)).activated_at,instant);
 const retired='1690e865-7898-4ca5-a76c-eaa159867a16';
 const cancelledTask=(await db.get('SELECT task_id FROM state_compute_scheduling WHERE id=?',cancelled)).task_id;
 const cancelledBefore=await db.get('SELECT to_jsonb(q) body FROM state_compute_scheduling q WHERE id=?',cancelled);
 await db.run(`INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
   baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at,task_id)
   SELECT ?,?,'2026-01-03T00:00:00.000Z',s.identity,s.ruler,s.mode,?,?,b.record,'waiting',s.score,s.metric,0,
     s.measured_by,s.started_at,s.finished_at,?,? FROM state_board_measurement_selection s JOIN entity_weapon_builds b ON b.id=s.identity
     WHERE s.identity=? AND s.ruler=? AND s.mode=?`,calculationId(normalized.id,ruler,mode),retired,engine,cancelled,old,cancelledTask,normalized.id,ruler,mode);
 const fresh=await(await post('/api/board/review',{identity:normalized.id,ruler,mode,engine,
  observed_score:13,baseline_score:12,metric:'kpm',request:calculation})).json();
 assert(fresh.accepted&&fresh.id!==retired,'a new click cannot coalesce to a cancelled old execution');
 assert((await pass()).staged);await pass();await answer('5',13);await answer('6',13);
 assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',fresh.id)).state,'succeeded');
 assert.deepEqual(await db.get('SELECT to_jsonb(q) body FROM state_compute_scheduling q WHERE id=?',cancelled),cancelledBefore,'a fresh request never reopens or rewrites the cancelled task');
 for(let i=0;i<100;i++){const step=await flow.step();if(step.status==='idle')break;assert(i<99);}
 const newest=await worker.fetch(new Request('https://wfsim.app/api/board/pages?'+new URLSearchParams({weapon:record.weapon,ruler})),env,{waitUntil(){}});
 assert((await newest.json()).rows.some(row=>row.score===13));
 await db.run("UPDATE entity_measurement_reviews SET created_at=?,round_at='2026-01-02T00:00:00.000Z' WHERE id=?",old,review.id);
 await db.run("UPDATE entity_measurement_reviews SET created_at=?,round_at='2026-01-04T00:00:00.000Z' WHERE id=?",old,fresh.id);
 const invalid=await(await post('/api/board/review',{identity:normalized.id,ruler,mode,engine,
  observed_score:14,baseline_score:13,metric:'kpm',request:{...calculation,duration:calculation.duration+1}})).json();
 assert(invalid.accepted);assert((await pass()).rejected,'an inexact observation must not enter execution');
 assert.equal((await db.get('SELECT state FROM entity_measurement_reviews WHERE id=?',invalid.id)).state,'failed');
 assert.equal((await requestRow('review:'+invalid.id)).phase,'rejected');
 assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',normalized.id)).score,13);
 const actualRiven={weapon:'coda_catabolyst',benchmark:ruler,mode,requested:true,
  mods:['frostbite','riven','galvanized_diffusion','galvanized_shot','lethal_torrent','pistol_elementalist','pressurized_magazine','critical_mutation'],
  arcanes:['secondary_enervate'],exilus:'ruinous_extension',valence:'heat',
  riven_pos:['critical_damage','electricity','viral'],riven_neg:'zoom',riven_rolls:[1.02,0.99,1.08,1.03]};
 const rivenResponse=await post('/api/board/submit',actualRiven);
 assert.equal(rivenResponse.status,200,await rivenResponse.clone().text());
 const rivenRequest=(await rivenResponse.json()).request;
 const standardized=await normalizeBoardRequest(binary,await requestRow(rivenRequest));
 const actual=standardized.flow.builds.find(row=>row.id!==standardized.id);
 assert.deepEqual(actual.record.riven_rolls,actualRiven.riven_rolls);
 assert.deepEqual(standardized.record.riven_rolls,[1.1,1.1,1.1,0.9]);
 assert.deepEqual(standardized.flow.nodes.map(node=>node.identity),[standardized.id]);
 assert((await pass()).staged);assert(!(await pass()).pending);
 const rivenStatus=await (await post('/api/board/requests/status',{ids:[rivenRequest]})).json();
 assert.equal(rivenStatus.requests[0].identity,standardized.id);
 assert.deepEqual(JSON.parse((await db.get('SELECT record FROM entity_weapon_builds WHERE id=?',actual.id)).record),actual.record);
 await answer('7',88,standardized.id);
 assert(!(await db.get('SELECT 1 FROM state_board_measurement_selection WHERE identity=?',standardized.id)));
 await answer('8',88,standardized.id);
 assert.equal((await db.get('SELECT score FROM state_board_measurement_selection WHERE identity=?',standardized.id)).score,88);
 assert(!(await db.get('SELECT 1 FROM state_board_measurement_selection WHERE identity=?',actual.id)));
 await db.run(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,score,metric,measured_by,cost_seconds,started_at,finished_at)
   VALUES(?,?,?,99,'kpm',?,0,?,?)`,actual.id,ruler,mode,'verified:'+engine,old,old);
 for(let i=0;i<100;i++){const step=await flow.step();if(step.status==='idle')break;assert(i<99);}
 const rivenPage=await worker.fetch(new Request('https://wfsim.app/api/board/pages?'+new URLSearchParams({weapon:actualRiven.weapon,ruler})),env,{waitUntil(){}});
 assert.equal(rivenPage.status,200);
 const rivenRows=(await rivenPage.json()).rows;
 assert(rivenRows.some(row=>row.score===88),'the public board gets the independently verified standard measurement');
 assert(!rivenRows.some(row=>row.score===99),'the same projection excludes an accepted legacy actual-roll measurement');
 const otherRuler='demolisher';
 await db.run('INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?)','legacy-alias',old,JSON.stringify(normalized.record));
 await db.run(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,score,metric,measured_by,cost_seconds,started_at,finished_at)
   VALUES('legacy-alias',?,'base',1,'kpm',?,0,?,?)`,otherRuler,'verified:'+engine,old,old);
 await post('/api/board/submit',{...body,benchmark:otherRuler});assert((await pass()).staged);
 let refused=false;
 for(let i=0;i<100;i++){
  try{const step=await flow.step();if(step.status==='idle')break;}
  catch(error){assert.match(error.message,/source identities require qualification/);refused=true;break;}
 }
 assert(refused,'an unknown legacy alias must prevent partition selection');
 const unsafe=await db.get('SELECT * FROM state_board_read_partitions WHERE weapon=? AND ruler=?',record.weapon,otherRuler);
 assert(!unsafe.enabled&&!unsafe.canonical_sources&&unsafe.revision===null);
 assert((await db.get("SELECT last_error FROM state_board_input_mailbox WHERE scope='partition' AND key=?",JSON.stringify([record.weapon,otherRuler]))).last_error);
 const blocked=await db.get("SELECT available_at,last_failed_at FROM state_board_input_mailbox WHERE scope='partition' AND key=?",JSON.stringify([record.weapon,otherRuler]));
 assert.equal(blocked.available_at,null,'deterministic qualification failures must leave the retry queue until explicit repair');
 for(let i=0;i<100;i++){const step=await flow.step();if(step.status==='idle')break;assert(i<99);}
 assert.equal((await db.get("SELECT last_failed_at FROM state_board_input_mailbox WHERE scope='partition' AND key=?",JSON.stringify([record.weapon,otherRuler]))).last_failed_at,blocked.last_failed_at,'an idle pass cannot repeat the failed full qualification');
 const fallback=await worker.fetch(new Request('https://wfsim.app/api/board/pages?'+new URLSearchParams({weapon:record.weapon,ruler:otherRuler})),env,{waitUntil(){}});
 assert.equal(fallback.status,404,'an unqualified partition retains the complete static view');
 await db.run('UPDATE state_board_read_partitions SET enabled=false WHERE weapon=? AND ruler=?',record.weapon,ruler);
 try{await flow.step();}catch(error){assert.match(error.message,/qualification|selection changed/);}
 assert.equal((await db.get('SELECT enabled FROM state_board_read_partitions WHERE weapon=? AND ruler=?',record.weapon,ruler)).enabled,false,'requested preparation cannot undo an operator rollback');
 assert.deepEqual(await db.all("SELECT * FROM state_task_pipeline_controls WHERE name='admission'"),fences);
 assert.deepEqual(await db.all('SELECT * FROM entity_compute_order_cuts'),cut);
 assert.deepEqual(await db.all('SELECT * FROM relation_compute_order_cut_protection'),protectedRows,'fresh leases cannot change the retained protection manifest');
 const familyBase={weapon:'burston_prime',mods:['serration','split_chamber','point_strike','vital_sense','hellfire',
  'infected_clip','stormbringer','gilded_truth'],arcanes:['primary_merciless'],benchmark:ruler,mode,requested:true};
 const familyRequest=await post('/api/board/submit',familyBase);
 assert.equal(familyRequest.status,200);
 const {request:familyRequestId}=await familyRequest.json();
 const admitRequest=async id=>{
  for(let i=0;i<8;i++){
   const row=await requestRow(id);
   if(row.phase==='admitted')return row;
   assert(['pending','staged'].includes(row.phase),JSON.stringify(row));
   await pass();
  }
  assert.fail('requested intake did not admit '+id);
 };
 const {workflow:familyRoot}=await admitRequest(familyRequestId);
 assert.equal((await db.get('SELECT count(*) n FROM state_build_workflow_nodes WHERE workflow=?',familyRoot)).n,1);
 assert.deepEqual(await familyBoardPass(runner,{binary}),{idle:true},'player intake works while family dispatch is paused');
 await db.run("UPDATE state_task_pipeline_controls SET paused=false WHERE name='family'");
 const rootIdentity=(await db.get('SELECT build_id FROM entity_build_workflows WHERE id=?',familyRoot)).build_id;
 const familyKey='family:'+rootIdentity;
 const findOptimizer=()=>db.get(`SELECT n.* FROM state_build_workflow_nodes n JOIN record_build_workflow_triggers t ON t.workflow=n.workflow
   WHERE t.id=? AND n.definition->>'kind'='optimize' AND n.state='queued'`,familyKey);
 let optimizer;
 for(let i=0;i<96&&!optimizer;i++){await familyBoardPass(runner,{binary});optimizer=await findOptimizer();}
 assert(optimizer,'the separate family pass admits the incompatible variant while general admission is paused');
 assert.equal((await db.get('SELECT zone FROM state_compute_work WHERE id=?',optimizer.computation)).zone,3);
 assert(optimizer.definition.request.starts[0].slots.includes(null));
 assert(!optimizer.definition.request.starts[0].slots.includes('gilded_truth'));
 const witnesses=factory({verbs:{optimize_report:{cap:3,lease_ms:60000,canon}},producers:{build_workflow:BUILD_WORKFLOW_PRODUCER}});
 await witnesses.answerOfficial(env,{device:'official:family-fixture',owner:'official:family-fixture'},
  {question:optimizer.computation,engine,result:{report:{results:[{mods:[...familyBase.mods.slice(0,-1),'speed_trigger'],
   arcane:familyBase.arcanes,mode}]}},work:100});
 const familyInputId='workflow:'+optimizer.workflow+':'+optimizer.id;
 const ownedFamily=(await db.get('SELECT input FROM state_build_family_inbox WHERE id=?',familyInputId)).input;
 assert(!(await db.get('SELECT 1 FROM record_build_submissions WHERE id=?',familyInputId)),'a family job owns its own winning build');
 const retryFamily=record=>db.run(`INSERT INTO record_build_submissions(id,at,record,initiator,family_source)
   VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING`,familyInputId,old,record,ownedFamily.initiator,ownedFamily.family_source);
 await retryFamily(ownedFamily.record);
 await assert.rejects(retryFamily('{"weapon":"braton"}'),/family source already owns a different input/);
 assert.deepEqual((await db.get('SELECT input FROM state_build_family_inbox WHERE id=?',familyInputId)).input,ownedFamily);
 const continued=await familyBoardPass(runner,{binary});
 assert.equal(continued.input,familyInputId);
 assert(!(await db.get('SELECT 1 FROM state_build_family_inbox WHERE id=?',familyInputId)),'only canonical intake acknowledges the family input');
 for(let i=0;i<96;i++){if((await familyBoardPass(runner,{binary})).idle)break;assert(i<95);}
 const children=await db.all("SELECT definition,state FROM state_build_workflow_nodes WHERE workflow=? AND expansion<>?",optimizer.workflow,optimizer.expansion);
 assert(children.length>0);
 assert(children.every(child=>child.definition.kind==='measure'&&child.state==='queued'));
 assert.equal((await db.get('SELECT state FROM state_build_workflow_nodes WHERE workflow=? AND id=?',optimizer.workflow,optimizer.id)).state,'succeeded');
 assert.equal((await db.get('SELECT count(*) n FROM entity_build_workflows WHERE source->>\'family\'=\'true\' AND build_id=?',rootIdentity)).n,1);
 const replay=await post('/api/board/submit',familyBase);assert.equal(replay.status,200);
 await admitRequest((await replay.json()).request);
 assert.deepEqual(await familyBoardPass(runner,{binary}),{idle:true},'repeat submission creates no sibling workflow');
 assert.deepEqual(await db.all("SELECT * FROM state_task_pipeline_controls WHERE name='admission'"),fences);
 if(process.argv.includes('--break-raw-ownership') || process.argv.includes('--break-raw-budget')) {
  const body=readFileSync(new URL('../worker/pg/raw_inbox_retention.sql',import.meta.url),'utf8');
  const broken=process.argv.includes('--break-raw-ownership')
   ? body.replace("THEN owner_input->'record'=build_raw_inbox_json(row_input.record)",'THEN true')
   : body.replace('WHEN p.bytes>row_max','WHEN false');
  assert.notEqual(broken,body);
  await sql.unsafe(broken);
 }
 const retiredAt='2020-01-01';
 await sql.unsafe(`INSERT INTO record_build_submissions(id,at,record,initiator) VALUES
   ('retention-a','${retiredAt}','{"weapon":"braton"}','anonymous'),
   ('retention-b','${retiredAt}','{"weapon":"burston"}','anonymous'),
   ('retention-c','${retiredAt}','{"weapon":"braton"}','anonymous');
   INSERT INTO entity_board_requests(id,kind,input,ruler,mode)
    SELECT id,'missing',jsonb_build_object('id',id,'at',at,'initiator',initiator,'record','{"weapon":"braton"}'::jsonb),'r','base'
    FROM record_build_submissions WHERE id IN ('retention-a','retention-b');
   INSERT INTO state_board_requests(id) VALUES('retention-a'),('retention-b');`);
 const retentionPage={after:'retention-',limit:3};
 const dry=await rawInboxRetentionPage(sql,retentionPage);
 assert.equal(dry.groups.request_copy.rows,1);
 assert.equal(dry.groups.unconfirmed_input.rows,2);
 assert.equal(dry.removed,0);
 await sql.begin(async tx=>{
  await tx.unsafe("SELECT 1 FROM record_build_submissions WHERE id='retention-a' FOR UPDATE");
  assert.equal((await rawInboxRetentionPage(sql,{...retentionPage,apply:true})).removed,0,'locked sources are deferred');
 });
 assert.equal((await rawInboxRetentionPage(sql,{...retentionPage,apply:true})).removed,1);
 assert.deepEqual(await db.all("SELECT id FROM record_build_submissions WHERE id LIKE 'retention-%' ORDER BY id"),[
  {id:'retention-b'},{id:'retention-c'}]);
 assert.equal((await requestRow('retention-a')).phase,'pending','retiring the exact copy preserves the pending request');
 assert(!(await db.get("SELECT 1 FROM state_build_inbox_positions WHERE id='retention-a'")));
 assert(await db.get("SELECT 1 FROM record_build_submissions WHERE id='old-inbox'"));
 const acknowledged={after:legacyInput.id.slice(0,-1),limit:1};
 const known=await rawInboxRetentionPage(sql,acknowledged);
 assert.equal(known.next,legacyInput.id);assert.equal(known.groups.canonical_unproven.rows,1);
 assert.equal((await rawInboxRetentionPage(sql,{...acknowledged,apply:true})).removed,0);
 await db.run('DELETE FROM record_build_submissions WHERE id=?',legacyInput.id);
 const changed={...legacyInput,record:'{"weapon":"braton"}'};
 await db.run('INSERT INTO record_build_submissions(id,at,record,initiator,observation) VALUES(?,?,?,?,?)',...Object.values(changed));
 assert.equal((await rawInboxRetentionPage(sql,{...acknowledged,apply:true})).removed,0,'a reused source ID does not prove its payload');
 assert.equal((await db.get('SELECT record FROM record_build_submissions WHERE id=?',legacyInput.id)).record,changed.record);
 assert.deepEqual(await db.get('SELECT to_jsonb(i) body FROM entity_build_intake_intents i WHERE id=?',legacy),legacyIntent);
 await sql.unsafe(`INSERT INTO record_build_submissions(id,at,record,initiator,family_source)
   VALUES('workflow:retention:child','${retiredAt}','{"weapon":"braton"}','system','workflow|retention|child');
   INSERT INTO state_build_family_inbox(id,retry_at,input)
    SELECT id,at,to_jsonb(b) FROM record_build_submissions b WHERE id='workflow:retention:child';`);
 const mirrorId='workflow:retention:child';
 await sql.unsafe(`CREATE ROLE ${pipelineRole}; GRANT USAGE ON SCHEMA ${schema} TO ${pipelineRole};
   GRANT SELECT ON state_build_inbox_inputs TO ${pipelineRole};`);
 const checkReader=()=>sql.begin(async tx=>{
   await tx.unsafe(`SET LOCAL ROLE ${pipelineRole}`);
   let probes=0;
   const reader=inboxSource((text,args)=>{
     if(!args.length)probes++;
     return tx.unsafe(placeholders(text),args);
   });
   const [keyed]=await reader.byIds([mirrorId]),[page]=await reader.after('workflow:retention:',1);
   assert.equal(keyed.id,mirrorId);assert.deepEqual(keyed,page);
   const seen=[];
   await captureInbox({source:{...reader,priority:async()=>[{id:mirrorId}],fresh:async()=>[],after:async()=>[]},
     maxRows:1,emit:async(line,row)=>seen.push(row.id)});
   assert.equal(probes,1,'each reader uses one schema capability probe');
   return {owned:keyed.owned_family,seen};
 });
 assert.deepEqual(await checkReader(),{owned:undefined,seen:[mirrorId]},'old intake grants still read the raw inbox');
 await sql.unsafe(`GRANT SELECT (id) ON state_build_family_inbox TO ${pipelineRole}`);
 assert.deepEqual(await checkReader(),{owned:true,seen:[]},'the new grant enables independent ownership');
 const familyMirror={after:'workflow:retention:',limit:1};
 assert.equal((await rawInboxRetentionPage(sql,familyMirror)).groups.family_copy.rows,1);
 assert.equal((await rawInboxRetentionPage(sql,{...familyMirror,apply:true})).removed,1);
 assert(await db.get("SELECT 1 FROM state_build_family_inbox WHERE id='workflow:retention:child'"));
 await sql.unsafe(`INSERT INTO record_build_submissions(id,at,record,initiator)
   VALUES('retirement-large-a','${retiredAt}',jsonb_build_object('weapon','braton','padding',repeat('x',131072))::text,'anonymous');
   INSERT INTO entity_board_requests(id,kind,input,ruler,mode)
    SELECT id,'missing',jsonb_build_object('id',id,'at',at,'initiator',initiator,'record',record::jsonb),'r','base'
    FROM record_build_submissions WHERE id='retirement-large-a';
   INSERT INTO state_board_requests(id) VALUES('retirement-large-a');`);
 const large=await db.get("SELECT octet_length(record) logical,pg_column_size(record) stored FROM record_build_submissions WHERE id='retirement-large-a'");
 assert(large.logical>65536 && large.stored<65536,'the test input is physically compressed below its logical budget');
 const oversized=await rawInboxRetentionPage(sql,{after:'retirement-large-',limit:1,apply:true});
 assert.equal(oversized.groups.oversize_input?.rows,1,'the logical row budget protects compressed inputs');
 assert.equal(oversized.removed,0);
 assert(await db.get("SELECT 1 FROM record_build_submissions WHERE id='retirement-large-a'"));
 await sql.unsafe(`INSERT INTO record_build_submissions(id,at,record)
   SELECT 'zz-budget-'||lpad(n::text,3,'0'),'${retiredAt}',repeat('y',60000) FROM generate_series(1,160) n;`);
 const bounded=await rawInboxRetentionPage(sql,{after:'zz-budget-',limit:160});
 assert(bounded.scanned<160 && bounded.groups.unconfirmed_input.bytes<=8*1024*1024);
 const remainder=await rawInboxRetentionPage(sql,{after:bounded.next,limit:160});
 assert.equal(bounded.scanned+remainder.scanned,160,'a page budget defers remaining rows to the next cursor');
 console.log(JSON.stringify({ok:true,source_kinds:2,canonical_identity:normalized.id,community_witnesses:8,standard_riven:standardized.id,
  published_score:13,old_inbox_retained:true,bulk_admission_paused:true,active_cut_preserved:true,public_work_and_answers:true,
  automatic_partition:true,legacy_unadmitted_replaced:true,cancelled_execution_preserved:true,unqualified_static_retained:true,namespace_isolation:true,
  retained_prefix_priority_preserved:true,shared_held_lease_preserved:true,explicit_missing_zone:0,
  family_creation_and_native_continuation:true,raw_retirement_requires_ownership:true,old_intake_grants_work:true}));
}catch(error){console.error({query:error.query,parameters:error.parameters,where:error.where});throw error;}finally{
 await flow?.close();await runner?.end();await sql.unsafe('DROP SCHEMA '+schema+' CASCADE');
 await sql.unsafe('DROP ROLE IF EXISTS '+runnerRole+','+pipelineRole);await sql.end();rmSync(root,{recursive:true,force:true});Math.random=realRandom;
}
