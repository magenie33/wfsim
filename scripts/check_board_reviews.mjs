import assert from 'node:assert/strict';
import { testDb } from './pg/test_db.mjs';
import { boardReviewsRoute, BOARD_REVIEW_PRODUCER, calibrateBoard } from '../worker/board_reviews.js';
import { BOARD_PRODUCERS } from '../worker/board_tasks.js';
import { facts } from '../worker/factory.js';
import { questionId, canon } from '../worker/tasks.js';
import { syncRoute } from '../worker/sync.js';
import { setDial } from '../worker/dials.js';
import { readFileSync } from 'node:fs';
import { tell } from './order_credit.mjs';
import { runInNewContext } from 'node:vm';
import { appraiseRoute } from '../worker/appraise.js';

const realRandom = Math.random; Math.random = () => 0.5;
const db = await testDb('library');
let engine = 'e1';
const env = { LIBRARY: db, ASSETS: { fetch: async () => Response.json({engine}) } };
const identity='a'.repeat(32), ruler='standard_single_target', mode='cycle';
const record={weapon:'laetum',mods:['serration']};
const target={identity,ruler,mode,engine};
const now=new Date().toISOString(), old='2026-01-01T00:00:00.000Z';
await db.run('INSERT INTO builds(id,at,record) VALUES (?,?,?)',identity,old,JSON.stringify(record));
await db.run('INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?,?,?,?,?,?,?,?,?)',identity,ruler,mode,'old-engine',10,'kpm',1,old,old);
const call=async(body,path='/api/board/review')=>{const res=await boardReviewsRoute(new Request('https://wfsim.test'+path,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)}),env,path);return {status:res.status,...await res.json()};};
let clockShift=0;
const age=()=>db.run('UPDATE board_reviews SET created_at=?,round_at=? WHERE id=?',old,new Date(Date.parse(old)+clockShift++*3_600_000).toISOString(),accepted.id);
const score=()=>db.get('SELECT * FROM scores WHERE identity=?',identity);
const state=async id=>(await call({ids:[id]},'/api/board/review/status')).reviews[0];
let passed=0;
const check=async(name,fn)=>{await fn();passed++;console.log('  ok  '+name);};
let accepted;
await check('only a current-engine stored row can ask for review',async()=>{
 assert.equal((await call({...target,identity:'b'.repeat(32)})).status,404);
 assert.equal((await call({...target,engine:'old'})).status,409);
 assert.equal((await call({...target,score:999})).status,400);
});
await check('ordinary scorer receipts never answer a fresh review round',async()=>{
 await tell([{identity,ruler,mode,score:999,metric:'kpm',work:100}],engine,async(action,body)=>{
  assert.equal(action,'answer');assert.ok(!body.producers.includes('board_review'));return {states:[]};
 });
});
const cached=await questionId('simulate',{record,ruler,mode});
await db.run("INSERT INTO questions(id,verb,request,state,due_at,opened_at,canon,fact_engine,fact_at) VALUES (?,'simulate',?,'fact',?,?,?,?,?)",cached,canon({record,ruler,mode}),old,old,canon({score:10,metric:'kpm',work:100}),engine,old);
await db.run('INSERT INTO answers(question,engine,device,owner,canon,result,work,at) VALUES (?,?,?,?,?,?,?,?)',cached,engine,'cached-witness','cached-owner',canon({score:10,metric:'kpm',work:100}),JSON.stringify({score:10,metric:'kpm'}),100,old);
await check('review opens a fresh question and preserves the old public score',async()=>{
 accepted=await call(target);assert.equal(accepted.ok,true);
 const row=await db.get('SELECT * FROM board_reviews');
 assert.notEqual(row.question,cached);assert.equal((await score()).score,10);
 assert.equal((await db.get('SELECT state FROM questions WHERE id=?',row.question)).state,'open');
});
await check('repeated and concurrent requests share one review',async()=>{
 const duplicates=await Promise.all([call(target),call(target),call(target)]);
 assert.ok(duplicates.every(row=>row.id===accepted.id));
 assert.equal((await db.get('SELECT count(*) AS n FROM board_reviews')).n,1);
});
await check('a lost acceptance retry keeps its frozen library record',async()=>{
 await db.run('UPDATE builds SET record=? WHERE id=?',JSON.stringify({...record,mods:['changed']}),identity);
 assert.equal((await call(target)).id,accepted.id);
 const review=await db.get('SELECT * FROM board_reviews WHERE id=?',accepted.id);
 const q=await db.get('SELECT request FROM questions WHERE id=?',review.question);
 assert.deepEqual(JSON.parse(q.request).record,record);
 await db.run('UPDATE builds SET record=? WHERE id=?',JSON.stringify(record),identity);
});
const witness=d=>({device:d.repeat(24),owner:'owner-'+d,siblings:[d.repeat(24)],net:'net-'+d,lanes:8});
await check('one witness cannot replace a published score',async()=>{
 const who=witness('1'),work=await facts().next(env,who,engine);
 assert.equal(work.demands[0].producer,'board_review');
 await facts().answer(env,who,{lease:work.lease,engine,result:{score:15,metric:'kpm'},work:100});
 assert.equal((await score()).score,10);assert.equal((await state(accepted.id)).state,'waiting');
});
await check('an independent agreeing witness atomically replaces only the reviewed row',async()=>{
 const who=witness('2'),work=await facts().next(env,who,engine);
 await facts().answer(env,who,{lease:work.lease,engine,result:{score:15,metric:'kpm'},work:100});
 assert.equal((await score()).score,15);assert.equal((await score()).review_id,accepted.id);
 assert.equal((await state(accepted.id)).state,'succeeded');
});
await check('a completed review is coalesced during its cooldown',async()=>{
 assert.equal((await call(target)).id,accepted.id);
 assert.equal((await db.get('SELECT count(*) AS n FROM questions')).n,2);
});
await check('older cached board facts cannot overwrite a newer verified review',async()=>{
 await db.run("INSERT INTO batches(id,at,total) VALUES ('cached',?,1)",now);
 await db.run("INSERT INTO queue(batch,build_id,ruler,mode) VALUES ('cached',?,?,?)",identity,ruler,mode);
 await facts().demand(env,{producer:'refresh',ref:identity+'|'+ruler+'|'+mode,verb:'simulate',request:{record,ruler,mode},due_in_ms:100,payload:{identity,ruler,mode,batch:'cached'}},engine);
 assert.equal((await score()).score,15);
});
await check('retracting an older ordinary fact preserves a reviewed score',async()=>{
 await BOARD_PRODUCERS.refresh.on_retract(env,{payload:{identity,ruler,mode,batch:'cached'}});
 assert.equal((await score()).score,15); assert.equal((await score()).review_id,accepted.id);
 await db.run("DELETE FROM queue WHERE batch='cached'");
});
await check('a changed baseline is not overwritten by a late review',async()=>{
 await age();
 accepted=await call(target);
 await db.run("UPDATE scores SET score=20,measured_by='e1',finished_at=?,review_id=NULL",now);
 for(const digit of ['3','4']){const who=witness(digit),work=await facts().next(env,who,engine);assert.ok(work);await facts().answer(env,who,{lease:work.lease,engine,result:{score:17,metric:'kpm'},work:100});}
 assert.equal((await score()).score,20);assert.equal((await state(accepted.id)).state,'superseded');
});
await check('both protocol 8 paths dispatch review rows as ordinary board work',async()=>{
 for(const proc of [0,1]){
  await age();const review=await call(target); accepted=review;
  await setDial(env,'sync_proc',proc,'check');
  const digit=String(5+proc),verifier=digit.repeat(24);
  const response=await syncRoute(new Request('https://wfsim.test/api/board/sync',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({verifier,engine,protocol:8,consent:{v:3,at:now},lanes:8,want_ms:1000})}),env);
  const got=await response.json();assert.ok(got.pack?.tasks?.some(task=>task.identity===identity),JSON.stringify(got));
  assert.equal((await score()).score,20);
  const q=await db.get('SELECT question FROM board_reviews WHERE id=?',review.id);
  await db.run("UPDATE board_reviews SET state='failed' WHERE id=?",review.id);
  await facts().withdraw(env,'board_review',identity+'|'+ruler+'|'+mode);
  await facts().release(env,verifier,got.pack.lease);
  assert.ok(q.question);
 }
});
await check('a withdrawn round terminates and can be requested again after cooldown',async()=>{
 await age(); accepted=await call(target);
 const review=await db.get('SELECT * FROM board_reviews WHERE id=?',accepted.id);
 await facts().withdraw(env,'board_review',review.ref);
 assert.equal((await state(accepted.id)).state,'failed');
 await age(); const fresh=await call(target); assert.notEqual(fresh.id,accepted.id);accepted=fresh;
});
await check('overdue reviews use the official backstop and the same guarded publication',async()=>{
 await db.run('UPDATE questions SET due_at=? WHERE id=(SELECT question FROM board_reviews WHERE id=?)',old,accepted.id);
 const who={device:'official:review'},[job]=await facts().claim(env,who,{states:['open'],producers:['board_review'],max:1},engine);
 assert.ok(job?.question);
 await facts().answerOfficial(env,who,{question:job.question,engine,result:{score:21,metric:'kpm'},work:100});
 assert.equal((await score()).score,21);assert.equal((await state(accepted.id)).state,'succeeded');
});
await check('retraction skips invalid older review proofs and leaves newer proofs intact',async()=>{
 const first=await db.get('SELECT * FROM board_reviews WHERE id=?',accepted.id);
 await age();accepted=await call(target);
 for(const digit of ['8','9']){const who=witness(digit),work=await facts().next(env,who,engine);assert.ok(work);await facts().answer(env,who,{lease:work.lease,engine,result:{score:22,metric:'kpm'},work:100});}
 const second=await db.get('SELECT * FROM board_reviews WHERE id=?',accepted.id);
 const retract=row=>BOARD_REVIEW_PRODUCER.on_retract(env,{ref:row.ref,question:row.question});
 await retract(first);assert.equal((await score()).score,22);assert.equal((await score()).review_id,second.id);
 await retract(second);assert.equal((await score()).score,20);assert.equal((await score()).review_id,null);
});
await check('the deployed daemon role can deliver a review with its proof atomically',async()=>{
 await age();accepted=await call(target);
 const row=await db.get('SELECT * FROM board_reviews WHERE id=?',accepted.id);
 const schema=(await db.get('SELECT current_schema() AS name')).name;
 await db.exec(`CREATE ROLE wfsim_factoryd; GRANT USAGE ON SCHEMA ${schema} TO wfsim_factoryd;`);
 const grants=readFileSync(new URL('../deploy/factoryd_grants.sql',import.meta.url),'utf8');
 await db.exec([...grants.matchAll(/GRANT (?!CONNECT)[^;]+;/g)].map(match=>match[0]).join('\n'));
 await db.exec('SET ROLE wfsim_factoryd');
 try{
  assert.equal(await BOARD_REVIEW_PRODUCER.still_wanted(env,{ref:row.ref,question:row.question}),true);
  await db.batch(BOARD_REVIEW_PRODUCER.on_fact_writes(env,{ref:row.ref,question:row.question,payload:{review:row.id}},
   {engine,result:{score:20,metric:'kpm'}}));
 }finally{await db.exec('RESET ROLE');}
 assert.equal((await score()).review_id,row.id);assert.equal((await state(row.id)).state,'succeeded');
});
await check('an invalidated review cannot resurrect an unproven community baseline',async()=>{
 await age();await db.run("UPDATE scores SET review_id=NULL,measured_by='verified:e1' WHERE identity=?",identity);
 accepted=await call(target);const row=await db.get('SELECT * FROM board_reviews WHERE id=?',accepted.id);
 const demand={ref:row.ref,question:row.question,payload:{review:row.id}};
 await db.batch(BOARD_REVIEW_PRODUCER.on_fact_writes(env,demand,{engine,result:{score:23,metric:'kpm'}}));
 await BOARD_REVIEW_PRODUCER.on_retract(env,demand);await BOARD_REVIEW_PRODUCER.on_retract(env,demand);
 assert.equal(await score(),null);
 assert.equal((await db.get('SELECT count(*) AS n FROM queue WHERE batch=?','review-repair:'+row.id)).n,1);
 await db.run('INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?,?,?,?,?,?,?,?,?)',identity,ruler,mode,'e1',20,'kpm',0,now,now);
 await db.run('DELETE FROM queue WHERE batch=?','review-repair:'+row.id);
});
await check('engine changes cannot publish an old review into the new release',async()=>{
 await age();accepted=await call(target);
 engine='e2'; env.ASSETS={fetch:async()=>Response.json({engine})};
 assert.equal((await state(accepted.id)).state,'failed');
 const who=witness('7'),work=await facts().next(env,who,engine);
 if(work)await facts().answer(env,who,{lease:work.lease,engine,result:{score:999,metric:'kpm'},work:100});
 assert.equal((await score()).score,20);
});
const expected = [];
await check('calibration scans past current-engine rows to fill ten in each category',async()=>{
 for(const category of [0,1])for(let i=0;i<26;i++){
  const id=(1000+category*100+i).toString(16).padStart(32,'0');
  const entry={weapon:'torid',mods:category?['riven']:['serration'],...(category?{riven_pos:['critical_damage','multishot']}: {})};
  const by=i<12?(i%2?'verified:e2':'e2'):i===12?'e20':'old';
  await db.run('INSERT INTO builds(id,at,record) VALUES (?,?,?)',id,old,JSON.stringify(entry));
  await db.run('INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?,?,?,?,?,?,?,?,?)',id,ruler,mode,by,100-i,'kpm',1,old,old);
  if(i>=12&&i<22)expected.push(id);
 }
 await calibrateBoard(env,{weapon:'torid',ruler,engine});
 const rows=await db.all("SELECT identity FROM board_reviews WHERE record::jsonb ->> 'weapon' = 'torid'");
 assert.deepEqual(rows.map(row=>row.identity).sort(),expected.sort());
 assert.equal(rows.length,20);
});
await check('parallel appraisal triggers reuse the same pending row reviews',async()=>{
 await Promise.all([calibrateBoard(env,{weapon:'torid',ruler,engine}),calibrateBoard(env,{weapon:'torid',ruler,engine})]);
 assert.equal((await db.get("SELECT count(*) AS n FROM board_reviews WHERE record::jsonb ->> 'weapon' = 'torid'")).n,20);
});
await check('the per-category calibration count is a dial and can exceed ten',async()=>{
 await setDial(env,'appraisal_calibration_rows',12,'check');
 await calibrateBoard(env,{weapon:'torid',ruler,engine});
 assert.equal((await db.get("SELECT count(*) AS n FROM board_reviews WHERE record::jsonb ->> 'weapon' = 'torid'")).n,24);
});
await check('same-engine rows need no review proof and another ruler adds nothing',async()=>{
 await db.run("UPDATE scores SET measured_by='verified:e2',review_id=NULL WHERE identity IN(SELECT id FROM builds WHERE weapon='torid')");
 await calibrateBoard(env,{weapon:'torid',ruler,engine});
 await calibrateBoard(env,{weapon:'torid',ruler:'another_ruler',engine});
 assert.equal((await db.get("SELECT count(*) AS n FROM board_reviews WHERE record::jsonb ->> 'weapon' = 'torid'")).n,24);
});
await check('disabled calibration and a stale engine enqueue no work',async()=>{
 await setDial(env,'appraisal_calibration_rows',0,'check');
 await calibrateBoard(env,{weapon:'torid',ruler,engine});
 await setDial(env,'appraisal_calibration_rows',10,'check');
 await calibrateBoard(env,{weapon:'torid',ruler,engine:'e1'});
 assert.equal((await db.get("SELECT count(*) AS n FROM board_reviews WHERE record::jsonb ->> 'weapon' = 'torid'")).n,24);
});
await check('calibration acknowledgment does not wait for its database scan',async()=>{
 let release;
 const blocked=new Promise(resolve=>{release=resolve;});
 const {forgetDials}=await import('../worker/dials.js'); forgetDials();
 const statement={all:()=>blocked,bind(){return this;}};
 const fake={...env,LIBRARY:{prepare:()=>statement}},background=[];
 const response=await boardReviewsRoute(new Request('https://wfsim.test/api/board/calibrate',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({weapon:'torid',ruler,engine})}),fake,'/api/board/calibrate',{waitUntil:work=>background.push(work)});
 assert.equal(response.status,200);assert.equal(background.length,1);
 release({results:[]});await Promise.all(background);forgetDials();
});
await check('the trigger rejects client scores and old engines',async()=>{
 assert.equal((await call({weapon:'torid',ruler,engine,score:999},'/api/board/calibrate')).status,400);
 assert.equal((await call({weapon:'torid',ruler,engine:'old'},'/api/board/calibrate')).status,409);
});
await check('a chat appraisal freeze triggers shared calibration in the background',async()=>{
 const id=(1000+25).toString(16).padStart(32,'0');
 await db.run("UPDATE scores SET measured_by='old' WHERE identity=?",id);
 const background=[],ctx={waitUntil:work=>background.push(work)},botEnv={...env,BOT_RELAY_TOKEN:'check'};
 const invoke=async(path,body)=>{
  const response=await appraiseRoute(new Request('https://wfsim.test'+path,{method:'POST',headers:{'content-type':'application/json',authorization:'Bearer check'},body:JSON.stringify(body)}),botEnv,path,ctx);
  assert.equal(response.status,200);return response.json();
 };
 const opened=await invoke('/api/appraise/new',{channel:'qq',asker:'calibration',weapon:'torid',ruler,riven:{bonuses:[]}});
 await invoke('/api/appraise/'+opened.code+'/request',{engine,request:{weapon:'torid',strategy:'quick'},context:{weapon:'torid'}});
 assert.equal(background.length,1);await Promise.all(background);
 assert.ok(await db.get('SELECT id FROM board_reviews WHERE identity=?',id));
 await invoke('/api/appraise/'+opened.code+'/request',{engine,request:{weapon:'torid',strategy:'quick'},context:{weapon:'torid'}});
 assert.equal(background.length,1);
});

