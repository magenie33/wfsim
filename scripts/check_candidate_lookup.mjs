// SPDX-License-Identifier: AGPL-3.0-or-later
// Candidate-first source and fact lookups stay indexed after history grows and an existing catalog upgrades.
import assert from 'node:assert/strict';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {openCatalog} from './incremental_inventory.mjs';
import {InventoryRelations} from './inventory_relations.mjs';
import {canon} from '../worker/task_contract.js';

const root=mkdtempSync(join(tmpdir(),'candidate-lookup-')),contract={v:1,entry_share:.2,rulers:[{id:'r',scene:''}]};
const operate=async(op,items)=>op==='describe'?items.map(({id,record})=>({id,candidate:record.input,bucket:['fixture',false],
 admit:'0000000000000001',default:null,modes:['base'],record,publication:{identity:record.input},publication_modes:[{id:'base',mode:''}]}))
 :items.map(row=>({...row,gate_ruler:'r',scene:''}));
try {
 for(const size of [1000,10000]) {
  const path=join(root,size+'.sqlite');let catalog=openCatalog(path,{engine:'0123456789ab',contract:{v:1},create:true}),index=new InventoryRelations(catalog,contract);
  const seen=[],q=index.q.bind(index);index.q=sql=>{seen.push(sql);return q(sql);};
  await index.sources([{id:'owner',record:{input:'candidate'}}],operate);
  index.transaction(()=>{for(let n=0;n<size;n++)index.q('INSERT INTO state_catalog_publication_sources VALUES(?,?,?,?,?,?)')
   .run('unrelated-'+n,n+2,'other-'+n,'fixture',1,'[]');});
  await index.facts([{identity:'candidate',ruler:'r',mode:'base',score:1,finished_at:'a'}],operate);
  const source=seen.find(sql=>/SELECT body FROM entity_catalog_sources WHERE candidate=/.test(sql));
  const fact=seen.find(sql=>/SELECT DISTINCT weapon FROM state_catalog_publication_sources WHERE candidate=/.test(sql));
  assert(source&&fact,'actual source/fact paths did not execute their reverse lookups');
  const indexed=()=>{
   const sourcePlan=catalog.db.prepare('EXPLAIN QUERY PLAN '+source).all('candidate'),factPlan=catalog.db.prepare('EXPLAIN QUERY PLAN '+fact).all('candidate');
   assert(sourcePlan.some(row=>/SEARCH .*rel_sources_candidate/.test(row.detail)),canon(sourcePlan));
   assert(factPlan.some(row=>/SEARCH .*rel_publication_sources_candidate/.test(row.detail)),canon(factPlan));
   assert(!factPlan.some(row=>/SCAN state_catalog_publication_sources/.test(row.detail)),canon(factPlan));
   return {source:sourcePlan,fact:factPlan};
  };
  indexed();catalog.setMeta({...catalog.meta,input:{file:1,offset:123,count:7}});
  const snapshot=()=>canon({meta:catalog.meta,sources:index.q('SELECT * FROM entity_catalog_sources ORDER BY seq').all(),
   publication:index.q('SELECT * FROM state_catalog_publication_sources ORDER BY id').all(),facts:index.q('SELECT * FROM state_catalog_measurements ORDER BY identity').all()});
  const before=snapshot();catalog.db.exec('DROP INDEX rel_publication_sources_candidate');assert.throws(indexed,/SCAN|SEARCH/);
  catalog.close();catalog=openCatalog(path);index=new InventoryRelations(catalog,contract);
  assert.equal(snapshot(),before,'index upgrade changed rows or a resumed cursor');indexed();
  console.log(JSON.stringify({history:size,...indexed(),cursor:catalog.meta.input,rows_preserved:true}));catalog.close();
 }
 console.log('ok: actual source/fact reverse lookups use candidate-prefix indexes at 1k/10k history; missing-index negative fails; existing-catalog upgrade preserves rows and cursor');
}finally{rmSync(root,{recursive:true,force:true});}
