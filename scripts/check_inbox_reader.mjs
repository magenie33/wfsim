// SPDX-License-Identifier: AGPL-3.0-or-later
// Exercise capture ordering and the acknowledgement boundary with independent queue fixtures.
import assert from 'node:assert/strict';
import { mkdtempSync,readFileSync,writeFileSync,rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync,spawnSync } from 'node:child_process';
import { captureInbox,commitInboxCursor,readInboxCursor,checkInboxReceipt,inboxReadUrl } from './inbox_reader.mjs';

const row=id=>({id,at:'2026-11-14',record:'{"weapon":"braton"}',initiator:'anonymous',family_source:null,observation:null,workflow_scope:null});
const ordinary=Array.from({length:12},(_,n)=>row('input-'+String(n).padStart(3,'0')));
let pending=[...ordinary,row('review:waiting')],priority=['review:waiting'];
const source={
  priority:async max=>priority.filter(id=>pending.some(row=>row.id===id)).slice(0,max).map(id=>({id,zone:1,position:'2026-11-13'})),
  byIds:async ids=>ids.flatMap(id=>pending.filter(row=>row.id===id)),
  after:async(id,max)=>pending.filter(row=>row.id>id).sort((a,b)=>a.id<b.id?-1:1).slice(0,max),
};
const capture=async options=>{const lines=[];const receipt=await captureInbox({source,pageRows:3,maxRows:5,emit:line=>lines.push(line),...options});return {receipt,text:lines.join('')};};
const directory=mkdtempSync(join(tmpdir(),'wfsim-inbox-reader-'));
const input=join(directory,'inbox.ndjson'),receiptFile=join(directory,'receipt.json'),cursorFile=join(directory,'cursor.json'),doneFile=join(directory,'done.txt');
const record=value=>{writeFileSync(input,value.text);writeFileSync(receiptFile,JSON.stringify(value.receipt));};
const commit=()=>commitInboxCursor({cursorFile,receiptFile,inputFile:input,doneFile});
try {
  const urgent=await capture({maxRows:1});
  assert.deepEqual(urgent.receipt.ids,['review:waiting'],'waiting review precedes lower ordinary IDs without filling from the slow ordinary scan');
  assert.equal(urgent.receipt.next_after,'');assert.equal(urgent.receipt.scan_exhausted,false);
  pending=pending.filter(row=>row.id!=='review:waiting');priority=[];
  const first=await capture();record(first);
  assert.deepEqual(first.receipt.ids,ordinary.slice(0,5).map(row=>row.id));
  assert.equal(first.receipt.next_after,'input-004','a row fetched beyond the emitted budget remains for the next capture');
  writeFileSync(doneFile,first.receipt.ids.slice(0,4).join('\n')+'\n');
  assert.throws(commit,/not acknowledged/);assert.equal(readInboxCursor(cursorFile),'');
  writeFileSync(doneFile,first.receipt.ids.join('\n')+'\n');
  writeFileSync(input,first.text+'changed');assert.throws(commit,/capture changed/);
  writeFileSync(input,first.text);commit();assert.equal(readInboxCursor(cursorFile),'input-004');
  const restarted=execFileSync(process.execPath,['--input-type=module','-e',
    `import {readInboxCursor} from ${JSON.stringify(new URL('./inbox_reader.mjs',import.meta.url).href)};process.stdout.write(readInboxCursor(process.argv[1]));`,cursorFile],{encoding:'utf8'});
  assert.equal(restarted,'input-004','a fresh process reads only the acknowledged checkpoint');
  pending=pending.filter(row=>!first.receipt.ids.includes(row.id));
  const second=await capture({after:readInboxCursor(cursorFile)});
  assert.deepEqual(second.receipt.ids,ordinary.slice(5,10).map(row=>row.id));
  pending=pending.filter(row=>!second.receipt.ids.includes(row.id));
  const last=await capture({after:second.receipt.next_after,maxRows:10});
  assert.deepEqual(last.receipt.ids,ordinary.slice(10).map(row=>row.id));assert.equal(last.receipt.wrapped,true);
  pending=pending.filter(row=>!last.receipt.ids.includes(row.id));pending.push(row('input-000-late'));
  const late=await capture({after:last.receipt.next_after});assert.deepEqual(late.receipt.ids,['input-000-late']);
  const budget=await capture({maxRows:1,maxBytes:1024});assert.equal(budget.receipt.rows,1);assert.equal(budget.receipt.scan_exhausted,false);
  let now=0;
  const slowSource={...source,after:async(...args)=>{now=20000;return source.after(...args);}};
  const slow=await capture({source:slowSource,maxSeconds:1,clock:()=>now});
  assert.deepEqual(slow.receipt.ids,['input-000-late'],'a completed bounded read makes progress even if it exhausted the cycle time');
  const zero=await capture({maxSeconds:0});assert.equal(zero.receipt.rows,0);assert.equal(zero.receipt.next_after,'');assert.equal(zero.receipt.scan_exhausted,false);
  await assert.rejects(capture({source:{...source,after:async()=>[row('z'),row('a')]}}),/did not advance/);
  pending=ordinary.slice(0,6);priority=[];let after='';
  for(let cycle=0;cycle<3;cycle++) {
    const id='review:continuous-'+cycle;pending.push(row(id));priority=[id];
    const batch=await capture({after,maxRows:3});after=batch.receipt.next_after;
    assert.equal(batch.receipt.ids[0],id);
    assert.equal(batch.receipt.ids.filter(id=>id.startsWith('input-')).length,2,'unused priority capacity advances ordinary inputs');
    pending=pending.filter(row=>!batch.receipt.ids.includes(row.id));
  }
  assert.ok(!pending.some(row=>row.id.startsWith('input-')));
  pending=[row('review:prefix')];priority=['review:prefix'];
  const partial=await capture({after:readInboxCursor(cursorFile),source:{...source,after:async()=>{throw Error('bounded read timeout');}}});
  assert.deepEqual(partial.receipt.ids,['review:prefix']);assert.match(partial.receipt.read_error,/read timeout/);record(partial);
  writeFileSync(doneFile,'review:prefix\n');commit();assert.equal(readInboxCursor(cursorFile),'input-004');
  assert.throws(()=>checkInboxReceipt({receiptFile,inputFile:input}),/read failed/,'committed prefix does not turn the failed read into success');
  const url=new URL(inboxReadUrl('postgres://fixture:local@127.0.0.1/db?sslmode=disable'));
  assert.equal(url.searchParams.get('statement_timeout'),'5000');assert.equal(url.searchParams.get('default_transaction_read_only'),'on');
  writeFileSync(input,'preserved previous capture');
  const refused=spawnSync(process.execPath,[new URL('./inbox_reader.mjs',import.meta.url).pathname,'--out',input],
    {env:{...process.env,PG_URL:'pglite:'+join(directory,'empty-db')},encoding:'utf8'});
  assert.notEqual(refused.status,0);assert.equal(readFileSync(input,'utf8'),'preserved previous capture');
  console.log('inbox reader: priority, bounded keyset pages, partial acknowledgement, capture integrity and restart/wrap pass');
} finally {rmSync(directory,{recursive:true,force:true});}
