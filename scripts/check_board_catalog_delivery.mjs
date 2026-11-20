// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {join} from 'node:path';
import {rankMeasurementKey} from './board_rank_keys.mjs';

export async function checkCatalogDelivery({sql,weapon,ruler,fresh,inputs,action,native,step,put,config}){
 const records=inputs.slice(2,26).map(({facts})=>facts[0]);
 const keys=records.map(x=>rankMeasurementKey(x.identity,ruler,x.mode));
 const versions=new Map();
 for(const [i,row] of records.entries()){
  const score=9000+i;
  await sql.unsafe('UPDATE board_facts SET score=$4 WHERE identity=$1 AND ruler=$2 AND mode=$3',[row.identity,ruler,row.mode,score]);
  versions.set(row.identity,String(await put('measurement',keys[i],{identity:row.identity,ruler,mode:row.mode,score})));
 }
 const base=inputs[5].record,record={...base,mods:base.mods.map((mod,i)=>i===2?'hellfire@4':mod)};
 const [described]=await native.call('describe',[{id:'late-delivery',record}]);
 const missed={identity:described.candidate,mode:described.modes[0]},freshKey=rankMeasurementKey(missed.identity,fresh,missed.mode);
 await sql.unsafe('INSERT INTO entity_weapon_builds VALUES($1,$2,$3,$4)',[missed.identity,weapon,'2026-11-16T01:00:00Z',JSON.stringify(record)]);
 await sql.unsafe('INSERT INTO board_facts SELECT $1,$2,mode,9999,metric,measured_by,finished_at,retired_at,review_id FROM board_facts WHERE identity=$3 AND ruler=$4 AND mode=$5',[missed.identity,fresh,records[0].identity,ruler,missed.mode]);
 const lateVersion=await put('measurement',freshKey,{identity:missed.identity,ruler:fresh,mode:missed.mode,score:9999});
 await sql.unsafe("UPDATE state_board_input_mailbox SET available_at='2999-01-01' WHERE scope='measurement' AND key IN(SELECT jsonb_array_elements_text($1::text::jsonb))",[JSON.stringify([...keys,freshKey])]);
 await sql.unsafe('DELETE FROM state_board_rank_recent_inputs WHERE (weapon,ruler,identity)=($1,$2,$3)',[weapon,fresh,missed.identity]);
 assert.equal((await sql.unsafe('SELECT 1 FROM state_board_rank_sources WHERE (weapon,ruler,identity)=($1,$2,$3)',[weapon,fresh,missed.identity])).length,0);
 assert((await sql.unsafe('SELECT 1 FROM state_board_rank_catalog_inputs WHERE (weapon,ruler,identity)=($1,$2,$3)',[weapon,fresh,missed.identity])).length);
 const journal=new DatabaseSync(join(config.state,'priority.sqlite'));
 try{journal.prepare("DELETE FROM priority_cursor WHERE feed='selected-inputs'").run();}finally{journal.close();}
 let nextCheck=0;
 const stop=Date.now()+45000;
 let landed=false;
 while(!landed&&Date.now()<stop){
  const stage=await step();
  if(stage.status==='recent-measurement'){
   const [transferred]=await sql.unsafe('SELECT count(*) n FROM state_board_rank_dirty WHERE weapon=$1 AND ruler=$2 AND recent_version>applied_version AND identity IN(SELECT jsonb_array_elements_text($3::text::jsonb))',[weapon,ruler,JSON.stringify(records.map(r=>r.identity))]);
   assert.equal(Number(transferred.n),records.length,'every claimed wakeup has a durable indexing obligation');
  }
  if(Date.now()>=nextCheck){await action('priority-step');nextCheck=Date.now()+20;}
  const known=await sql.unsafe('SELECT s.identity,i.modes FROM state_board_rank_sources s JOIN state_board_rank_input_sets i USING(weapon,ruler,identity) WHERE s.weapon=$1 AND s.ruler=$2 AND s.identity IN (SELECT jsonb_array_elements_text($3::text::jsonb))',
   [weapon,ruler,JSON.stringify(records.map(r=>r.identity))]);
  const [late]=await sql.unsafe('SELECT modes->>$4 v FROM state_board_rank_input_sets WHERE weapon=$1 AND ruler=$2 AND identity=$3',[weapon,fresh,missed.identity,missed.mode]);
  landed=String(late?.v)===String(lateVersion)&&known.length===records.length&&known.every(k=>String(k.modes[records.find(r=>r.identity===k.identity).mode])===versions.get(k.identity));
 }
 assert(landed,'all simultaneous wakeups and the missed selected identity reach the exact accepted versions while ordinary mail is delayed');
 for(let turn=0;turn<250;turn++){
  const [head]=await sql.unsafe('SELECT board_read_page($1,$2,NULL,0,128) page',[weapon,fresh]);
  if(head.page.rows.some(r=>r.score===9999))return;
  await step();
 }
 assert.fail('recovered accepted score did not reach the public ranking');
}
