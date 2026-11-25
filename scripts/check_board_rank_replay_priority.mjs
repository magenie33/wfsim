// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRankPartition} from './board_rank_index.mjs';
import {putRankDirty,reconcileRankBaseline} from './board_rank_work.mjs';

export async function checkReplayPriority(sql,source,native){
 const weapon='replay_priority_fixture',ruler='parent',earlier='2026-11-19T00:00:00.000Z',at='2026-11-20T00:00:00.000Z';
 const scope={weapon,ruler,source,native,cut:'fixture-cut'};
 const add=async r=>{
  await createRankPartition(sql,{...scope,ruler:r});
  await sql.unsafe("UPDATE state_board_rank_partitions SET seeded=true,catalog_phase='replay',catalog_proof='{}' WHERE weapon=$1 AND ruler=$2",[weapon,r]);
 };
 await add(ruler);await add('without_parent');
 await sql.unsafe('INSERT INTO state_board_rank_new_scopes(weapon,ruler,zone,zone_position,queued_at) VALUES($1,$2,0,100,$3)',[weapon,ruler,at]);
 const identities=Array.from({length:40},(_,i)=>'replay-priority-'+String(i).padStart(2,'0'));
 for(const id of identities){
  await sql.unsafe("INSERT INTO entity_weapon_builds VALUES($1,$2,$3,'{}')",[id,weapon,at]);
  await sql.unsafe("INSERT INTO board_facts VALUES($1,$2,'base',1,'kpm','fixture',$3,NULL,NULL)",[id,ruler,at]);
  await sql.unsafe('INSERT INTO state_board_rank_catalog_inputs VALUES($1,$2,$3)',[weapon,ruler,id]);
 }
 await sql.unsafe("INSERT INTO board_facts VALUES($1,'without_parent','base',1,'kpm','fixture',$2,NULL,NULL)",[identities[0],at]);
 await sql.unsafe("INSERT INTO state_board_rank_catalog_inputs VALUES($1,'without_parent',$2)",[weapon,identities[0]]);
 const dirty=identity=>({weapon,ruler,identity});
 await sql.begin(async tx=>{
  await putRankDirty(tx,{...dirty(identities[1]),zone:0,zone_position:50,queued_at:earlier});
  await putRankDirty(tx,{...dirty(identities[2]),zone:3,zone_position:77,queued_at:at});
  await reconcileRankBaseline(tx,scope);
  await reconcileRankBaseline(tx,{...scope,ruler:'without_parent'});
 });
 const rows=async()=>sql.unsafe(`SELECT identity,zone,zone_position::text,queued_at,version::text,applied_version::text,
   lease,lease_until,available_at FROM state_board_rank_dirty WHERE weapon=$1 AND ruler=$2 ORDER BY identity`,[weapon,ruler]);
 const initial=await rows();
 assert.equal(initial.length,40);
 assert.deepEqual([initial[0].zone,initial[0].zone_position,initial[0].queued_at],[0,'100',at]);
 assert.deepEqual([initial[1].zone,initial[1].zone_position,initial[1].queued_at],[0,'50',earlier]);
 assert.deepEqual([initial[2].zone,initial[2].zone_position,initial[2].queued_at],[0,'100',at]);
 const [ordinary]=await sql.unsafe("SELECT zone FROM state_board_rank_dirty WHERE weapon=$1 AND ruler='without_parent'",[weapon]);
 assert.equal(ordinary.zone,99,'a replay without a live parent keeps default priority');

 // Restore the observed old scheduling form, plus controls that must stay untouched.
 await sql.unsafe("UPDATE state_board_rank_dirty SET zone=99,zone_position=1000,queued_at='2026-11-25' WHERE weapon=$1 AND ruler=$2 AND identity<>$3",[weapon,ruler,identities[1]]);
 await sql.unsafe("UPDATE state_board_rank_dirty SET version=version+100000 WHERE weapon=$1 AND ruler=$2 AND identity=$3",[weapon,ruler,identities[3]]);
 await sql.unsafe("UPDATE state_board_rank_dirty SET applied_version=version WHERE weapon=$1 AND ruler=$2 AND identity=$3",[weapon,ruler,identities[4]]);
 await sql.unsafe("UPDATE state_board_rank_dirty SET lease='held',lease_until='2999-01-01' WHERE weapon=$1 AND ruler=$2 AND identity=$3",[weapon,ruler,identities[5]]);
 const before=await rows();
 const repair=readFileSync(new URL('./board_rank_replay_priority.sql',import.meta.url),'utf8');
 const run=async revision=>sql.begin(async tx=>{
  await tx.unsafe('CREATE TEMP TABLE rank_replay_scope(weapon text,ruler text,source text,native text,cut text,revision bigint,zone smallint,zone_position bigint,queued_at text) ON COMMIT DROP');
  await tx.unsafe('INSERT INTO rank_replay_scope VALUES($1,$2,$3,$4,$5,$6,0,100,$7)',[weapon,ruler,source,native,scope.cut,revision,at]);
  return tx.unsafe(repair);
 });
 assert.equal((await run(999)).length,0,'a mismatched cut revision cannot schedule any row');
 const first=await run(0),second=await run(0);
 assert.equal(first.length,32);assert.equal(second.length,4);
 assert.equal((await run(0)).length,0,'same repair is idempotent');
 const after=await rows();
 for(const i of [1,3,4,5])assert.deepEqual(after[i],before[i]);
 for(const [i,row] of after.entries()){
  const {zone,zone_position,queued_at,...other}=row;
  const {zone:_zone,zone_position:_pos,queued_at:_at,...old}=before[i];
  assert.deepEqual(other,old,'only queue fields may change');
  if(![1,3,4,5].includes(i))assert.deepEqual([zone,zone_position,queued_at],[0,'100',at]);
 }
 assert.equal((await sql.unsafe("SELECT zone FROM state_board_rank_dirty WHERE weapon=$1 AND ruler='without_parent'",[weapon]))[0].zone,99);
 console.log('ok — inherited replay priority and bounded exact in-place repair');
}
