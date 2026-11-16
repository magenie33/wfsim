// SPDX-License-Identifier: AGPL-3.0-or-later
// Per-identity readiness joins canonical inputs to one atomic derived-index commit.
import {randomUUID,createHash} from 'node:crypto';
import {prepareRankSources,applyRankSources,BoardRankConflict,BoardRankQualificationError} from './board_rank_index.mjs';
import {lockRankInputs} from './board_rank_keys.mjs';
const now="board_input_timestamp(clock_timestamp())";
const until="board_input_timestamp(clock_timestamp()+interval '3minutes')";
const hash=value=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const measurementKey=(identity,ruler,mode)=>'['+[identity,ruler,mode].map(JSON.stringify).join(', ')+']';

export async function putRankDirty(tx,{weapon,ruler,identity}) {
 const [row]=await tx.unsafe(`INSERT INTO state_board_rank_dirty(weapon,ruler,identity) VALUES($1,$2,$3)
  ON CONFLICT(weapon,ruler,identity) DO UPDATE SET version=nextval('board_rank_generation'),error=NULL,
   available_at=CASE WHEN state_board_rank_dirty.lease_until>${now} THEN state_board_rank_dirty.lease_until ELSE ${now} END
  RETURNING version`,[weapon,ruler,identity]);
 return row.version;
}

export async function putRankPublication(tx,{weapon,ruler}) {
 await tx.unsafe(`INSERT INTO state_board_rank_publication_ready(weapon,ruler) VALUES($1,$2)
  ON CONFLICT(weapon,ruler) DO UPDATE SET version=nextval('board_rank_generation'),error=NULL,
   available_at=CASE WHEN state_board_rank_publication_ready.lease_until>${now} THEN state_board_rank_publication_ready.lease_until ELSE ${now} END`,[weapon,ruler]);
}

export async function routeRankInput(tx,input,source) {
 await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
 let identity=input.scope==='build'?input.key:input.scope==='measurement'?JSON.parse(input.key)[0]:input.body?.identity;
 const ruler=input.scope==='measurement'?JSON.parse(input.key)[1]:input.body?.ruler;
 await lockRankInputs(tx,source,{identities:identity?[identity]:[],reviews:input.scope==='review'?[input.key]:[]});
 const dependencies=await tx.unsafe(`SELECT DISTINCT weapon,ruler,identity FROM state_board_rank_dependencies
  WHERE scope=$1 AND key=$2 AND(version IS NULL OR version<$3::bigint)`,[input.scope,input.key,input.version]);
 const targets=new Map(dependencies.map(row=>[JSON.stringify([row.weapon,row.ruler,row.identity]),row]));
 if(identity){
  const [build]=await tx.unsafe('SELECT weapon FROM entity_weapon_builds WHERE id=$1',[identity]);
  const weapon=build?.weapon||(input.scope==='build'?input.body?.weapon:null);
  if(weapon){
   const parts=await tx.unsafe('SELECT weapon,ruler FROM state_board_rank_partitions WHERE weapon=$1 AND ($2::text IS NULL OR ruler=$2)',[weapon,ruler||null]);
   for(const part of parts){
    const [covered]=await tx.unsafe(`SELECT 1 FROM state_board_rank_dependencies WHERE weapon=$1 AND ruler=$2 AND identity=$3
     AND scope=$4 AND key=$5 AND version>=$6::bigint`,[part.weapon,part.ruler,identity,input.scope,input.key,input.version]);
    if(!covered)targets.set(JSON.stringify([part.weapon,part.ruler,identity]),{...part,identity});
   }
   if(input.deleted)await tx.unsafe('DELETE FROM state_board_read_routes WHERE scope=$1 AND key=$2',[input.scope,input.key]);
   else await tx.unsafe(`INSERT INTO state_board_read_routes VALUES($1,$2,$3,$4) ON CONFLICT(scope,key)
    DO UPDATE SET weapon=excluded.weapon,ruler=excluded.ruler`,[input.scope,input.key,weapon,ruler||'']);
  }
 }
 if(targets.size||input.scope==='review'||(identity&&input.deleted))
  await tx.unsafe(`INSERT INTO state_board_rank_route_inputs VALUES($1,$2,$3) ON CONFLICT(scope,key)
   DO UPDATE SET version=greatest(state_board_rank_route_inputs.version,excluded.version)`,[input.scope,input.key,input.version]);
 for(const row of targets.values())await putRankDirty(tx,row);
 return {identities:targets.size};
}

