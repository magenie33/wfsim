// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {isDeepStrictEqual} from 'node:util';
import worker from '../worker/index.js';
import {pgDb,postgresClient} from '../worker/db.js';
import {requestCatalogFixture} from './check_board_catalog_reservation.mjs';
import {prepareRankSources} from './board_rank_index.mjs';
import {batchFixture} from './check_catalog_batch_fixture.mjs';
import {runBatch} from './adopt_rank_catalog_batch.mjs';
import {captureSelected} from './board_catalog_readback.mjs';

export async function checkBatch({sql,schema,native,binary,work,weapon,ruler,source,inputs,read,until,put,priority=false,step:readerStep,restart}){
 const second='standard_multi_target',url=new URL(process.env.WFSIM_BOARD_RANK_TEST_URL);
 url.searchParams.set('search_path',schema);process.env.PG_URL=url.href;
 await requestCatalogFixture(sql,native,weapon,ruler,source);
 const head=async id=>(await sql.unsafe('SELECT revision FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,id]))[0]?.revision;
 await until(()=>head(ruler),'initial measured ranking');
 const retained=await head(ruler),protectedHead=[];
 const prepared=[];
 for(let i=0;i<inputs.length;i+=32)prepared.push(...(await prepareRankSources(native,{weapon,ruler,inputs:inputs.slice(i,i+32)})).rows);
 for(let after=0;;){
  const page=await read(retained,after);
  for(const body of page.rows){
   const row=prepared.find(r=>isDeepStrictEqual(JSON.parse(r.page),body));assert(row,'original public result from Native');
   protectedHead.push({id:row.identity,mode:row.mode,body});
  }
  if(page.next===null)break;after=page.next;
 }
 assert(protectedHead.length>128,'fixture must span public pages');
 const [cols]=await sql.unsafe('SELECT count(*) n FROM board_facts');
 await sql.unsafe('INSERT INTO board_facts SELECT identity,$1,mode,score,metric,measured_by,finished_at,retired_at,review_id FROM board_facts WHERE ruler=$2',[second,ruler]);
 const request=await requestCatalogFixture(sql,native,weapon,second,source);
 request.read={enabled:false,canonical_sources:false};
 const config={...await batchFixture({sql,native,work,weapon,ruler,second,source,inputs,retained,request,protectedHead}),native:binary};
 const servers=[];let wrong=false,off=false,temporary=false;
 const inv=()=>JSON.parse(readFileSync(config.reservations,'utf8'));
 const launch=async i=>{
  const server=createServer(async(req,res)=>{
   try{
    const request=new Request('http://127.0.0.1'+req.url);
    if(i&&temporary){temporary=false;res.writeHead(503);res.end();return;}
    const response=await worker.fetch(request,{LIBRARY:pgDb(postgresClient(sql))},{waitUntil(){}});
    const body=await response.json();
    if(i&&wrong&&body.ok&&body.rows?.length)body.rows[0].score+=1;
    if(i&&off&&body.partitions)for(const p of body.partitions)p.revision='0'.repeat(64);
    res.writeHead(response.status,{'content-type':'application/json'});res.end(JSON.stringify(body));
   }catch(e){res.writeHead(500);res.end(JSON.stringify({error:e.message}));}
  });
  servers.push(server);server.listen(0,'127.0.0.1');await once(server,'listening');return 'http://127.0.0.1:'+server.address().port;
 };
 const opts={available:()=>128*1024**3,pageBudget:1};
 const action=(name,extra={})=>runBatch(config,name,{...opts,...extra});
 const journal=fn=>{const db=new DatabaseSync(join(config.state,'adoption.sqlite'));try{return fn(db);}finally{db.close();}};
 try{
  config.origins=await Promise.all([launch(0),launch(1)]);
  if(priority){
   const {checkPriority}=await import('./check_board_catalog_priority.mjs');
   return await checkPriority({sql,schema,config,action,weapon,ruler,second,source,inputs,step:readerStep,restart,put});
  }
  const proof=new DatabaseSync(config.pins[0].proof);
  const first=proof.prepare('SELECT * FROM live_audit WHERE ordinal=1').get();
  proof.prepare("UPDATE live_audit SET id='unknown' WHERE ordinal=1").run();
  await assert.rejects(action('plan'),/protected identity differs/);
  proof.prepare('UPDATE live_audit SET id=? WHERE ordinal=1').run(first.id);proof.close();
  assert.equal((await action('plan')).status,'planned');
  assert.equal((await action('plan')).scopes,4);
  await assert.rejects(action('step',{available:()=>1024**3}),/reserve exhausted/);
  assert.equal((await sql.unsafe('SELECT * FROM state_board_rank_catalog_scan')).length,0);
  const lock=await sql.reserve();try{
   await lock.unsafe('SELECT pg_advisory_lock(hashtextextended($1,0))',[source+'/catalog-batch']);
   await assert.rejects(action('step'),/another catalog/);
   await lock.unsafe('SELECT pg_advisory_unlock(hashtextextended($1,0))',[source+'/catalog-batch']);
  }finally{lock.release();}
  temporary=true;let step=await action('step');while(step.status==='scanning')step=await action('step');
  assert.equal(temporary,false);
  assert.equal(step.status,'reading');assert.equal(journal(db=>db.prepare('SELECT count(*) n FROM pages').get().n),1);
  wrong=true;await assert.rejects(action('step'),/public rows differ/);wrong=false;
  assert.equal(journal(db=>db.prepare('SELECT count(*) n FROM pages').get().n),1);
  off=true;assert.equal((await action('step')).status,'awaiting-origins');off=false;
  assert.equal((await action('step')).status,'retained');
  const original=inv(),bad=structuredClone(original);bad.reservations[0].request.position='0';
  writeFileSync(config.reservations,JSON.stringify(bad));await assert.rejects(action('step'),/original player reservation changed/);
  writeFileSync(config.reservations,JSON.stringify(original));
  assert.equal((await action('step')).status,'frozen');assert.equal(await head(second),null);
  assert.equal((await action('step')).status,'awaiting-reader');
  const check=()=>journal(db=>db.prepare('SELECT phase,manifest FROM scopes WHERE weapon=? AND ruler=?').get(weapon,second));
  const saved=check().manifest;
  journal(db=>db.prepare("UPDATE scopes SET phase='preparing' WHERE weapon=? AND ruler=?").run(weapon,second));
  assert.equal((await action('step')).status,'awaiting-reader');assert.equal(check().manifest,saved);
  await until(()=>head(second),'first public catalog head');
  do{step=await action('step');}while(step.status==='reading');
  assert.equal(step.status,'adopted');
  assert.equal((await action('step')).status,'frozen');await until(()=>head('demolisher'),'empty selected head');
  assert.equal((await action('step')).status,'adopted');
  assert.equal(journal(db=>db.prepare("SELECT last FROM pages WHERE ruler='demolisher'").get().last),0);
  assert.equal((await action('step')).status,'retired-ruler');
  const f=inputs[0].facts[0],score=f.score+67;
  await sql.unsafe('UPDATE board_facts SET score=$1 WHERE identity=$2 AND ruler=$3 AND mode=$4',[score,f.identity,ruler,f.mode]);
  const [{k}]=await sql.unsafe('SELECT jsonb_build_array($1::text,$2::text,$3::text)::text k',[f.identity,ruler,f.mode]);
  await put('measurement',k,{identity:f.identity,ruler,mode:f.mode,score});
  await until(async()=>await head(ruler)!==retained,'accepted new head before batch close');
  assert.equal((await action('step')).status,'advanced');
  do{step=await action('step');}while(step.status==='reading');
  assert.equal(step.status,'advanced');
  assert.equal((await action('run')).status,'closed');assert.equal((await action('step')).status,'closed');
  assert.equal(journal(db=>db.prepare('SELECT count(*) n FROM verified').get().n),4);
  const status=await action('status');assert.equal(status.run.closed,1);assert(status.storage.samples>10);
  const advanced=await captureSelected(sql,new Map(),{source,native:native.adapter});
  assert.equal(advanced.length,3);assert(advanced.every(x=>/^[a-f0-9]{64}$/.test(x.pages_sha256)));
  assert.equal(Number((await sql.unsafe('SELECT count(*) n FROM board_facts'))[0].n),2*Number(cols.n));
  console.log('ok — batch checked '+protectedHead.length+' protected rows, resumed after a bad origin and stale reservation, adopted a private cut, checked an advanced head and closed once');
 }finally{for(const server of servers){server.closeAllConnections();server.close();}}
}
