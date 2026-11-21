// SPDX-License-Identifier: AGPL-3.0-or-later
// Acknowledged canonical input survives admission pauses, crashes and retries independently of task creation.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {randomBytes,createHash} from 'node:crypto';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {createInterface} from 'node:readline';
import {pgDb,placeholders,postgresClient} from '../worker/db.js';
import {facts} from '../worker/factory.js';
import {prepareIntakeBuild} from './build_batches.mjs';
import {captureInbox,inboxSource} from './inbox_reader.mjs';
import {buildAdmissionSchemaSql,buildAdmissionIndexSql} from './build_admission_schema.mjs';

async function openDatabase() {
  const url=process.env.WFSIM_BUILD_ADMISSION_TEST_URL;
  if(!url)return (await import('./pg/test_db.mjs')).testDb('library');
  assert(['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname));
  const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
  const schema='build_admission_'+randomBytes(6).toString('hex');
  const sql=postgres(url,{max:4,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000,lock_timeout:2000},
    types:{bigint:{to:20,from:[20,1700],parse:Number,serialize:String}}});
  await sql.unsafe(`CREATE SCHEMA ${schema}`);
  try {
    await sql.unsafe(['schema','library','sync','inventory','build_workflows','board_admission']
      .map(name=>readFileSync(new URL(`../worker/pg/${name}.sql`,import.meta.url),'utf8')).join('\n'));
    const client=postgresClient(sql);
    return Object.assign(pgDb(client),{
      exec:async text=>{
        if(!/^\s*BEGIN;/.test(text))return sql.unsafe(text);
        const held=await sql.reserve();
        try{return await held.unsafe(text).simple();}
        catch(error){await held.unsafe('ROLLBACK');throw error;}
        finally{await held.release();}
      },
      run:async(text,...args)=>(await client.query(placeholders(text),args)).affected,
      get:async(text,...args)=>(await client.query(placeholders(text),args)).rows[0]||null,
      all:async(text,...args)=>(await client.query(placeholders(text),args)).rows,
      close:async()=>{try{await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`);}finally{await sql.end();}},
    });
  }catch(error){await sql.unsafe(`DROP SCHEMA ${schema} CASCADE`);await sql.end();throw error;}
}
const db=await openDatabase(),env={LIBRARY:db},f=facts();
const schemaSource=readFileSync(new URL('../worker/pg/build_workflows.sql',import.meta.url),'utf8');
if(process.argv.includes('--break-eager'))await db.exec(`CREATE OR REPLACE FUNCTION build_intake_batch(text) RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path FROM CURRENT AS $$ BEGIN PERFORM build_intake_register($1);END $$;`);
if(process.argv.includes('--break-claim')) {
  const original=schemaSource.match(/CREATE OR REPLACE FUNCTION build_admission_run[^]*?END \$\$;/)[0];
  const broken=original.replace(' AND lease=$3 AND lease_until>$5','');assert.notEqual(broken,original);await db.exec(broken);
}
if(process.argv.includes('--break-alias-force')) {
  const original=schemaSource.match(/CREATE OR REPLACE FUNCTION build_workflow_admit_nodes[^]*?END \$\$;/)[0];
  const broken=original.replace(" AND NOT EXISTS(SELECT 1 FROM state_build_workflow_nodes prior\n        WHERE prior.workflow=workflow_id AND prior.computation=question AND prior.task_id IS NOT NULL)",'');
  assert.notEqual(broken,original);await db.exec(broken);
}
if(process.argv.includes('--break-first-arrival')) {
  const original=schemaSource.match(/CREATE OR REPLACE FUNCTION build_intake_register[^]*?END \$\$;/)[0];
  const broken=original.replace(" OR coalesce((input->>'admission_new')::boolean,false)",'');
  assert.notEqual(broken,original);await db.exec(broken);
}
const databaseTime=async()=>Number((await db.get('SELECT ceil(extract(epoch FROM clock_timestamp())*1000)::bigint now')).now);
let now=await databaseTime();
const at=new Date(now).toISOString();
let ordinal=0;
const fixture=async({initiator='anonymous',system_source,...extra}={})=>{
  const id=(++ordinal).toString(16).padStart(32,'0');
  const templates=Array.from({length:4},(_,n)=>({weapon:'braton',record:{weapon:'braton',mods:['serration']},runs:1,seed:ordinal*100+n,duration:2}));
  return prepareIntakeBuild({id,at,initiator,from:['source-'+id],record:{weapon:'braton',mods:['serration']},system_source,...extra,
    flow:{schema:1,scope:{boards:[{id:'standard_single_target'},{id:'standard_multi_target'}]},builds:[],templates,patches:[{set:{},remove:[]}],
      nodes:templates.map((_,n)=>({kind:'measure',identity:id,ruler:n<2?'standard_single_target':'standard_multi_target',mode:n%2?'cycle':'base',template:n,patch:0}))}});
};
const intake=async input=>{
  const result=await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
  const observed=await databaseTime();
  now=Math.max(now,observed);
  return result;
};
const claim=async(kind,time=now)=>(await db.get('SELECT build_admission_claim(?::text,?::text) claim',kind,new Date(time).toISOString())).claim;
const run=async(job,max=2,time=now)=>(await db.get('SELECT build_admission_run(?::text,?::text,?::text,?::bigint,?::text) result',job.kind,job.key,job.lease,max,new Date(time).toISOString())).result;
const count=async table=>Number((await db.get('SELECT count(*) n FROM '+table)).n);
async function crashAfterClaim() {
  if(!process.env.WFSIM_BUILD_ADMISSION_TEST_URL)return claim('intake');
  const {schema}=await db.get('SELECT current_schema() schema');assert.match(schema,/^[a-z_][a-z0-9_]*$/);
  const code=`import postgres from ${JSON.stringify(new URL('./pg/node_modules/postgres/src/index.js',import.meta.url).href)};
    const sql=postgres(process.env.WFSIM_BUILD_ADMISSION_TEST_URL,{max:1,fetch_types:false,onnotice(){},connection:{search_path:${JSON.stringify(schema)},statement_timeout:5000}});
    const [row]=await sql.unsafe("SELECT build_admission_claim('intake',$1::text) claim",[${JSON.stringify(new Date(now).toISOString())}]);
    console.log(JSON.stringify(row.claim));await new Promise(resolve=>setTimeout(resolve,60000));await sql.end();`;
  const child=spawn(process.execPath,['--input-type=module','-e',code],{stdio:['ignore','pipe','pipe']});
  const lines=createInterface({input:child.stdout});let timer;
  try {
    const failed=new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('claim child did not acknowledge its committed lease')),10000);});
    const [line]=await Promise.race([once(lines,'line'),failed]);clearTimeout(timer);
    const job=JSON.parse(line);assert(job?.lease);
    const ended=once(child,'exit');assert(child.kill('SIGKILL'));const [,signal]=await ended;assert.equal(signal,'SIGKILL');
    return job;
  }finally{clearTimeout(timer);lines.close();if(child.exitCode===null&&child.signalCode===null){const ended=once(child,'exit');child.kill('SIGKILL');await ended;}}
}
const drain=async(time=now)=>{
  for(let n=0;n<100 && await count('state_build_admission_work');n++)await f.runStage(env,'admission',time,{max:2,budget_ms:5000});
  assert.equal(await count('state_build_admission_work'),0);
};
async function checkQueryBounds(input) {
  if(process.argv.includes('--break-trigger-index'))await db.exec('DROP INDEX build_workflow_triggers_workflow');
  if(process.argv.includes('--break-consumer-bound')) {
    const original=schemaSource.match(/CREATE OR REPLACE FUNCTION build_intake_register[^]*?END \$\$;/)[0];
    const broken=original.replace(" AND d.ref>=flow_id AND d.ref<flow_id||'}'\n          ",' ');
    assert.notEqual(broken,original);await db.exec(broken);
  }
  if(process.argv.includes('--break-expansion-bounds')) {
    for(const name of ['build_workflow_capture','build_workflow_admit_nodes','build_admission_run']) {
      const original=schemaSource.match(new RegExp('CREATE OR REPLACE FUNCTION '+name+'[^]*?END \\$\\$;'))[0];
      const broken=original.replace('workflow=$1 AND expansion=expansion_id','expansion=expansion_id')
        .replaceAll('workflow=workflow_id AND expansion=expansion_id','expansion=expansion_id')
        .replaceAll('node.workflow=workflow_id AND node.expansion=expansion_id','node.expansion=expansion_id')
        .replace('FROM entity_build_workflow_expansions e JOIN state_build_workflow_nodes n ON n.workflow=e.workflow\n      WHERE e.id=held.key AND n.expansion=e.id',
          'FROM state_build_workflow_nodes n WHERE n.expansion=held.key');
      assert.notEqual(broken,original);await db.exec(broken);
    }
  }
  const {workflow}=await db.get('SELECT id workflow FROM entity_build_workflows WHERE build_id=?',input.id);
  const literal=value=>"'"+String(value).replaceAll("'","''")+"'";
  await db.exec(`INSERT INTO entity_build_workflow_scopes VALUES('query-bounds','{}');
    INSERT INTO entity_build_workflows(id,build_id,scope_key,kind,scope_id,source,created_at)
      SELECT 'scale-'||n,${literal(input.id)},'scale-'||n,'intake','query-bounds','{}',${literal(at)} FROM generate_series(1,4096) n;
    INSERT INTO record_build_workflow_triggers(id,workflow,source,at)
      SELECT id||':scale',id,'{}',created_at FROM entity_build_workflows WHERE scope_id='query-bounds';
    INSERT INTO entity_build_workflow_expansions(id,workflow,plan,created_at)
      SELECT id||':root',id,'{}',created_at FROM entity_build_workflows WHERE scope_id='query-bounds';
    INSERT INTO state_build_workflow_nodes(workflow,id,expansion,definition,state,retry_at,queue_position)
      SELECT workflow,'0',id,'{}','pending',created_at,nextval('compute_queue_position') FROM entity_build_workflow_expansions WHERE workflow LIKE 'scale-%';
    INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,zone)
      SELECT 'build_workflow',workflow||'|0','scale-'||workflow,'live',created_at,created_at,9
      FROM entity_build_workflow_expansions WHERE workflow LIKE 'scale-%';`);
  for(const parent of ['bounded','untouched'])await db.run('SELECT build_workflow_capture(?::text,?::text,(?::text)::jsonb,?::text,NULL)',workflow,parent,JSON.stringify(input.flow),at);
  const probes=['','ab','x%_','中文','ab|nested',workflow];
  const refs=probes.flatMap(flow=>['','|','|left|right','|中文','_','a','{','}','}outside'].map(suffix=>({ref:flow+suffix,producer:'build_workflow',state:'live'})));
  refs.push({ref:workflow+'|served',producer:'build_workflow',state:'served'},{ref:workflow+'|other',producer:'another',state:'live'});
  await db.run(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,zone)
    SELECT producer,ref,'query-bounds',state,?2,?2,9 FROM jsonb_to_recordset((?1::text)::jsonb) x(producer text,ref text,state text) ON CONFLICT DO NOTHING`,JSON.stringify(refs),at);
  await db.exec('ANALYZE relation_task_consumers;ANALYZE state_build_workflow_nodes;ANALYZE entity_build_workflow_expansions;ANALYZE record_build_workflow_triggers;');
  const body=async signature=>(await db.get('SELECT prosrc body FROM pg_proc WHERE oid=?::regprocedure',signature)).body;
  const register=await body('build_intake_register(text)');
  const update=register.match(/UPDATE relation_task_consumers d SET[^;]+;/)[0]
    .replaceAll("coalesce(input#>>ARRAY['deadlines',reason],at)",'$3').replaceAll(/\bpriority\b/g,'$2').replaceAll(/\bflow_id\b/g,'$1');
  const predicate=update.slice(update.indexOf('WHERE ')+6).replace(/;$/,'');
  for(const flow of probes) {
    const expected=await db.all("SELECT d.producer,d.ref,d.state FROM relation_task_consumers d WHERE d.producer='build_workflow' AND split_part(d.ref,'|',1)=?1 AND d.state='live' ORDER BY d.ref",flow);
    const actual=await db.all('SELECT d.producer,d.ref,d.state FROM relation_task_consumers d WHERE '+predicate.replaceAll('$1','?1')+' ORDER BY d.ref',flow);
    assert.deepEqual(actual,expected,'consumer bound must preserve bare, empty, pipe, wildcard, Unicode and neighboring refs');
  }
  const flatten=plan=>[plan,...(plan.Plans||[]).flatMap(flatten)];
  const explain=async(name,sql,types,args)=>{
    const result=await db.exec(`BEGIN;SET LOCAL plan_cache_mode='force_generic_plan';
      PREPARE ${name}(${types}) AS ${sql.replace(/;$/,'')};
      EXPLAIN(FORMAT JSON) EXECUTE ${name}(${args.map(literal).join(',')});DEALLOCATE ${name};ROLLBACK;`);
    const rows=result.flatMap(part=>part.rows||part),plan=rows.find(row=>row['QUERY PLAN'])?.['QUERY PLAN'];
    assert(plan,'missing actual prepared query plan: '+name);return flatten(plan[0].Plan);
  };
  const consumer=await explain('bounds_consumer',update,'text,bigint,text',[workflow,3,at]);
  assert(consumer.some(node=>node['Relation Name']==='relation_task_consumers' && /producer/.test(node['Index Cond']||'') && /ref >= \$1/.test(node['Index Cond']||'') && /ref < /.test(node['Index Cond']||'')),
    'consumer update must seek both reference bounds through the existing index under a generic plan');
  const capture=await body('build_workflow_capture(text,text,jsonb,text,jsonb)');
  const admit=await body('build_workflow_admit_nodes(text,bigint,text)');
  const runBody=await body('build_admission_run(text,text,text,bigint,text)');
  const queries=[
    ['capture','SELECT min(queue_position) '+capture.match(/FROM state_build_workflow_nodes WHERE[^;]+? HAVING/)[0].replace(/ HAVING$/,'')],
    ['slice',admit.match(/FOR n IN (SELECT \* FROM state_build_workflow_nodes[^]+?) LOOP/)[1].replace('LIMIT $2','LIMIT 1')],
    ['node_update',admit.match(/UPDATE state_build_workflow_nodes node SET[^;]+;/)[0]],
    ['review_update',admit.match(/UPDATE entity_measurement_reviews r SET[^;]+;/)[0]],
    ['sources','SELECT node.id '+admit.match(/FROM state_build_workflow_nodes node JOIN record_build_workflow_triggers[^;]+? ON CONFLICT/)[0].replace(/ ON CONFLICT$/,'')],
    ['pending',runBody.match(/SELECT EXISTS\([^;]+\) INTO pending/)[0].replace(' INTO pending','')],
  ];
  for(const [name,source] of queries) {
    const sql=source.replaceAll('workflow=$1','workflow=workflow_id').replaceAll(/\bworkflow_id\b/g,'$1')
      .replaceAll(/\bexpansion_id\b/g,'$2').replaceAll('held.key','$2').replaceAll(/\btouched\b/g,'$3');
    const types=sql.includes('$3')?'text,text,text[]':sql.includes('$2')?'text,text':'text';
    const args=[workflow,workflow+':bounded','{bounded.0}'].slice(0,types.split(',').length);
    const nodes=await explain('bounds_'+name,sql,types,args);
    assert(nodes.some(node=>node['Relation Name']==='state_build_workflow_nodes' && /workflow = /.test(node['Index Cond']||'')),
      name+' must constrain the node index by the expansion owner workflow');
    if(name==='sources')assert(nodes.some(node=>node['Index Name']==='build_workflow_triggers_workflow' && /workflow = /.test(node['Index Cond']||'')),
      'source bindings must seek captured triggers by workflow instead of scanning their history per node');
  }
  const before=await db.all('SELECT producer,ref,state,zone,due_at FROM relation_task_consumers ORDER BY producer,ref');
  const due='2025-01-01T00:00:00.000Z';
  await db.run('SELECT build_intake_register(?::text)',JSON.stringify([{...input,from:['query-bounds-repeat'],deadlines:{intake:due}}]));
  const after=await db.all('SELECT producer,ref,state,zone,due_at FROM relation_task_consumers ORDER BY producer,ref');
  assert.deepEqual(after,before.map(row=>row.producer==='build_workflow' && row.state==='live' && row.ref.split('|')[0]===workflow
    ? {...row,zone:Math.min(row.zone??3,3),due_at:row.due_at<due?row.due_at:due}:row));
  const held=await db.all('SELECT workflow,id,state,task_id FROM state_build_workflow_nodes WHERE expansion<>? ORDER BY workflow,id',workflow+':bounded');
  const processed=await db.get('SELECT build_workflow_admit_nodes(?::text,1,?::text) n',workflow+':bounded',at);assert.equal(Number(processed.n),1);
  assert.deepEqual(await db.all('SELECT workflow,id,state,task_id FROM state_build_workflow_nodes WHERE expansion<>? ORDER BY workflow,id',workflow+':bounded'),held);
  const sourceQuery=queries.find(([name])=>name==='sources')[1]
    .replace('SELECT node.id ','SELECT node.id,t.id trigger_id,t.source ')
    .replaceAll(/\bworkflow_id\b/g,literal(workflow)).replaceAll(/\bexpansion_id\b/g,literal(workflow+':bounded'))
    .replaceAll(/\btouched\b/g,literal('{bounded.0}')+'::text[]')+' ORDER BY node.id,t.id';
  const indexed=await db.all(sourceQuery);assert(indexed.length>0,'source equivalence must exercise admitted work');
  const scanned=await db.exec('BEGIN;SET LOCAL enable_indexscan=off;SET LOCAL enable_bitmapscan=off;'+sourceQuery+';ROLLBACK;');
  assert.deepEqual(scanned.flatMap(part=>part.rows||part).filter(row=>row.trigger_id),indexed);
  const bindings=await db.all(`SELECT s.trigger_id,s.payload::jsonb->'source' source FROM relation_compute_task_sources s
    JOIN state_build_workflow_nodes n ON n.task_id=s.task_id AND s.ref=n.workflow||'|'||n.id
    WHERE n.workflow=?1 AND n.id='bounded.0' AND s.producer='build_workflow'
      AND EXISTS(SELECT 1 FROM record_build_workflow_triggers t WHERE t.id=s.trigger_id AND t.workflow=n.workflow) ORDER BY s.trigger_id`,workflow);
  assert.deepEqual(bindings,indexed.map(({trigger_id,source})=>({trigger_id,source})));
  console.log('ok: generic plans seek consumer refs, all six workflow node queries and captured triggers amid 4096 unrelated workflows; exact matching, promotion and one-node slice preserve other work');
}
try {
  const input=await fixture();
  await db.run('INSERT INTO record_build_submissions(id,at,record,initiator) VALUES(?,?,?,?)',input.from[0],at,JSON.stringify(input.record),input.initiator);
  await f.controlStage(env,'admission',{paused:true,by:'fixture'},now);
  await intake(input);
  assert.equal(await count('entity_weapon_builds'),1);assert.equal(await count('entity_build_intake_intents'),1);
  assert.equal(await count('entity_build_workflows'),0);assert.equal(await count('state_compute_scheduling'),0);
  const captured=await db.get('SELECT id,input FROM entity_build_intake_intents');
  await db.run('SELECT build_spend_submissions(?::text)',JSON.stringify(input.from));
  assert.equal(await count('record_build_submissions'),0);assert.equal(await count('state_build_inbox_positions'),0);
  assert.equal((await f.runStage(env,'admission',now)).paused,true);
  assert.equal(await count('state_build_admission_work'),1);
  await Promise.all([intake(input),intake({...input,deadlines:{intake:'2099-01-01'}})]);
  assert.equal(await count('entity_build_intake_intents'),1);
  assert.deepEqual((await db.get('SELECT input FROM entity_build_intake_intents')).input,captured.input);
  await assert.rejects(db.run("UPDATE entity_build_intake_intents SET input='{}'"),/immutable/);
  console.log('ok: canonical build and immutable obligation survive source ACK, pause and concurrent retries before any task exists');

  await f.controlStage(env,'admission',{paused:false,by:'fixture'},now);
  assert.equal(await claim('intake',Date.parse(captured.input.admission_at)-1),null);
  const initial=await claim('intake');await run(initial);
  assert.equal(await count('state_build_workflow_nodes'),4);assert.equal(await count('state_compute_scheduling'),0);
  const expansion=await claim('expansion');assert.equal((await run(expansion)).processed,2);
  assert.equal(await count('state_compute_scheduling'),2);
  assert.equal(Number((await db.get("SELECT count(*) n FROM state_build_workflow_nodes WHERE state='pending'")).n),2);
  await assert.rejects(run(expansion),/claim changed/);
  await drain();assert.equal(await count('state_compute_scheduling'),4);
  const positions=(await db.all('SELECT zone_position FROM state_compute_scheduling ORDER BY zone_position')).map(r=>Number(r.zone_position));
  assert.deepEqual(positions,captured.input.admission_positions);
  assert.equal(Number((await db.get('SELECT count(*) n FROM state_compute_scheduling q WHERE NOT EXISTS(SELECT 1 FROM relation_compute_task_sources s WHERE s.task_id=q.task_id)')).n),0);
  assert.equal(await count('record_candidate_answers'),0);assert.equal(await count('record_hourly_compute_totals'),0);
  console.log('ok: bounded pending-node commits preserve captured FIFO positions and source bindings without changing results or credit');

  const crash=await fixture();await intake(crash);
  const lost=await crashAfterClaim();assert.equal(await claim('intake'),null);
  const resumed=await claim('intake',now+120001);assert(resumed && resumed.lease!==lost.lease);
  await assert.rejects(run(lost,2,now+120001),/claim changed/);
  assert.equal((await db.get('SELECT build_admission_retry(?::text,?::text,?::text,?::text,?::text) changed',lost.kind,lost.key,lost.lease,'old worker',at)).changed,false);
  await run(resumed,2,now+120001);await drain(now+120001);
  assert.equal(await count('entity_build_intake_intents'),2);assert.equal(await count('state_compute_scheduling'),8);
  console.log('ok: a committed claim outlives its worker; expiry replaces its token and stale run/retry cannot consume the new claim');

  const fault=await fixture();await intake(fault);await run(await claim('intake'));
  await db.exec(`CREATE OR REPLACE FUNCTION admission_ack_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'cut before admission ACK';END $$;
    CREATE TRIGGER admission_ack_fault BEFORE DELETE ON state_build_admission_work FOR EACH ROW WHEN(OLD.kind='expansion') EXECUTE FUNCTION admission_ack_fault();`);
  await assert.rejects(f.runStage(env,'admission',now,{max:16}),/before admission ACK/);
  assert.equal(await count('state_compute_scheduling'),8,'node admission must roll back with its failed ACK');
  assert.equal(Number((await db.get("SELECT count(*) n FROM state_build_workflow_nodes n JOIN entity_build_workflows f ON f.id=n.workflow WHERE f.build_id=? AND n.state='pending'",fault.id)).n),4);
  await db.exec('DROP TRIGGER admission_ack_fault ON state_build_admission_work');
  await drain(now+31000);assert.equal(await count('state_compute_scheduling'),12);
  assert.equal((await f.pipelineStatus(env,now)).find(s=>s.name==='admission').progress.failures,1);
  console.log('ok: failure between node admission and ACK rolls back both; retry preserves the canonical source and creates one set of tasks');

  await db.run("DELETE FROM state_build_admission_turns WHERE kind='intake'");
  const requested1=await fixture(),bulk=await fixture({initiator:'system'}),requested2=await fixture();
  await intake(requested1);await intake(bulk);await intake(requested2);
  const picked=[];
  for(let n=0;n<3;n++){const job=await claim('intake');picked.push(job);await run(job);}
  assert.deepEqual(picked.map(j=>j.lane),['requested','bulk','requested']);
  assert(picked[0].position<picked[2].position);
  await drain();
  const ordinary=await fixture(),urgent=await fixture({initiator:'system',system_source:{id:'explicit-system-source'},zone:0});
  await intake(ordinary);await intake(urgent);
  const urgentJob=await claim('intake');assert.equal(urgentJob.zone,0);await run(urgentJob);await drain();
  console.log('ok: requested and retained bulk lanes both progress in FIFO order; declared task zones precede lane fairness');

  for(const id of ['legacy-a','legacy-b'])await db.run('INSERT INTO record_build_submissions(id,at,record) VALUES(?,?,?)',id,at,'{"weapon":"braton"}');
  await db.exec("DELETE FROM state_build_inbox_positions WHERE id LIKE 'legacy-%'");
  for(const id of ['z-fresh-first','a-fresh-second'])await db.run('INSERT INTO record_build_submissions(id,at,record) VALUES(?,?,?)',id,at,'{"weapon":"braton"}');
  const selected=[];await captureInbox({source:inboxSource((text,args)=>db.all(text,...args)),maxRows:4,emit:async(line,row)=>selected.push(row.id)});
  assert.deepEqual(selected.slice(0,2),['z-fresh-first','a-fresh-second']);
  assert(selected.includes('legacy-a')&&selected.includes('legacy-b'));
  console.log('ok: future arrivals use indexed arrival FIFO while each bounded capture reserves progress for the retained legacy scan');
  const aliases=await fixture({initiator:'system',system_source:{id:'bounded-alias-round'}});
  aliases.flow.nodes.forEach(n=>{n.template=0;});await prepareIntakeBuild(aliases);await intake(aliases);
  await run(await claim('intake'),1);const aliasClaim=await claim('expansion');await run(aliasClaim,1);
  const first=await db.get("SELECT n.computation,n.task_id FROM state_build_workflow_nodes n JOIN entity_build_workflows w ON w.id=n.workflow WHERE w.build_id=? AND n.state='queued'",aliases.id);
  await f.answerOfficial(env,{device:'official:alias-fixture',owner:'official:alias-fixture'},
    {question:first.computation,engine:'e',result:{score:7,metric:'kpm'},work:1});
  await drain();
  const aliasRows=await db.all('SELECT n.task_id FROM state_build_workflow_nodes n JOIN entity_build_workflows w ON w.id=n.workflow WHERE w.build_id=?',aliases.id);
  assert.equal(aliasRows.length,4);assert(aliasRows.every(n=>n.task_id===first.task_id),'a result accepted between slices must not force a second round for an alias of the same captured workflow');
  console.log('ok: aliases admitted in later slices share the captured forced round after its result has already been accepted');
  await checkQueryBounds(input);
  const baseline=JSON.parse(readFileSync(new URL('../tests/fixtures/build-admission-base5a/source.json',import.meta.url),'utf8'));
  const loadBaseline=name=>{
    const body=readFileSync(new URL('../tests/fixtures/build-admission-base5a/'+name,import.meta.url));
    assert.equal(createHash('sha256').update(body).digest('hex'),baseline.files[name]);return body.toString();
  };
  await db.exec(`TRUNCATE entity_weapon_builds CASCADE;
    DROP INDEX build_workflow_triggers_workflow;
    DROP TABLE relation_build_intake_sources,entity_build_intake_intents,entity_build_intake_plans,state_build_admission_work,state_build_admission_turns,state_build_inbox_positions CASCADE;
    DROP TRIGGER build_inbox_positioned ON record_build_submissions;DROP TRIGGER equipment_inbox_positioned ON record_equipment_submissions;
    ALTER TABLE state_build_workflow_nodes DROP COLUMN queue_position;`);
  await db.exec(loadBaseline('build_workflows.sql'));await db.exec(loadBaseline('compute_admit_tasks.sql'));
  const oldInput=await fixture();await intake(oldInput);
  const oldNode=await db.get('SELECT computation,task_id FROM state_build_workflow_nodes ORDER BY position LIMIT 1');
  await f.answerOfficial(env,{device:'official:upgrade-fixture',owner:'official:upgrade-fixture'},
    {question:oldNode.computation,engine:'e',result:{score:11,metric:'kpm'},work:1});
  const frozen=await db.all('SELECT id,task_id,state,result_id,zone_position,lease FROM state_compute_scheduling ORDER BY id');
  const results=await db.all('SELECT * FROM entity_compute_results ORDER BY id');
  const {storage}=await db.get("SELECT relfilenode::text storage FROM pg_class WHERE oid='state_build_workflow_nodes'::regclass");
  await db.exec('DROP INDEX build_workflow_nodes_pending');
  await assert.rejects(db.exec(buildAdmissionSchemaSql({role:null})),/indexes must already be valid/);
  await db.exec('ROLLBACK');
  const restore=loadBaseline('build_workflows.sql').match(/CREATE INDEX IF NOT EXISTS build_workflow_nodes_pending[^;]+;/)[0];await db.exec(restore);
  const upgrade=buildAdmissionSchemaSql({role:null});
  assert(!upgrade.includes('CREATE INDEX IF NOT EXISTS build_workflow_triggers_workflow'),'minimum upgrade must not build the additive index inside its transaction');
  await assert.rejects(db.exec(upgrade),/additive workflow trigger index must already be valid/);await db.exec('ROLLBACK');
  if(process.env.WFSIM_BUILD_ADMISSION_TEST_URL) {
    await db.exec("INSERT INTO record_build_workflow_triggers SELECT id||':duplicate',workflow,source,at FROM record_build_workflow_triggers");
    await assert.rejects(db.exec('CREATE UNIQUE INDEX CONCURRENTLY build_workflow_triggers_workflow ON record_build_workflow_triggers(workflow)'),/duplicate|could not create unique/i);
    assert.equal((await db.get("SELECT indisvalid FROM pg_index WHERE indexrelid='build_workflow_triggers_workflow'::regclass")).indisvalid,false);
  }else await db.exec('CREATE INDEX build_workflow_triggers_workflow ON record_build_workflow_triggers(id,workflow)');
  await assert.rejects(db.exec(upgrade),/additive workflow trigger index must already be valid/);await db.exec('ROLLBACK');
  await db.exec('DROP INDEX '+(process.env.WFSIM_BUILD_ADMISSION_TEST_URL?'CONCURRENTLY ':'')+'build_workflow_triggers_workflow');
  await db.exec(buildAdmissionIndexSql({concurrently:!!process.env.WFSIM_BUILD_ADMISSION_TEST_URL}));
  const familyInput='workflow:old-parent:old-node',familyRecord='{"weapon":"braton"}';
  // Exercise the deployed family queue shape with its outstanding raw reference.
  await db.exec(`ALTER TABLE state_build_family_inbox DROP COLUMN input;
    ALTER TABLE state_build_family_inbox ADD CONSTRAINT state_build_family_inbox_id_fkey
      FOREIGN KEY(id) REFERENCES record_build_submissions(id) ON DELETE CASCADE;`);
  await db.run('INSERT INTO record_build_submissions(id,at,record,family_source) VALUES(?,?,?,?)',
    familyInput,at,familyRecord,'workflow|old-parent|old-node');
  await db.run('INSERT INTO state_build_family_inbox(id,retry_at) VALUES(?,?)',familyInput,at);
  await db.exec(buildAdmissionSchemaSql({role:null}));await db.exec(buildAdmissionSchemaSql({role:null}));
  const source=inboxSource((text,args)=>db.all(text,...args));
  const seen=[];
  await captureInbox({source:{...source,priority:async()=>[{id:familyInput}],fresh:async()=>[],after:async()=>[]},
    maxRows:1,emit:async(line,row)=>seen.push(row.id)});
  assert.deepEqual(seen,[],'general intake skips a retained family mirror');
  await db.run('DELETE FROM record_build_submissions WHERE id=?',familyInput);
  await db.exec("UPDATE state_task_pipeline_controls SET paused=false WHERE name='family'");
  const {input:ownedFamily}=await db.get('SELECT build_family_claim(?::text) AS input',at);
  assert.equal(ownedFamily.id,familyInput);assert.equal(ownedFamily.record,familyRecord);
  assert.equal(ownedFamily.family_source,'workflow|old-parent|old-node');
  if(process.argv.includes('--break-first-arrival')) {
    const original=schemaSource.match(/CREATE OR REPLACE FUNCTION build_intake_register[^]*?END \$\$;/)[0];
    await db.exec(original.replace(" OR coalesce((input->>'admission_new')::boolean,false)",''));
  }
  assert.deepEqual(await db.all('SELECT id,task_id,state,result_id,zone_position,lease FROM state_compute_scheduling ORDER BY id'),frozen);
  assert.deepEqual(await db.all('SELECT * FROM entity_compute_results ORDER BY id'),results);
  assert.equal((await db.get("SELECT relfilenode::text storage FROM pg_class WHERE oid='state_build_workflow_nodes'::regclass")).storage,storage);
  const next=await fixture();await intake(next);assert.equal(await count('entity_build_intake_intents'),1);
  await drain();
  console.log('ok: exact frozen baseline upgrades twice without rewriting old nodes, leases, accepted results or positions; missing historical and missing/invalid additive indexes refuse before any rebuild');
  const empty=await fixture();empty.flow.nodes=[];
  await intake(empty);await drain();
  const completion=await db.get('SELECT s.state FROM entity_build_workflows w JOIN state_build_workflows s ON s.id=w.id WHERE w.build_id=?',empty.id);
  assert.equal(completion?.state,'succeeded');
  console.log('ok: a first legal arrival with an empty applicable scope preserves its captured completion across deferred registration');
}finally{await db.close();}
