// SPDX-License-Identifier: AGPL-3.0-or-later
// Fast and slow consumers retain their own snapshots, replay positions and audit acknowledgements.
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { createHash } from 'node:crypto';
import { PreparedBoardInputs,preparedBatchHash } from './prepared_board_inputs.mjs';
import { canon } from '../worker/task_contract.js';
import {inflateRawSync,deflateRawSync} from 'node:zlib';

const db=new DatabaseSync(':memory:'),contract={engine:'e',normalizer:{v:1}};
const queries=[],prepare=db.prepare.bind(db);db.prepare=sql=>{queries.push(sql);return prepare(sql);};
const inputs=new PreparedBoardInputs(db,contract);
const receipt=epoch=>({epoch,snapshot:'snapshot-'+epoch,started_at:'2026-11-14T00:00:00.000Z',...contract});
const source=(id,n)=>({id,candidate:id,bucket:['weapon',false],admit:'2000000000000000',publication_modes:[{id:'base',mode:'base'}],n});
function seal(id,items) {
 const digest=createHash('sha256').update(preparedBatchHash(items)+'\n');
 inputs.seal(id,{inputCount:items.length,digest:digest.digest('hex')});
}
function finish(name,id) {
 let page;while((page=inputs.page(name,id,1)).length)inputs.progress(name,id,page.at(-1).ordinal+1);
 inputs.acknowledge(name,id,{complete:true,cut:id,snapshot:inputs.cut(id).receipt.snapshot,engine:'e'});
}
try {
 const first=inputs.begin(receipt('one')),one=[{kind:'source',key:'a',ordinal:0,value:source('a',1)}];
 inputs.append(first,one);assert.equal(inputs.append(first,one),1);
 assert.throws(()=>inputs.append(first,[{...one[0],value:source('a',99)}]),/replay changed/);
 assert.equal(inputs.next('projection'),null);seal(first,one);
 assert.throws(()=>inputs.append(first,[]),/sealed/);
 assert.equal(inputs.value('source','a',first).value.n,1);
 assert.equal(inputs.value('source','a',first).admit,'2000000000000000');
 finish('projection',first);assert.equal(inputs.collect().floor,0);
 const second=inputs.begin(receipt('two')),two=[{kind:'source',key:'a',ordinal:0,value:source('a',2)},
  {kind:'source',key:'b',ordinal:1,value:source('b',2)}];
 inputs.append(second,two);seal(second,two);
 finish('projection',second);assert.equal(inputs.consumer('inventory').acknowledged,0);
 assert.equal(inputs.value('source','a',first).value.n,1,'fast projection rewrote the slow inventory snapshot');
 assert.equal(inputs.page('inventory',first)[0].value.n,1);
 finish('inventory',first);assert.equal(inputs.collect().floor,first);
 assert.equal(inputs.value('source','a',first).value.n,1);
 const block=db.prepare('SELECT versions FROM record_prepared_board_changes WHERE cut=?').get(second).versions;
 const swapped=JSON.parse(inflateRawSync(block)).reverse();
 db.prepare('UPDATE record_prepared_board_changes SET versions=? WHERE cut=?').run(deflateRawSync(JSON.stringify(swapped)),second);
 assert.throws(()=>inputs.page('inventory',second),/block changed/);
 db.prepare('UPDATE record_prepared_board_changes SET versions=? WHERE cut=?').run(block,second);
 assert.throws(()=>inputs.acknowledge('inventory',second,{complete:true,cut:second,snapshot:'snapshot-two',engine:'e'}),/incomplete/);
 inputs.progress('inventory',second,1);
 const reopened=new PreparedBoardInputs(db,contract);
 assert.equal(reopened.page('inventory',second)[0].key,'b');
 assert.throws(()=>reopened.progress('inventory',second,0),/progress/);
 finish('inventory',second);const collected=inputs.collect();assert.equal(collected.floor,second);assert(collected.versions>0);
 const retentionSql=queries.find(sql=>sql.startsWith('DELETE FROM entity_prepared_board_versions WHERE id IN('));
 const indexed=()=>assert(db.prepare('EXPLAIN QUERY PLAN ' + retentionSql).all(second,4096).some(row=>/prepared_board_expired/.test(row.detail)));
 indexed();db.exec('DROP INDEX prepared_board_expired');assert.throws(indexed);db.exec('CREATE INDEX prepared_board_expired ON entity_prepared_board_versions(last_cut,id) WHERE last_cut IS NOT NULL');indexed();
 assert.equal(inputs.value('source','a',second).value.n,2);
 assert.throws(()=>new PreparedBoardInputs(db,contract,{consumers:['late']}),/retained baseline/);
 assert.throws(()=>new PreparedBoardInputs(db,{...contract,engine:'changed'}),/generation changed/);
 console.log('ok: separate snapshots and consumer cursors survive replay/restart; fast ACK cannot erase slow inputs; audit gates and slowest-ACK retention remain independent');
} finally {db.close();}
