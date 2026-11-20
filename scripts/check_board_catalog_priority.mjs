// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import {join} from 'node:path';
import {preparePriority,nextPriority} from './board_catalog_priority.mjs';
import {prioritySchema} from './board_catalog_priority_schema.mjs';

export async function checkPriority({sql,schema,config,action,weapon,ruler,second,inputs,step,restart,put}){
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
 await restart();
 const scheduler=new DatabaseSync(':memory:'),ledger=new DatabaseSync(':memory:');
 try{
  ledger.exec('CREATE TABLE receipts(weapon TEXT,ruler TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID');
  const known=ledger.prepare('INSERT INTO receipts VALUES(?,?)');
  for(let i=1;i<=80;i++)known.run('aaold'+i,'fixture');
  known.run('zznew','fresh');known.run('historical','ruler');
  preparePriority(scheduler,{source:'test',archive});
  await sql.unsafe(`INSERT INTO state_board_rank_new_scopes(weapon,ruler,queued_at) SELECT 'aaold'||n,'fixture','2000-01-01' FROM generate_series(1,80)n`);
  const context={sql,journal:scheduler,ledger};
  await nextPriority(context);
  await sql.unsafe("INSERT INTO state_board_rank_new_scopes(weapon,ruler) VALUES('zznew','fresh')");
  await sql.unsafe("INSERT INTO state_board_rank_new_scopes(weapon,ruler,queued_at) VALUES('zzfirst','fresh','2000-01-02')");
  const seen=[];
  for(let i=0;i<17;i++){
   known.run('newer'+i,'fresh');
   await sql.unsafe("INSERT INTO state_board_rank_new_scopes(weapon,ruler) VALUES($1,'fresh')",['newer'+i]);
   const job=await nextPriority(context);
   if(job){
    seen.push(job);
    scheduler.prepare("UPDATE priority_work SET phase='pending',due=? WHERE weapon=? AND ruler=?").run(Date.now()+60000,job.weapon,job.ruler);
   }
  }
  assert(seen.some(j=>j.weapon==='zzfirst'&&j.priority===-2),'an older first-ever score has a turn during continuous updates');
  assert(seen.some(j=>j.weapon==='zznew'&&j.priority===-1),'new arrivals get a bounded turn ahead of an established queue');
  assert(seen.some(j=>j.weapon.startsWith('aaold'))&&seen.some(j=>j.priority===2),'existing queued and historical scopes also make progress');
  await sql.unsafe("DELETE FROM state_board_rank_new_scopes WHERE weapon LIKE 'aaold%' OR weapon IN('zznew','zzfirst') OR weapon LIKE 'newer%'");
 }finally{scheduler.close();ledger.close();}
 const sourceFact=inputs[0].facts[0];
 const id=sourceFact.identity,mode=sourceFact.mode;
 await sql.unsafe('INSERT INTO board_facts SELECT identity,$2,mode,71,metric,measured_by,finished_at,retired_at,review_id FROM board_facts WHERE identity=$1 AND ruler=$3 AND mode=$4',[id,fresh,ruler,mode]);
 const notify=async score=>put('measurement',JSON.stringify([id,fresh,mode]).replaceAll(',',', '),{identity:id,ruler:fresh,mode,score});
 await notify(71);
 const arrival=async()=>(await sql.unsafe('SELECT queued_at,last_measured_at FROM state_board_rank_new_scopes WHERE weapon=$1 AND ruler=$2',[weapon,fresh]))[0];
 assert((await arrival()).last_measured_at);
 await sql.unsafe("UPDATE state_board_rank_new_scopes SET last_measured_at='2000-01-01' WHERE weapon=$1 AND ruler=$2",[weapon,fresh]);
 const before=await arrival();
 await notify(72);
 const after=await arrival();
 assert.equal(after.queued_at,before.queued_at);
 assert(after.last_measured_at>before.last_measured_at,'another accepted measurement advances a waiting scope');
 await sql.unsafe('INSERT INTO state_board_rank_new_scopes(weapon,ruler) VALUES($1,$2) ON CONFLICT DO NOTHING',[weapon,fresh]);
 // An unavailable proof for one scope cannot hold an unrelated current correction or first score.
 await sql.unsafe("INSERT INTO state_board_rank_new_scopes(weapon,ruler) VALUES($1,'removed_ruler')",[weapon]);
 const page=async scope=>(await sql.unsafe('SELECT board_read_page($1,$2,NULL,0,128) page',[weapon,scope]))[0].page;
 const position=(await sql.unsafe('SELECT position FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2',[weapon,second]))[0].position;
 let revised=false,warm=false;
 const stop=Date.now()+75000;let last;
 while(Date.now()<stop){
  last=await action('priority-step');
  if(last.status==='held'&&last.ruler!=='removed_ruler')throw Error('unexpected hold '+JSON.stringify(last));
  if(last.status==='frozen'&&last.ruler===fresh&&!revised){
   await sql.unsafe('UPDATE board_facts SET score=96 WHERE identity=$1 AND ruler=$2 AND mode=$3',[id,fresh,mode]);
   await notify(96);revised=true;
   await sql.unsafe('UPDATE board_facts SET score=999 WHERE identity=$1 AND ruler=$2 AND mode=$3',[id,ruler,mode]);
   await put('measurement',JSON.stringify([id,ruler,mode]).replaceAll(',',', '),{identity:id,ruler,mode,score:999});
  }
  for(let n=0;n<12;n++)await step();
  warm||=(await page(ruler)).rows.some(r=>r.score===999);
  const current=await page(fresh),retained=await page(second);
  if(revised&&warm&&current.ok&&current.rows.some(r=>r.score===96)&&retained.ok&&retained.revision!==old){
   const status=await action('priority-status');
   if(!status.held.some(r=>r.ruler==='removed_ruler'))continue;
   const [request]=await sql.unsafe('SELECT position,activated_at FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2',[weapon,second]);
   assert.equal(request.position,position);assert(request.activated_at);
   assert.equal((await sql.unsafe('SELECT board_read_page($1,$2,$3,0,128) p',[weapon,second,old]))[0].p.rows[0].score,1397);
   const q=new DatabaseSync(join(config.state,'priority.sqlite'),{readOnly:true});
   try{
    q.exec('BEGIN');assert(q.prepare("SELECT 1 FROM priority_work WHERE weapon=? AND ruler=?").get(weapon,fresh));
    await action('priority-step');
   }finally{q.close();}
   console.log('ok — retained archive and requested position, fresh fenced capture and changed identity, unrelated warm correction through held work, priority restart');
   return;
  }
 }
 throw Error('priority publication did not drain: '+JSON.stringify({last,warm,revised,status:await action('priority-status')}));
}
