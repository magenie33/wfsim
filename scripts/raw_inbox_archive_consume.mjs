// SPDX-License-Identifier: AGPL-3.0-or-later
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { buildBatches } from './build_batches.mjs';
import { close, transaction } from './db.mjs';

const MAX_BYTES = 8392704;

export async function consumeArchive(bytes, digest, native, sqlMs) {
  if (!Buffer.isBuffer(bytes) || bytes.length>MAX_BYTES || !/^[0-9a-f]{64}$/.test(digest) ||
      createHash('sha256').update(bytes).digest('hex')!==digest) throw Error('raw archive checksum or size changed');
  if (!Number.isSafeInteger(sqlMs) || sqlMs<1 || sqlMs>12000) throw Error('invalid raw archive statement limit');
  const body=new TextDecoder('utf-8',{fatal:true}).decode(bytes);
  const packet=JSON.parse(body);
  if (!Array.isArray(packet.rows) || packet.rows.length<1 || packet.rows.length>512 || !Number.isSafeInteger(packet.part) || packet.part<1)
    throw Error('invalid raw archive rows');
  const [[{pending}]]=await transaction([{sql:'SELECT raw_archive_pending(?::uuid,?::bigint,?::text) AS pending',
    params:[packet.generation,packet.part,digest]}]);
  const skip=new Set(pending.skip);
  const inputs=pending.replay ? [] : packet.rows.map(item=>item.input).filter(input=>!skip.has(input.id));
  const {ids,batch}=await normalize(inputs,native);
  const [,[receipt]]=await transaction([
    {sql:"SELECT set_config('statement_timeout',?,true)",params:[sqlMs+'ms']},
    {sql:'SELECT raw_archive_consume(?::uuid,?::bigint,?::text,ARRAY(SELECT jsonb_array_elements_text((?::text)::jsonb)),?::text) AS result',
      params:[packet.generation,packet.part,body,JSON.stringify(ids),JSON.stringify(batch)]},
  ]);
  return receipt.result;
}

async function normalize(inputs,native) {
  if(!inputs.length)return {ids:[],batch:[]};
  const source=inputs.map(input=>JSON.stringify(input)).join('\n')+'\n';
  const scratch=mkdtempSync(join(tmpdir(),'wfsim-raw-archive-'));
  try {
    const done=join(scratch,'done'), builds=join(scratch,'builds.ndjson');
    const result=spawnSync(native,['--deadline','15','--done',done],{input:source,timeout:20000,maxBuffer:MAX_BYTES});
    if(result.status!==0)throw Error('raw archive Native intake failed');
    const ids=readFileSync(done,'utf8').split('\n').filter(Boolean);
    if(JSON.stringify(ids)!==JSON.stringify(inputs.map(input=>input.id)))
      throw Error('raw archive Native intake leaves unacknowledged inputs');
    writeFileSync(builds,result.stdout);
    const batch=[];
    for await(const statement of buildBatches(builds)) {
      if(statement.sql!=='SELECT build_intake_batch(?::text)')throw Error('raw archive output requires another intake owner');
      batch.push(...JSON.parse(statement.params[0]));
    }
    return {ids,batch};
  } finally { rmSync(scratch,{recursive:true,force:true}); }
}

if(process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const [native,digest,timeout]=process.argv.slice(2);
    if(!native||!digest||!timeout)throw Error('raw archive consume requires Native, digest and timeout');
    let size=0;
    const chunks=[];
    for await(const chunk of process.stdin) {
      size+=chunk.length;
      if(size>MAX_BYTES)throw Error('raw archive packet exceeds its transfer bound');
      chunks.push(chunk);
    }
    console.log(JSON.stringify(await consumeArchive(Buffer.concat(chunks),digest,native,Number(timeout))));
  } catch(error) {
    console.error('raw archive consume refused'+(/^[A-Z0-9]{5}$/.test(error.code||'')?' ('+error.code+')':''));
    process.exitCode=1;
  } finally { await close(); }
}
