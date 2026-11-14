// SPDX-License-Identifier: AGPL-3.0-or-later
// Capture sourced inputs before normalization; advance a scan only after intake acknowledges it.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, renameSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { query,close } from './db.mjs';

export const INBOX_PAGE_ROWS=100, INBOX_MAX_ROWS=250000, INBOX_MAX_BYTES=128*1024*1024;
const fields='i.id,i.at,i.record,i.initiator,i.family_source,i.observation,i.workflow_scope';
const digest=value=>createHash('sha256').update(value).digest('hex');
const save=(path,value)=>{const temporary=path+'.pending-'+process.pid;writeFileSync(temporary,JSON.stringify(value)+'\n');renameSync(temporary,path);};

export function inboxReadUrl(value) {
  if(value.startsWith('pglite:'))return value;
  const url=new URL(value);
  for(const [key,setting] of Object.entries({statement_timeout:'5000',lock_timeout:'500',
    default_transaction_read_only:'on',application_name:'wfsim-inbox'}))url.searchParams.set(key,setting);
  return url.toString();
}

export function inboxSource(run=query) {
  return {
    priority: max=>run(`SELECT p.id,p.zone,p.position FROM state_build_inbox_priorities p
      WHERE EXISTS(SELECT 1 FROM state_build_inbox_inputs i WHERE i.id=p.id)
      ORDER BY p.zone,p.position,p.id LIMIT ?::bigint`,[max]),
    byIds: ids=>run(`WITH wanted AS MATERIALIZED (
        SELECT value AS id,ordinality FROM jsonb_array_elements_text((?::text)::jsonb) WITH ORDINALITY)
      SELECT ${fields} FROM wanted w JOIN state_build_inbox_inputs i ON i.id=w.id ORDER BY w.ordinality`,[JSON.stringify(ids)]),
    after: (id,max)=>run(`SELECT ${fields} FROM state_build_inbox_inputs i
      WHERE i.id>?::text ORDER BY i.id LIMIT ?::bigint`,[id,max]),
  };
}

export function readInboxCursor(path) {
  if(!path || !existsSync(path))return '';
  const value=JSON.parse(readFileSync(path,'utf8'));
  if(value.v!==1 || typeof value.after!=='string')throw Error('invalid inbox scan cursor');
  return value.after;
}

export async function captureInbox({source=inboxSource(),emit,after='',maxRows=INBOX_MAX_ROWS,maxBytes=INBOX_MAX_BYTES,
  maxSeconds=900,pageRows=INBOX_PAGE_ROWS,clock=Date.now}={}) {
  if(typeof emit!=='function' || typeof after!=='string' || ![maxRows,maxBytes,pageRows].every(n=>Number.isSafeInteger(n)&&n>0)
    || maxRows>INBOX_MAX_ROWS || pageRows>INBOX_PAGE_ROWS || !Number.isFinite(maxSeconds) || maxSeconds<0)throw Error('invalid inbox capture bounds');
  const started=clock(),seen=new Set(),ids=[],hash=createHash('sha256');
  const receipt={v:1,from_after:after,next_after:after,rows:0,bytes:0,scan_exhausted:false,wrapped:false,stop:'budget',ids};
  const timedOut=()=>clock()-started>=maxSeconds*1000;
  const read=async operation=>{
    try {return await operation();}catch(error){
      if(!receipt.rows)throw error;
      receipt.read_error=String(error.message||error);receipt.stop='read_failed';return null;
    }
  };
  const take=async row=>{
    if(!row || typeof row.id!=='string')throw Error('an inbox input requires its original id');
    if(seen.has(row.id))return true;
    const line=JSON.stringify(row)+'\n',bytes=Buffer.byteLength(line);
    if(bytes>maxBytes)throw Error('one inbox input exceeds the capture byte budget');
    if(receipt.rows>=maxRows || receipt.bytes+bytes>maxBytes)return false;
    await emit(line,row);hash.update(line);seen.add(row.id);ids.push(row.id);receipt.rows++;receipt.bytes+=bytes;return true;
  };
  const finish=()=>({...receipt,output_sha256:hash.digest('hex'),elapsed_seconds:(clock()-started)/1000});
  if(timedOut())return finish();
  const priority=await source.priority(maxRows+1);
  for(let offset=0;offset<priority.length;offset+=pageRows) {
    if(timedOut() || receipt.rows>=maxRows)return finish();
    const rows=await read(()=>source.byIds(priority.slice(offset,offset+pageRows).map(row=>row.id)));
    if(!rows)return finish();
    for(const row of rows)if(!await take(row))return finish();
  }
  if(priority.length>maxRows) {receipt.stop='priority_continues';return finish();}
  while(!timedOut() && receipt.rows<maxRows) {
    const rows=await read(()=>source.after(receipt.next_after,pageRows));
    if(!rows)return finish();
    if(!rows.length) {receipt.next_after='';receipt.wrapped=true;receipt.scan_exhausted=true;receipt.stop='scan_end';break;}
    for(const row of rows) {
      if(typeof row.id!=='string' || row.id<=receipt.next_after)throw Error('inbox scan did not advance in id order');
      if(!await take(row))return finish();
      receipt.next_after=row.id;
    }
  }
  return finish();
}

