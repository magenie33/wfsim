// SPDX-License-Identifier: AGPL-3.0-or-later
// Captured routing, baseline work, dirty normalization and complete publication own separate handoffs.
import {join} from 'node:path';
import {stat,mkdir} from 'node:fs/promises';
import {openInventoryOperator} from './inventory_operator.mjs';
import {BoardInputMailbox} from './board_input_mailbox.mjs';
import {boardStorage} from './board_storage.mjs';
import {guardRankSpace} from './board_rank_storage.mjs';
import {enqueueRequestedPartition} from './board_requested_partitions.mjs';
import {BoardRankConflict,BoardRankQualificationError} from './board_rank_index.mjs';
import {captureRankBaseline,beginRankBaseline,prepareRankBaselinePage,applyRankBaselinePage} from './board_rank_baseline.mjs';
import {putRankDirty,routeRankInput,reconcileRankBaseline,finishRankBaseline,claimRankDirty,prepareRankIdentity,applyRankDirty,retryRankDirty} from './board_rank_work.mjs';
import {claimRankPublication,startRankGeneration,floorRankGeneration,uploadRankGeneration,publishRankGeneration,reuseRankGeneration,retryRankPublication,yieldRankPublication} from './board_rank_publication.mjs';
import {collectRankStorage,collectRankOrphan} from './board_rank_gc.mjs';

