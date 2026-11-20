// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import {join} from 'node:path';
import {preparePriority,nextPriority} from './board_catalog_priority.mjs';
import {routeRankInput} from './board_rank_work.mjs';
import {prioritySchema} from './board_catalog_priority_schema.mjs';

export async function checkPriority({sql,schema,config,action,weapon,ruler,second,source,inputs,step,restart,put}){
 const fresh='heavy_gunner',archive=createHash('sha256').update('immutable archive fixture').digest('hex'),old='a'.repeat(64);
 const installer=await sql.reserve();
 try{await installer.unsafe(prioritySchema(schema));}finally{installer.release();}
 await sql.begin(async tx=>{
  await tx.unsafe('INSERT INTO state_board_public_import VALUES(1,$1)',[archive]);
  await tx.unsafe("INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,$4,'',$5,1,'[]','old','old')",[weapon,second,old,'archive:'+archive,archive]);
  await tx.unsafe('INSERT INTO state_board_read_rows VALUES($1,$2,$3,1,$4)',[weapon,second,old,JSON.stringify({score:1397})]);
  await tx.unsafe('INSERT INTO state_board_public_heads SELECT weapon,ruler,scene,$3 FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,second,old]);
 });
 await assert.rejects(sql.unsafe('SELECT board_requested_partition_selected($1,$2,false)',[weapon,second]),/requires a qualified first publication/);
 await restart(3);
 const scan=await action('priority-step');assert.equal(scan.status,'scanning');
 while(!(await sql.unsafe('SELECT complete FROM state_board_rank_catalog_scan WHERE id=1'))[0].complete)await action('priority-step');
 const accepted=inputs[0].facts[0],reservedKey=JSON.stringify([accepted.identity,second,accepted.mode]).replaceAll(',',', ');
 await sql.unsafe('UPDATE board_facts SET score=1777 WHERE identity=$1 AND ruler=$2 AND mode=$3',[accepted.identity,second,accepted.mode]);
 const body={identity:accepted.identity,ruler:second,mode:accepted.mode,score:1777};
 const reservedVersion=await put('measurement',reservedKey,body);
 await assert.rejects(sql.begin(async tx=>{
  const deleted=await tx.unsafe('DELETE FROM state_board_rank_catalog_inputs WHERE weapon=$1 AND ruler=$2 AND identity=$3',[weapon,second,accepted.identity]);
  assert.equal(deleted.count,1);
  await routeRankInput(tx,{scope:'measurement',key:reservedKey,body,version:reservedVersion},source,true);
 }),/ranking partition capture is not registered/);
 let routed=false;
 for(let n=0;n<120&&!routed;n++){
  await step();
  const [mail]=await sql.unsafe("SELECT applied_version FROM state_board_input_mailbox WHERE scope='measurement' AND key=$1",[reservedKey]);
  routed=String(mail.applied_version)===String(reservedVersion);
 }
 assert(routed,'an unindexed reservation with a durable catalog obligation can acknowledge its current source');
 assert.equal((await sql.unsafe('SELECT 1 FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,second])).length,0);
 const scheduler=new DatabaseSync(':memory:'),ledger=new DatabaseSync(':memory:');
 try{
  ledger.exec('CREATE TABLE receipts(weapon TEXT,ruler TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID');
  ledger.prepare('INSERT INTO receipts VALUES(?,?)').run('dex_nikana','fresh');
  scheduler.exec(`CREATE TABLE priority_work(weapon TEXT,ruler TEXT,priority INT,phase TEXT,due INT,reason TEXT,arrival TEXT,
   PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
   INSERT INTO priority_work VALUES('retained','fresh',-1,'pending',123540,'wait','2026-11-19T15:36:00.000Z');`);
  const isolated={unsafe:(query,args)=>query.startsWith('SELECT weapon,ruler,position FROM state_board_requested_partitions')?[]:sql.unsafe(query,args)};
  const context={sql:isolated,journal:scheduler,ledger,maxPending:2},now=Date.now();
  preparePriority(scheduler,{source:'test',archive});
  preparePriority(scheduler,{source:'test',archive});
  const job=weapon=>scheduler.prepare('SELECT * FROM priority_work WHERE weapon=?').get(weapon);
  assert.deepEqual([job('retained').priority,job('retained').phase,job('retained').due,job('retained').reason],[3,'pending',123540,'wait']);
  scheduler.prepare("UPDATE priority_work SET due=? WHERE weapon='retained'").run(now+60000);
  await sql.unsafe(`INSERT INTO state_board_rank_new_scopes(weapon,ruler,queued_at,zone) VALUES
   ('correction','fresh','2020-01-01T00:00:00Z',1),('dex_nikana','fresh','2020-01-02T00:00:00Z',0),
   ('user_second','fresh','2020-01-03T00:00:00Z',0),('ordinary','fresh','2020-01-04T00:00:00Z',3)`);
  const next=async()=>{for(let i=0;i<8;i++){const found=await nextPriority(context,now);if(found)return found;}return null;};
  scheduler.prepare("INSERT OR REPLACE INTO priority_cursor VALUES('turn','2')").run();
  assert.equal((await next()).weapon,'dex_nikana','a qualified unpublished scope gets the same zone as any other admitted request');
  const first=job('dex_nikana');
  const set=scheduler.prepare('UPDATE priority_work SET phase=?,due=? WHERE weapon=?');
  set.run('pending',now+60000,'dex_nikana');
  assert.equal(await next(),null,'the shared cap holds new jobs while existing leases are busy');
  set.run('selected',0,'retained');
  assert.equal((await next()).weapon,'user_second','an unavailable same-zone task does not block the next FIFO task');
  set.run('pending',now+60000,'user_second');
  assert.equal(await next(),null);
  set.run('held',now+60000,'user_second');
  assert.equal((await next()).weapon,'correction','the lowest executable zone is claimed before a later zone');
  set.run('selected',0,'correction');
  await sql.unsafe("DELETE FROM state_board_rank_new_scopes WHERE weapon IN ('correction','user_second')");
  await sql.unsafe("UPDATE state_board_rank_new_scopes SET last_measured_at=board_input_timestamp(clock_timestamp()) WHERE weapon='dex_nikana'");
  scheduler.prepare("INSERT OR REPLACE INTO priority_cursor VALUES('turn','0')").run();
  await next();
  assert.deepEqual([job('dex_nikana').position,job('dex_nikana').arrival],[first.position,first.arrival],'new scores preserve the oldest outstanding age and queue position');
  await sql.unsafe("DELETE FROM state_board_rank_new_scopes WHERE weapon IN ('dex_nikana','ordinary')");
 }finally{scheduler.close();ledger.close();}
 const sourceFact=inputs[0].facts[0];
 const id=sourceFact.identity,mode=sourceFact.mode;
 await sql.unsafe('INSERT INTO board_facts SELECT identity,$2,mode,71,metric,measured_by,finished_at,retired_at,review_id FROM board_facts WHERE identity=$1 AND ruler=$3 AND mode=$4',[id,fresh,ruler,mode]);
 const notify=async (score,publication_zone=3)=>put('measurement',JSON.stringify([id,fresh,mode]).replaceAll(',',', '),{identity:id,ruler:fresh,mode,score,publication_zone});
 await notify(71);
 const arrival=async()=>(await sql.unsafe('SELECT queued_at,last_measured_at,zone,zone_position FROM state_board_rank_new_scopes WHERE weapon=$1 AND ruler=$2',[weapon,fresh]))[0];
 assert((await arrival()).last_measured_at);
 await sql.unsafe("UPDATE state_board_rank_new_scopes SET last_measured_at='2000-01-01' WHERE weapon=$1 AND ruler=$2",[weapon,fresh]);
 const before=await arrival();
 await notify(72,1);
 const after=await arrival();
 assert.equal(after.queued_at,before.queued_at);
 assert.equal(after.zone,1);assert(BigInt(after.zone_position)>BigInt(before.zone_position),'promotion moves to the new zone tail');
 assert(after.last_measured_at>before.last_measured_at,'another accepted measurement advances a waiting scope');
 await sql.unsafe('INSERT INTO state_board_rank_new_scopes(weapon,ruler) VALUES($1,$2) ON CONFLICT DO NOTHING',[weapon,fresh]);
 // An unavailable proof for one scope cannot hold an unrelated current correction or first score.
 await sql.unsafe("INSERT INTO state_board_rank_new_scopes(weapon,ruler) VALUES($1,'removed_ruler')",[weapon]);
 const page=async scope=>(await sql.unsafe('SELECT board_read_page($1,$2,NULL,0,128) page',[weapon,scope]))[0].page;
 const position=(await sql.unsafe('SELECT position FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2',[weapon,second]))[0].position;
 const frozen=new Set();let revised=false,warm=false,concurrent=false;
 const stop=Date.now()+75000;let last;
 while(Date.now()<stop){
  last=await action('priority-step');
  if(last.status==='held'&&last.ruler!=='removed_ruler')throw Error('unexpected hold '+JSON.stringify(last));
  if(last.status==='frozen')frozen.add(last.ruler);
  if(last.status==='frozen'&&last.ruler===fresh&&!revised){
   await sql.unsafe('UPDATE board_facts SET score=96 WHERE identity=$1 AND ruler=$2 AND mode=$3',[id,fresh,mode]);
   await notify(96);revised=true;
   await sql.unsafe('UPDATE board_facts SET score=999 WHERE identity=$1 AND ruler=$2 AND mode=$3',[id,ruler,mode]);
   await put('measurement',JSON.stringify([id,ruler,mode]).replaceAll(',',', '),{identity:id,ruler,mode,score:999});
  }
  if(!frozen.has(fresh)||!frozen.has(second))continue;
  for(let n=0;n<12;n++){
   const work=await step();
   if(work.status==='baseline-batch'){
    const targets=work.items.map(x=>x.weapon+'/'+x.ruler);assert.equal(new Set(targets).size,targets.length);
    concurrent=true;
   }
  }
  warm||=(await page(ruler)).rows.some(r=>r.score===999);
  const current=await page(fresh),retained=await page(second);
  if(revised&&warm&&current.ok&&current.rows.some(r=>r.score===96)&&retained.ok&&retained.revision!==old){
   const status=await action('priority-status');
   if(!status.held.some(r=>r.ruler==='removed_ruler'))continue;
   const [request]=await sql.unsafe('SELECT position,activated_at FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2',[weapon,second]);
   assert.equal(request.position,position);assert(request.activated_at);
   assert(retained.rows.some(r=>r.score===1777),'reserved first publication replays the already acknowledged change');
   assert.equal((await sql.unsafe('SELECT board_read_page($1,$2,$3,0,128) p',[weapon,second,old]))[0].p.rows[0].score,1397);
   const q=new DatabaseSync(join(config.state,'priority.sqlite'),{readOnly:true});
   try{
    q.exec('BEGIN');assert(q.prepare("SELECT 1 FROM priority_work WHERE weapon=? AND ruler=?").get(weapon,fresh));
    await action('priority-step');
   }finally{q.close();}
   const delivery=async()=>(await sql.unsafe('SELECT version,delivered_output FROM state_board_rank_publication_ready WHERE weapon=$1 AND ruler=$2',[weapon,ruler]))[0];
   const previous=await delivery();assert(previous.delivered_output);
   const receipt=await put('measurement',JSON.stringify([id,ruler,mode]).replaceAll(',',', '),{identity:id,ruler,mode,score:999,source_ack:'later'});
   let indexed=false;
   for(let n=0;n<300&&!indexed;n++){
    const activity=await step();
    if(activity.status==='indexed'&&activity.ruler===ruler&&activity.identity===id){
     const [observed]=await sql.unsafe('SELECT modes->>$4 AS version FROM state_board_rank_input_sets WHERE weapon=$1 AND ruler=$2 AND identity=$3',[weapon,ruler,id,mode]);
     if(String(observed.version)===String(receipt)){
      assert.equal(activity.winners_changed,false);assert.equal(activity.publication_requested,false);indexed=true;
     }
    }
   }
   assert(concurrent,'independent first publications share the database pool and one Native');
   assert(indexed,'new canonical acknowledgement is durable without rebuilding unchanged public winners');
   assert.deepEqual(await delivery(),previous);
   console.log('ok — retained archive and requested position, fresh fenced capture and changed identity, unrelated warm correction through held work, priority restart');
   return;
  }
 }
 throw Error('priority publication did not drain: '+JSON.stringify({last,warm,revised,status:await action('priority-status')}));
}
