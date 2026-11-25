// SPDX-License-Identifier: AGPL-3.0-or-later
// Interrupted staged generations cannot expose partial pages or advance public clocks.
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdtempSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {randomBytes,createHash} from 'node:crypto';
import postgres from './pg/node_modules/postgres/src/index.js';
import {openInventoryOperator} from './inventory_operator.mjs';
import {createRankPartition,prepareRankSources,applyRankSources,prepareRankSnapshot} from './board_rank_index.mjs';
import {putRankPublication,putRankDirty,claimRankDirty,applyRankDirty} from './board_rank_work.mjs';
import {boardRankReindexUpgrade} from './board_rank_schema.mjs';
import {abandonRankGeneration,BoardRankRivenReindex,deferRankRivenReindex,claimRankPublication,startRankGeneration,floorRankGeneration,uploadRankGeneration,publishRankGeneration,reuseRankGeneration,yieldRankPublication} from './board_rank_publication.mjs';
import {collectRankStorage} from './board_rank_gc.mjs';
import {stageRankRivenRepair,withdrawOldRankRevision} from './board_rank_riven_repair.mjs';
const url=process.env.WFSIM_BOARD_RANK_TEST_URL;assert(url&&['localhost','127.0.0.1'].includes(new URL(url).hostname));
const schema='rank_pages_'+randomBytes(6).toString('hex'),source='00000000-0000-4000-8000-000000000043';
const sql=postgres(url,{max:4,fetch_types:false,onnotice(){},connection:{search_path:schema,statement_timeout:10000}});
const native=await openInventoryOperator(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const work=mkdtempSync(join(tmpdir(),'rank-pages-')),engine='8ce96e275dc2',ruler='standard_single_target';
const base=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[1].record,weapon=base.weapon;
const context={work,source,engine,native:native.adapter,verified:new Map(),prefixes:new Map()};
const oldRevision='a'.repeat(64),oldClock='2026-01-01T00:00:00.000Z';
const records=Array.from({length:150},(_,i)=>({...base,mods:['serration@'+i%10,'vital_sense@'+Math.floor(i/10)%5,'hellfire@'+Math.floor(i/50),
 'speed_trigger','galvanized_aptitude','hammer_shot','vigilante_fervor','magnetic_capacity']}));
const descriptors=await native.call('describe',records.map((record,i)=>({id:'fixture-'+i,record})));
const input=records.map((record,i)=>({identity:descriptors[i].candidate,record,facts:descriptors[i].modes.map(mode=>({identity:descriptors[i].candidate,ruler,mode,
 score:100+i,metric:'kpm',measured_by:'verified:'+engine,finished_at:'2026-11-16T00:00:01.123Z',retired_at:null}))}));
const page=async(revision=null,after=0)=>(await sql.unsafe('SELECT board_read_page($1,$2,$3,$4::bigint,128::bigint) page',[weapon,ruler,revision,after]))[0].page;
const clock=async()=>(await sql.unsafe('SELECT published_at FROM record_board_read_snapshots s JOIN state_board_read_partitions p USING(weapon,ruler,revision) WHERE weapon=$1 AND ruler=$2',[weapon,ruler]))[0].published_at;
try{
 await sql.unsafe('CREATE SCHEMA '+schema);
 for(const name of ['board_mailbox','board_pages','board_rank','board_rank_compact','board_rank_recent','board_rank_zones','board_rank_pages','board_rank_riven_policy'])await sql.unsafe(readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8'));
 await sql.unsafe('UPDATE state_board_input_source SET source=$1',[source]);
 const [{owner}]=await sql.unsafe('SELECT current_user owner');
 const [{oid:readyOid}]=await sql.unsafe("SELECT 'state_board_rank_publication_ready'::regclass::oid::text oid");
 await sql.unsafe('DROP TABLE state_board_rank_reindex_dependencies');
 const upgrade=boardRankReindexUpgrade({schema,readerRole:owner,tablespace:'pg_default'});
 const installer=await sql.reserve();
 try{await installer.unsafe(upgrade);await installer.unsafe(upgrade);}finally{installer.release();}
 assert.equal((await sql.unsafe("SELECT 'state_board_rank_publication_ready'::regclass::oid::text oid"))[0].oid,readyOid);

 let part=await createRankPartition(sql,{weapon,ruler,source,native:native.adapter,cut:'fixture'});
 for(let i=0;i<input.length;i+=64){const prepared=await prepareRankSources(native,{weapon,ruler,inputs:input.slice(i,i+64)});part=await sql.begin(tx=>applyRankSources(tx,part,prepared));}
 await sql.unsafe('UPDATE state_board_rank_partitions SET complete=true,seeded=true,reconciled=true WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 await sql.begin(async tx=>{
  await tx.unsafe('INSERT INTO state_board_read_partitions VALUES($1,$2,$3,true,true,$4)',[weapon,ruler,native.contract.rulers.find(row=>row.id===ruler).scene,oldRevision]);
  await tx.unsafe("INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,$4,$5,$6,1,'[]',$7,$7)",[weapon,ruler,oldRevision,source,engine,'b'.repeat(64),oldClock]);
  await tx.unsafe('INSERT INTO state_board_read_rows VALUES($1,$2,$3,1,$4)',[weapon,ruler,oldRevision,JSON.stringify({score:99,mode:'base'})]);
 });
 await sql.begin(tx=>putRankPublication(tx,{weapon,ruler}));
 const claim=await claimRankPublication(sql,{source,native:native.adapter});assert(claim);
 let generation=await startRankGeneration(sql,native,claim,context);
 await assert.rejects(floorRankGeneration(sql,native,claim,generation,{...context,fault:async()=>{throw Error('injected output interruption');}}),/injected/);
 assert.equal((await page()).revision,oldRevision);assert.equal(await clock(),oldClock);
 assert.equal((await sql.unsafe('SELECT output_rows FROM state_board_rank_generations WHERE token=$1::uuid',[generation.token]))[0].output_rows,'0');
 while(generation.phase==='floor')generation=await floorRankGeneration(sql,native,claim,generation,context);
 assert.equal(Number(generation.output_rows),300,'every sustainable mode is retained');
 assert.equal((await page()).revision,oldRevision,'floor completion alone is private');
 await assert.rejects(uploadRankGeneration(sql,claim,generation,{...context,fault:async()=>{throw Error('injected upload interruption');}}),/injected/);
 assert.equal(Number((await sql.unsafe('SELECT count(*) n FROM state_board_rank_snapshot_rows WHERE token=$1::uuid',[generation.token]))[0].n),0);
 generation=await uploadRankGeneration(sql,claim,generation,{...context,verified:new Map()});
 assert.equal(Number(generation.uploaded_rows),256);assert.equal((await page()).revision,oldRevision);
 await assert.rejects(publishRankGeneration(sql,native,claim,generation,context),/not completely uploaded/);
 assert.equal(await clock(),oldClock,'partial upload cannot advance the clock');
 const other='heavy_gunner',otherKey={weapon,ruler:other,zone:0};
 await createRankPartition(sql,{...otherKey,source,native:native.adapter,cut:'second'});
 await sql.unsafe('UPDATE state_board_rank_partitions SET complete=true,seeded=true,reconciled=true WHERE weapon=$1 AND ruler=$2',[weapon,other]);
 await sql.unsafe('INSERT INTO state_board_read_partitions VALUES($1,$2,$3,true,true,NULL)',[weapon,other,native.contract.rulers.find(r=>r.id===other).scene]);
 await sql.begin(tx=>putRankPublication(tx,otherKey));
 assert(await yieldRankPublication(sql,claim));
 const small=await claimRankPublication(sql,context);assert.equal(small.ruler,other);
 let empty=await startRankGeneration(sql,native,small,context);
 empty=await floorRankGeneration(sql,native,small,empty,context);
 assert.equal((await publishRankGeneration(sql,native,small,empty,context)).status,'published');
 assert.equal((await page()).revision,oldRevision);
 assert.equal((await sql.unsafe('SELECT uploaded_rows FROM state_board_rank_generations WHERE token=$1::uuid',[generation.token]))[0].uploaded_rows,'256');
 const reclaim=await claimRankPublication(sql,context);assert.equal(reclaim.ruler,ruler);
 assert.equal(await yieldRankPublication(sql,claim),false,'a prior lease cannot yield resumed work');
 Object.assign(claim,reclaim);
 await sql.unsafe('DELETE FROM state_board_rank_publication_ready WHERE weapon=$1 AND ruler=$2',[weapon,other]);
 while(!generation.uploaded)generation=await uploadRankGeneration(sql,claim,generation,context);
 const originalRevision=generation.index_revision;
 await sql.unsafe("UPDATE state_board_rank_generations SET index_revision=9007199254740992 WHERE token=$1::uuid",[generation.token]);
 await sql.unsafe('UPDATE state_board_rank_publication_ready SET delivered_revision=9007199254740993');
 assert.equal((await publishRankGeneration(sql,native,claim,generation,context)).status,'superseded','consecutive revisions beyond safe integer range cannot regress publication');
 assert.equal((await page()).revision,oldRevision);assert.equal(await clock(),oldClock);
 await sql.unsafe("UPDATE state_board_rank_generations SET phase='upload',index_revision=$2 WHERE token=$1::uuid",[generation.token,originalRevision]);
 await sql.unsafe('UPDATE state_board_rank_publication_ready SET delivered_revision=0,applied_version=-1,available_at=board_input_timestamp(clock_timestamp())');
 const resumed=await claimRankPublication(sql,{source,native:native.adapter});assert(resumed);
 Object.assign(claim,resumed);
 await sql.begin(tx=>putRankPublication(tx,{weapon,ruler}));
 const published=await publishRankGeneration(sql,native,claim,generation,context);assert.equal(published.status,'published');
 const expected=await prepareRankSnapshot(sql,native,{weapon,ruler,source,work}),rows=[];
 let cursor=0;
 do{const response=await page(published.revision,cursor);rows.push(...response.rows);cursor=response.next;}while(cursor!==null);
 assert.deepEqual(rows,expected.rows,'all immutable pages retain the complete Native order and values');
 assert.equal((await page(oldRevision)).rows[0].score,99,'the prior complete reference remains readable');
 const currentClock=await clock();
 const [ready]=await sql.unsafe('SELECT * FROM state_board_rank_publication_ready WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 assert(BigInt(ready.version)>BigInt(ready.applied_version),'an update arriving during publication remains ready');
 const next=await claimRankPublication(sql,{source,native:native.adapter});
 let same=await startRankGeneration(sql,native,next,context);
 while(same.phase==='floor')same=await floorRankGeneration(sql,native,next,same,context);
 assert.equal((await reuseRankGeneration(sql,next,same,context)).status,'unchanged');
 assert.equal(await clock(),currentClock,'identical output retains its public clock');
 assert.equal(Number((await sql.unsafe('SELECT count(*) n FROM state_board_rank_snapshot_rows WHERE token=$1::uuid',[same.token]))[0].n),0);
 await sql.begin(tx=>putRankPublication(tx,{weapon,ruler}));
 const badClaim=await claimRankPublication(sql,{source,native:native.adapter});
 let bad=await startRankGeneration(sql,native,badClaim,context);bad=await floorRankGeneration(sql,native,badClaim,bad,context);
 const path=join(bad.directory,'rows.ndjson'),bytes=readFileSync(path);bytes[1]^=1;writeFileSync(path,bytes);
 await assert.rejects(floorRankGeneration(sql,native,badClaim,bad,context),/prefix changed/);
 assert.equal(await clock(),currentClock);assert.deepEqual((await page(published.revision)).rows,rows.slice(0,128));
 for(let i=0;i<5;i++){
  const revision=String(i).repeat(64);
  await sql.unsafe(`INSERT INTO record_board_read_snapshots SELECT weapon,ruler,$3,source,engine,inputs_sha256,rows,leaders,captured_at,
   board_input_timestamp(clock_timestamp()) FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2 AND revision=$4`,[weapon,ruler,revision,published.revision]);
  await sql.unsafe('INSERT INTO record_board_rank_snapshot_refs VALUES($1,$2,$3,$4::uuid)',[weapon,ruler,revision,generation.token]);
 }
 assert.notEqual((await collectRankStorage(sql,context))?.status,'retired-reference','recent pinned pages retain ten minutes');
 await sql.unsafe(`UPDATE record_board_read_snapshots SET published_at='2026-10-01T00:00:00.000Z' WHERE revision<>$1 AND revision<>$2`,[published.revision,oldRevision]);
 for(let i=0;i<8;i++)await collectRankStorage(sql,context);
 assert.equal(Number((await sql.unsafe('SELECT count(*) n FROM record_board_rank_snapshot_refs WHERE weapon=$1 AND ruler=$2',[weapon,ruler]))[0].n),3,'current plus newer retained versions survive collection');
 assert.deepEqual((await page(published.revision)).rows,rows.slice(0,128));
 await sql.unsafe("UPDATE state_board_rank_generations SET phase='abandoned' WHERE token=$1::uuid",[bad.token]);
 await sql.unsafe("INSERT INTO state_board_rank_snapshot_rows SELECT $1::uuid,n,'{}' FROM generate_series(1,1201)n",[bad.token]);
 const gc=await collectRankStorage(sql,context);assert.equal(gc.status,'collected-rows');assert.equal(gc.rows,1000);
 assert.equal(Number((await sql.unsafe('SELECT count(*) n FROM state_board_rank_snapshot_rows WHERE token=$1::uuid',[bad.token]))[0].n),201);
 for(let i=0;i<4;i++)await collectRankStorage(sql,context);
 assert.equal(Number((await sql.unsafe('SELECT count(*) n FROM state_board_rank_generations WHERE token=$1::uuid',[bad.token]))[0].n),0);
 await yieldRankPublication(sql,badClaim);
 await sql.unsafe('INSERT INTO state_board_rank_riven_policy VALUES(1,$1)',[native.adapter]);
 await sql.unsafe('UPDATE state_board_rank_generations SET riven_policy=NULL WHERE token=$1::uuid',[generation.token]);
 await assert.rejects(sql.unsafe(`INSERT INTO record_board_rank_snapshot_refs VALUES($1,$2,$3,$4::uuid) ON CONFLICT DO NOTHING`,
  [weapon,ruler,published.revision,generation.token]),/riven policy changed/);
 const bound={...context,policy:native.adapter};
 await assert.rejects(withdrawOldRankRevision(sql,bound,{weapon,ruler,revision:published.revision}),/needs a verified replacement/);
 const repair={weapon,ruler,revision:published.revision,index_revision:part.revision,identities:[input[0].identity]};
 await assert.rejects(stageRankRivenRepair(sql,bound,{...repair,index_revision:'0'}),/index or public revision advanced/);
 assert.equal((await stageRankRivenRepair(sql,bound,repair)).scheduled,1);
 assert.equal(await claimRankPublication(sql,context),null,'publication waits for the scoped index work');
 assert.equal((await page()).revision,published.revision);
 const updatedInput=structuredClone(input[0]);updatedInput.facts[0].score+=1;
 const prepared=await prepareRankSources(native,{weapon,ruler,inputs:[updatedInput]});
 part=await sql.begin(tx=>applyRankSources(tx,part,prepared,{live:true}));
 await sql.unsafe('UPDATE state_board_rank_dirty SET applied_version=version WHERE (weapon,ruler,identity)=($1,$2,$3)',[weapon,ruler,input[0].identity]);
 const repairClaim=await claimRankPublication(sql,context);assert(repairClaim);
 async function buildShadow(){
  let shadow=await startRankGeneration(sql,native,repairClaim,context);
  while(shadow.phase==='floor')shadow=await floorRankGeneration(sql,native,repairClaim,shadow,context);
  while(!shadow.uploaded)shadow=await uploadRankGeneration(sql,repairClaim,shadow,context);
  return shadow;
 }
 const staleShadow=await buildShadow();
 part=await sql.begin(tx=>applyRankSources(tx,part,prepared,{live:true}));
 await assert.rejects(publishRankGeneration(sql,native,repairClaim,staleShadow,context),/ranking or public revision advanced/);
 assert.equal((await page(published.revision)).ok,true,'a lost compare-and-switch leaves the current head readable');
 await abandonRankGeneration(sql,repairClaim,staleShadow);
 const repaired=await publishRankGeneration(sql,native,repairClaim,await buildShadow(),context);
 assert.equal((await page()).revision,repaired.revision);
 assert.equal((await page(published.revision)).error,'revision_unavailable','replacement and withdrawal commit together');
 assert.equal((await withdrawOldRankRevision(sql,bound,{weapon,ruler,revision:oldRevision})).withdrawn,true);
 assert.equal((await page(oldRevision)).error,'revision_unavailable');
 const latentId=input.at(-1).identity,earlyId=input.at(-2).identity;
 const invalidRiven={bonuses:['critical_damage','multishot'],malus:null,rolls:[1.02,0.99]};
 await sql.unsafe(`UPDATE state_board_rank_winners SET riven=true,page=jsonb_set(page::jsonb,'{riven}',$4::jsonb)::text
  WHERE weapon=$1 AND ruler=$2 AND identity IN (SELECT jsonb_array_elements_text($3::text::jsonb))`,[weapon,ruler,JSON.stringify([latentId,earlyId]),JSON.stringify(invalidRiven)]);
 await sql.begin(async tx=>{
  await putRankPublication(tx,{weapon,ruler,zone:3,zone_position:100,queued_at:oldClock});
  await putRankDirty(tx,{weapon,ruler,identity:latentId,zone:3,zone_position:200,queued_at:oldClock});
  await putRankDirty(tx,{weapon,ruler,identity:earlyId,zone:3,zone_position:50,queued_at:oldClock});
 });
 const latentClaim=await claimRankPublication(sql,context);
 const latent=await startRankGeneration(sql,native,latentClaim,context);let invalid;
 await assert.rejects(floorRankGeneration(sql,native,latentClaim,latent,context),error=>{
  invalid=error;return error instanceof BoardRankRivenReindex&&error.identities.includes(latentId)&&error.identities.includes(earlyId);
 });
 assert.equal((await page()).revision,repaired.revision,'latent legacy winners cannot enter a new public floor');
 assert.equal((await deferRankRivenReindex(sql,latentClaim,latent,invalid)).dependencies,2);
 assert.equal((await sql.unsafe('SELECT phase FROM state_board_rank_generations WHERE token=$1::uuid',[latent.token]))[0].phase,'abandoned');
 const blocked=await claimRankPublication(sql,context);assert.equal(blocked,null,'the earlier parent waits for the exact reindex versions');
 const dirty=await sql.unsafe(`SELECT d.identity,d.zone,d.zone_position::text pos,d.queued_at,d.version::text,d.applied_version::text,i.version::text dep
  FROM state_board_rank_reindex_dependencies i JOIN state_board_rank_dirty d USING(weapon,ruler,identity)
  WHERE weapon=$1 AND ruler=$2 ORDER BY d.zone,d.zone_position`,[weapon,ruler]);
 assert.deepEqual(dirty.map(d=>[d.identity,d.zone,d.pos,d.queued_at]),[
  [earlyId,3,'50',oldClock],[latentId,3,'100',oldClock]]);
 assert(dirty.every(d=>d.version===d.dep&&BigInt(d.applied_version)<BigInt(d.dep)));
 await sql.begin(tx=>putRankPublication(tx,otherKey));
 const unrelated=await claimRankPublication(sql,context);assert.equal(unrelated.ruler,other,'the parent waits without blocking other publications');
 await sql.unsafe('UPDATE state_board_rank_publication_ready SET applied_version=version,lease=NULL,lease_version=NULL,lease_until=NULL,available_at=NULL WHERE weapon=$1 AND ruler=$2',[weapon,other]);
 for(const identity of [earlyId,latentId]){
  const claim=await claimRankDirty(sql,{...context,recent:true});assert.equal(claim.identity,identity);
  const prepared=await prepareRankSources(native,{weapon,ruler,inputs:[input.find(row=>row.identity===identity)]});
  part=(await sql.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]))[0];
  assert.equal((await applyRankDirty(sql,claim,part,prepared)).status,'indexed');
  if(identity===earlyId)assert.equal(await claimRankPublication(sql,context),null,'one unfinished version still holds publication');
 }
 const recoveredClaim=await claimRankPublication(sql,context);assert(recoveredClaim);
 let recovered=await startRankGeneration(sql,native,recoveredClaim,context);
 while(recovered.phase==='floor')recovered=await floorRankGeneration(sql,native,recoveredClaim,recovered,context);
 while(!recovered.uploaded)recovered=await uploadRankGeneration(sql,recoveredClaim,recovered,context);
 const valid=await publishRankGeneration(sql,native,recoveredClaim,recovered,context);
 assert.equal((await page()).revision,valid.revision);
 assert.equal((await page(repaired.revision)).ok,true,'automatic reindex does not withdraw the previous public revision');
 assert.equal((await sql.unsafe('SELECT count(*) n FROM state_board_rank_reindex_dependencies'))[0].n,'0');
 console.log('ok — real staged append/upload rollback, prefix recovery, complete atomic pages, preserved old refs, concurrent readiness and unchanged clocks');
}finally{await native.close();await sql.unsafe('DROP SCHEMA IF EXISTS '+schema+' CASCADE');await sql.end();rmSync(work,{recursive:true,force:true});}
