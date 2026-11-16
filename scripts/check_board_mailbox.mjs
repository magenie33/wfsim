// Captured state changes commit and retry independently of the total backlog.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {placeholders} from '../worker/placeholders.js';

export async function boardMailboxCases(db,{noise=2048}={}) {
  const scalar=async(sql,...args)=>Object.values(await db.get(sql,...args))[0];
  const source=await scalar('SELECT source FROM state_board_input_source WHERE id=1');
  const put=(key,body)=>scalar('SELECT board_input_put(?::text,?::text,?::text)','measurement',key,body===null?null:JSON.stringify(body));
  const seed=(key,body)=>scalar('SELECT board_input_seed(?::text,?::text,?::text)','measurement',key,JSON.stringify(body));
  const claim=(token,limit=256)=>scalar('SELECT board_input_claim(?::bigint,?::text,60::bigint,?::text)',limit,token,source);
  const read=async(item,token,bytes=8192)=>{
    const input=await scalar('SELECT board_input_read(?::text,?::text,?::text,?::bigint,?::bigint)',item.scope,item.key,token,item.version,bytes);
    return input.status==='input'?{...input,body:JSON.parse(input.payload??'null')}:input;
  };
  const ack=(item,token)=>scalar('SELECT board_input_ack(?::text,?::text,?::text,?::bigint)',item.scope,item.key,token,item.version);
  const retry=(item,token,message,delay)=>scalar('SELECT board_input_retry(?::text,?::text,?::text,?::bigint,?::text,?::bigint)',item.scope,item.key,token,item.version,message,delay);
  const row=key=>db.get("SELECT * FROM state_board_input_mailbox WHERE scope='measurement' AND key=?",key);
  let passed=0;
  const check=async(name,run)=>{await run();passed++;console.log('ok: '+name);};
  await check('source and handoff share commit and rollback',async()=>{
    await db.exec('CREATE TABLE producer_fixture(id text PRIMARY KEY,value jsonb)');
    await db.exec('BEGIN');await db.run("INSERT INTO producer_fixture VALUES('rolled','{}')");await put('rolled',{score:1});await db.exec('ROLLBACK');
    assert.equal(await row('rolled'),null);assert.equal(await db.get("SELECT * FROM producer_fixture WHERE id='rolled'"),null);
    await db.exec('BEGIN');await db.run("INSERT INTO producer_fixture VALUES('committed','{}')");await put('committed',{score:2});await db.exec('COMMIT');
    const item=(await claim('committed')).inputs[0];assert.equal((await read(item,'committed')).body.score,2);assert.equal(await ack(item,'committed'),'acknowledged');
  });
  await check('a new value survives the acknowledgement of an older claim',async()=>{
    const v1=await put('moving',{score:1});assert.equal(await put('moving',{score:1}),v1);
    const first=(await claim('first')).inputs[0];assert.equal(first.version,String(v1));
    const v2=await put('moving',{score:2});assert(BigInt(v2)>BigInt(v1));
    assert.equal((await read(first,'first')).status,'superseded');assert.equal(await ack(first,'wrong'),'stale');
    assert.equal(await ack(first,'first'),'acknowledged');assert.equal(await ack(first,'first'),'already_applied');
    const next=(await claim('second')).inputs[0];assert.equal(next.version,String(v2));
    assert.equal((await read(next,'second')).body.score,2);assert.equal(await ack(next,'second'),'acknowledged');
    assert.equal((await row('moving')).body,null);assert.equal(await put('moving',{score:2}),v2);
    assert.deepEqual((await claim('empty')).inputs,[]);
  });
  await check('expired credentials cannot consume a replacement claim',async()=>{
    await put('expired',{score:3});const first=(await claim('old')).inputs[0];
    await db.run("UPDATE state_board_input_mailbox SET lease_until=board_input_timestamp(clock_timestamp()-interval '1 second'),available_at=board_input_timestamp(clock_timestamp()-interval '1 second') WHERE key='expired'");
    const second=(await claim('new')).inputs[0];assert.equal((await read(first,'old')).status,'stale');
    assert.equal(await ack(first,'old'),'stale');assert.equal(await retry(first,'old','late',0),false);
    assert.equal(await ack(second,'new'),'acknowledged');
  });
  await check('a permanent failure is visible and a newer version can proceed',async()=>{
    await put('failed',{score:4});const first=(await claim('failure')).inputs[0];
    assert.equal(await retry(first,'failure','invalid admission proof',null),true);
    assert.deepEqual((await claim('blocked')).inputs,[]);assert.equal((await row('failed')).last_error,'invalid admission proof');
    await put('failed',{score:5});const next=(await claim('repaired')).inputs[0];
    assert.equal((await read(next,'repaired')).body.score,5);assert.equal(String((await row('failed')).last_error_version),String(first.version));
    await ack(next,'repaired');
  });
  await check('failure of an old version does not block its newer value',async()=>{
    await put('advanced',{score:6});const first=(await claim('advance')).inputs[0];await put('advanced',{score:7});
    assert.equal(await retry(first,'advance','old failure',3600),true);
    const next=(await claim('advance-next')).inputs[0];assert.equal(next.key,'advanced');await ack(next,'advance-next');
  });
  await check('historical imports cannot overwrite live values or removals',async()=>{
    assert.equal(await seed('baseline',{score:1}),true);const baseline=(await claim('baseline')).inputs[0];assert.equal(baseline.version,'0');await ack(baseline,'baseline');
    await put('baseline',{score:8});assert.equal(await seed('baseline',{score:1}),false);
    await put('baseline',null);assert.equal(await seed('baseline',{score:1}),false);
    const removed=(await claim('remove')).inputs[0];assert.equal(removed.deleted,true);assert.equal((await read(removed,'remove')).deleted,true);await ack(removed,'remove');
    const deletedVersion=(await row('baseline')).version;await put('baseline',{score:9});assert(BigInt((await row('baseline')).version)>BigInt(deletedVersion));
    const recreated=(await claim('recreate')).inputs[0];await ack(recreated,'recreate');
  });
  await check('claims return bounded headers and oversized bodies remain recoverable',async()=>{
    await put('large',{data:'x'.repeat(16384)});const item=(await claim('large')).inputs[0];
    assert.equal(Object.hasOwn(item,'body'),false);assert(item.body_bytes>16384);
    assert.equal((await read(item,'large',1024)).status,'oversized');
    assert.equal((await read(item,'large',32768)).body.data.length,16384);
    await ack(item,'large');
  });
  await check('unused sequence values never hide ready keys',async()=>{
    await db.run("SELECT nextval('board_input_revision')");await put('later',{score:10});
    const item=(await claim('gap')).inputs[0];assert.equal(item.key,'later');await ack(item,'gap');
    await put('earlier-key',{score:11});const later=(await claim('gap-next')).inputs[0];assert.equal(later.key,'earlier-key');await ack(later,'gap-next');
  });
  await check('historical seed backlog does not put live results behind the import',async()=>{
    for(let i=0;i<32;i++)await seed('seed-backlog-'+i,{score:1});
    await put('live-during-import',{score:13});
    const live=(await claim('live-during-import',1)).inputs[0];assert.equal(live.key,'live-during-import');await ack(live,'live-during-import');
    const baseline=await claim('seed-drain');assert.equal(baseline.inputs.length,32);
    for(const item of baseline.inputs){assert.equal(item.version,'0');await ack(item,'seed-drain');}
  });
  await check('ready reads remain bounded beside completed and delayed history',async()=>{
    assert(Number.isSafeInteger(noise)&&noise>=2048);
    await db.exec(`INSERT INTO state_board_input_mailbox(scope,key,version,body,removed,body_hash,applied_version,available_at)
      SELECT 'measurement','done-'||i,1,jsonb_build_object('fixture',repeat('x',1024)),false,repeat('a',64),1,NULL FROM generate_series(1,${noise})i;
      INSERT INTO state_board_input_mailbox(scope,key,version,body,removed,body_hash,available_at)
      SELECT 'measurement','later-'||i,CASE WHEN i%2=0 THEN 0 ELSE 1 END,jsonb_build_object('fixture',repeat('x',1024)),false,repeat('a',64),board_input_timestamp(clock_timestamp()+interval '1 day') FROM generate_series(1,${noise})i;
      ANALYZE state_board_input_mailbox;`);
    const measured=async()=>{
      await db.exec("BEGIN;SET LOCAL plan_cache_mode='force_generic_plan';SET LOCAL jit=off");
      try {
        const value=await db.get("EXPLAIN(ANALYZE,BUFFERS,FORMAT JSON,TIMING OFF) SELECT board_input_claim(1,'bounded',60,?::text)",source);
        const plan=value['QUERY PLAN'][0].Plan;return (plan['Shared Hit Blocks']||0)+(plan['Shared Read Blocks']||0);
      }finally{await db.exec('ROLLBACK');}
    };
    await put('ready',{score:12});const indexed=await measured();assert(indexed<256,'claim reads unrelated backlog');
    await db.exec('DROP INDEX board_input_ready');const missing=await measured();assert(missing>=256,'missing-index control must fail the same read budget');
    await db.exec('CREATE INDEX board_input_ready ON state_board_input_mailbox((version=0),available_at,scope,key) WHERE version>applied_version AND available_at IS NOT NULL');
    const repaired=await measured();assert(repaired<256,'repaired ready index restores bounded reads');
    console.log(`ready buffers beside ${noise} completed and ${noise} delayed inputs: ${indexed}; missing index: ${missing}; restored: ${repaired}`);
  });
  console.log(`${passed} board mailbox checks passed`);return passed;
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href) {
  const {PGlite}=await import('./pg/node_modules/@electric-sql/pglite/dist/index.js');const pg=new PGlite();
  const q=(sql,args)=>pg.query(placeholders(sql),args).then(r=>r.rows);
  const db={exec:sql=>pg.exec(sql),run:(sql,...args)=>q(sql,args),get:async(sql,...args)=>(await q(sql,args))[0]??null};
  try {await pg.exec(readFileSync(new URL('../worker/pg/board_mailbox.sql',import.meta.url),'utf8'));await boardMailboxCases(db);}
  finally {await pg.close();}
}
