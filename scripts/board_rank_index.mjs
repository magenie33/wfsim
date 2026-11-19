// SPDX-License-Identifier: AGPL-3.0-or-later
// Native describes changed identities; PostgreSQL retains their publication order and winners.
import {createHash} from 'node:crypto';
import {mkdtemp,open,rm} from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {createInterface} from 'node:readline';
import {join} from 'node:path';
import {boardStorage} from './board_storage.mjs';
import {storeRankInputs} from './board_rank_inputs.mjs';
import {lockRankInputs} from './board_rank_keys.mjs';
const digest=value=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const MAX_SOURCES=64,MAX_BYTES=4*1024*1024;
export class BoardRankConflict extends Error {}
export class BoardRankQualificationError extends Error {}

export async function prepareRankSources(native,{weapon,ruler,inputs,cached=[]}) {
 if(!Array.isArray(inputs)||inputs.length>MAX_SOURCES||Buffer.byteLength(JSON.stringify(inputs))>MAX_BYTES)
  throw new BoardRankQualificationError('rank preparation input exceeds its page budget');
 const target=native.contract.rulers.findIndex(row=>row.id===ruler);
 if(target<0)throw new BoardRankQualificationError('unknown ranking ruler');
 const old=new Map(cached.map(row=>[row.identity,row])),sources=[],requests=[],owners=[];
 const missing=[];
 for(let i=0;i<inputs.length;i++){
  const input=inputs[i],record=typeof input.record==='string'?JSON.parse(input.record):input.record;
  const record_sha256=digest(record),prior=old.get(input.identity);
  sources.push({identity:input.identity,record_sha256,descriptor:prior?.record_sha256===record_sha256?prior.descriptor:null,
   input_sha256:digest(input),dependencies:input.dependencies||[]});
  if(!sources.at(-1).descriptor)missing.push({index:i,id:input.identity,record});
 }
 if(missing.length){
  const values=await native.call('describe',missing.map(({id,record})=>({id,record})));
  for(let i=0;i<missing.length;i++)sources[missing[i].index].descriptor=values[i];
 }
 for(let i=0;i<inputs.length;i++){
  const input=inputs[i],source=sources[i],d=source.descriptor;
  if(d.candidate!==null&&d.candidate!==input.identity)throw new BoardRankQualificationError('board source identities require qualification');
  if(d.candidate===null)continue;
  if(d.bucket?.[0]!==weapon)throw new BoardRankQualificationError('rank source changed its weapon');
  if((BigInt('0x'+d.admit)&(1n<<BigInt(target)))===0n)continue;
  for(const fact of input.facts||[]){
   if(fact.identity!==input.identity||fact.ruler!==ruler)throw new BoardRankQualificationError('rank fact changed its scope');
   const mode=d.publication_modes.find(row=>row.id===fact.mode);
   if(!mode||fact.retired_at!=null)continue;
   requests.push({benchmark:ruler,mode:mode.mode,row:{...d.record,...d.publication},fact});
   owners.push({identity:input.identity,mode:mode.mode,mode_order:mode.ordinal});
  }
 }
 const rows=[];
 for(let i=0;i<requests.length;i+=256){
  const page=requests.slice(i,i+256);
  if(Buffer.byteLength(JSON.stringify(page))>MAX_BYTES)throw new BoardRankQualificationError('rank rendering exceeds its page budget');
  const values=await native.call('render',page);
  for(let n=0;n<values.length;n++){
   const value=values[n];
   if(typeof value.page!=='string'||Buffer.byteLength(value.page)>65536||!Number.isFinite(value.score))
    throw new BoardRankQualificationError('invalid native ranking row');
   rows.push({...owners[i+n],...value});
  }
 }
 if(Buffer.byteLength(JSON.stringify(rows))>MAX_BYTES)throw new BoardRankQualificationError('rank destination exceeds its page budget');
 return {sources,rows};
}

export async function createRankPartition(sql,{weapon,ruler,source,native,cut}) {
 await sql.unsafe(`INSERT INTO state_board_rank_partitions(weapon,ruler,source,native,cut,input_sha256)
  VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING`,[weapon,ruler,source,native,cut,digest({source,native,cut})]);
 const [part]=await sql.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
 if(part.source!==source||part.native!==native||part.cut!==cut)throw new BoardRankQualificationError('ranking projection recovery boundary changed');
 return part;
}