const headless=runInNewContext(['83-headless-card.js','87-headless.js'].map(name=>readFileSync(new URL('../web/src/static/app/'+name,import.meta.url),'utf8')).join('\n')+`
headlessBoardRowAxes=()=>({slots:[]}); headlessWithDrafts=(_meta,card)=>card;
headlessSeat=()=>({}); headlessScenarioOnto=()=>({}); headlessBuiltinStarts=()=>[];
headlessAppraisalReplay;
`);
await check('grading reads the current board after replay and reranks corrected leaders',async()=>{
 const say={meta:{metrics:[{id:'kpm',field:'score'}],benchmarks:[{id:ruler,scenario:{metric:'kpm',runs:1}}]},t:x=>x};
 const w={id:'torid',modes:['base','cycle']},job={ruler,riven:{bonuses:[]},result:{build:{mods:[]}}};
 const row=(score,riven,mode='base')=>({benchmark:ruler,mode,score,...(riven?{riven:{bonuses:['multishot'],rolls:[1.1]}}:{})});
 let replayed=false;
 const current=[row(120,false),row(200,true),row(240,true,'cycle')];
 const verdict=await headless(say,w,job,[row(100,false),row(300,true)],{
  check:async()=>({ok:true}),simulate:async()=>{replayed=true;return {score:180};},
  board:async(weapon,scene)=>{assert.ok(replayed);assert.equal(weapon,'torid');assert.equal(scene,ruler);return current;},
 });
 assert.equal(verdict.grade,50);assert.equal(verdict.top,'120.0000');assert.equal(verdict.best,'240.0000');
 assert.equal(verdict.reference_rows.length,3);
 current[2]={...current[2],score:999};assert.equal(verdict.grade,50);
 const failed=await headless(say,w,job,[],{check:async()=>({ok:true}),simulate:async()=>({score:180}),board:async()=>null});
 assert.equal(failed.ok,false);
});
Math.random = realRandom;
console.log('\n'+passed+' board review checks passed');
