// SPDX-License-Identifier: AGPL-3.0-or-later
// Per-identity readiness joins canonical inputs to one atomic derived-index commit.
import {randomUUID,createHash} from 'node:crypto';
import {prepareRankSources,applyRankSources,BoardRankConflict,BoardRankQualificationError} from './board_rank_index.mjs';
import {cachedRankInputs,registeredRankInputs} from './board_rank_inputs.mjs';
import {lockRankInputs,rankMeasurementKey} from './board_rank_keys.mjs';
const now="board_input_timestamp(clock_timestamp())";
const until="board_input_timestamp(clock_timestamp()+interval '3minutes')";
const hash=value=>createHash('sha256').update(JSON.stringify(value)).digest('hex');


// Preserve the age and highest priority of an open item even while its lease is held.
const mergeQueue = `zone=CASE WHEN queued.version>queued.applied_version THEN least(queued.zone,excluded.zone) ELSE excluded.zone END,
 zone_position=CASE WHEN queued.version<=queued.applied_version OR excluded.zone<queued.zone THEN excluded.zone_position
 WHEN excluded.zone=queued.zone THEN CASE WHEN queued.zone_position IS NULL THEN NULL ELSE least(queued.zone_position,excluded.zone_position) END
 ELSE queued.zone_position END,
 queued_at=CASE WHEN queued.version>queued.applied_version THEN least(coalesce(queued.queued_at,queued.available_at),excluded.queued_at)
 ELSE excluded.queued_at END`;

export async function putRankDirtyBatch(tx,targets,recent=false) {
 if(!targets.length)return [];
 return tx.unsafe(`WITH inputs AS MATERIALIZED(SELECT x.*,nextval('board_rank_generation') v
  FROM jsonb_to_recordset($1::text::jsonb)x(weapon text,ruler text,identity text,zone smallint,queued_at text,zone_position bigint) ORDER BY weapon,ruler,identity)
  INSERT INTO state_board_rank_dirty AS queued(weapon,ruler,identity,version,recent_version,zone,queued_at,zone_position)
  SELECT weapon,ruler,identity,v,CASE WHEN $2::boolean THEN v END,coalesce(zone,CASE WHEN $2 THEN 3 ELSE 99 END),coalesce(queued_at,${now}),coalesce(zone_position,v) FROM inputs
  ON CONFLICT(weapon,ruler,identity) DO UPDATE SET version=nextval('board_rank_generation'),
   recent_version=CASE WHEN $2 THEN greatest(queued.version+1,excluded.version) ELSE queued.recent_version END,
   ${mergeQueue},error=NULL,
   available_at=CASE WHEN queued.lease_until>${now} THEN queued.lease_until ELSE least(queued.available_at,${now}) END
  RETURNING weapon,ruler,identity,version`,[JSON.stringify(targets),recent]);
}

export async function putRankDirty(tx,target,recent=false) {
 const [row]=await putRankDirtyBatch(tx,[target],recent);return row.version;
}

export async function putRankPublication(tx,{weapon,ruler,winnersChanged,catalog_phase,zone=null,queued_at=null,zone_position=null},recent=false) {
 const unchanged=winnersChanged===false&&catalog_phase!=='frozen'&&catalog_phase!=='replay';
 const write=await tx.unsafe(`INSERT INTO state_board_rank_publication_ready AS queued(weapon,ruler,version,recent_version,zone,queued_at,zone_position)
  SELECT $1,$2,seq.v,CASE WHEN $5::boolean THEN seq.v END,least($3::smallint,s.zone),coalesce(least($4::text,s.queued_at),${now}),
   CASE WHEN s.zone<$3 THEN s.zone_position WHEN s.zone=$3 THEN least(s.zone_position,$6::bigint)
        ELSE coalesce($6::bigint,seq.v) END
  FROM (SELECT nextval('board_rank_generation') v) seq
  LEFT JOIN state_board_rank_new_scopes s ON(s.weapon,s.ruler)=($1,$2)
  ON CONFLICT(weapon,ruler) DO UPDATE SET version=nextval('board_rank_generation'),
   recent_version=CASE WHEN $5 THEN greatest(queued.version+1,excluded.version) ELSE queued.recent_version END,
   ${mergeQueue},error=NULL,
   available_at=CASE WHEN queued.lease_until>${now} THEN queued.lease_until ELSE least(queued.available_at,${now}) END
   ${unchanged?'WHERE queued.delivered_output IS NULL OR queued.error IS NOT NULL':''}`,[weapon,ruler,zone??(recent?3:99),queued_at,recent,zone_position]);
 return write.count>0;
}

