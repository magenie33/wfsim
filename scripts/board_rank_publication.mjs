// SPDX-License-Identifier: AGPL-3.0-or-later
// Bounded generation pages remain private until their complete snapshot reference commits.
import {createHash,randomUUID} from 'node:crypto';
import {createReadStream,lstatSync,realpathSync,statSync} from 'node:fs';
import {mkdir,open} from 'node:fs/promises';
import {join,resolve,dirname,basename} from 'node:path';
import {boardStorage} from './board_storage.mjs';
import {rankFilePage,syncRankDirectory} from './board_rank_files.mjs';
import {captureRankWinners,BoardRankConflict,BoardRankQualificationError} from './board_rank_index.mjs';
const now="board_input_timestamp(clock_timestamp())";
const until="board_input_timestamp(clock_timestamp()+interval '3minutes')";
const hash=value=>createHash('sha256').update(value).digest('hex');
const empty=hash('');

async function generationEpoch(sql,generation,{source,native,engine}) {
 const [part]=await sql.unsafe(`SELECT p.*,(SELECT source FROM state_board_input_source WHERE id=1) actual
  FROM state_board_rank_partitions p WHERE weapon=$1 AND ruler=$2`,[generation.weapon,generation.ruler]);
 if(!part?.complete||part.actual!==source||part.source!==source||generation.source!==source||part.native!==native
  ||generation.native!==native||part.cut!==generation.cut||generation.engine!==engine)
  throw new BoardRankQualificationError('ranking generation recovery boundary changed');
}

async function fileHash(path,bytes=null) {
 const value=createHash('sha256');
 if(bytes!==0)for await(const block of createReadStream(path,bytes==null?{}:{start:0,end:bytes-1}))value.update(block);
 return value;
}
function directoryOf(generation,work) {
 const selected=resolve(generation.directory),root=resolve(boardStorage(work));
 if(dirname(selected)!==root||basename(selected)!=='rank-generation-'+generation.token
  ||lstatSync(selected).isSymbolicLink()||realpathSync(selected)!==selected||statSync(selected).st_dev!==statSync(root).st_dev)
  throw new BoardRankQualificationError('ranking generation work root changed');
 return selected;
}
async function verifyFile(path,expected,size=null,verified=new Map()) {
 const state=statSync(path),fingerprint=[state.dev,state.ino,state.size,state.mtimeMs,expected].join(':');
 if(lstatSync(path).isSymbolicLink()||realpathSync(path)!==path||(size!=null&&state.size!==Number(size)))
  throw new BoardRankQualificationError('ranking generation file changed');
 if(verified.get(path)!==fingerprint){
  if((await fileHash(path)).digest('hex')!==expected)throw new BoardRankQualificationError('ranking generation file changed');
  verified.set(path,fingerprint);
 }
}
async function heldPublication(tx,claim) {
 const [held]=await tx.unsafe(`SELECT * FROM state_board_rank_publication_ready WHERE weapon=$1 AND ruler=$2
  AND lease=$3 AND lease_version=$4 AND lease_until>${now} FOR UPDATE`,[claim.weapon,claim.ruler,claim.lease,claim.lease_version]);
 if(!held)throw new BoardRankConflict('ranking publication lease changed');
 return held;
}
async function acknowledge(tx,claim,version) {
 await heldPublication(tx,claim);
 await tx.unsafe(`UPDATE state_board_rank_publication_ready SET applied_version=greatest(applied_version,$5::bigint),
  lease=NULL,lease_version=NULL,lease_until=NULL,error=NULL,
  available_at=CASE WHEN version>greatest(applied_version,$5::bigint) THEN ${now} ELSE NULL END
  WHERE weapon=$1 AND ruler=$2 AND lease=$3 AND lease_version=$4`,[claim.weapon,claim.ruler,claim.lease,claim.lease_version,version]);
}

export async function claimRankPublication(sql,{source,native}) {
 const [row]=await sql.unsafe(`WITH chosen AS MATERIALIZED(SELECT d.weapon,d.ruler FROM state_board_rank_publication_ready d
  JOIN state_board_rank_partitions p USING(weapon,ruler) WHERE p.complete AND p.source=$1 AND p.native=$2
   AND EXISTS(SELECT 1 FROM state_board_read_partitions s WHERE s.weapon=p.weapon AND s.ruler=p.ruler
    AND(s.enabled OR(NOT s.canonical_sources AND s.revision IS NULL)))
   AND d.version>d.applied_version AND d.available_at<=${now} ORDER BY d.available_at,d.weapon,d.ruler
  LIMIT 1 FOR UPDATE OF d SKIP LOCKED)
  UPDATE state_board_rank_publication_ready d SET lease=$3,lease_version=d.version,lease_until=${until},available_at=${until}
  FROM chosen c WHERE (d.weapon,d.ruler)=(c.weapon,c.ruler) RETURNING d.*`,[source,native,randomUUID()]);
 return row||null;
}

