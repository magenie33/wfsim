// SPDX-License-Identifier: AGPL-3.0-or-later
// A contract migration creates a separate database and requires a new complete audit over preserved producer output.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,rmSync,existsSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {canon} from '../worker/task_contract.js';
import {openInventoryOperator} from './inventory_operator.mjs';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {PreparedBoardProjection} from './prepared_board_projection.mjs';
import {boardConsumerContract} from './split_board_pipeline.mjs';
import {migrateBoardAudit} from './migrate_board_audit.mjs';
import {fileHash} from './inventory_publication_io.mjs';

const root=mkdtempSync(join(tmpdir(),'board-audit-migration-')),native=await openInventoryOperator(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'));
const engine='0123456789ab',contract={engine,normalizer:{v:1},record_storage:'interned-deflate-v1'},generation='fixture';
const state={engine,contract,generation},sourcePath=join(root,'source.sqlite'),oldPath=join(root,'old.sqlite'),newPath=join(root,'new.sqlite');
const target=boardConsumerContract('projection',state),fromContract={...target,code:'fdb9277cb94ba9e23da556ff9a99554336bbcde5bcee548c9d3480d958bde15f'};
const records=openCatalog(sourcePath,{engine,contract,generation,create:true}),inputs=new PreparedBoardInputs(records.db,contract);
let board;
try {
 const record=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[0].record,[d]=await native.call('describe',[{id:'seed',record}]);
 const raw={};for(const [kind,rows]of Object.entries({builds:[{id:'seed',record}],facts:[{identity:d.candidate,ruler:native.contract.rulers[0].id,mode:d.modes[0],score:123,
  measured_by:'verified:migration-fixture',started_at:'2026-11-09T00:00:00Z',finished_at:'2026-11-09T00:00:01Z',cost_seconds:1}],queue:[],tasks:[]})) {
  raw[kind]=join(root,kind+'.ndjson');writeFileSync(raw[kind],rows.map(row=>JSON.stringify(row)+'\n').join(''));
 }
 const cut={bootstrap:true,raw,meta:{counts:{library:1,facts:1,queue:0,tasks:0}},receipt:{epoch:'one',engine,snapshot:'old-fixture',started_at:'2026-11-09T00:00:00Z'}};
 let prepared;do{prepared=await prepareBoardCut(inputs,records,cut,native.call);}while(prepared.pending);
 mkdirSync(join(root,'pub/data'),{recursive:true});writeFileSync(join(root,'pub/data/board_state.yaml'),'boards: {}\n');mkdirSync(join(root,'verified'));
 const legacy='[{ "opaque": "original legacy bytes" }]';writeFileSync(join(root,'verified',d.bucket[0]+'.json'),legacy);
 board=new PreparedBoardProjection(oldPath,sourcePath,fromContract,native.contract,root,{engine,generation,create:true,fault:async stage=>{
  if(stage==='after-board-audit-before-package')throw Error('unacknowledged completed producer');
 }});
 await assert.rejects(async()=>{for(let n=0;;n++){assert(n<100);await board.step(native.call,{budgetMs:20,enqueue:async()=>{throw Error('must not enqueue');}});}},/unacknowledged completed producer/);
 assert(board.board.meta.audit_ready);const output=readFileSync(join(root,'verified',d.bucket[0]+'.json'),'utf8');
 const baselines=board.records.db.prepare('SELECT * FROM state_board_audit_baselines ORDER BY dir,weapon').all(),setup=board.records.db.prepare('SELECT * FROM state_board_audit_setup').all();
 board.close();board=null;const oldSha=await fileHash(oldPath),options={oldPath,newPath,sourcePath,state,fromContract,databaseBytes:16*1024**2};
 await assert.rejects(migrateBoardAudit({...options,fromContract:{...fromContract,code:'unknown'}}),/unsupported/);
 await assert.rejects(migrateBoardAudit({...options,newPath:oldPath}),/separate unlinked/);
 await assert.rejects(migrateBoardAudit({...options,state:{...state,engine:'ffffffffffff'}}),/unacknowledged first cut/);
 assert(!existsSync(newPath));const receipt=await migrateBoardAudit(options);
 assert.equal(await fileHash(oldPath),oldSha);assert.equal(receipt.old_audit_reused,false);assert.equal(receipt.new_legacy_freeze,false);
 board=new PreparedBoardProjection(newPath,sourcePath,target,native.contract,root,{engine,generation});
 assert.equal(board.consumer.state().acknowledged,0);assert.equal(board.board.meta.audit_ready,null);
 assert.equal(board.records.db.prepare('SELECT count(*) n FROM state_board_audit_round').get().n,0);
 assert.equal(canon(board.records.db.prepare('SELECT * FROM state_board_audit_baselines ORDER BY dir,weapon').all()),canon(baselines));
 assert.equal(canon(board.records.db.prepare('SELECT * FROM state_board_audit_setup').all()),canon(setup));
 let nativeRows=0,enqueued=0;
 for(let n=0;;n++){assert(n<100);const step=await board.step(async(op,items)=>{if(op==='render')nativeRows+=items.length;return native.call(op,items);},
  {budgetMs:20,enqueue:async()=>{enqueued++;return {local_migration_fixture:true};}});if(!step.pending)break;}
 assert(nativeRows>0,'new contract reused the old audit completion');assert.equal(enqueued,1);assert.equal(board.consumer.state().acknowledged,1);
 assert.equal(readFileSync(join(root,'verified',d.bucket[0]+'.json'),'utf8'),output);assert.equal(await fileHash(oldPath),oldSha);
 assert(baselines.some(row=>row.path&&readFileSync(row.path,'utf8')===legacy));
 console.log('board audit migration PASS: separate database, exact original output/baselines/setup, fresh native re-audit and ACK; unknown contract, path reuse and generation mismatch reject; original database unchanged');
}finally{board?.close();records.close();await native.close();rmSync(root,{recursive:true,force:true});}
