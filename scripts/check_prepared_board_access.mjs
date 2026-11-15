// SPDX-License-Identifier: AGPL-3.0-or-later
// A local weapon reads its own source prefix rather than unrelated normalized history.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,rmSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs,preparedBatchHash,PREPARED_SOURCE_ORDER_SQL} from './prepared_board_inputs.mjs';
import {PreparedBoardReader,PREPARED_WEAPON_ROWS_SQL} from './prepared_board_reader.mjs';
import {BoardProjectionState} from './board_projection_state.mjs';
import {PUBLICATION_ROWS_SQL} from './inventory_publication.mjs';

const root=mkdtempSync(join(tmpdir(),'prepared-access-')),contract={engine:'0123456789ab',normalizer:{v:1}};
try {
 for(const history of [1000,10000]) {
  const source=join(root,'source-'+history+'.sqlite'),records=openCatalog(source,{engine:contract.engine,contract:{record_storage:'interned-deflate-v1'},create:true});
  const inputs=new PreparedBoardInputs(records.db,contract),id=inputs.begin({...contract,epoch:'one',snapshot:'frozen',started_at:'2026-11-14T00:00:00Z'}),digest=createHash('sha256');
  const prepare=records.db.prepare.bind(records.db);let allocations=0;
  records.db.prepare=sql=>{if(sql===PREPARED_SOURCE_ORDER_SQL)allocations++;return prepare(sql);};
  for(let from=0;from<=history;from+=256) {
   const items=Array.from({length:Math.min(256,history+1-from)},(_,n)=>{
    const ordinal=from+n,key=String(ordinal).padStart(8,'0'),weapon=ordinal?'other':'target';
    return {kind:'source',key,ordinal,value:{id:key,candidate:key,bucket:[weapon,false],admit:'0000000000000001',publication:{},publication_modes:[{id:'base',mode:''}]}};
   });inputs.append(id,items);digest.update(preparedBatchHash(items)+'\n');
  }
  inputs.seal(id,{inputCount:history+1,digest:digest.digest('hex')});
  const batchAllocations=allocations;assert.equal(batchAllocations,Math.ceil((history+1)/256),'one ordinal lookup per committed source batch');
  const ordinalPlan=sql=>{
   const plan=records.db.prepare('EXPLAIN QUERY PLAN '+sql).all();
   assert(plan.some(row=>/SEARCH .*COVERING INDEX prepared_board_source_order \(source_order>/.test(row.detail)),JSON.stringify(plan));
   assert(!plan.some(row=>/TEMP B-TREE|SCAN /.test(row.detail)),JSON.stringify(plan));return plan;
  };
  const ordinal=ordinalPlan(PREPARED_SOURCE_ORDER_SQL);
  assert.equal(records.db.prepare(PREPARED_SOURCE_ORDER_SQL).get().source_order,history+1);
  assert.throws(()=>ordinalPlan("SELECT coalesce(max(source_order),0)+1 n FROM entity_prepared_board_versions WHERE kind='source'"),/AssertionError/);
  records.db.exec('DROP INDEX prepared_board_source_order');assert.throws(()=>ordinalPlan(PREPARED_SOURCE_ORDER_SQL),/no such index/);
  new PreparedBoardInputs(records.db,contract);ordinalPlan(PREPARED_SOURCE_ORDER_SQL);
  const db=new DatabaseSync(join(root,'consumer-'+history+'.sqlite')),reader=new PreparedBoardReader(db,source,contract);reader.bindViews(id);
  const projection=new BoardProjectionState({db},{}),publicationIndex=projection.index({});
  const publicationPlan=()=>{
   db.prepare('SELECT 1 FROM prepared.entity_prepared_board_versions LIMIT 1').get();
   const plan=publicationIndex.q('EXPLAIN QUERY PLAN '+PUBLICATION_ROWS_SQL).all('target',1n,'fixture','','','',0,0,256);
   assert(plan.some(row=>/SEARCH .*prepared_board_sources \(weapon=/.test(row.detail)),JSON.stringify(plan));
   assert(plan.some(row=>/SEARCH .*prepared_board_measurements \(identity=\? AND ruler=\?/.test(row.detail)),JSON.stringify(plan));return plan;
  };
  publicationPlan();records.db.exec('DROP INDEX prepared_board_measurements');assert.throws(publicationPlan);
  new PreparedBoardInputs(records.db,contract);publicationPlan();
  for(const file of ['prepared_board_projection.mjs','audit_board_projection.mjs']) {
   const sql=readFileSync(new URL(file,import.meta.url),'utf8').match(/'SELECT DISTINCT weapon FROM state_catalog_publication_sources WHERE candidate=\?'/)?.[0].slice(1,-1);
   assert(sql,'capture the actual producer and independent audit reverse-lookup SQL');
   const reverse=()=>{
    db.prepare('SELECT 1 FROM prepared.entity_prepared_board_versions LIMIT 1').get();
    const plan=db.prepare('EXPLAIN QUERY PLAN '+sql).all('00000000');
    assert(plan.some(row=>/SEARCH .*prepared_board_candidates \(candidate=\?\)/.test(row.detail)),JSON.stringify(plan));return plan;
   };
   assert.deepEqual(db.prepare(sql).all('00000000').map(row=>row.weapon),['target']);
   const plan=reverse();records.db.exec('DROP INDEX prepared_board_candidates');assert.throws(reverse);
   new PreparedBoardInputs(records.db,contract);reverse();assert.equal(reader.sourceCount(),history+1);
   console.log(JSON.stringify({history,reverse_lookup:file,plan}));
  }
  const indexed=()=>{
   reader.raw.prepare('SELECT 1 FROM prepared.entity_prepared_board_versions LIMIT 1').get();
   const plan=reader.db.prepare('EXPLAIN QUERY PLAN '+PREPARED_WEAPON_ROWS_SQL).all('target',id,id,'','',0);
   assert(plan.some(row=>/SEARCH .*prepared_board_sources/.test(row.detail)),JSON.stringify(plan));
   assert(!plan.some(row=>/TEMP B-TREE|SCAN .*entity_prepared_board_versions/.test(row.detail)),JSON.stringify(plan));return plan;
  };
  indexed();assert.equal([...reader.weaponRows('target',id)].length,1);assert.equal(reader.sourceCount(),history+1);
  records.db.exec('DROP INDEX prepared_board_sources');assert.throws(indexed);assert.throws(publicationPlan);
  new PreparedBoardInputs(records.db,contract);indexed();
  console.log(JSON.stringify({history,plan:indexed(),ordinal_plan:ordinal,ordinal_queries:batchAllocations,local_sources:1}));db.close();records.close();
 }
 console.log('ok: actual prepared weapon pages use their indexed prefix at1k/10k history; removing the index fails; restoration preserves source count');
 console.log('ok: source order uses one bounded descending index lookup per batch; the measured MAX(kind) scan and missing ordinal index both fail');
 console.log('ok: actual producer/auditor candidate lookups use their prefix at1k/10k; a missing index fails and restoration preserves input counts');
 console.log('ok: actual prepared publication query is weapon-first with point measurements; removing either required index fails');
}finally{rmSync(root,{recursive:true,force:true});}
