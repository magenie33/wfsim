// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {setTimeout as pause} from 'node:timers/promises';
import {rankOutputRevision} from './board_rank_publication.mjs';
import {catalogPageDigest} from './board_rank_catalog_replay.mjs';

export const sha256 = value => createHash('sha256').update(value).digest('hex');
export const scopeKey = (weapon,ruler) => weapon+'/'+ruler;

export async function captureSelected(sql,sets,contract){
 const pins=new Map();
 for(const {scopes} of sets.values())for(const s of scopes)pins.set(scopeKey(s.weapon,s.ruler)+'/'+s.revision,s.total);
 return sql.begin('isolation level repeatable read read only',async tx=>{
  const rows=await tx.unsafe(`SELECT s.weapon,s.ruler,s.revision,h.rows,h.source,h.engine,p.source rank_source,p.complete,p.native,
   g.token,g.output_sha256,g.output_rows FROM state_board_read_partitions s
   LEFT JOIN record_board_read_snapshots h USING(weapon,ruler,revision)
   LEFT JOIN state_board_rank_partitions p USING(weapon,ruler)
   LEFT JOIN record_board_rank_snapshot_refs f ON (f.weapon,f.ruler,f.revision)=(s.weapon,s.ruler,s.revision)
   LEFT JOIN state_board_rank_generations g ON g.token=f.token WHERE s.enabled AND s.canonical_sources`);
  for(const h of rows){
   assert(h.complete&&h.source===contract.source&&h.rank_source===contract.source&&h.native===contract.native&&h.revision,
    'existing head does not belong to the qualified Native and source');
   const key=scopeKey(h.weapon,h.ruler)+'/'+h.revision;
   if(pins.has(key)){assert.equal(Number(h.rows),pins.get(key));continue;}
   assert(h.token&&Number(h.output_rows)===Number(h.rows)&&rankOutputRevision(h)===h.revision,
    'existing head needs a protected revision or a complete Native generation');
   h.pages_sha256=await catalogPageDigest(tx,h);
  }
  return rows;
 });
}

export async function catalogHead(sql,weapon,ruler){
 const [row]=await sql.unsafe(`SELECT s.scene,s.enabled,s.canonical_sources,s.revision,snap.source,snap.engine,snap.rows,
  snap.leaders,snap.published_at,p.source rank_source,p.native,p.cut,p.complete,p.catalog_phase,p.catalog_proof,p.error,
  p.available_at,ready.error publication_error,ready.available_at publication_available,g.token,g.output_sha256,g.output_rows
  FROM state_board_read_partitions s LEFT JOIN state_board_rank_partitions p USING(weapon,ruler)
  LEFT JOIN state_board_rank_publication_ready ready USING(weapon,ruler)
  LEFT JOIN record_board_read_snapshots snap ON (snap.weapon,snap.ruler,snap.revision)=(s.weapon,s.ruler,s.revision)
  LEFT JOIN record_board_rank_snapshot_refs refs ON (refs.weapon,refs.ruler,refs.revision)=(s.weapon,s.ruler,s.revision)
  LEFT JOIN state_board_rank_generations g ON g.token=refs.token WHERE s.weapon=$1 AND s.ruler=$2`,[weapon,ruler]);
 return row||null;
}

export function checkCanonicalHead(head,receipt,scene,manifest){
 assert(head?.enabled&&head.canonical_sources&&head.complete&&head.scene===scene,'public selection is incomplete');
 assert(head.source===receipt.source&&head.rank_source===receipt.source&&head.native===receipt.native,'public ranking provenance changed');
 assert(/^[a-f0-9]{64}$/.test(head.revision)&&Number.isSafeInteger(Number(head.rows))&&Number(head.rows)>=0,'invalid public snapshot');
 if(manifest||head.catalog_phase){
  const proof=head.catalog_proof;
  assert(head.catalog_phase==='active'&&proof?.checked===receipt.pages_sha256&&proof.archive===receipt.archive&&
   proof.snapshot===receipt.snapshot&&proof.rows===receipt.rows&&proof.scene===scene&&(!manifest||head.cut===manifest),
   'selected index has no matching frozen and replayed qualification');
 }
 if(head.token)assert(Number(head.output_rows)===Number(head.rows)&&rankOutputRevision({source:head.source,engine:head.engine,
  native:head.native,output_sha256:head.output_sha256})===head.revision,'selected revision differs from its Native output');
}