export async function applyRankSources(tx,part,prepared,{live=false,cursor=null,loaded=null}={}) {
 await tx.unsafe('SELECT board_rank_source_lock($1::text)',[part.source]);
 await lockRankInputs(tx,part.source,{identities:prepared.sources.map(row=>row.identity),
  reviews:prepared.sources.flatMap(row=>row.dependencies.filter(d=>d.scope==='review').map(d=>d.key))});
 const dependencies=prepared.sources.flatMap(row=>row.dependencies);
 if(live&&dependencies.length){
  const ahead=await tx.unsafe(`SELECT i.scope,i.key FROM state_board_rank_route_inputs i
   JOIN jsonb_to_recordset(($1::text)::jsonb)d(scope text,key text,version bigint) USING(scope,key)
   WHERE d.version IS NULL OR i.version>d.version LIMIT 1`,[JSON.stringify(dependencies)]);
  if(ahead.length)throw new BoardRankConflict('source routing advanced during native preparation');
 }
 const [locked]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 FOR NO KEY UPDATE',[part.weapon,part.ruler]);
 if(!locked||locked.source!==part.source||locked.native!==part.native||locked.cut!==part.cut)
  throw new BoardRankQualificationError('ranking projection source changed');
 if(String(locked.revision)!==String(part.revision))throw new BoardRankConflict('ranking projection changed during native preparation');
 const identities=prepared.sources.map(row=>row.identity);
 const prior=await tx.unsafe(`SELECT identity,live FROM state_board_rank_sources WHERE weapon=$1 AND ruler=$2
  AND identity IN(SELECT jsonb_array_elements_text(($3::text)::jsonb))`,[part.weapon,part.ruler,JSON.stringify(identities)]);
 const keep=new Set(identities.filter(id=>live||!prior.some(row=>row.identity===id&&row.live)));
 const sources=prepared.sources.filter(row=>keep.has(row.identity)),rows=prepared.rows.filter(row=>keep.has(row.identity));
 const keys=JSON.stringify([...keep]);
 const old=await tx.unsafe(`SELECT DISTINCT shape FROM state_board_rank_rows WHERE weapon=$1 AND ruler=$2
  AND identity IN(SELECT jsonb_array_elements_text(($3::text)::jsonb))`,[part.weapon,part.ruler,keys]);
 await storeRankInputs(tx,part,sources,live);
 await tx.unsafe(`DELETE FROM state_board_rank_rows WHERE weapon=$1 AND ruler=$2 AND identity IN(SELECT jsonb_array_elements_text(($3::text)::jsonb))`,[part.weapon,part.ruler,keys]);
 await tx.unsafe(`INSERT INTO state_board_rank_rows SELECT $1,$2,x.identity,x.mode,x.mode_order,x.shape,x.floor,x.def,x.score,x.riven,x.page
  FROM jsonb_to_recordset(($3::text)::jsonb)x(identity text,mode text,mode_order bigint,shape text,floor text,def boolean,score double precision,riven boolean,page text)`,
  [part.weapon,part.ruler,JSON.stringify(rows.map(row=>({...row,def:row.default})))]);
 const shapes=JSON.stringify([...new Set([...old.map(row=>row.shape),...rows.map(row=>row.shape)])]);
 await tx.unsafe(`DELETE FROM state_board_rank_winners WHERE weapon=$1 AND ruler=$2 AND shape IN(SELECT jsonb_array_elements_text(($3::text)::jsonb))`,[part.weapon,part.ruler,shapes]);
 await tx.unsafe(`INSERT INTO state_board_rank_winners SELECT winner.* FROM jsonb_array_elements_text(($3::text)::jsonb) AS affected(shape)
  CROSS JOIN LATERAL (SELECT r.* FROM state_board_rank_rows r WHERE r.weapon=$1 AND r.ruler=$2 AND r.shape=affected.shape
   ORDER BY r.score DESC,r.def DESC,r.identity,r.mode_order LIMIT 1) winner`,[part.weapon,part.ruler,shapes]);
 const [updated]=await tx.unsafe(`UPDATE state_board_rank_partitions SET revision=nextval('board_rank_generation'),input_sha256=$3,
  cursor=coalesce($4::bigint,cursor),loaded=coalesce($5::bigint,loaded) WHERE weapon=$1 AND ruler=$2 RETURNING *`,
  [part.weapon,part.ruler,digest({previous:locked.input_sha256,sources,rows,live,cursor,loaded}),cursor,loaded]);
 return updated;
}