export async function retryRankPublication(sql,claim,error) {
 const delay=error instanceof BoardRankQualificationError?null:error instanceof BoardRankConflict?0:5;
 await sql.unsafe(`UPDATE state_board_rank_publication_ready SET lease=NULL,lease_version=NULL,lease_until=NULL,error=$5,
  available_at=CASE WHEN version>$4 THEN ${now} WHEN $6::bigint IS NULL THEN NULL
   ELSE board_input_timestamp(clock_timestamp()+make_interval(secs=>$6::bigint)) END
  WHERE weapon=$1 AND ruler=$2 AND lease=$3 AND lease_version=$4`,[claim.weapon,claim.ruler,claim.lease,claim.lease_version,error.message,delay]);
}

export async function startRankGeneration(sql,native,claim,{source,work,engine}) {
 const token=randomUUID(),directory=join(boardStorage(work),'rank-generation-'+token);await mkdir(directory,{mode:0o750});
 const captured=await captureRankWinners(sql,native,{weapon:claim.weapon,ruler:claim.ruler,source,path:join(directory,'winners.ndjson')});
 const file=await open(join(directory,'rows.ndjson'),'wx');await file.sync();await file.close();
 syncRankDirectory(directory);syncRankDirectory(work);
 return sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  await heldPublication(tx,claim);
  const [generation]=await tx.unsafe(`INSERT INTO state_board_rank_generations(token,weapon,ruler,source,native,cut,engine,index_revision,ready_version,
   phase,directory,winners_sha256,output_sha256,captured_at,input_sha256)
   VALUES($1::uuid,$2,$3,$4,$5,$6,$7,$8,$9,'floor',$10,$11,$12,$13,$14) RETURNING *`,
   [token,claim.weapon,claim.ruler,source,native.adapter,captured.cut,engine,captured.revision,claim.lease_version,directory,captured.winners_sha256,empty,captured.captured_at,captured.input_sha256]);
  return generation;
 });
}

