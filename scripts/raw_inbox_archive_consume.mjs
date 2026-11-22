// SPDX-License-Identifier: AGPL-3.0-or-later
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, statfsSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { buildBatches } from './build_batches.mjs';
import { close, transaction } from './db.mjs';

const MAX_BYTES = 8392704;
const PACKAGE_ROWS = 2048;
const SLICE_ROWS = 64;
const CLI_START_BUDGET_MS = 10000;

class SliceLimit extends Error {}

export async function consumeArchive(bytes, digest, native, sqlMs, {startUntil=Infinity, hasRoom=()=>true}={}) {
  if (!Buffer.isBuffer(bytes) || bytes.length>MAX_BYTES || !/^[0-9a-f]{64}$/.test(digest) ||
      createHash('sha256').update(bytes).digest('hex')!==digest) throw Error('raw archive checksum or size changed');
  if (!Number.isSafeInteger(sqlMs) || sqlMs<1 || sqlMs>12000) throw Error('invalid raw archive statement limit');
  const body=new TextDecoder('utf-8',{fatal:true}).decode(bytes);
  const packet=JSON.parse(body);
  if (!Array.isArray(packet.rows) || packet.rows.length<1 || packet.rows.length>PACKAGE_ROWS || !Number.isSafeInteger(packet.part) || packet.part<1)
    throw Error('invalid raw archive rows');
  const timeout={sql:"SELECT set_config('statement_timeout',?,true)",params:[sqlMs+'ms']};
  const [,[{pending}]]=await transaction([timeout,{sql:'SELECT raw_archive_pending(?::uuid,?::bigint,?::text) AS pending',
    params:[packet.generation,packet.part,digest]}]);
  if(pending.replay)return {committed:true,replay:true,rows:packet.rows.length};
  const skip=new Set(pending.skip);
  let offset=pending.offset, limit=pending.limit, replay=true;
  if(!Number.isSafeInteger(offset)||offset<0||offset>=packet.rows.length)throw Error('invalid raw archive offset');
  if(!Number.isSafeInteger(limit)||limit<1||limit>SLICE_ROWS)throw Error('invalid raw archive slice limit');
  while(offset<packet.rows.length){
    if(performance.now()>=startUntil)return {committed:false,offset};
    if(!hasRoom())return {committed:false,offset,paused:'filesystem_reserve'};
    const end=Math.min(offset+limit,packet.rows.length);
    const inputs=packet.rows.slice(offset,end).map(item=>item.input).filter(input=>!skip.has(input.id));
    let intake;
    try{intake=await normalize(inputs,native);}
    catch(error){
      if(!(error instanceof SliceLimit))throw error;
      if(end-offset===1)return {committed:false,offset,blocked:'single_input_limit'};
      limit=Math.floor((end-offset)/2);
      if(!hasRoom())return {committed:false,offset,paused:'filesystem_reserve'};
      await transaction([timeout,{sql:'SELECT raw_archive_narrow_slice(?::uuid,?::bigint,?::text,?::int,?::int)',
        params:[packet.generation,packet.part,digest,offset,limit]}]);
      continue;
    }
    if(!hasRoom())return {committed:false,offset,paused:'filesystem_reserve'};
    const [,[{result}]]=await transaction([
      timeout,
      {sql:'SELECT raw_archive_consume_slice(?::uuid,?::bigint,?::text,?::int,?::int,ARRAY(SELECT jsonb_array_elements_text((?::text)::jsonb)),?::text) AS result',
        params:[packet.generation,packet.part,body,offset,end,JSON.stringify(intake.ids),intake.body]},
    ]);
    if(!result.committed||!Number.isSafeInteger(result.offset)||result.offset<=offset||result.offset>packet.rows.length)
      throw Error('raw archive offset did not advance');
    replay=replay&&result.replay;
    offset=result.offset;
  }
  return {committed:true,replay,rows:packet.rows.length};
}

async function normalize(inputs,native) {
  if(!inputs.length)return {ids:[],body:'[]'};
  const source=inputs.map(input=>JSON.stringify(input)).join('\n')+'\n';
  const scratch=mkdtempSync(join(tmpdir(),'wfsim-raw-archive-'));
  try {
    const done=join(scratch,'done'), builds=join(scratch,'builds.ndjson');
    const result=spawnSync(native,['--deadline','15','--done',done],{input:source,timeout:20000,maxBuffer:MAX_BYTES});
    if(['ENOBUFS','ETIMEDOUT'].includes(result.error?.code))throw new SliceLimit();
    if(result.status!==0)throw Error('raw archive Native intake failed');
    const ids=readFileSync(done,'utf8').split('\n').filter(Boolean);
    if(ids.some((id,index)=>id!==inputs[index]?.id))
      throw Error('raw archive Native intake changed its acknowledged inputs');
    if(ids.length<inputs.length)throw new SliceLimit();
    writeFileSync(builds,result.stdout);
    const batch=[];
    try{
      for await(const statement of buildBatches(builds)) {
        if(statement.sql!=='SELECT build_intake_batch(?::text)')throw Error('raw archive output requires another intake owner');
        batch.push(...JSON.parse(statement.params[0]));
      }
    }catch(error){
      if(error.message==='one intake row exceeds the batch byte limit')throw new SliceLimit();
      throw error;
    }
    const body=JSON.stringify(batch);
    if(batch.length>1000||Buffer.byteLength(body)>MAX_BYTES)throw new SliceLimit();
    return {ids,body};
  } finally { rmSync(scratch,{recursive:true,force:true}); }
}

function diskRoom(work, rootBlocks, workBlocks) {
  const available=path=>statfsSync(path,{bigint:true}).bavail;
  return available('/')>=rootBlocks&&available(work)>=workBlocks;
}

if(process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const [native,digest,timeout,work,rootBlocks,workBlocks]=process.argv.slice(2);
    if(!native||!digest||!timeout||!work||!rootBlocks||!workBlocks)
      throw Error('raw archive consume requires Native, digest, timeout and disk reserves');
    let size=0;
    const chunks=[];
    for await(const chunk of process.stdin) {
      size+=chunk.length;
      if(size>MAX_BYTES)throw Error('raw archive packet exceeds its transfer bound');
      chunks.push(chunk);
    }
    const root=BigInt(rootBlocks), data=BigInt(workBlocks);
    console.log(JSON.stringify(await consumeArchive(Buffer.concat(chunks),digest,native,Number(timeout),{
      startUntil:performance.now()+CLI_START_BUDGET_MS,hasRoom:()=>diskRoom(work,root,data),
    })));
  } catch(error) {
    console.error('raw archive consume refused'+(/^[A-Z0-9]{5}$/.test(error.code||'')?' ('+error.code+')':''));
    process.exitCode=1;
  } finally { await close(); }
}
