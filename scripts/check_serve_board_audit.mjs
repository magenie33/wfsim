// SPDX-License-Identifier: AGPL-3.0-or-later
// The production inventory caller shares one audit allowance between durable state and scratch.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {copyFileSync,mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';

const source=resolve(process.argv[2]||fileURLToPath(new URL('..',import.meta.url)));
const load=name=>import(pathToFileURL(join(source,'scripts',name)).href);
const [{serveBoardStage,splitBoardContext,boardConsumerContract},{openCatalog},{PreparedBoardInputs},
 {PreparedWorkAudit},{openInventoryOperator}]=await Promise.all([
 load('split_board_pipeline.mjs'),load('incremental_inventory.mjs'),load('prepared_board_inputs.mjs'),
 load('audit_prepared_work.mjs'),load('inventory_operator.mjs')]);
const root=mkdtempSync(join(tmpdir(),'serve-board-audit-')),bin=join(root,'bin');mkdirSync(bin);
const settings=['PG_URL','ONCE','WFSIM_BOARD_AUDIT_BYTES','WFSIM_BOARD_STORAGE_REQUIRED',
 'WFSIM_BOARD_WORK_ROOT','WFSIM_BOARD_DATA_MOUNT','WFSIM_BOARD_DATA_UUID'];
const previous=Object.fromEntries(settings.map(key=>[key,process.env[key]]));
const close=PreparedWorkAudit.prototype.close,observed=[];
let native;
try {
 for(const key of settings)delete process.env[key];
 process.env.PG_URL='postgres://unused:unused@127.0.0.1:1/unused';process.env.ONCE='1';
 copyFileSync(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'),join(bin,'wfsim-board'));
 writeFileSync(join(bin,'ENGINE'),'0123456789ab\n');writeFileSync(join(bin,'VERSION'),'fixture\n');
 native=await openInventoryOperator(join(bin,'wfsim-board'));
 const context=await splitBoardContext(bin,native),generation=createHash('sha256').update(JSON.stringify(context.contract)).digest('hex');
 PreparedWorkAudit.prototype.close=function() {
  const pageSize=this.db.prepare('PRAGMA page_size').get().page_size;
  const pages=this.db.prepare('PRAGMA max_page_count').get().max_page_count;
  observed.push({state:pages*pageSize,scratch:this.scratchBytes,pageSize});
  return close.call(this);
 };
 for(const [index,total]of [24*1024**3,3*1024**2+1].entries()) {
  const work=join(root,'case-'+index),inventoryRoot=join(work,'inventory');mkdirSync(inventoryRoot,{recursive:true});
  const state={...context,generation,catalog:join(work,'inputs.sqlite'),inventory:join(work,'inventory.sqlite'),
   audit:join(work,'audit.sqlite'),board:join(work,'board.sqlite')};
  const records=openCatalog(state.catalog,{...context,generation,create:true,databaseBytes:16*1024**2});
  try {new PreparedBoardInputs(records.db,context.contract);}finally{records.close();}
  writeFileSync(join(inventoryRoot,'split-state.json'),JSON.stringify(state));
  process.env.WFSIM_BOARD_AUDIT_BYTES=String(total);
  await serveBoardStage('inventory',work,bin);
  assert.equal(observed.length,index+1,'the real ONCE inventory caller must construct and close its audit');
  const actual=observed[index],stateBytes=Math.floor(total/2);
  console.log(JSON.stringify({total,...actual,source_contract:context.contract,consumer_contract:boardConsumerContract('inventory',state)}));
  assert.equal(actual.state,Math.floor(stateBytes/actual.pageSize)*actual.pageSize,'production state cap');
  assert.equal(actual.scratch,total-stateBytes,'production scratch cap retains the odd byte');
  assert(actual.state+actual.scratch<=total,'the production caller exceeds its combined audit allowance');
 }
 console.log('ok: actual inventory ONCE splits24GiB and an odd audit allowance; SQLite state and scratch caps share one total');
}finally {
 PreparedWorkAudit.prototype.close=close;
 if(native)await native.close();
 for(const [key,value]of Object.entries(previous))if(value===undefined)delete process.env[key];else process.env[key]=value;
 rmSync(root,{recursive:true,force:true});
}