export async function reconcileRankBaseline(tx,{weapon,ruler,source}) {
 const [part]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 if(!part.seeded||part.reconciled)return;
 const rows=await tx.unsafe(`SELECT DISTINCT d.identity FROM state_board_rank_dependencies d
  JOIN state_board_input_mailbox m ON m.scope=d.scope AND m.key=d.key
  WHERE d.weapon=$1 AND d.ruler=$2 AND d.identity>$3 AND d.version IS DISTINCT FROM m.version ORDER BY d.identity LIMIT 64`,[weapon,ruler,part.reconcile_cursor]);
 await lockRankInputs(tx,source,{identities:rows.map(row=>row.identity)});
 for(const {identity} of rows){
  const version=await putRankDirty(tx,{weapon,ruler,identity});
  await tx.unsafe(`INSERT INTO state_board_rank_baseline_obligations VALUES($1,$2,$3,$4)
   ON CONFLICT(weapon,ruler,identity) DO UPDATE SET version=excluded.version`,[weapon,ruler,identity,version]);
 }
 const [locked]=await tx.unsafe('SELECT reconcile_cursor FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 FOR UPDATE',[weapon,ruler]);
 if(locked.reconcile_cursor!==part.reconcile_cursor)throw new BoardRankConflict('baseline reconciliation advanced');
 await tx.unsafe('UPDATE state_board_rank_partitions SET reconcile_cursor=$3,reconciled=$4 WHERE weapon=$1 AND ruler=$2',
  [weapon,ruler,rows.at(-1)?.identity||part.reconcile_cursor,rows.length<64]);
}

export async function finishRankBaseline(tx,{weapon,ruler}) {
 const [row]=await tx.unsafe(`UPDATE state_board_rank_partitions p SET complete=true WHERE weapon=$1 AND ruler=$2 AND seeded AND reconciled AND NOT complete
  AND NOT EXISTS(SELECT 1 FROM state_board_rank_baseline_obligations o JOIN state_board_rank_dirty d USING(weapon,ruler,identity)
   WHERE o.weapon=p.weapon AND o.ruler=p.ruler AND d.applied_version<o.version) RETURNING weapon,ruler`,[weapon,ruler]);
 if(row)await putRankPublication(tx,row);
 return !!row;
}

export async function claimRankDirty(sql,{source,native}) {
 const token=randomUUID();
 const [row]=await sql.unsafe(`WITH chosen AS MATERIALIZED(SELECT d.weapon,d.ruler,d.identity FROM state_board_rank_dirty d
  JOIN state_board_rank_partitions p USING(weapon,ruler) WHERE p.source=$1 AND p.native=$2
   AND d.version>d.applied_version AND d.available_at<=${now} ORDER BY d.available_at,d.weapon,d.ruler,d.identity
  LIMIT 1 FOR UPDATE OF d SKIP LOCKED)
  UPDATE state_board_rank_dirty d SET lease=$3,lease_until=${until},available_at=${until} FROM chosen c
  WHERE (d.weapon,d.ruler,d.identity)=(c.weapon,c.ruler,c.identity) RETURNING d.*`,[source,native,token]);
 return row||null;
}

