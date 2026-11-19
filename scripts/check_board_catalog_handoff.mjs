// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {registerCatalogScope} from './adopt_rank_catalog.mjs';
import {captureRankBaseline} from './board_rank_baseline.mjs';
import {prepareRankSources} from './board_rank_index.mjs';
import {startCatalogScan,scanCatalogPage,finishCatalogScan} from './board_rank_catalog_scan.mjs';
import {checkOccupiedReservation,requestCatalogFixture} from './check_board_catalog_reservation.mjs';
import {rankMeasurementKey} from './board_rank_keys.mjs';
const sha=data=>createHash('sha256').update(data).digest('hex');

export async function checkCatalogHandoff({sql,native,work,weapon,ruler,source,inputs,late,read,put,until,restart}){
 const [correct,remove]=inputs.slice(0,2),seeds=[correct,remove].map(x=>({...x,facts:[x.facts[0]]}));
 await sql.unsafe('DELETE FROM board_facts');
 const insert=f=>sql.unsafe('INSERT INTO board_facts SELECT * FROM jsonb_populate_record(NULL::board_facts,$1::text::jsonb)',[JSON.stringify(f)]);
 for(const row of seeds)await insert(row.facts[0]);
 const allAck=async()=>!(await sql.unsafe('SELECT 1 FROM state_board_input_mailbox WHERE version>applied_version LIMIT 1')).length;
 await put('review','before-cut',{identity:correct.identity,ruler});await until(allAck,'seed acknowledgements before catalog registration');
 const [{floor}]=await sql.unsafe('SELECT last_value::text floor FROM board_input_revision');
 const manifest=await captureRankBaseline(sql,{source,weapon,ruler,native:native.adapter,work,mailboxFloor:floor});
 const epoch=manifest.snapshot;
 const {rows}=await prepareRankSources(native,{weapon,ruler,inputs:seeds});
 const shapes=new Set(),winners=rows.sort((a,b)=>b.score-a.score).filter(r=>!shapes.has(r.shape)&&shapes.add(r.shape));
 const mask=await native.call('floor',winners.map(row=>({score:row.score,leader:Math.max(...winners.filter(x=>x.floor===row.floor).map(x=>x.score))})));
 const visible=winners.filter((r,i)=>mask[i].keep);assert(visible.length);
 const page=sha(visible.map(row=>row.page+'\n').join(''));
 const receipt={source,weapon,ruler,snapshot:manifest.snapshot,native:native.adapter,archive:'c'.repeat(64),members:visible.length,
  eligible:true,unresolved:0,rows:visible.length,pages_sha256:sha('1 '+visible.length+' '+page+'\n')};
 const register=expectedRevision=>registerCatalogScope(sql,{manifest,receipt,contract:native.contract,work,expectedRevision});
 const measurement=(x,body)=>put('measurement',rankMeasurementKey(x.identity,ruler,x.facts[0].mode),body);
 await sql.unsafe('INSERT INTO entity_weapon_builds VALUES($1,$2,$3,$4)',[late[0].identity,weapon,'2026-11-17',JSON.stringify(late[0].record)]);
 await put('build',late[0].identity,{identity:late[0].identity,weapon});
 for(const [i,row] of late.entries()){
  const fact={...row.facts[0],score:1100+i};await insert(fact);await measurement(row,fact);
 }
 await sql.unsafe('UPDATE board_facts SET score=629,review_id=$2 WHERE identity=$1',[correct.identity,'adoption-review']);
 await measurement(correct,{...correct.facts[0],score:629,review_id:'adoption-review'});
 await put('review','adoption-review',{identity:correct.identity,ruler});
 await sql.unsafe('DELETE FROM board_facts WHERE identity=$1',[remove.identity]);await measurement(remove,null);
 await until(allAck,'late first measurements, correction and tombstone acknowledged without a partition');
 const [acks]=await sql.unsafe("SELECT count(*) n FROM state_board_input_mailbox WHERE scope='measurement' AND key IN ($1,$2) AND version=applied_version AND body IS NULL",
  late.map(row=>rankMeasurementKey(row.identity,ruler,row.facts[0].mode)));
 assert.equal(Number(acks.n),2);
 await assert.rejects(register(),/no complete change capture/);
 const writer=await sql.reserve();
 try{
  await writer.unsafe('BEGIN');await writer.unsafe("SELECT board_input_put('review','open-writer',$1)",[JSON.stringify({identity:correct.identity,ruler})]);
  const busy={begin:fn=>sql.begin(async tx=>{await tx.unsafe("SET LOCAL lock_timeout='75ms'");return fn(tx);})};
  await assert.rejects(startCatalogScan(busy,{source,floor,epoch}),e=>e.code==='55P03');
  assert.equal((await sql.unsafe('SELECT count(*) n FROM state_board_rank_catalog_scan'))[0].n,'0');
  await writer.unsafe('COMMIT');
 }finally{writer.release();}
 await startCatalogScan(sql,{source,floor,epoch});
 const first=await scanCatalogPage(sql,source,8);assert(!first.complete);
 await assert.rejects(register(),/no complete change capture/);
 const again=await startCatalogScan(sql,{source,floor,epoch});assert.equal(again.last_key,first.last_key);
 let scan=first;while(!scan.complete)scan=await scanCatalogPage(sql,source,32);
 await assert.rejects(registerCatalogScope(sql,{manifest:{...manifest,mailbox_floor:'0'},receipt,contract:native.contract,work}),/no complete change capture/);
 const orphan=inputs[10],post={...orphan.facts[0],score:501};
 await sql.unsafe('DELETE FROM entity_weapon_builds WHERE id=$1',[orphan.identity]);await put('build',orphan.identity,null);
 await insert(post);await measurement(orphan,post);
 await sql.unsafe('INSERT INTO entity_weapon_builds VALUES($1,$2,$3,$4)',[orphan.identity,weapon,'day',JSON.stringify(orphan.record)]);
 await put('build',orphan.identity,{identity:orphan.identity,weapon});
 await until(allAck,'first fact rebound to its build after the mailbox scan');
 assert.equal((await sql.unsafe("SELECT identity FROM state_board_rank_catalog_inputs WHERE weapon=$1 AND ruler='' AND identity=$2",[weapon,orphan.identity])).length,1);
 assert.equal((await register()).status,'frozen');assert.equal((await register()).status,'frozen');
 await assert.rejects(registerCatalogScope(sql,{work,manifest,contract:native.contract,receipt:{...receipt,rows:receipt.rows+1}}),/reservation changed/);
 await sql.unsafe('UPDATE board_facts SET score=630 WHERE identity=$1',[correct.identity]);
 await measurement(correct,{...correct.facts[0],score:630,review_id:'adoption-review'});
 await until(allAck,'a change during hidden registration reaches the same dirty queue');
 await until(row=>row.status==='snapshot-started','stage frozen publication');
 assert.equal((await read()).error,'not_ready');await restart();
 await sql.unsafe("UPDATE state_board_rank_publication_ready SET lease_until='2000-01-01',available_at='2000-01-01' WHERE weapon=$1 AND ruler=$2",[weapon,ruler]);
 // Mutating one stored output byte must hold the public head, even with the same row count.
 await until(row=>row.status==='snapshot-upload','frozen rows in PostgreSQL');
 const [original]=await sql.unsafe('SELECT token,ordinal,body FROM state_board_rank_snapshot_rows ORDER BY ordinal LIMIT 1');
 await sql.unsafe("UPDATE state_board_rank_snapshot_rows SET body=body||' ' WHERE token=$1 AND ordinal=$2",[original.token,original.ordinal]);
 await until(row=>row.status==='publication_error'&&row.error.includes('staged PostgreSQL pages differ'),'wrong stored page held');
 assert.equal((await read()).error,'not_ready');
 await sql.unsafe('UPDATE state_board_rank_publication_ready SET available_at=board_input_timestamp(clock_timestamp()),error=NULL WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 await sql.unsafe("UPDATE state_board_rank_partitions SET catalog_proof=jsonb_set(catalog_proof,'{pages_sha256}',to_jsonb(repeat('a',64))) WHERE weapon=$1 AND ruler=$2",[weapon,ruler]);
 await until(row=>row.status==='publication_error'&&row.error.includes('qualified catalog'),'independent page mismatch holds correct staged bytes');
 assert.equal((await read()).error,'not_ready');
 await sql.unsafe("UPDATE state_board_rank_partitions SET catalog_proof=jsonb_set(catalog_proof,'{pages_sha256}',to_jsonb($3::text)) WHERE weapon=$1 AND ruler=$2",[weapon,ruler,receipt.pages_sha256]);
 await sql.unsafe('UPDATE state_board_rank_publication_ready SET available_at=board_input_timestamp(clock_timestamp()),error=NULL WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 await until(row=>row.status==='catalog-replay','verified frozen rows start durable replay');
 assert.equal((await register()).status,'replay');assert.equal((await read()).error,'not_ready');
 await restart();
 await until(row=>row.status==='snapshot-started','first current generation after catchup');
 await sql.begin(async tx=>{
  await tx.unsafe("INSERT INTO record_board_read_snapshots VALUES($1,$2,'protected',$3,'engine','digest',0,'[]','day','day')",[weapon,ruler,source]);
  await tx.unsafe("UPDATE state_board_read_partitions SET enabled=true,revision='protected' WHERE weapon=$1 AND ruler=$2",[weapon,ruler]);
 });
 await until(row=>row.status==='publication_error'&&row.error.includes('catalog public selection changed'),'concurrent public head retained');
 assert.equal((await read()).revision,'protected');
 await sql.begin(async tx=>{
  await tx.unsafe('UPDATE state_board_read_partitions SET enabled=false,revision=NULL WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
  await tx.unsafe('UPDATE state_board_rank_publication_ready SET available_at=board_input_timestamp(clock_timestamp()),error=NULL WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 });
 await until(row=>row.status==='published','first visible head after catchup');
 const [head]=await sql.unsafe('SELECT catalog_phase,catalog_proof FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 assert.equal(head.catalog_phase,'active');assert.equal(head.catalog_proof.checked,receipt.pages_sha256);
 const indexed=await sql.unsafe('SELECT identity,score FROM state_board_rank_rows ORDER BY identity');
 assert.deepEqual(indexed.map(x=>[x.identity,x.score]).sort(),[[late[0].identity,1100],[late[1].identity,1101],[correct.identity,630],[orphan.identity,501]].sort());
 const actual=await read();assert.equal(actual.rows[0].score,1101);
 assert.equal((await register(actual.revision)).status,'retained');assert.equal((await register('old-public-revision')).status,'advanced');
 assert.equal((await read()).revision,actual.revision);
 const emptyRuler=native.contract.rulers.find(row=>![ruler,'standard_multi_target'].includes(row.id)).id;
 const empty=await captureRankBaseline(sql,{source,weapon,ruler:emptyRuler,native:native.adapter,work,mailboxFloor:floor});
 const emptyReceipt={...receipt,ruler:emptyRuler,snapshot:empty.snapshot,rows:0,members:0,pages_sha256:sha('')};
 await sql.unsafe("INSERT INTO entity_weapon_builds SELECT 'unmeasured-'||n,$1,'day','{}' FROM generate_series(1,32768) n",[weapon]);
 const emptyArgs={epoch,work,manifest:empty,receipt:emptyReceipt,contract:native.contract};
 const reserved=await requestCatalogFixture(sql,native,weapon,emptyRuler,source);
 await assert.rejects(registerCatalogScope(sql,emptyArgs),/pinned requested reservation/);
 await assert.rejects(registerCatalogScope(sql,{...emptyArgs,reservation:{...reserved,partition_mail:{version:'-9',applied:-1}}}),/partition input changed/);
 await registerCatalogScope(sql,{...emptyArgs,reservation:reserved});
 const [originalMail]=await sql.unsafe("SELECT version,applied_version FROM state_board_input_mailbox WHERE scope='partition' AND key=$1",[JSON.stringify([weapon,emptyRuler])]);
 assert.equal(originalMail.version,reserved.partition_mail.version);assert.equal(originalMail.applied_version,reserved.partition_mail.applied);
 await until(row=>row.status==='catalog-replay'&&row.ruler===emptyRuler,'qualified empty partition');
 const [staged]=await sql.unsafe('SELECT catalog_proof FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,emptyRuler]);
 let checkpoints=0;
 await until(async row=>{
  if(row.status==='baseline-reconciliation')checkpoints++;
  return !!(await sql.unsafe('SELECT revision FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,emptyRuler]))[0].revision;
 },'reused unchanged generation');
 assert(checkpoints<=2,'unmeasured library members must not add catalog checkpoints: '+checkpoints);
 console.log('ok — 32768 unrelated builds, '+checkpoints+' catchup checkpoints');
 const [published]=await sql.unsafe('SELECT token FROM record_board_rank_snapshot_refs WHERE weapon=$1 AND ruler=$2',[weapon,emptyRuler]);
 assert.equal(published.token,staged.catalog_proof.generation);
 const [activated]=await sql.unsafe('SELECT * FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2',[weapon,emptyRuler]);
 assert.equal(activated.position,reserved.request.position);assert(activated.activated_at);
 await checkOccupiedReservation({sql,native,work,source,weapon,inputs,until,insert,floor,epoch});
 await assert.rejects(finishCatalogScan(sql,source,native.adapter,[{weapon,ruler:'missing'}],epoch),/selection is incomplete/);
 const closed=await finishCatalogScan(sql,source,native.adapter,[{weapon,ruler},{weapon,ruler:emptyRuler},{weapon,ruler:'standard_multi_target'}],epoch);
 assert.deepEqual(closed,{closed:true,checked:3});
 assert.equal((await sql.unsafe('SELECT count(*) n FROM state_board_rank_catalog_inputs'))[0].n,'0');
 assert((await startCatalogScan(sql,{source,floor,epoch})).closed);
 console.log('ok — frozen PostgreSQL pages checked, two acknowledged first scores, corrected and removed scores replayed before first public head, restart and newer head retained');
}
