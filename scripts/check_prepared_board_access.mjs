// SPDX-License-Identifier: AGPL-3.0-or-later
// A local weapon reads its own source prefix rather than unrelated normalized history.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs,preparedBatchHash} from './prepared_board_inputs.mjs';
import {PreparedBoardReader,PREPARED_WEAPON_ROWS_SQL} from './prepared_board_reader.mjs';

const root=mkdtempSync(join(tmpdir(),'prepared-access-')),contract={engine:'0123456789ab',normalizer:{v:1}};
try {
 for(const history of [1000,10000]) {
  const source=join(root,'source-'+history+'.sqlite'),records=openCatalog(source,{engine:contract.engine,contract:{record_storage:'interned-deflate-v1'},create:true});
  const inputs=new PreparedBoardInputs(records.db,contract),id=inputs.begin({...contract,epoch:'one',snapshot:'frozen',started_at:'2026-11-14T00:00:00Z'}),digest=createHash('sha256');
  for(let from=0;from<=history;from+=256) {
   const items=Array.from({length:Math.min(256,history+1-from)},(_,n)=>{
    const ordinal=from+n,key=String(ordinal).padStart(8,'0'),weapon=ordinal?'other':'target';
    return {kind:'source',key,ordinal,value:{id:key,candidate:key,bucket:[weapon,false],admit:'0000000000000001',publication:{},publication_modes:[{id:'base',mode:''}]}};
   });inputs.append(id,items);digest.update(preparedBatchHash(items)+'\n');
  }
  inputs.seal(id,{inputCount:history+1,digest:digest.digest('hex')});
  const db=new DatabaseSync(join(root,'consumer-'+history+'.sqlite')),reader=new PreparedBoardReader(db,source,contract);reader.bindViews(id);
  const indexed=()=>{
   reader.raw.prepare('SELECT 1 FROM prepared.entity_prepared_board_versions LIMIT 1').get();
   const plan=reader.db.prepare('EXPLAIN QUERY PLAN '+PREPARED_WEAPON_ROWS_SQL).all('target',id,id,'','',0);
   assert(plan.some(row=>/SEARCH .*prepared_board_sources/.test(row.detail)),JSON.stringify(plan));
   assert(!plan.some(row=>/TEMP B-TREE|SCAN .*entity_prepared_board_versions/.test(row.detail)),JSON.stringify(plan));return plan;
  };
  indexed();assert.equal([...reader.weaponRows('target',id)].length,1);assert.equal(reader.sourceCount(),history+1);
  records.db.exec('DROP INDEX prepared_board_sources');assert.throws(indexed);
  new PreparedBoardInputs(records.db,contract);indexed();
  console.log(JSON.stringify({history,plan:indexed(),local_sources:1}));db.close();records.close();
 }
 console.log('ok: actual prepared weapon pages use their indexed prefix at1k/10k history; removing the index fails; restoration preserves source count');
}finally{rmSync(root,{recursive:true,force:true});}
