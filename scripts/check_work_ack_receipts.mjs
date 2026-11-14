// SPDX-License-Identifier: AGPL-3.0-or-later
// A receiver may miss intermediate audited replies without losing their contiguous input receipts.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {PreparedBoardInputs,preparedBatchHash} from './prepared_board_inputs.mjs';
import {PreparedBoardReader} from './prepared_board_reader.mjs';
import {PreparedWorkConsumer,validateWorkSpan,workSpanReceipt} from './prepared_work_consumer.mjs';

const root=mkdtempSync(join(tmpdir(),'work-acks-')),path=join(root,'source.sqlite'),sourceDb=new DatabaseSync(path),db=new DatabaseSync(join(root,'consumer.sqlite'));
sourceDb.exec('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL');
const contract={engine:'fixture',normalizer:{v:1}},inputs=new PreparedBoardInputs(sourceDb,contract,{consumers:['inventory']});
let reader,consumer;
function seal() {
 const id=inputs.begin({engine:contract.engine,normalizer:contract.normalizer,epoch:String(sourceDb.prepare('SELECT count(*) n FROM record_prepared_board_cuts').get().n+1),
  snapshot:'fixture-snapshot',started_at:'2026-11-09T00:00:00Z'});
 const items=[{kind:'source',key:'fixture',ordinal:0,value:{n:id}}];inputs.append(id,items);
 inputs.seal(id,{inputCount:1,digest:createHash('sha256').update(preparedBatchHash(items)+'\n').digest('hex')});return id;
}
function consume(beforeComplete=()=>{}) {
 db.exec('BEGIN');try{const cut=consumer.begin();for(;;){const page=consumer.page();if(!page.length)break;consumer.progress(page.at(-1).ordinal+1);}
  const span=consumer.auditSpan();beforeComplete();
  const result=consumer.complete({complete:true,cut:cut.id,snapshot:cut.receipt.snapshot,engine:cut.receipt.engine,input_span:span});db.exec('COMMIT');return result;
 }catch(error){db.exec('ROLLBACK');throw error;}
}
try {
 seal();reader=new PreparedBoardReader(db,path,contract);consumer=new PreparedWorkConsumer(db,reader,'inventory',{fixture:'receipt-transport-only'});
 const first=consume();inputs.observeAcknowledgement('inventory',first);inputs.collect();
 seal();const second=consume();seal();seal();seal();const fifth=consume();
 assert.equal(inputs.consumer('inventory').acknowledged,1);assert.equal(fifth.input_span.after,1);assert.equal(fifth.input_span.first,2);assert.equal(fifth.input_span.last,5);
 validateWorkSpan(inputs,fifth.input_span,5,{after:1});
 const latestOnly=workSpanReceipt(inputs,2,5);
 assert.throws(()=>validateWorkSpan(inputs,latestOnly,5,{after:1}),/invalid work input span/);
 inputs.observeAcknowledgement('inventory',second);inputs.collect();
 assert.equal(inputs.db.prepare('SELECT count(*) n FROM record_prepared_board_changes WHERE cut<=2').get().n,0);
 validateWorkSpan(inputs,fifth.input_span,5,{after:2});inputs.observeAcknowledgement('inventory',fifth);inputs.collect();
 validateWorkSpan(inputs,fifth.input_span,5,{after:5});inputs.observeAcknowledgement('inventory',fifth);
 assert.throws(()=>inputs.observeAcknowledgement('inventory',second),/invalid prepared acknowledgement/);
 const held=inputs.cut(3).sealed_digest;sourceDb.prepare('UPDATE record_prepared_board_cuts SET sealed_digest=? WHERE id=3').run('0'.repeat(64));
 assert.throws(()=>validateWorkSpan(inputs,fifth.input_span,5,{after:5}),/differs/);sourceDb.prepare('UPDATE record_prepared_board_cuts SET sealed_digest=? WHERE id=3').run(held);
 seal();db.prepare("DELETE FROM record_work_input_cuts WHERE name='inventory' AND cut=5").run();
 // A prior cut outside the observed prefix is no longer required; an unobserved one is mandatory.
 const sixth=consume();assert.equal(sixth.input_span.after,5);
 seal();seal();const eighth=consume(()=>{inputs.observeAcknowledgement('inventory',sixth);inputs.collect();});
 assert.equal(eighth.input_span.after,5);validateWorkSpan(inputs,eighth.input_span,8,{after:6});
 seal();db.prepare("DELETE FROM record_work_input_cuts WHERE name='inventory' AND cut=7").run();
 assert.throws(consume,/progress receipt differs/);assert.equal(consumer.state().acknowledged,8);assert.equal(inputs.consumer('inventory').acknowledged,6);
 console.log('ok: source sees ACK1 then only ACK5; exact overlapped receipts cover every missed cut, GC/retry is idempotent, corrupt/gapped prefixes and lost progress reject, ACK never regresses; metadata-only transport fixtures do not claim a work audit');
}finally{db.close();sourceDb.close();rmSync(root,{recursive:true,force:true});}
