// SPDX-License-Identifier: AGPL-3.0-or-later
// Partition input versions remain exact for null headers, absent modes and removed reviews.
import {rankMeasurementKey} from './board_rank_keys.mjs';
import {createHash} from 'node:crypto';

export async function cachedRankInputs(sql,part,inputs){
 const keys=inputs.map(({identity,record})=>({identity,sha:createHash('sha256').update(JSON.stringify(record)).digest('hex')}));
 return sql.unsafe(`SELECT k.identity,coalesce(s.live,false) live,k.sha record_sha256,
  coalesce(d.descriptor,CASE WHEN s.record_sha256=k.sha THEN nullif(s.descriptor,'null'::jsonb) END) descriptor
  FROM jsonb_to_recordset($1::text::jsonb) k(identity text,sha text)
  LEFT JOIN state_board_rank_sources s ON (s.weapon,s.ruler,s.identity)=($2,$3,k.identity)
  LEFT JOIN state_board_rank_descriptions d ON (d.native,d.identity,d.record_sha256)=($4,k.identity,k.sha)`,
  [JSON.stringify(keys),part.weapon,part.ruler,part.native]);
}

function versions(source,ruler){
 let build=null;
 const modes=Object.create(null),reviews=Object.create(null);
 for(const d of source.dependencies){
  if(d.scope==='build'&&d.key===source.identity&&build===null){build=d.version??'-1';continue;}
  if(d.scope==='review'&&!Object.hasOwn(reviews,d.key)){reviews[d.key]=d.version??null;continue;}
  if(d.scope==='measurement'){
   const [id,board,mode]=JSON.parse(d.key);
   if(id===source.identity&&board===ruler&&typeof mode==='string'&&!Object.hasOwn(modes,mode)&&d.key===rankMeasurementKey(id,board,mode)){
    modes[mode]=d.version??null;continue;
   }
  }
  throw Error('duplicate or misplaced rank dependency');
 }
 return {build,modes,reviews};
}

export async function storeRankInputs(tx,part,sources,live){
 if(!sources.length)return;
 const items=sources.map(s=>({identity:s.identity,record_sha256:s.record_sha256,descriptor:s.descriptor,
  input_sha256:s.input_sha256,...versions(s,part.ruler)}));
 const body=JSON.stringify(items),targets=[part.weapon,part.ruler,body];
 const prior=await tx.unsafe(`SELECT s.description FROM jsonb_array_elements($3::text::jsonb) x
  JOIN state_board_rank_input_sets s ON (s.identity,s.ruler,s.weapon)=(x->>'identity',$2,$1)`,targets);
 await tx.unsafe(`INSERT INTO state_board_rank_descriptions(native,identity,record_sha256,descriptor)
  SELECT $1,x.identity,x.record_sha256,x.descriptor FROM jsonb_to_recordset($2::text::jsonb)
    x(identity text,record_sha256 text,descriptor jsonb) WHERE NOT EXISTS(SELECT 1 FROM state_board_rank_descriptions e
     WHERE (e.native,e.identity,e.record_sha256)=($1,x.identity,x.record_sha256)) ORDER BY x.identity
  ON CONFLICT(native,identity,record_sha256) DO NOTHING`,[part.native,body]);
 await tx.unsafe(`INSERT INTO state_board_rank_sources(weapon,ruler,identity,record_sha256,descriptor,input_sha256,live)
  SELECT $1,$2,x.identity,x.record_sha256,'null'::jsonb,x.input_sha256,$4 FROM jsonb_to_recordset($3::text::jsonb)
    x(identity text,record_sha256 text,input_sha256 text)
  ON CONFLICT(weapon,ruler,identity) DO UPDATE SET record_sha256=excluded.record_sha256,descriptor=excluded.descriptor,
    input_sha256=excluded.input_sha256,live=excluded.live`,[...targets,live]);
 const written=await tx.unsafe(`INSERT INTO state_board_rank_input_sets(identity,ruler,weapon,description,build_version,modes,reviews)
  SELECT x.identity,$2,$1,d.id,x.build,x.modes,x.reviews FROM jsonb_to_recordset($3::text::jsonb)
   x(identity text,record_sha256 text,descriptor jsonb,build bigint,modes jsonb,reviews jsonb)
  JOIN state_board_rank_descriptions d ON (d.native,d.identity,d.record_sha256)=($4,x.identity,x.record_sha256)
   AND d.descriptor=x.descriptor
  ON CONFLICT(identity,ruler,weapon) DO UPDATE SET description=excluded.description,build_version=excluded.build_version,
    modes=excluded.modes,reviews=excluded.reviews RETURNING identity`,[...targets,part.native]);
 if(written.length!==sources.length)throw Error('immutable Native description disagrees with prepared source');
 await tx.unsafe(`DELETE FROM state_board_rank_legacy_dependencies WHERE weapon=$1 AND ruler=$2
   AND identity IN(SELECT x->>'identity' FROM jsonb_array_elements($3::text::jsonb) x)`,targets);
 if(prior.length)await tx.unsafe(`DELETE FROM state_board_rank_descriptions d
  WHERE id IN(SELECT value::bigint FROM jsonb_array_elements_text($1::text::jsonb))
  AND NOT EXISTS(SELECT 1 FROM state_board_rank_input_sets s WHERE s.description=d.id)`,[JSON.stringify(prior.map(r=>r.description))]);
}

export async function registeredRankInputs(tx,{scope,key}){
 const legacy=`SELECT weapon,ruler,identity,version FROM state_board_rank_legacy_dependencies WHERE scope=$1 AND key=$2`;
 if(scope==='build')return tx.unsafe(legacy+` UNION ALL SELECT weapon,ruler,identity,nullif(build_version,-1) FROM state_board_rank_input_sets
   WHERE identity=$2 AND build_version IS NOT NULL`,[scope,key]);
 if(scope==='review')return tx.unsafe(legacy+` UNION ALL SELECT weapon,ruler,identity,(reviews->>$2)::bigint FROM state_board_rank_input_sets
   WHERE reviews<>'{}'::jsonb AND reviews ? $2`,[scope,key]);
 if(scope==='measurement'){
  const [identity,ruler,mode]=JSON.parse(key);
  if(key!==rankMeasurementKey(identity,ruler,mode))return tx.unsafe(legacy,[scope,key]);
  return tx.unsafe(legacy+` UNION ALL SELECT weapon,ruler,identity,(modes->>$5)::bigint FROM state_board_rank_input_sets
   WHERE identity=$3 AND ruler=$4 AND modes ? $5`,[scope,key,identity,ruler,mode]);
 }
 return [];
}