export async function routeRankInput(tx,input,source,catalogRequests=false) {
 await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
 let identity=input.scope==='build'?input.key:input.scope==='measurement'?JSON.parse(input.key)[0]:input.body?.identity;
 const ruler=input.scope==='measurement'?JSON.parse(input.key)[1]:input.body?.ruler;
 await lockRankInputs(tx,source,{identities:identity?[identity]:[],reviews:input.scope==='review'?[input.key]:[]});
 const registered=await registeredRankInputs(tx,input),targets=new Map(),covered=new Set();
 for(const {weapon,ruler,identity,version} of registered){
  const key=JSON.stringify([weapon,ruler,identity]);
  if(version!=null&&BigInt(version)>=BigInt(input.version))covered.add(key);
  else targets.set(key,{weapon,ruler,identity});
 }
 if(identity){
  const [build]=await tx.unsafe('SELECT weapon FROM entity_weapon_builds WHERE id=$1',[identity]);
  const weapon=build?.weapon||(input.scope==='build'?input.body?.weapon:null);
  if(weapon){
   if(input.scope==='build'||input.scope==='measurement'){
    const args=[weapon,ruler||null];
    const retained=catalogRequests&&BigInt(input.version)>0n?`AND (p.enabled OR NOT (
     EXISTS(SELECT 1 FROM state_board_input_mailbox m WHERE m.scope='partition' AND m.key='["'||p.weapon||'","'||p.ruler||'"]' AND NOT m.removed)
     AND EXISTS(SELECT 1 FROM state_board_rank_catalog_inputs k WHERE (k.weapon,k.ruler,k.identity)=($1,$3,$4))
     AND EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='state_board_input_mailbox'::regclass
      AND tgname='board_rank_catalog_input' AND tgfoid=to_regprocedure('board_rank_catalog_input()') AND tgenabled IN('O','A'))))`:'';
    if(retained)args.push(input.scope==='measurement'?ruler:'',identity);
    const [capturing]=await tx.unsafe(`SELECT 1 FROM state_board_read_partitions p WHERE p.weapon=$1
     AND ($2::text IS NULL OR p.ruler=$2) AND(p.enabled OR(NOT p.canonical_sources AND p.revision IS NULL))
     AND NOT EXISTS(SELECT 1 FROM state_board_rank_partitions r WHERE r.weapon=p.weapon AND r.ruler=p.ruler) ${retained} LIMIT 1`,args);
    if(capturing)throw new BoardRankConflict('ranking partition capture is not registered');
   }
   const parts=await tx.unsafe('SELECT weapon,ruler FROM state_board_rank_partitions WHERE weapon=$1 AND ($2::text IS NULL OR ruler=$2)',[weapon,ruler||null]);
   for(const part of parts){
    const key=JSON.stringify([part.weapon,part.ruler,identity]);
    if(!covered.has(key))targets.set(key,{...part,identity});
   }
   if(input.deleted)await tx.unsafe('DELETE FROM state_board_read_routes WHERE scope=$1 AND key=$2',[input.scope,input.key]);
   else await tx.unsafe(`INSERT INTO state_board_read_routes VALUES($1,$2,$3,$4) ON CONFLICT(scope,key)
    DO UPDATE SET weapon=excluded.weapon,ruler=excluded.ruler`,[input.scope,input.key,weapon,ruler||'']);
  }
 }
 if(targets.size||input.scope==='review'||(identity&&input.deleted))
  await tx.unsafe(`INSERT INTO state_board_rank_route_inputs VALUES($1,$2,$3) ON CONFLICT(scope,key)
   DO UPDATE SET version=greatest(state_board_rank_route_inputs.version,excluded.version)`,[input.scope,input.key,input.version]);
 const acceptedZone=input.body?.publication_zone;
 const zone=input.scope==='measurement'&&BigInt(input.version)>0n?(Number.isInteger(acceptedZone)&&acceptedZone>=0&&acceptedZone<=99?acceptedZone:3)
  :input.scope==='review'&&(input.body?.state==='succeeded'||input.body?.invalidated_at)?1:99;
 await putRankDirtyBatch(tx,[...targets.values()].map(target=>({...target,zone})));
 return {identities:targets.size};
}

