// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync,realpathSync,statfsSync,statSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { close,query,transaction } from './db.mjs';

const positive=value=>Number.isSafeInteger(value)&&value>0;
const change=async(sql,params=[],milliseconds=8000)=>((await transaction([
  {sql:"SELECT set_config('statement_timeout',?,true),set_config('lock_timeout','200ms',true)",params:[milliseconds+'ms']},
  {sql,params},
]))[1]||[])[0];
export const NEXT_INBOX_GENERATION_SQL=`SELECT g.generation::text,g.original,g.retained FROM state_build_inbox_generations g
  WHERE g.generation<(SELECT closed_before FROM state_build_inbox_storage WHERE id=1)
    AND (g.original IS NOT NULL OR NOT EXISTS(SELECT FROM state_build_inbox_positions p
      WHERE p.generation=g.generation AND p.payload_generation=-g.generation))
  ORDER BY generation LIMIT 1`;
export function inboxCapacity(path) {
  const value=JSON.parse(readFileSync(path,'utf8'));
  if(value.v!==1||![value.root_reserve_bytes,value.data_reserve_bytes].every(positive)
    ||!positive(value.rows)||value.rows>64||!positive(value.seconds)||value.seconds>10
    ||typeof value.work!=='string')throw Error('a bounded approved filesystem budget is required');
  return {...value,work:realpathSync(resolve(value.work))};
}
const RETIRE_FLOOR_BYTES=268435456n;
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const lockBusy=error=>error?.code==='55P03'||/could not obtain lock|lock timeout/i.test(error?.message||'');
async function gate(capacity,{retiring=false}={}) {
  const [{location}]=await query(`SELECT CASE WHEN t.spcname='pg_default' THEN current_setting('data_directory')
    ELSE pg_tablespace_location(t.oid) END AS location FROM state_build_inbox_storage s JOIN pg_tablespace t ON t.oid=s.tablespace WHERE s.id=1`);
  if(statSync(realpathSync(location)).dev!==statSync(capacity.work).dev)throw Error('run maintenance beside the actual PostgreSQL data mount');
  const available=path=>{const fs=statfsSync(path,{bigint:true});return fs.bavail*fs.bsize;};
  const rootFloor=retiring?RETIRE_FLOOR_BYTES:BigInt(capacity.root_reserve_bytes)+67108864n;
  const dataFloor=retiring?RETIRE_FLOOR_BYTES:BigInt(capacity.data_reserve_bytes)+16777216n;
  if(available('/')<rootFloor||available(capacity.work)<dataFloor)
    throw Error('inbox maintenance paused at the approved filesystem reserve');
  if(!retiring&&process.platform==='linux') {
    const full=readFileSync('/proc/pressure/io','utf8').split('\n').find(line=>line.startsWith('full '));
    const avg=name=>Number(full?.match(new RegExp(name+'=([0-9.]+)'))?.[1]);
    if(!(avg('avg10')<=4&&avg('avg60')<=6))throw Error('inbox maintenance paused by whole-host I/O pressure');
  }
}
async function status() {
  const [value]=await query(`SELECT jsonb_build_object('enabled',s.enabled,'automatic',s.automatic,
    'closed_before',s.closed_before::text,'current',build_inbox_current_generation()::text,'readiness',s.readiness,
    'generations',(SELECT coalesce(jsonb_agg(jsonb_build_object('generation',generation::text,
      'original',original::regclass::text,'retained',retained::regclass::text) ORDER BY generation),'[]')
      FROM state_build_inbox_generations)) AS status FROM state_build_inbox_storage s WHERE id=1`);
  return value.status;
}
export async function inboxStorageAction(action,options={}) {
  if(action==='status')return status();
  if(action==='enable') {
    const bytes=readFileSync(options.readiness),receipt=JSON.parse(bytes);
    if(receipt.v!==1||!/^[0-9a-f]{40}$/.test(receipt.source||'')
      ||!['schema','worker','live','hourly','typed','native'].every(key=>/^[0-9a-f]{64}$/.test(receipt[key]||'')))
      throw Error('prove the deployed schema, writers, readers and Native first');
    await change('SELECT build_inbox_enable(?::text,?::boolean)',[createHash('sha256').update(bytes).digest('hex'),options.automatic===true]);
    return status();
  }
  const capacity=inboxCapacity(options.capacity);
  await gate(capacity,{retiring:action==='tick'});
  if(action==='bridge')return change('SELECT build_inbox_adopt_arrivals(?::integer) AS moved',[capacity.rows]);
  if(action==='finish-bridge')return change('SELECT build_inbox_finish_bridge() AS old_bytes');
  if(!['prepare','tick'].includes(action))throw Error('unknown inbox storage action');
  const before=await status();
  if(action==='tick'&&(!before.enabled||!before.automatic))return {paused:'automatic maintenance is disabled'};
  const end=Date.now()+capacity.seconds*1000,receipts=[];
  const perform=async(sql,params=[],retiring=false,waitMs=2000)=>{
    const deadline=Math.min(end,Date.now()+waitMs);
    for(;;) {
      if(Date.now()>=end)throw Error('bounded inbox maintenance window ended');
      await gate(capacity,{retiring});
      try {return await change(sql,params,Math.max(1,Math.min(8000,end-Date.now())));}
      catch(error) {if(action!=='tick'||!lockBusy(error)||Date.now()+50>=deadline)throw error;await sleep(50);}
    }
  };
  let reserveHeld=false;
  try{await gate(capacity);}catch(e){if(action!=='tick')throw e;reserveHeld=true;}
  const current=Number(before.current);
  if(!reserveHeld)for(const generation of [current-1,current,current+1])if(generation>=Number(before.closed_before)) {
    try{await perform('SELECT build_inbox_prepare(?::bigint)',[generation]);}
    catch(e){if(action!=='tick'||!lockBusy(e))throw e;}
  }
  if(action==='prepare')return status();
  try{await perform('SELECT build_inbox_close_generations()',[],true,1500);}
  catch(e){if(!lockBusy(e))throw e;}
  const nextSql=reserveHeld?NEXT_INBOX_GENERATION_SQL.replace('ORDER BY',
    'AND (g.original IS NULL OR NOT EXISTS(SELECT FROM state_build_inbox_positions p WHERE p.generation=g.generation AND p.payload_generation=g.generation)) ORDER BY'):NEXT_INBOX_GENERATION_SQL;
  for(let step=0;step<16&&Date.now()+500<end;step++) {
    const [old]=await query(nextSql);
    if(!old)break;
    const generation=Number(old.generation);
    const pending=async sign=>(await query('SELECT EXISTS(SELECT FROM state_build_inbox_positions WHERE generation=?::bigint AND payload_generation=?::bigint) AS pending',
      [generation,generation*sign]))[0].pending;
    let progressed=false;
    try {
      if(old.original!==null) {
        if(await pending(1)) {
          if(reserveHeld)break;
          await perform('SELECT build_inbox_prepare_retained(?::bigint)',[generation]);
          receipts.push(await perform('SELECT build_inbox_carry(?::bigint,?::integer) AS moved',[generation,capacity.rows]));
          progressed=true;
        }
        if(!await pending(1)){receipts.push(await perform('SELECT build_inbox_retire(?::bigint,false) AS retired',[generation],true));progressed=true;}
      }
      if(!await pending(-1)){receipts.push(await perform('SELECT build_inbox_retire(?::bigint,true) AS retired',[generation],true));progressed=true;}
    } catch(e) {if(lockBusy(e))break;throw e;}
    if(!progressed)break;
  }
  return {receipts};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href) {
  try {
    const [action,...args]=process.argv.slice(2),options={};
    for(let at=0;at<args.length;at++) {
      if(args[at]==='--automatic')options.automatic=true;
      else if(['--capacity','--readiness'].includes(args[at])&&args[at+1])options[args[at].slice(2)]=args[++at];
      else throw Error('use status|prepare|enable|tick|bridge|finish-bridge with explicit readiness or capacity');
    }
    console.log(JSON.stringify(await inboxStorageAction(action,options)));
  } catch(error) {console.error('inbox storage refused: '+error.message);process.exitCode=1;}
  finally {await close();}
}
