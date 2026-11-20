// Real transaction order, row locks and acknowledgement loss preserve keyed delivery.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {randomBytes} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {placeholders} from '../worker/placeholders.js';
import {boardMailboxCases} from './check_board_mailbox.mjs';
import {BoardInputMailbox} from './board_input_mailbox.mjs';
import {boardCaptureCases} from './check_board_capture.mjs';

const url=process.env.WFSIM_BOARD_MAILBOX_TEST_URL;
if(!url)throw Error('set WFSIM_BOARD_MAILBOX_TEST_URL to an isolated local PostgreSQL');
assert(['127.0.0.1','localhost','[::1]'].includes(new URL(url).hostname));
const schema='board_mailbox_'+randomBytes(6).toString('hex'),role=schema+'_worker';
const sql=postgres(url,{max:5,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:30000,lock_timeout:3000}});
const query=(text,args=[])=>sql.unsafe(placeholders(text),args);
const db={exec:text=>sql.unsafe(text),run:(text,...args)=>query(text,args),get:async(text,...args)=>(await query(text,args))[0]??null};
const scalar=async(text,...args)=>Object.values(await db.get(text,...args))[0];
let releaseProducer,releaseBlocker,pendingProducer,pendingBlocker;
try {
 await sql.unsafe('CREATE SCHEMA '+schema);
 await sql.unsafe(readFileSync(new URL('../worker/pg/board_mailbox.sql',import.meta.url),'utf8'));
 const fixture=await sql.reserve();
 try {
  const fq=(text,args=[])=>fixture.unsafe(placeholders(text),args);
  await boardMailboxCases({exec:text=>fixture.unsafe(text),run:(text,...args)=>fq(text,args),get:async(text,...args)=>(await fq(text,args))[0]??null},{noise:100001});
  const [{source}]=await fixture.unsafe('SELECT source FROM state_board_input_source WHERE id=1');
  for(const live of [true,false]){
   await fixture.unsafe('VACUUM (ANALYZE) state_board_input_mailbox');
   await fixture.unsafe("BEGIN;SET LOCAL plan_cache_mode='force_generic_plan';SET LOCAL jit=off");
   try{
    await fixture.unsafe("UPDATE state_board_input_mailbox SET available_at='9999' WHERE scope='measurement' AND key='ready'");
    await fixture.unsafe("UPDATE state_board_input_mailbox SET available_at='2000-01-01' WHERE key LIKE 'later-%' AND ($1 OR version=0)",[live]);
    await fixture.unsafe('ANALYZE state_board_input_mailbox');
    const [r]=await fixture.unsafe("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT board_input_claim(1,'deep-backlog',60,$1)",[source]);
    const plan=r['QUERY PLAN'][0].Plan,blocks=plan['Shared Hit Blocks']+plan['Shared Read Blocks'];
    assert(blocks<1024,'mixed ready backlog must use indexed lane boundaries, touched '+blocks+' blocks');
    console.log('ready '+(live?'live and seed':'seed only')+' claim: '+blocks+' shared blocks');
    const [owned]=await fixture.unsafe("SELECT version FROM state_board_input_mailbox WHERE lease_token='deep-backlog'");
    assert.equal(BigInt(owned.version)>0n,live);
   }finally{await fixture.unsafe('ROLLBACK');}
  }
 }finally{fixture.release();}
 const source=await scalar('SELECT source FROM state_board_input_source WHERE id=1');
 const mailbox=new BoardInputMailbox(sql,{source});
 for(const item of (await mailbox.claim({token:'cleanup'})).inputs)
  await query('SELECT board_input_ack(?::text,?::text,?::text,?::bigint)',[item.scope,item.key,'cleanup',item.version]);
 const put=async(key,value,tx=sql)=>(await tx.unsafe('SELECT board_input_put($1::text,$2::text,$3::text) version',
  ['measurement',key,value===null?null:JSON.stringify(value)]))[0].version;
 await sql.unsafe(`CREATE TABLE projection_fixture(key text PRIMARY KEY,version bigint,value jsonb);
   CREATE TABLE next_stage_fixture(key text,version bigint,PRIMARY KEY(key,version))`);
 const write=async(tx,input)=>{
  await tx.unsafe(`INSERT INTO projection_fixture VALUES($1,$2::bigint,($3::text)::jsonb) ON CONFLICT(key)
    DO UPDATE SET version=excluded.version,value=excluded.value WHERE excluded.version>projection_fixture.version`,
   [input.key,input.version,JSON.stringify(input.body)]);
  await tx.unsafe('INSERT INTO next_stage_fixture VALUES($1,$2::bigint) ON CONFLICT DO NOTHING',[input.key,input.version]);
  return {key:input.key,version:input.version};
 };
 await put('atomic',{score:1});const batch=await mailbox.claim({token:'atomic'}),item=batch.inputs[0];
 const input=await mailbox.read(item,'atomic');
 await assert.rejects(mailbox.apply(input,'atomic',async(tx,packet)=>{await write(tx,packet);throw Error('crash before commit');}),/crash before commit/);
 assert.equal(await scalar("SELECT count(*) FROM projection_fixture WHERE key='atomic'"),'0');
 assert.equal((await mailbox.read(item,'atomic')).status,'input');
 assert.equal((await mailbox.apply(input,'atomic',write)).status,'applied');
 assert.equal((await mailbox.apply(input,'atomic',()=>{throw Error('repeated side effect');})).status,'already_applied');
 assert.equal(await scalar("SELECT count(*) FROM next_stage_fixture WHERE key='atomic'"),'1');
 console.log('ok: projection, next-stage handoff and ACK commit together; rollback and lost reply are safe');

 await put('expires-during-write',{score:2});
 const expiring=(await mailbox.claim({token:'expires-during-write',leaseSeconds:2,items:1})).inputs[0];
 const expires=await mailbox.read(expiring,'expires-during-write');
 await assert.rejects(mailbox.apply(expires,'expires-during-write',async(tx,packet)=>{
  await write(tx,packet);await tx.unsafe('SELECT pg_sleep(2.05)');
 }),/claim expired during projection/);
 assert.equal(await scalar("SELECT count(*) FROM projection_fixture WHERE key='expires-during-write'"),'0');
 assert.equal(await scalar("SELECT count(*) FROM next_stage_fixture WHERE key='expires-during-write'"),'0');
 const recovered=(await mailbox.claim({token:'expires-recovered',items:1})).inputs[0];
 assert.equal(recovered.key,'expires-during-write');
 await mailbox.apply(await mailbox.read(recovered,'expires-recovered'),'expires-recovered',write);
 console.log('ok: lease expiry during a write rolls back the projection and downstream readiness');

 await put('moving-packet',{score:2});const moving=(await mailbox.claim({token:'moving'})).inputs[0];
 const old=await mailbox.read(moving,'moving');await put('moving-packet',{score:3});
 assert.equal((await mailbox.apply(old,'moving',()=>{throw Error('superseded input was applied');})).status,'superseded');
 const newer=(await mailbox.claim({token:'newer'})).inputs[0],current=await mailbox.read(newer,'newer');
 current.body.score=999;
 assert.equal((await mailbox.apply(current,'newer',write)).status,'applied');
 assert.equal((await db.get("SELECT value FROM projection_fixture WHERE key='moving-packet'")).value.score,3);
 const changed={...current,payload:'{"score":4}'};
 await assert.rejects(mailbox.apply(changed,'newer',write),/captured board input changed/);
 const wrongSource=new BoardInputMailbox(sql,{source:'different-source'});
 await assert.rejects(wrongSource.claim(),/source changed/);
 console.log('ok: superseded packets, mutated copies and another source cannot replace the captured value');

 let ready;
 const written=new Promise(resolve=>{ready=resolve;});
 const gate=new Promise(resolve=>{releaseProducer=resolve;});
 const low=pendingProducer=sql.begin(async tx=>{const revision=await put('low-uncommitted',{score:4},tx);ready(revision);await gate;return revision;});
 const lowVersion=await Promise.race([written,low.then(()=>assert.fail('producer ended before its barrier'))]);
 const highVersion=await put('high-committed',{score:5});assert(BigInt(highVersion)>BigInt(lowVersion));
 const high=(await mailbox.claim({token:'high',items:1})).inputs[0];assert.equal(high.key,'high-committed');
 await mailbox.apply(await mailbox.read(high,'high'),'high',write);
 releaseProducer();await low;
 const lower=(await mailbox.claim({token:'low',items:1})).inputs[0];assert.equal(lower.key,'low-uncommitted');
 assert(BigInt(lower.version)<BigInt(high.version));await mailbox.apply(await mailbox.read(lower,'low'),'low',write);
 console.log('ok: an earlier sequence value committed late is still delivered after a later value');

 for(const key of ['locked-a','locked-b','locked-c'])await put(key,{score:6});
 let lockReady;
 const started=new Promise(resolve=>{lockReady=resolve;}),locked=new Promise(resolve=>{releaseBlocker=resolve;});
 const blocker=pendingBlocker=sql.begin(async tx=>{await tx.unsafe("SELECT 1 FROM state_board_input_mailbox WHERE key='locked-a' FOR UPDATE");lockReady();await locked;});
 await Promise.race([started,blocker.then(()=>assert.fail('blocker ended before its barrier'))]);
 const claimed=await Promise.all([mailbox.claim({token:'parallel-a',items:1}),mailbox.claim({token:'parallel-b',items:1})]);
 const keys=claimed.flatMap(x=>x.inputs.map(row=>row.key));assert.deepEqual([...keys].sort(),['locked-b','locked-c']);
 releaseBlocker();await blocker;
 for(const claim of claimed)for(const row of claim.inputs)await mailbox.apply(await mailbox.read(row,claim.token),claim.token,write);
 const first=(await mailbox.claim({token:'unlocked',items:1})).inputs[0];assert.equal(first.key,'locked-a');
 await mailbox.apply(await mailbox.read(first,'unlocked'),'unlocked',write);
 console.log('ok: concurrent bounded claims skip a locked key and never share a claim');

 await sql.unsafe(`CREATE ROLE ${role} NOLOGIN NOINHERIT;GRANT USAGE ON SCHEMA ${schema} TO ${role};
   GRANT EXECUTE ON FUNCTION board_input_claim(bigint,text,bigint,text),board_input_read(text,text,text,bigint,bigint),
    board_input_lock(text,text,text,bigint),board_input_ack(text,text,text,bigint),board_input_retry(text,text,text,bigint,text,bigint) TO ${role}`);
 const worker=await sql.reserve();
 try {
  await worker.unsafe('SET ROLE '+role);
  await assert.rejects(worker.unsafe('SELECT * FROM state_board_input_mailbox'),/permission denied/);
  await assert.rejects(worker.unsafe("SELECT board_input_put('measurement','forged','{}')"),/permission denied/);
  await assert.rejects(worker.unsafe("SELECT nextval('board_input_revision')"),/permission denied/);
  await worker.unsafe("CREATE TEMP TABLE state_board_input_source(id integer,source text);INSERT INTO pg_temp.state_board_input_source VALUES(1,'forged');SET search_path TO pg_temp,"+schema);
  const actual=(await worker.unsafe("SELECT board_input_claim(1,'role-check',60,$1::text) claim",[source]))[0].claim;
  assert.equal(actual.source,source);
 }finally{await worker.unsafe('DROP TABLE IF EXISTS pg_temp.state_board_input_source;RESET ROLE;SET search_path TO '+schema);worker.release();}
 console.log('ok: consumer permissions and definer search paths prevent source forgery');
 await sql.unsafe("SELECT setval('board_input_revision',9007199254740992)");await put('large-version',{score:7});
 const large=(await mailbox.claim({token:'large-version'})).inputs[0];assert(BigInt(large.version)>9007199254740991n);
 await mailbox.apply(await mailbox.read(large,'large-version'),'large-version',write);
 console.log('ok: versions above JavaScript safe integers retain exact identity');
 const capture=await sql.reserve();
 try {
  const cq=(text,args=[])=>capture.unsafe(placeholders(text),args);
  await boardCaptureCases({exec:text=>capture.unsafe(text),run:(text,...args)=>cq(text,args),get:async(text,...args)=>(await cq(text,args))[0]??null});
 }finally{capture.release();}
 console.log('real PostgreSQL board mailbox checks passed');
}finally {
 releaseProducer?.();releaseBlocker?.();
 await Promise.allSettled([pendingProducer,pendingBlocker].filter(Boolean));
 try {await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');await sql.unsafe('DROP ROLE IF EXISTS '+role);}
 finally {await sql.end();}
}