export async function captureRankWinners(sql,native,{weapon,ruler,source,path}) {
 let file;
 try{
  file=await open(path,'wx');let diskBytes=0;const checksum=createHash('sha256');
  const captured=await sql.begin('isolation level repeatable read read only',async tx=>{
   const [part]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2',[weapon,ruler]);
   if(!part?.complete||part.source!==source||part.native!==native.adapter)throw new BoardRankQualificationError('ranking baseline is incomplete or changed');
   const [{captured_at}]=await tx.unsafe(`SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') captured_at`);
   await tx.unsafe(`DECLARE board_rank_output NO SCROLL CURSOR FOR SELECT w.*,
    (SELECT m.score FROM state_board_rank_winners m WHERE m.weapon=w.weapon AND m.ruler=w.ruler AND m.floor=w.floor ORDER BY m.score DESC LIMIT 1) leader
    FROM state_board_rank_winners w WHERE weapon=$1 AND ruler=$2 ORDER BY score DESC,def DESC,identity,mode_order`,[weapon,ruler]);
   const started=Date.now();
   for(;;){
    if(Date.now()-started>120000)throw new BoardRankQualificationError('ranking index capture exceeded its time budget');
    const page=await tx.unsafe('FETCH FORWARD 256 FROM board_rank_output');
    for(const row of page){
     const body=JSON.stringify(row)+'\n';diskBytes+=Buffer.byteLength(body);
     if(diskBytes>256*1024*1024)throw new BoardRankQualificationError('ranking index staging exceeds its disk budget');
     checksum.update(body);await file.writeFile(body);
    }
    if(page.length<256)break;
   }
   await tx.unsafe('CLOSE board_rank_output');
   return {...part,captured_at};
  });
  await file.sync();return {...captured,winners_sha256:checksum.digest('hex'),disk_bytes:diskBytes};
 }finally{await file?.close();}
}

export async function prepareRankSnapshot(sql,native,{weapon,ruler,source,work}) {
 work=boardStorage(work);
 const directory=await mkdtemp(join(work,'board-rank-')),path=join(directory,'winners.ndjson');
 try{
  const captured=await captureRankWinners(sql,native,{weapon,ruler,source,path});
  const rows=[],pages=[],leaders=new Map();let pending=[],bytes=0;
  const consume=async()=>{
   const decisions=await native.call('floor',pending.map(row=>({score:Number(row.score),leader:Number(row.leader)})));
   for(let i=0;i<pending.length;i++)if(decisions[i].keep){
    const row=pending[i],body=JSON.parse(row.page);bytes+=Buffer.byteLength(row.page)+1;
    if(bytes>8*1024*1024)throw new BoardRankQualificationError('native partition output exceeds byte budget');
    rows.push(body);pages.push(row.page);
    const key=JSON.stringify([row.mode,row.riven]);if(!leaders.has(key))leaders.set(key,{mode:row.mode,riven:row.riven,body});
   }
   pending=[];
  };
  for await(const line of createInterface({input:createReadStream(path),crlfDelay:Infinity})){
   pending.push(JSON.parse(line));if(pending.length===256)await consume();
  }
  if(pending.length)await consume();
  const heads=[...leaders.values()].sort((a,b)=>a.mode<b.mode?-1:a.mode>b.mode?1:Number(a.riven)-Number(b.riven)).map(row=>row.body);
  return {weapon,ruler,rows,pages,leaders:heads,captured_at:captured.captured_at,index_revision:captured.revision,
   inputs_sha256:digest({source,cut:captured.cut,generation:captured.revision,inputs:captured.input_sha256,native:native.adapter})};
 }finally{await rm(directory,{recursive:true,force:true});}
}