export function checkInboxReceipt({receiptFile,inputFile,allowReadError=false}) {
  const receipt=JSON.parse(readFileSync(receiptFile,'utf8'));
  if(receipt.v!==1 || digest(readFileSync(inputFile))!==receipt.output_sha256)throw Error('inbox capture changed before acknowledgement');
  if(receipt.read_error && !allowReadError)throw Error('inbox capture read failed: '+receipt.read_error);
  return receipt;
}

export function commitInboxCursor({cursorFile,receiptFile,inputFile,doneFile}) {
  const receipt=checkInboxReceipt({receiptFile,inputFile,allowReadError:true});
  if(receipt.v!==1 || !Array.isArray(receipt.ids) || receipt.rows!==receipt.ids.length
    || typeof receipt.from_after!=='string' || typeof receipt.next_after!=='string')throw Error('invalid inbox capture receipt');
  if(readInboxCursor(cursorFile)!==receipt.from_after)throw Error('inbox scan cursor changed before acknowledgement');
  const done=new Set(doneFile ? readFileSync(doneFile,'utf8').split('\n').filter(Boolean) : []);
  if(receipt.ids.some(id=>!done.has(id)))throw Error('intake has not acknowledged every captured input');
  save(cursorFile,{v:1,after:receipt.next_after});
}

async function main(args) {
  const options={};
  for(let i=0;i<args.length;i+=2) {
    if(!['--out','--receipt','--cursor','--done','--commit','--check','--allow-read-error','--max-rows','--max-seconds'].includes(args[i]) || args[i+1]===undefined)throw Error('invalid inbox reader arguments');
    options[args[i]]=args[i+1];
  }
  if(options['--commit']) {
    commitInboxCursor({cursorFile:options['--cursor'],receiptFile:options['--commit'],inputFile:options['--out'],doneFile:options['--done']});return;
  }
  if(options['--check']) {
    checkInboxReceipt({receiptFile:options['--check'],inputFile:options['--out'],allowReadError:options['--allow-read-error']==='true'});return;
  }
  if(!options['--out'])throw Error('inbox reader requires an output file');
  const output=resolve(options['--out']),temporary=output+'.pending-'+process.pid;
  const after=readInboxCursor(options['--cursor']);
  // This child changes only its own read connection; the parent retains intake's write connection.
  if(process.env.PG_URL)process.env.PG_URL=inboxReadUrl(process.env.PG_URL);
  writeFileSync(temporary,'');
  try {
    const receipt=process.env.PG_URL ? await captureInbox({after,
      maxRows:Number(options['--max-rows']||INBOX_MAX_ROWS),maxSeconds:Number(options['--max-seconds']||900),
      emit:line=>writeFileSync(temporary,line,{flag:'a'})})
      : {v:1,from_after:after,next_after:after,rows:0,ids:[],bytes:0,scan_exhausted:false,configured:false,output_sha256:digest('')};
    renameSync(temporary,output);
    if(options['--receipt'])save(options['--receipt'],receipt);
    console.log(`inbox: read ${receipt.rows} record(s)${receipt.scan_exhausted?'; scan reached its end, next cycle starts before the cursor':'; bounded capture, unread arrivals remain'}`);
  } finally {rmSync(temporary,{force:true});}
}
if(process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try {await main(process.argv.slice(2));}catch(error){console.error('inbox: '+error.message);process.exitCode=1;}
  finally {await close();}
}
