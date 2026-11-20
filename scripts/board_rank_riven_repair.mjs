// SPDX-License-Identifier: AGPL-3.0-or-later
// Schedule scoped index work against a witnessed head and withdraw obsolete public revisions.
import {readFile,stat} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {BoardRankConflict} from './board_rank_index.mjs';
import {lockRankInputs} from './board_rank_keys.mjs';
import {putRankDirtyBatch,putRankPublication} from './board_rank_work.mjs';

const isDigest=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const isKey=value=>typeof value==='string'&&/^[a-z0-9_]{1,160}$/.test(value);

async function checkAuthority(tx,{source,policy}) {
 await tx.unsafe('SELECT board_rank_source_lock($1)',[source]);
 const [bound]=await tx.unsafe(`SELECT digest,(SELECT pg_get_userbyid(relowner) FROM pg_class
  WHERE oid='state_board_read_partitions'::regclass)=current_user AS owner FROM state_board_rank_riven_policy WHERE id=1 FOR SHARE`);
 if(!bound?.owner||bound.digest!==policy)throw Error('ranking repair requires the storage owner and the active riven policy');
}

export async function stageRankRivenRepair(sql,{source,native,policy},{weapon,ruler,revision,index_revision,identities}) {
 if(!isKey(weapon)||!isKey(ruler)||!isDigest(revision)||!/^[0-9]+$/.test(String(index_revision))||
   !Array.isArray(identities)||!identities.length||identities.length>512||new Set(identities).size!==identities.length||
   identities.some(identity=>typeof identity!=='string'||!/^[a-f0-9]{30,32}$/.test(identity)))throw Error('invalid ranking repair scope');
 return sql.begin(async tx=>{
  await checkAuthority(tx,{source,policy});
  await lockRankInputs(tx,source,{identities});
  const [part]=await tx.unsafe(`SELECT p.source,p.native,p.revision,s.revision AS head FROM state_board_rank_partitions p
   JOIN state_board_read_partitions s USING(weapon,ruler) JOIN state_board_public_heads h ON (h.weapon,h.ruler,h.revision)=(s.weapon,s.ruler,s.revision)
   WHERE (p.weapon,p.ruler)=($1,$2) AND p.complete AND s.enabled FOR UPDATE OF p,s,h`,[weapon,ruler]);
  if(!part||part.source!==source||part.native!==native||part.head!==revision||String(part.revision)!==String(index_revision))
   throw new BoardRankConflict('ranking repair source, index or public revision advanced');
  const queued=await putRankDirtyBatch(tx,identities.map(identity=>({weapon,ruler,identity,zone:99})),true);
  await tx.unsafe(`INSERT INTO state_board_rank_riven_repairs VALUES($1,$2,$3,($4::text)::jsonb)
   ON CONFLICT(weapon,ruler) DO UPDATE SET revision=excluded.revision,inputs=excluded.inputs`,[weapon,ruler,revision,JSON.stringify(queued)]);
  await putRankPublication(tx,{weapon,ruler,zone:99},true);
  return {weapon,ruler,scheduled:queued.length,revision};
 });
}

export async function withdrawOldRankRevision(sql,bound,{weapon,ruler,revision}) {
 if(!isKey(weapon)||!isKey(ruler)||!isDigest(revision))throw Error('invalid historical revision');
 return sql.begin(async tx=>{
  await checkAuthority(tx,bound);
  const [head]=await tx.unsafe('SELECT revision FROM state_board_public_heads WHERE (weapon,ruler)=($1,$2) FOR SHARE',[weapon,ruler]);
  if(head?.revision===revision)throw new BoardRankConflict('the current ranking revision needs a verified replacement');
  const inserted=await tx.unsafe(`INSERT INTO state_board_public_withdrawn_revisions SELECT weapon,ruler,revision
   FROM record_board_read_snapshots WHERE (weapon,ruler,revision)=($1,$2,$3) ON CONFLICT DO NOTHING`,[weapon,ruler,revision]);
  return {weapon,ruler,revision,withdrawn:inserted.count===1};
 });
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 const path=process.argv[2];
 if(!path||!process.env.PG_URL||(await stat(path)).size>1024*1024)throw Error('PG_URL and a bounded witnessed repair manifest are required');
 const plan=JSON.parse(await readFile(path,'utf8'));
 if(!isDigest(plan.native)||!isDigest(plan.policy)||typeof plan.source!=='string'||!Array.isArray(plan.scopes)||
   !Array.isArray(plan.historical)||plan.scopes.length+plan.historical.length>512)throw Error('invalid ranking repair manifest');
 const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
 const sql=postgres(process.env.PG_URL,{max:1,fetch_types:false,connection:{application_name:'wfsim-board-riven-repair',lock_timeout:1000,statement_timeout:10000}});
 try{
  for(const scope of plan.scopes)console.log(JSON.stringify(await stageRankRivenRepair(sql,plan,scope)));
  for(const revision of plan.historical)console.log(JSON.stringify(await withdrawOldRankRevision(sql,plan,revision)));
 }finally{await sql.end();}
}