export async function reconcileRankBaseline(tx,{weapon,ruler,source}) {
 const [part]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 if(!part.seeded||part.reconciled)return;
 const catalog=part.catalog_phase==='replay';
 const seeded='SELECT identity FROM state_board_rank_sources WHERE weapon=$1 AND ruler=$2 AND identity>$3 ORDER BY identity LIMIT 64';
 const changes="SELECT identity FROM state_board_rank_catalog_inputs WHERE weapon=$1 AND identity>$3 AND ruler=";
 const scan=catalog?`SELECT identity FROM ((${seeded}) UNION (${changes}$2 ORDER BY identity LIMIT 64)
  UNION (${changes}'' ORDER BY identity LIMIT 64)) ids ORDER BY identity LIMIT 64`:seeded;
 const ids=await tx.unsafe(scan,[weapon,ruler,part.reconcile_cursor]);
 const rows=await tx.unsafe(`SELECT k.identity FROM jsonb_array_elements_text($3::text::jsonb) k(identity)
  WHERE EXISTS(SELECT 1 FROM LATERAL (SELECT scope,key,version FROM state_board_rank_dependencies
    WHERE weapon=$1 AND ruler=$2 AND identity=k.identity OFFSET 0) d
   CROSS JOIN LATERAL (SELECT version FROM state_board_input_mailbox WHERE scope=d.scope AND key=d.key OFFSET 0) m
   WHERE d.version IS DISTINCT FROM m.version)
  ${catalog?`OR(NOT EXISTS(SELECT 1 FROM state_board_rank_sources s WHERE (s.weapon,s.ruler,s.identity)=($1,$2,k.identity))
   AND EXISTS(SELECT 1 FROM entity_weapon_builds b WHERE b.id=k.identity AND b.weapon=$1)
   AND EXISTS(SELECT 1 FROM board_facts f WHERE f.identity=k.identity AND f.ruler=$2))`:''}`,[weapon,ruler,JSON.stringify(ids.map(row=>row.identity))]);
 await lockRankInputs(tx,source,{identities:rows.map(row=>row.identity)});
 if(rows.length){
  const marked=await putRankDirtyBatch(tx,rows.map(({identity})=>({weapon,ruler,identity})));
  await tx.unsafe(`INSERT INTO state_board_rank_baseline_obligations(weapon,ruler,identity,version)
   SELECT $1,$2,x.identity,x.version FROM jsonb_to_recordset($3::text::jsonb)x(identity text,version bigint)
   ON CONFLICT(weapon,ruler,identity) DO UPDATE SET version=excluded.version`,[weapon,ruler,JSON.stringify(marked)]);
 }
 const [locked]=await tx.unsafe('SELECT reconcile_cursor FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 FOR UPDATE',[weapon,ruler]);
 if(locked.reconcile_cursor!==part.reconcile_cursor)throw new BoardRankConflict('baseline reconciliation advanced');
 await tx.unsafe('UPDATE state_board_rank_partitions SET reconcile_cursor=$3,reconciled=$4,available_at=board_input_timestamp(clock_timestamp()) WHERE weapon=$1 AND ruler=$2',
  [weapon,ruler,ids.at(-1)?.identity||part.reconcile_cursor,ids.length<64]);
}

