// SPDX-License-Identifier: AGPL-3.0-or-later
// Transport fixtures exercise retention fences; they do not establish native consumer audit correctness.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,existsSync,rmSync,symlinkSync,renameSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {canon} from '../worker/task_contract.js';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {SourceCaptureGC} from './source_capture_gc.mjs';

const root=mkdtempSync(join(tmpdir(),'source-capture-gc-')),owned=join(root,'inventory');mkdirSync(owned);
const db=new DatabaseSync(join(root,'controller.sqlite')),source=new DatabaseSync(join(root,'inputs.sqlite'));
db.exec('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL');source.exec('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL');
const hash=value=>createHash('sha256').update(value).digest('hex'),contract={engine:'fixture',normalizer:{v:1}},context={generation:'owned-generation',engine:'fixture',contract};
const inputs=new PreparedBoardInputs(source,contract),gc=new SourceCaptureGC(db,owned,context);
function capture({baseline=false,collector=gc}={}) {
 const owner=collector.reserve(),raw=join(owner.directory,'delta.jsonl');writeFileSync(raw,'fixture transport only\n');
 const receipt={generation:collector.context.generation,engine:context.engine,contract,status:'sealed',epoch:owner.id,snapshot:'old-fixture-clock',started_at:'2026-11-09T00:00:00Z'};
 const path=join(owner.directory,'capture.json'),manifest={receipt,files:[{path:raw,sha256:hash(readFileSync(raw))}]};
 writeFileSync(path,canon(manifest));const cut={receipt,capture:path,manifest_sha:hash(readFileSync(path)),bootstrap:baseline};collector.seal(owner.id,cut);
 const id=inputs.begin({...receipt,normalizer:contract.normalizer,capture:cut});inputs.seal(id,{inputCount:0,digest:hash('')});collector.bind(receipt.epoch,id);
 return {...owner,cut:id,raw,capture:path};
}
function ack(name,id) {
 const cut=inputs.cut(id);inputs.observeAcknowledgement(name,{complete:true,cut:id,engine:context.engine,snapshot:cut.receipt.snapshot,input_count:0,input_digest:cut.sealed_digest});
}
const collect=(cut,options={})=>gc.collect(inputs,{normalizedCut:cut,...options});
try {
 const baseline=capture({baseline:true}),pending=capture();ack('projection',pending.cut);
 assert.equal((await collect(pending.cut)).retired,0);assert(existsSync(pending.raw));
 ack('inventory',pending.cut);assert.equal((await collect(baseline.cut)).retired,0,'source acknowledgement still pending');
 assert.equal((await collect(pending.cut,{references:[pending.capture]})).retired,0);
 gc.pin('restore',pending.id);assert.equal((await collect(pending.cut)).retired,0);gc.unpin('restore');
 assert.equal((await collect(pending.cut)).retired,1);assert(!existsSync(pending.directory));assert(existsSync(baseline.raw));
 assert(db.prepare('SELECT 1 FROM record_source_capture_manifests WHERE id=?').get(pending.id),'sealed receipt retained');
 assert.equal((await collect(pending.cut)).retired,0);

 const limited=capture();ack('inventory',limited.cut);ack('projection',limited.cut);
 const tooSmall=await collect(limited.cut,{maxBytes:1});assert.equal(tooSmall.files,0);assert(tooSmall.blocked.some(row=>row.reason.includes('byte bound')));assert(existsSync(limited.raw));
 const originalReceipt=source.prepare('SELECT receipt FROM record_prepared_board_cuts WHERE id=?').get(limited.cut).receipt;
 source.prepare('UPDATE record_prepared_board_cuts SET receipt=? WHERE id=?').run(canon({...JSON.parse(originalReceipt),generation:'wrong-generation'}),limited.cut);
 assert((await collect(limited.cut)).blocked.some(row=>row.id===limited.id));assert(existsSync(limited.raw));
 source.prepare('UPDATE record_prepared_board_cuts SET receipt=? WHERE id=?').run(originalReceipt,limited.cut);assert.equal((await collect(limited.cut)).retired,1);

 const bounded=capture();ack('projection',bounded.cut);ack('inventory',bounded.cut);
 let result=await collect(bounded.cut,{maxFiles:1,maxBytes:128});assert.equal(result.files,1);assert.equal(result.retired,0);assert(existsSync(bounded.capture));
 assert.throws(()=>gc.pin('too-late',bounded.id),/unavailable/);
 result=await collect(bounded.cut,{maxFiles:1,maxBytes:8192});assert.equal(result.retired,1);

 for(const stage of ['after-capture-gc-claim','before-capture-file-unlink','after-capture-file-unlink','after-capture-directory-remove']) {
  const victim=capture();ack('inventory',victim.cut);ack('projection',victim.cut);let crashed=false;
  await assert.rejects(collect(victim.cut,{fault:async current=>{if(current===stage&&!crashed){crashed=true;throw Error('crash '+stage);}}}),/crash/);
  const restarted=new SourceCaptureGC(db,owned,context);assert.equal((await restarted.collect(inputs,{normalizedCut:victim.cut})).retired,1);assert(!existsSync(victim.directory));
 }

 const unknown=capture();writeFileSync(join(unknown.directory,'unknown'),'retain');ack('inventory',unknown.cut);ack('projection',unknown.cut);
 assert((await collect(unknown.cut)).blocked.some(row=>row.id===unknown.id));assert(existsSync(unknown.raw));
 const linked=capture(),outside=join(root,'protected-old-capture');writeFileSync(outside,'protected');rmSync(linked.raw);symlinkSync(outside,linked.raw);
 ack('inventory',linked.cut);ack('projection',linked.cut);assert((await collect(linked.cut)).blocked.some(row=>row.id===linked.id));assert.equal(readFileSync(outside,'utf8'),'protected');
 const replaced=capture();renameSync(replaced.raw,replaced.raw+'.old');writeFileSync(replaced.raw,'replacement');
 ack('inventory',replaced.cut);ack('projection',replaced.cut);assert((await collect(replaced.cut)).blocked.some(row=>row.id===replaced.id));assert(existsSync(replaced.raw));
 const foreignGC=new SourceCaptureGC(db,owned,{...context,generation:'another-generation'}),foreign=capture({collector:foreignGC});ack('inventory',foreign.cut);ack('projection',foreign.cut);
 await collect(foreign.cut);assert(existsSync(foreign.raw));
 const unowned=join(owned,'capture-legacy');mkdirSync(unowned);writeFileSync(join(unowned,'capture.json'),'unknown');await collect(foreign.cut);assert(existsSync(unowned));
 assert.throws(()=>gc.bind(pending.id,foreign.cut),/binding changed/);
 const held=inputs.consumer('inventory').audit;source.prepare("UPDATE state_prepared_board_consumers SET audit='{}' WHERE name='inventory'").run();
 await assert.rejects(collect(foreign.cut),/not audited/);source.prepare("UPDATE state_prepared_board_consumers SET audit=? WHERE name='inventory'").run(held);
 await assert.rejects(gc.collect({...inputs,contract:{engine:'foreign'}},{normalizedCut:foreign.cut}),/contract differs/);
 const rootLink=join(root,'linked-root');symlinkSync(owned,rootLink);assert.throws(()=>new SourceCaptureGC(db,rootLink,context),/linked/);
 assert.throws(()=>db.prepare('DELETE FROM record_source_capture_manifests WHERE id=?').run(pending.id),/retained/);
 assert.throws(()=>db.prepare('UPDATE record_source_capture_owners SET generation=? WHERE id=?').run('wrong',baseline.id),/immutable/);
 console.log('owned capture GC transport fences PASS: paused/source-pending consumers, pins, baseline, bounded files/bytes, durable crash/retry, unknown/cross-generation/links/replacement protection, retained receipts and wrong-audit rejection');
}finally{source.close();db.close();rmSync(root,{recursive:true,force:true});}