export async function openBoardRankFlow({sql,source,engine,binary,work,nativeSha,requestedPartitions=false,baselineConcurrency=1,fault=async()=>{}}) {
 if(!Number.isInteger(baselineConcurrency)||baselineConcurrency<1||baselineConcurrency>3)throw Error('invalid ranking baseline concurrency');
 work=boardStorage(work);await mkdir(work,{recursive:true});
 const [storage]=await sql.unsafe(`SELECT to_regclass('state_board_rank_descriptions') IS NOT NULL
  AND to_regclass('state_board_rank_input_sets') IS NOT NULL AND to_regclass('state_board_rank_legacy_dependencies') IS NOT NULL
  AND EXISTS(SELECT 1 FROM pg_class WHERE oid=to_regclass('state_board_rank_dependencies') AND relkind='v') AS ready`);
 if(!storage.ready)throw new BoardRankQualificationError('install the compact ranking storage upgrade before starting the reader');
 const [boundary]=await sql.unsafe('SELECT weapon,ruler FROM state_board_rank_partitions WHERE source<>$1 OR native<>$2 LIMIT 1',[source,nativeSha]);
 if(boundary)throw new BoardRankQualificationError('ranking projection recovery boundary changed');
 const [{catalog}]=await sql.unsafe("SELECT to_regprocedure('board_rank_catalog_activate(text,text,text,text)') IS NOT NULL AS catalog");
 const identity=await stat(binary),native=await openInventoryOperator(binary);
 if(native.adapter!==nativeSha){await native.close();throw new BoardRankQualificationError('board native contract changed');}
 const mailbox=new BoardInputMailbox(sql,{source}),abort=new AbortController();
 const context={source,engine,native:native.adapter,work,catalog,verified:new Map(),prefixes:new Map(),fault};
 const captures=new Map();let publication=null,turn=0,closed=false,orphanAfter=0;
 const sameNative=async()=>{
  const current=await stat(binary);
  if(current.ino!==identity.ino||current.size!==identity.size||current.mtimeMs!==identity.mtimeMs)
   throw new BoardRankQualificationError('board native contract changed');
 };
 const partitionKey=(weapon,ruler)=>JSON.stringify([weapon,ruler]);
 const deterministic=error=>error instanceof BoardRankQualificationError||['ENOENT','ENOTDIR','ELOOP','53100','53200','53400','54000'].includes(error.code);
 const ensurePartition=async()=>sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  const [part]=await tx.unsafe(`SELECT p.* FROM state_board_read_partitions p WHERE (p.enabled OR($1::boolean AND NOT p.canonical_sources AND p.revision IS NULL))
   AND NOT EXISTS(SELECT 1 FROM state_board_rank_partitions r WHERE r.weapon=p.weapon AND r.ruler=p.ruler)
   AND NOT EXISTS(SELECT 1 FROM state_board_rank_route_inputs i WHERE i.scope='partition' AND i.key='["'||p.weapon||'","'||p.ruler||'"]')
   AND (p.enabled OR NOT EXISTS(SELECT FROM state_board_input_mailbox m WHERE m.scope='partition'
    AND m.key='["'||p.weapon||'","'||p.ruler||'"]' AND NOT m.removed))
   ORDER BY p.weapon,p.ruler LIMIT 1`,[requestedPartitions]);
  if(!part)return;
  const key=JSON.stringify([part.weapon,part.ruler]);
  const [{version}]=await tx.unsafe('SELECT board_input_put($1,$2,$3) version', ['partition',key,JSON.stringify({weapon:part.weapon,ruler:part.ruler,indexed:true,...(!part.enabled?{qualify_requested:true}:{})})]);
  await tx.unsafe(`INSERT INTO state_board_rank_route_inputs VALUES('partition',$1,$2) ON CONFLICT(scope,key) DO NOTHING`,[key,version]);
 });
 const retryInput=async(job,error)=>{
  await mailbox.retry(job.item,job.token,{error:error.message,delaySeconds:deterministic(error)?null:5});
 };
 const finishCapture=async(key,job)=>{
  if(!job.result)return null;
  captures.delete(key);
  if(job.result.error){await retryInput(job,job.result.error);return {status:'baseline_error',error:job.result.error.message};}
  try{return {...await mailbox.apply(job.input,job.token,tx=>beginRankBaseline(tx,job.result.manifest)),stage:'baseline-capture'};}
  catch(error){await retryInput(job,error);return {status:'baseline_error',error:error.message};}
 };
 const route=async()=>{
  for(const [key,job] of captures)if(job.result)return finishCapture(key,job);
  if(requestedPartitions)await enqueueRequestedPartition(sql,source,native.contract.rulers);
  await ensurePartition();
  const claim=await mailbox.claim({items:1,leaseSeconds:300});
  if(!claim.inputs.length)return null;
  const item=claim.inputs[0],job={item,token:claim.token};
  try{
   const input=await mailbox.read(item,claim.token);job.input=input;
   if(input.status==='superseded'){
    await sql.unsafe('SELECT board_input_ack($1,$2,$3,$4::bigint)',[item.scope,item.key,claim.token,item.version]);return {status:'superseded'};
   }
   if(input.status!=='input')throw Error('board input '+input.status);
   if(input.scope!=='partition')return {...await mailbox.apply(input,claim.token,(tx,packet)=>routeRankInput(tx,packet,source)),stage:'routing'};
   const {weapon,ruler}=input.body,key=partitionKey(weapon,ruler);
   const [existing]=await sql.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
   if(existing){
    if(existing.source!==source||existing.native!==native.adapter)throw new BoardRankQualificationError('ranking projection recovery boundary changed');
    return {...await mailbox.apply(input,claim.token,async()=>({baseline:'already-recorded'})),stage:'baseline-capture'};
   }
   const [selected]=await sql.unsafe('SELECT * FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
   if(!selected)throw new BoardRankQualificationError('ranking partition is not selected for preparation');
   if(!selected.enabled){
    if(!requestedPartitions||!input.body.qualify_requested)throw new BoardRankQualificationError('ranking partition requires explicit qualification');
    await sql.unsafe('SELECT board_requested_partition_selected($1,$2,false)',[weapon,ruler]);
   }
   const [warming]=await sql.unsafe('SELECT weapon FROM state_board_rank_partitions WHERE NOT complete AND available_at IS NOT NULL LIMIT 1');
   if(captures.size||warming){await mailbox.retry(item,claim.token,{error:'another bounded baseline is active',delaySeconds:1});return {status:'deferred'};}
   job.promise=captureRankBaseline(sql,{weapon,ruler,source,native:native.adapter,work,signal:abort.signal})
    .then(manifest=>{job.result={manifest};},error=>{job.result={error};});
   captures.set(key,job);return {status:'capturing',weapon,ruler};
  }catch(error){await retryInput(job,error);if(!native.healthy)throw error;return {status:'routing_error',error:error.message};}
 };
 const baselinePart=async(part,operator)=>{
  try{
   if(!part.seeded){
    const page=await prepareRankBaselinePage(sql,operator,part,context);await fault('after-baseline-native-before-commit');
    await applyRankBaselinePage(sql,part,page);if(page.done)context.verified.delete(part.path);
    return {status:'baseline-page',weapon:part.weapon,ruler:part.ruler,loaded:page.loaded};
   }
   await sql.begin(async tx=>{
    if(part.catalog_phase==='frozen'){
     await tx.unsafe("UPDATE state_board_rank_partitions SET reconciled=true WHERE weapon=$1 AND ruler=$2 AND catalog_phase='frozen'",[part.weapon,part.ruler]);
    }else await reconcileRankBaseline(tx,part);
    await finishRankBaseline(tx,part);
   });
   return {status:'baseline-reconciliation',weapon:part.weapon,ruler:part.ruler};
  }catch(error){
   if(error instanceof BoardRankConflict)return {status:'baseline-conflict',weapon:part.weapon,ruler:part.ruler};
   const blocked=deterministic(error);
   await sql.unsafe(`UPDATE state_board_rank_partitions SET error=$3,available_at=CASE WHEN $4 THEN NULL
    ELSE board_input_timestamp(clock_timestamp()+interval '5seconds') END WHERE weapon=$1 AND ruler=$2`,[part.weapon,part.ruler,error.message,blocked]);
   if(!native.healthy)throw error;return {status:'baseline_error',blocked,error:error.message};
  }
 };
 const baseline=async()=>{
  const parts=await sql.unsafe(`SELECT * FROM state_board_rank_partitions WHERE NOT complete AND(NOT seeded OR NOT reconciled)
   AND source=$1 AND native=$2 AND available_at<=board_input_timestamp(clock_timestamp())
   ORDER BY available_at,revision,weapon,ruler LIMIT $3`,[source,native.adapter,baselineConcurrency]);
  if(!parts.length)return null;
  if(parts.length===1)return baselinePart(parts[0],native);
  // Each partition owns its database checkpoint. The Native process accepts one call at a time.
  let tail=Promise.resolve();
  const operator={adapter:native.adapter,contract:native.contract,call:(op,items)=>{
   const reply=tail.then(()=>native.call(op,items));tail=reply.catch(()=>{});return reply;
  }};
  const settled=await Promise.allSettled(parts.map(part=>baselinePart(part,operator)));
  const failed=settled.find(r=>r.status==='rejected');if(failed)throw failed.reason;
  return {status:'baseline-batch',items:settled.map(r=>r.value)};
 };
 const dirty=async(recent=false)=>{
  const claim=await claimRankDirty(sql,{source,native:native.adapter,recent,catalog});if(!claim)return null;
  try{
   const [part]=await sql.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[claim.weapon,claim.ruler]);
   const prepared=await prepareRankIdentity(sql,native,part,claim.identity);await fault('after-dirty-native-before-commit');
   return {...await applyRankDirty(sql,claim,part,prepared),stage:'indexing'};
  }catch(error){await retryRankDirty(sql,claim,error);if(!native.healthy)throw error;return {status:'index_error',error:error.message};}
 };
 const [{recent:recentInstalled,discovery}]=await sql.unsafe(`SELECT to_regclass('state_board_rank_recent_inputs') IS NOT NULL AS recent,
  to_regprocedure('board_rank_discover_scope(uuid,text,text)') IS NOT NULL AS discovery`);
 let publishTurn=0,pages=0;
 const finishPublication=(generation,result)=>{
  context.verified.delete(join(generation.directory,'winners.ndjson'));
  context.verified.delete(join(generation.directory,'rows.ndjson'));
  context.prefixes.delete(join(generation.directory,'rows.ndjson'));
  publication=null;return result;
 };
 const publish=async()=>{
  if(publication&&pages>=4){
   await yieldRankPublication(sql,publication);publication=null;
  }
  if(!publication){
   const boundary={source,native:native.adapter,catalog};
   const lane=publishTurn++%3;
   if(lane===0&&recentInstalled)publication=await claimRankPublication(sql,{...boundary,recent:true});
   if(lane===1&&catalog)publication=await claimRankPublication(sql,{...boundary,first:true});
   publication ||= await claimRankPublication(sql,boundary);
   if(!publication)return null;
   pages=0;
  }
  pages++;
  const scope={weapon:publication.weapon,ruler:publication.ruler};
  try{
   const [generation]=await sql.unsafe(`SELECT * FROM state_board_rank_generations WHERE weapon=$1 AND ruler=$2 AND phase IN('floor','upload') ORDER BY created_at,token LIMIT 1`,[publication.weapon,publication.ruler]);
   if(!generation){await startRankGeneration(sql,native,publication,context);return {status:'snapshot-started',...scope};}
   if(generation.phase==='floor'){await floorRankGeneration(sql,native,publication,generation,context);return {status:'snapshot-floor',...scope};}
   if(Number(generation.upload_cursor)===0){const reused=await reuseRankGeneration(sql,publication,generation,context);if(reused)return finishPublication(generation,{...reused,...scope});}
   if(Number(generation.uploaded_rows)!==Number(generation.output_rows)||Number(generation.upload_cursor)!==Number(generation.output_size)){
    await uploadRankGeneration(sql,publication,generation,context);return {status:'snapshot-upload',...scope};
   }
   const result=await publishRankGeneration(sql,native,publication,generation,context);return finishPublication(generation,{...result,...scope});
  }catch(error){
   const failed=publication;publication=null;
   if(deterministic(error)){
    await sql.unsafe(`UPDATE state_board_rank_generations g SET phase='abandoned' WHERE weapon=$1 AND ruler=$2 AND phase IN('floor','upload')
     AND EXISTS(SELECT 1 FROM state_board_rank_publication_ready r WHERE r.weapon=g.weapon AND r.ruler=g.ruler
      AND r.lease=$3 AND r.lease_version=$4 AND r.lease_until>board_input_timestamp(clock_timestamp()))`,[failed.weapon,failed.ruler,failed.lease,failed.lease_version]);
    error=new BoardRankQualificationError(error.message);
   }
   await retryRankPublication(sql,failed,error);if(!native.healthy)throw error;return {status:'publication_error',error:error.message};
  }
 };
 const collect=async()=>{
  const result=await collectRankStorage(sql,context);if(result)return result;
  if(Date.now()<orphanAfter)return null;orphanAfter=Date.now()+60000;return collectRankOrphan(sql,context);
 };
 const stages=[route,baseline,dirty,publish,collect];
 if(recentInstalled){
  stages.push(()=>sql.begin(async tx=>{
   await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
   const [target]=await tx.unsafe(`DELETE FROM state_board_rank_recent_inputs i USING
    (SELECT weapon,ruler,identity FROM state_board_rank_recent_inputs ORDER BY queued_at,weapon,ruler,identity
     LIMIT 1 FOR UPDATE SKIP LOCKED) first
    WHERE (i.weapon,i.ruler,i.identity)=(first.weapon,first.ruler,first.identity) RETURNING i.weapon,i.ruler,i.identity`);
   if(!target)return null;
   const [part]=await tx.unsafe('SELECT 1 FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 AND source=$3 AND native=$4 AND complete',
    [target.weapon,target.ruler,source,native.adapter]);
   if(part)await putRankDirty(tx,target,true);
   return {status:'recent-measurement',...target};
  }),()=>dirty(true));
 }
 if(requestedPartitions){
  if(discovery){
   const rulers=JSON.stringify(native.contract.rulers.map(row=>row.id));
   stages.push(async()=>{
    const [{scope}]=await sql.unsafe('SELECT board_rank_discover_scope($1::uuid,$2,$3) AS scope',[source,native.adapter,rulers]);
    return scope?{status:'discovered-scope',...scope}:null;
   });
  }
  const [installed]=await sql.unsafe("SELECT to_regprocedure('board_requested_result_take(uuid,text)') IS NOT NULL AS ready");
  if(installed.ready)stages.push(()=>sql.begin(async tx=>{
   const [{target}]=await tx.unsafe('SELECT board_requested_result_take($1::uuid,$2::text) AS target',[source,native.adapter]);
   if(!target)return null;
   await putRankDirty(tx,target,recentInstalled);
   return {status:'requested-result',...target};
  }));
 }
 return {contract:native.contract,step:async()=>{
  if(closed)throw Error('ranking reader is closed');guardRankSpace(work);await sameNative();
  for(let i=0;i<stages.length;i++){const result=await stages[turn++%stages.length]();if(result)return result;}
  return {status:'idle'};
 },close:async()=>{closed=true;abort.abort();
  try{await Promise.all([...captures.values()].map(job=>job.promise));if(publication)await yieldRankPublication(sql,publication);}
  finally{await native.close();}
 }};
}