export async function finishRankBaseline(tx,{weapon,ruler}) {
 const [row]=await tx.unsafe(`UPDATE state_board_rank_partitions p SET complete=true WHERE weapon=$1 AND ruler=$2 AND seeded AND reconciled AND NOT complete
  AND NOT EXISTS(SELECT 1 FROM state_board_rank_baseline_obligations o JOIN state_board_rank_dirty d USING(weapon,ruler,identity)
   WHERE o.weapon=p.weapon AND o.ruler=p.ruler AND d.applied_version<o.version) RETURNING weapon,ruler`,[weapon,ruler]);
 if(row)await putRankPublication(tx,row);
 return !!row;
}

export async function claimRankDirty(sql,{source,native,recent=false,catalog=false}) {
 const token=randomUUID();
 const [row]=await sql.unsafe(`WITH chosen AS MATERIALIZED(SELECT d.weapon,d.ruler,d.identity FROM state_board_rank_dirty d
  JOIN state_board_rank_partitions p USING(weapon,ruler) WHERE p.source=$1 AND p.native=$2
   ${catalog?"AND p.catalog_phase IS DISTINCT FROM 'frozen'":''}
   ${recent?'AND d.recent_version>d.applied_version':''} AND d.version>d.applied_version AND d.available_at<=${now} ORDER BY d.zone,coalesce(d.zone_position,0),coalesce(d.queued_at,d.available_at),d.weapon,d.ruler,d.identity
  LIMIT 1 FOR UPDATE OF d SKIP LOCKED)
  UPDATE state_board_rank_dirty d SET lease=$3,lease_until=${until},available_at=${until} FROM chosen c
  WHERE (d.weapon,d.ruler,d.identity)=(c.weapon,c.ruler,c.identity) RETURNING d.*`,[source,native,token]);
 return row||null;
}

export async function prepareRankIdentity(sql,native,part,identity) {
 for(let attempt=0;attempt<3;attempt++){
  const [before]=await sql.unsafe('SELECT record FROM entity_weapon_builds WHERE id=$1',[identity]);
  const record=before?JSON.parse(before.record):null;
  const cached=await cachedRankInputs(sql,part,[{identity,record}]);
  const described=await prepareRankSources(native,{weapon:part.weapon,ruler:part.ruler,inputs:[{identity,record,facts:[]}],cached});
  const descriptor=described.sources[0];
  const captured=await sql.begin('isolation level repeatable read read only',async tx=>{
   const [build]=await tx.unsafe('SELECT record FROM entity_weapon_builds WHERE id=$1',[identity]);
   if(hash(build?JSON.parse(build.record):null)!==descriptor.record_sha256)return null;
   const modes=descriptor.descriptor.publication_modes||[];
   const facts=build?await tx.unsafe(`SELECT * FROM board_facts WHERE identity=$1 AND ruler=$2
    AND mode IN(SELECT jsonb_array_elements_text(($3::text)::jsonb)) ORDER BY mode`,[identity,part.ruler,JSON.stringify(modes.map(row=>row.id))]):[];
   const dependencies=[{scope:'build',key:identity},...modes.map(mode=>({scope:'measurement',key:rankMeasurementKey(identity,part.ruler,mode.id)})),
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
  const publication=updated.complete
   ?await putRankPublication(tx,{...updated,zone:held.zone,queued_at:held.queued_at,zone_position:held.zone_position},held.recent_version!=null&&BigInt(held.recent_version)>BigInt(held.applied_version))
   :await finishRankBaseline(tx,updated);
  return {status:'indexed',weapon:part.weapon,ruler:part.ruler,identity:claim.identity,winners_changed:updated.winnersChanged,publication_requested:publication};
 });
}

export async function retryRankDirty(sql,claim,error) {
 const delay=error instanceof BoardRankQualificationError?null:error instanceof BoardRankConflict?0:5;
 await sql.unsafe(`UPDATE state_board_rank_dirty SET lease=NULL,lease_until=NULL,error=$5,
  available_at=CASE WHEN version>$4 THEN ${now} WHEN $6::bigint IS NULL THEN NULL
   ELSE board_input_timestamp(clock_timestamp()+make_interval(secs=>$6::bigint)) END
  WHERE weapon=$1 AND ruler=$2 AND identity=$3 AND lease=$7`,[claim.weapon,claim.ruler,claim.identity,claim.version,error.message,delay,claim.lease]);
}