export async function floorRankGeneration(sql,native,claim,generation,{work,source,engine,fault=async()=>{},verified=new Map(),prefixes=new Map()}={}) {
 await generationEpoch(sql,generation,{source,native:native.adapter,engine});
 const directory=directoryOf(generation,work),winners=join(directory,'winners.ndjson'),output=join(directory,'rows.ndjson');
 await verifyFile(winners,generation.winners_sha256,null,verified);
 const page=rankFilePage(winners,Number(generation.winners_cursor),256);
 const decisions=page.rows.length?await native.call('floor',page.rows.map(row=>({score:Number(row.score),leader:Number(row.leader)}))):[];
 const kept=page.rows.filter((_,i)=>decisions[i].keep),body=kept.map(row=>row.page+'\n').join('');
 const leaders=new Map(generation.leaders.map(row=>[JSON.stringify([row.mode,row.riven]),row]));
 for(const row of kept){
  if(Buffer.byteLength(row.page)>65536)throw new BoardRankQualificationError('ranking row exceeds its byte budget');
  const key=JSON.stringify([row.mode,row.riven]);if(!leaders.has(key))leaders.set(key,{mode:row.mode,riven:row.riven,body:JSON.parse(row.page)});
 }
 if(leaders.size>256||Number(generation.output_size)+Buffer.byteLength(body)>256*1024*1024)
  throw new BoardRankQualificationError('ranking output exceeds its disk budget');
 let prefix;
 const result=await sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  await heldPublication(tx,claim);
  const [current]=await tx.unsafe('SELECT * FROM state_board_rank_generations WHERE token=$1::uuid FOR UPDATE',[generation.token]);
  if(current.phase!=='floor'||String(current.winners_cursor)!==String(generation.winners_cursor)
   ||String(current.output_size)!==String(generation.output_size))throw new BoardRankConflict('ranking floor cursor changed');
  if(lstatSync(output).isSymbolicLink()||realpathSync(output)!==output||statSync(output).size<Number(current.output_size))
   throw new BoardRankQualificationError('ranking output prefix is missing');
  const state=statSync(output),saved=prefixes.get(output),fingerprint=[state.dev,state.ino,state.size,state.mtimeMs].join(':');
  const checksum=saved?.fingerprint===fingerprint&&saved.sha===current.output_sha256&&saved.size===Number(current.output_size)
   ?saved.hash.copy():await fileHash(output,Number(current.output_size));
  if(checksum.copy().digest('hex')!==current.output_sha256)throw new BoardRankQualificationError('ranking output prefix changed');
  const file=await open(output,'r+');
  try{
   await file.truncate(Number(current.output_size));
   const bytes=Buffer.from(body);let written=0;
   while(written<bytes.length){const result=await file.write(bytes,written,bytes.length-written,Number(current.output_size)+written);if(!result.bytesWritten)throw Error('ranking output write stopped');written+=result.bytesWritten;}
   await file.sync();await fault('after-output-before-checkpoint');
  }finally{await file.close();}
  checksum.update(body);
  const outputSha=checksum.copy().digest('hex');
  const [updated]=await tx.unsafe(`UPDATE state_board_rank_generations SET winners_cursor=$2,output_size=$3,output_sha256=$4,
   output_rows=output_rows+$5,leaders=($6::text)::jsonb,phase=$7 WHERE token=$1::uuid RETURNING *`,
   [current.token,page.offset,Number(current.output_size)+Buffer.byteLength(body),outputSha,kept.length,JSON.stringify([...leaders.values()]),page.done?'upload':'floor']);
  await tx.unsafe(`UPDATE state_board_rank_publication_ready SET lease_until=${until},available_at=${until} WHERE weapon=$1 AND ruler=$2 AND lease=$3`,[claim.weapon,claim.ruler,claim.lease]);
  prefix={hash:checksum,sha:outputSha,size:Number(updated.output_size)};return updated;
 });
 const state=statSync(output);prefixes.set(output,{...prefix,fingerprint:[state.dev,state.ino,state.size,state.mtimeMs].join(':')});
 return result;
}

export async function uploadRankGeneration(sql,claim,generation,{work,source,native,engine,fault=async()=>{},verified=new Map()}={}) {
 await generationEpoch(sql,generation,{source,native,engine});
 const path=join(directoryOf(generation,work),'rows.ndjson');await verifyFile(path,generation.output_sha256,generation.output_size,verified);
 const page=rankFilePage(path,Number(generation.upload_cursor),256);
 return sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  await heldPublication(tx,claim);
  const [current]=await tx.unsafe('SELECT * FROM state_board_rank_generations WHERE token=$1::uuid FOR UPDATE',[generation.token]);
  if(current.phase!=='upload'||String(current.upload_cursor)!==String(generation.upload_cursor))throw new BoardRankConflict('ranking upload cursor changed');
  const rows=page.literals.map((body,i)=>({ordinal:Number(current.uploaded_rows)+i+1,body}));
  await tx.unsafe(`INSERT INTO state_board_rank_snapshot_rows SELECT $1::uuid,x.ordinal,x.body
   FROM jsonb_to_recordset(($2::text)::jsonb)x(ordinal bigint,body text)`,[current.token,JSON.stringify(rows)]);
  await fault('after-upload-before-checkpoint');
  const [updated]=await tx.unsafe(`UPDATE state_board_rank_generations SET upload_cursor=$2,uploaded_rows=uploaded_rows+$3 WHERE token=$1::uuid RETURNING *`,[current.token,page.offset,rows.length]);
  await tx.unsafe(`UPDATE state_board_rank_publication_ready SET lease_until=${until},available_at=${until} WHERE weapon=$1 AND ruler=$2 AND lease=$3`,[claim.weapon,claim.ruler,claim.lease]);
  return {...updated,uploaded:page.done};
 });
}

export const rankOutputRevision=({source,engine,native,output_sha256})=>hash(JSON.stringify({source,engine,native,rows:output_sha256}));

