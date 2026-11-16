// SPDX-License-Identifier: AGPL-3.0-or-later
// Public revisions retain ten minutes and three newer versions; orphan rows retire in small transactions.
import {lstat,unlink,rmdir,readdir} from 'node:fs/promises';
import {resolve,dirname,basename,join} from 'node:path';
import {boardStorage} from './board_storage.mjs';
import {BoardRankQualificationError} from './board_rank_index.mjs';

async function removeArtifacts(directory,work,prefix,names) {
 const selected=resolve(directory),root=resolve(boardStorage(work));
 if(dirname(selected)!==root||!new RegExp('^'+prefix+'[a-f0-9-]{36}$').test(basename(selected)))
  throw new BoardRankQualificationError('ranking cleanup path changed');
 let state;try{state=await lstat(selected);}catch(error){if(error.code==='ENOENT')return;throw error;}
 if(state.isSymbolicLink()||!state.isDirectory())throw new BoardRankQualificationError('ranking cleanup directory changed');
 for(const name of await readdir(selected))if(!names.includes(name))throw new BoardRankQualificationError('ranking cleanup contains an unknown file');
 for(const name of names){
  const path=join(selected,name);
  try{const entry=await lstat(path);if(entry.isSymbolicLink()||!entry.isFile())throw new BoardRankQualificationError('ranking cleanup file changed');await unlink(path);}
  catch(error){if(error.code!=='ENOENT')throw error;}
 }
 await rmdir(selected);
}

export async function collectRankStorage(sql,{work,source}) {
 work=boardStorage(work);
 const retired=await sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  const [row]=await tx.unsafe(`SELECT s.weapon,s.ruler,s.revision FROM record_board_read_snapshots s
   JOIN record_board_rank_snapshot_refs r USING(weapon,ruler,revision)
   JOIN state_board_read_partitions p USING(weapon,ruler)
   WHERE s.revision IS DISTINCT FROM p.revision
    AND s.published_at<board_input_timestamp(clock_timestamp()-interval '10minutes')
    AND (SELECT count(*) FROM(SELECT 1 FROM record_board_read_snapshots newer WHERE newer.weapon=s.weapon AND newer.ruler=s.ruler
     AND (newer.published_at,newer.revision)>(s.published_at,s.revision) LIMIT 3)x)=3
   ORDER BY s.published_at,s.weapon,s.ruler LIMIT 1 FOR UPDATE OF s SKIP LOCKED`);
  if(row)await tx.unsafe('DELETE FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2 AND revision=$3',[row.weapon,row.ruler,row.revision]);
  return row;
 });
 if(retired)return {status:'retired-reference'};
 const [finished]=await sql.unsafe("SELECT token,directory FROM state_board_rank_generations WHERE phase='published' AND directory<>'' ORDER BY created_at,token LIMIT 1");
 if(finished){
  await removeArtifacts(finished.directory,work,'rank-generation-',['winners.ndjson','rows.ndjson']);
  await sql.unsafe("UPDATE state_board_rank_generations SET directory='' WHERE token=$1::uuid AND phase='published'",[finished.token]);
  return {status:'collected-published-files'};
 }
 const [generation]=await sql.unsafe(`SELECT g.* FROM state_board_rank_generations g WHERE phase IN('published','abandoned')
  AND NOT EXISTS(SELECT 1 FROM record_board_rank_snapshot_refs r WHERE r.token=g.token) ORDER BY created_at,token LIMIT 1`);
 if(generation){
  const removed=await sql.unsafe(`DELETE FROM state_board_rank_snapshot_rows WHERE token=$1::uuid AND ordinal IN
   (SELECT ordinal FROM state_board_rank_snapshot_rows WHERE token=$1::uuid ORDER BY ordinal LIMIT 1000) RETURNING ordinal`,[generation.token]);
  if(removed.length)return {status:'collected-rows',rows:removed.length};
  if(generation.directory)await removeArtifacts(generation.directory,work,'rank-generation-',['winners.ndjson','rows.ndjson']);
  await sql.unsafe(`DELETE FROM state_board_rank_generations g WHERE token=$1::uuid AND phase IN('published','abandoned')
   AND NOT EXISTS(SELECT 1 FROM record_board_rank_snapshot_refs r WHERE r.token=g.token)`,[generation.token]);
  return {status:'collected-generation'};
 }
 const [part]=await sql.unsafe(`SELECT * FROM state_board_rank_partitions WHERE complete AND path<>'' ORDER BY weapon,ruler LIMIT 1`);
 if(part){
  await removeArtifacts(dirname(part.path),work,'rank-cut-',['inputs.ndjson']);
  await sql.unsafe("UPDATE state_board_rank_partitions SET path='' WHERE weapon=$1 AND ruler=$2 AND path=$3 AND complete",[part.weapon,part.ruler,part.path]);
  return {status:'collected-baseline'};
 }
 return null;
}

export async function collectRankOrphan(sql,{work}) {
 work=boardStorage(work);
 for(const name of await readdir(work)){
  if(!/^rank-(cut|generation)-[a-f0-9-]{36}$/.test(name))continue;
  const directory=join(work,name),entry=await lstat(directory);
  if(entry.mtimeMs>Date.now()-600000)continue;
  const [retained]=name.startsWith('rank-cut-')
   ?await sql.unsafe('SELECT weapon FROM state_board_rank_partitions WHERE path=$1 LIMIT 1',[join(directory,'inputs.ndjson')])
   :await sql.unsafe('SELECT token FROM state_board_rank_generations WHERE directory=$1 LIMIT 1',[directory]);
  if(retained)continue;
  const cut=name.startsWith('rank-cut-');
  await removeArtifacts(directory,work,cut?'rank-cut-':'rank-generation-',cut?['inputs.ndjson']:['winners.ndjson','rows.ndjson']);
  return {status:'collected-orphan-files'};
 }
 return null;
}