export async function prepareRankIdentity(sql,native,part,identity) {
 const [cached]=await sql.unsafe('SELECT * FROM state_board_rank_sources WHERE weapon=$1 AND ruler=$2 AND identity=$3',[part.weapon,part.ruler,identity]);
 for(let attempt=0;attempt<3;attempt++){
  const [before]=await sql.unsafe('SELECT record FROM entity_weapon_builds WHERE id=$1',[identity]);
  const record=before?JSON.parse(before.record):null;
  const described=await prepareRankSources(native,{weapon:part.weapon,ruler:part.ruler,inputs:[{identity,record,facts:[]}],cached:cached?[cached]:[]});
  const descriptor=described.sources[0];
  const captured=await sql.begin('isolation level repeatable read read only',async tx=>{
   const [build]=await tx.unsafe('SELECT record FROM entity_weapon_builds WHERE id=$1',[identity]);
   if(hash(build?JSON.parse(build.record):null)!==descriptor.record_sha256)return null;
   const modes=descriptor.descriptor.publication_modes||[];
   const facts=build?await tx.unsafe(`SELECT * FROM board_facts WHERE identity=$1 AND ruler=$2
    AND mode IN(SELECT jsonb_array_elements_text(($3::text)::jsonb)) ORDER BY mode`,[identity,part.ruler,JSON.stringify(modes.map(row=>row.id))]):[];
   const dependencies=[{scope:'build',key:identity},...modes.map(mode=>({scope:'measurement',key:measurementKey(identity,part.ruler,mode.id)})),
    ...[...new Set(facts.map(row=>row.review_id).filter(Boolean))].map(key=>({scope:'review',key}))];
   const headers=await tx.unsafe(`SELECT scope,key,version FROM state_board_input_mailbox WHERE (scope,key) IN
    (SELECT x.scope,x.key FROM jsonb_to_recordset(($1::text)::jsonb)x(scope text,key text))`,[JSON.stringify(dependencies)]);
   return {identity,record,facts,dependencies:dependencies.map(row=>({...row,version:headers.find(h=>h.scope===row.scope&&h.key===row.key)?.version??null}))};
  });
  if(captured)return prepareRankSources(native,{weapon:part.weapon,ruler:part.ruler,inputs:[captured],cached:[descriptor]});
 }
 throw new BoardRankConflict('canonical source changed during Native description');
}

export async function applyRankDirty(sql,claim,part,prepared) {
 return sql.begin(async tx=>{
  await lockRankInputs(tx,part.source,{identities:[claim.identity],reviews:prepared.sources.flatMap(row=>row.dependencies.filter(d=>d.scope==='review').map(d=>d.key))});
  const [held]=await tx.unsafe(`SELECT * FROM state_board_rank_dirty WHERE weapon=$1 AND ruler=$2 AND identity=$3
   AND lease=$4 AND lease_until>${now} FOR UPDATE`,[claim.weapon,claim.ruler,claim.identity,claim.lease]);
  if(!held)throw new BoardRankConflict('rank input lease changed');
  if(String(held.version)!==String(claim.version)){
   await tx.unsafe(`UPDATE state_board_rank_dirty SET lease=NULL,lease_until=NULL,available_at=${now}
    WHERE weapon=$1 AND ruler=$2 AND identity=$3`,[claim.weapon,claim.ruler,claim.identity]);
   return {status:'superseded'};
  }
  const updated=await applyRankSources(tx,part,prepared,{live:true});
  await tx.unsafe(`UPDATE state_board_rank_dirty SET applied_version=$4,lease=NULL,lease_until=NULL,available_at=NULL,error=NULL
   WHERE weapon=$1 AND ruler=$2 AND identity=$3`,[claim.weapon,claim.ruler,claim.identity,claim.version]);
  if(updated.complete)await putRankPublication(tx,updated);
  else await finishRankBaseline(tx,updated);
  return {status:'indexed',identity:claim.identity};
 });
}

export async function retryRankDirty(sql,claim,error) {
 const delay=error instanceof BoardRankQualificationError?null:error instanceof BoardRankConflict?0:5;
 await sql.unsafe(`UPDATE state_board_rank_dirty SET lease=NULL,lease_until=NULL,error=$5,
  available_at=CASE WHEN version>$4 THEN ${now} WHEN $6::bigint IS NULL THEN NULL
   ELSE board_input_timestamp(clock_timestamp()+make_interval(secs=>$6::bigint)) END
  WHERE weapon=$1 AND ruler=$2 AND identity=$3 AND lease=$7`,[claim.weapon,claim.ruler,claim.identity,claim.version,error.message,delay,claim.lease]);
}