export async function reuseRankGeneration(sql,claim,generation,{source,engine,native}) {
 const revision=rankOutputRevision({source,engine,native,output_sha256:generation.output_sha256});
 return sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  const [part]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 FOR SHARE',[claim.weapon,claim.ruler]);
  const delivery=await heldPublication(tx,claim);
  const [selected]=await tx.unsafe('SELECT revision FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 AND enabled',[claim.weapon,claim.ruler]);
  if(!part?.complete||part.source!==source||part.native!==native||part.cut!==generation.cut||generation.engine!==engine)
   throw new BoardRankQualificationError('ranking generation recovery boundary changed');
  if(delivery.delivered_output!==revision||selected?.revision!==revision)return null;
  await tx.unsafe("UPDATE state_board_rank_generations SET phase='abandoned' WHERE token=$1::uuid AND phase='upload'",[generation.token]);
  await tx.unsafe('UPDATE state_board_rank_publication_ready SET delivered_revision=greatest(delivered_revision,$3::bigint) WHERE weapon=$1 AND ruler=$2',[claim.weapon,claim.ruler,generation.index_revision]);
  await acknowledge(tx,claim,generation.ready_version);return {status:'unchanged',revision};
 });
}

export async function publishRankGeneration(sql,native,claim,generation,{source,engine}) {
 return sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  const [part]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 FOR SHARE',[claim.weapon,claim.ruler]);
  const delivery=await heldPublication(tx,claim);
  const [current]=await tx.unsafe('SELECT * FROM state_board_rank_generations WHERE token=$1::uuid FOR UPDATE',[generation.token]);
  if(!part?.complete||part.source!==source||current.source!==source||part.native!==native.adapter||current.native!==native.adapter||part.cut!==current.cut||current.engine!==engine)
   throw new BoardRankQualificationError('ranking generation recovery boundary changed');
  if(current.phase!=='upload'||Number(current.uploaded_rows)!==Number(current.output_rows)||Number(current.upload_cursor)!==Number(current.output_size))
   throw new BoardRankQualificationError('ranking generation is not completely uploaded');
  const [{rows,last}]=await tx.unsafe('SELECT count(*) rows,max(ordinal) last FROM state_board_rank_snapshot_rows WHERE token=$1::uuid',[current.token]);
  if(Number(rows)!==Number(current.output_rows)||(Number(rows)>0&&Number(last)!==Number(rows)))throw new BoardRankQualificationError('ranking output coverage changed');
  const [selected]=await tx.unsafe('SELECT * FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 FOR UPDATE',[claim.weapon,claim.ruler]);
  if(!selected||(!selected.enabled&&(selected.canonical_sources||selected.revision)))throw new BoardRankQualificationError('ranking partition selection changed');
  const revision=rankOutputRevision({source,engine,native:native.adapter,output_sha256:current.output_sha256});
  if(Number(delivery.delivered_revision)>Number(current.index_revision)){
   await tx.unsafe("UPDATE state_board_rank_generations SET phase='abandoned' WHERE token=$1::uuid",[current.token]);
   await acknowledge(tx,claim,current.ready_version);return {status:'superseded'};
  }
  const heads=[...current.leaders].sort((a,b)=>a.mode<b.mode?-1:a.mode>b.mode?1:Number(a.riven)-Number(b.riven)).map(row=>row.body);
  await tx.unsafe(`INSERT INTO record_board_read_snapshots VALUES($1,$2,$3,$4,$5,$6,$7,($8::text)::jsonb,$9,${now}) ON CONFLICT DO NOTHING`,
   [claim.weapon,claim.ruler,revision,source,engine,current.input_sha256,current.output_rows,JSON.stringify(heads),current.captured_at]);
  await tx.unsafe(`INSERT INTO record_board_rank_snapshot_refs VALUES($1,$2,$3,$4::uuid) ON CONFLICT DO NOTHING`,[claim.weapon,claim.ruler,revision,current.token]);
  if(!selected.enabled)await tx.unsafe('SELECT board_requested_partition_activate($1,$2,$3,$4::uuid)',[claim.weapon,claim.ruler,revision,source]);
  else await tx.unsafe('UPDATE state_board_read_partitions SET revision=$3 WHERE weapon=$1 AND ruler=$2',[claim.weapon,claim.ruler,revision]);
  await tx.unsafe(`UPDATE state_board_rank_generations SET phase='published',published_at=${now} WHERE token=$1::uuid`,[current.token]);
  await tx.unsafe('UPDATE state_board_rank_publication_ready SET delivered_revision=$3,delivered_output=$4 WHERE weapon=$1 AND ruler=$2',[claim.weapon,claim.ruler,current.index_revision,revision]);
  await acknowledge(tx,claim,current.ready_version);
  return {status:'published',revision,rows:Number(current.output_rows)};
 });
}