async function get(origin,path,params){
 const url=new URL(path,origin);url.search=new URLSearchParams(params).toString();
 for(let attempt=0;;attempt++){
  let response;
  try{
   response=await fetch(url,{signal:AbortSignal.timeout(15000),cache:'no-store'});
   if(response.status===409||response.status===404)return null;
   if(!response.ok)throw Error('public board read returned HTTP '+response.status+' from '+url.origin);
   return await response.json();
  }catch(e){
   if(attempt===2||(response&&response.status<500&&response.status!==429))throw e;
   await pause(350*2**attempt);
  }
 }
}

export async function selectedAtOrigins(origins,head,weapon,ruler){
 const results=await Promise.all(origins.map(async origin=>{
  let after='';
  do{
   const page=await get(origin,'/api/board/pages/catalog',{weapon,scene:head.scene,after});
   assert(page?.ok&&Array.isArray(page.partitions),'unavailable public catalog');
   for(const p of page.partitions){
    const key=scopeKey(p.weapon,p.ruler);assert(p.weapon===weapon&&p.scene===head.scene&&key>after,'invalid catalog order');
    after=key;
    if(p.ruler===ruler){
     if(p.revision!==head.revision)return false;
     assert(Number(p.rows)===Number(head.rows)&&isDeepStrictEqual(p.leaders,head.leaders),'public catalog differs from its selected revision');
     return true;
    }
    if(p.ruler>ruler)return false;
   }
   if(page.next===null)return false;
   assert(page.partitions.length&&page.next===after,'invalid public catalog cursor');
  }while(true);
 }));
 return results.every(Boolean);
}

export async function agreedPage(sql,origins,head,weapon,ruler,after,protectedRows){
 const params={weapon,ruler,revision:head.revision,after:String(after)};
 const count=Number(head.rows),last=Math.min(after+128,count);
 const [raw,...pages]=await Promise.all([
  head.token?sql.unsafe('SELECT ordinal,body FROM state_board_rank_snapshot_rows WHERE token=$1 AND ordinal>$2 AND ordinal<=$3 ORDER BY ordinal',
   [head.token,after,last]):sql.unsafe('SELECT ordinal,body FROM state_board_read_rows WHERE weapon=$1 AND ruler=$2 AND revision=$3 AND ordinal>$4 AND ordinal<=$5 ORDER BY ordinal',
   [weapon,ruler,head.revision,after,last]),
  ...origins.map(origin=>get(origin,'/api/board/pages',params))
 ]);
 if(pages.some(p=>p===null))return null;
 assert(raw.length===last-after,'incomplete stored public page');
 const hash=createHash('sha256'),rows=raw.map((row,i)=>{
  assert(Number(row.ordinal)===after+i+1&&typeof row.body==='string','invalid stored row ordinal or wire');
  hash.update(row.body+'\n');return JSON.parse(row.body);
 });
 if(protectedRows)assert(isDeepStrictEqual(rows,protectedRows),'public revision differs from its protected rows');
 for(const page of pages)assert(page?.ok&&page.weapon===weapon&&page.ruler===ruler&&page.revision===head.revision&&
  page.source===head.source&&page.engine===head.engine&&page.count===count&&page.after===after&&
  page.next===(last===count?null:last)&&isDeepStrictEqual(page.rows,rows),'public rows differ from the pinned database page');
 return {after,last,digest:hash.digest('hex')};
}
