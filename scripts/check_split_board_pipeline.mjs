// SPDX-License-Identifier: AGPL-3.0-or-later
// Separate native consumers keep potential-work semantics while board delivery proceeds during inventory pauses.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,rmSync,readdirSync,existsSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {PreparedBoardProjection} from './prepared_board_projection.mjs';
import {PreparedWorkInventory} from './prepared_work_inventory.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {PreparedWorkAudit} from './audit_prepared_work.mjs';
import {InventoryRelations} from './inventory_relations.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {BoardPipeline} from './board_pipeline.mjs';
import {execFileSync} from 'node:child_process';

const root=mkdtempSync(join(tmpdir(),'split-board-')),native=await openInventoryOperator(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'));
const engine='0123456789ab',contract={v:1,record_storage:'interned-deflate-v1'},sourcePath=join(root,'source.sqlite');
const records=openCatalog(sourcePath,{engine,contract,create:true}),inputs=new PreparedBoardInputs(records.db,{engine,normalizer:{v:1}});
const raw=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url))).slice(0,3).map(row=>row.record);
raw.push({...raw[2],mods:raw[2].mods.map((id,n)=>n?id:'hellfire')});
const descriptions=await native.call('describe',raw.map((record,n)=>({id:'raw-'+n,record})));
const sources=raw.map((record,n)=>({id:'raw-'+n,record}));
for(let n=0;n<32;n++)sources.push({id:'unrelated-'+n,record:raw[1]});
const facts=descriptions.slice(0,2).flatMap((description,n)=>native.contract.rulers.flatMap(ruler=>description.modes.map(mode=>({
 identity:description.candidate,ruler:ruler.id,mode,score:123+n,measured_by:'verified:fixture',
 started_at:'2026-11-14T00:00:00.000Z',finished_at:'2026-11-14T00:00:01.000Z',cost_seconds:1,
}))));
const queue=[{build_id:descriptions[2].candidate,ruler:native.contract.rulers[0].id,mode:descriptions[2].modes[0]}];
const rawPaths={};for(const [kind,rows]of Object.entries({builds:sources,facts,queue,tasks:[]})) {
 rawPaths[kind]=join(root,kind+'.ndjson');writeFileSync(rawPaths[kind],rows.map(row=>JSON.stringify(row)+'\n').join(''));
}
const snapshot='frozen',started_at='2026-11-14T00:00:00.000Z';
const cut={bootstrap:true,raw:rawPaths,meta:{snapshot,started_at,engine,counts:{library:sources.length,facts:facts.length,queue:queue.length,tasks:0}},
 receipt:{epoch:'one',snapshot,started_at,engine}};
let board,inventory;const outbox=new BoardPipeline(root),workAudit=new PreparedWorkAudit(join(root,'audit.sqlite'));
const legacy='[{ "opaque": {"bytes":"preserved"} }]';
function fullOracle(currentSources,currentFacts) {
 const oracle=join(root,'native-oracle');rmSync(oracle,{recursive:true,force:true});mkdirSync(oracle);
 writeFileSync(join(oracle,descriptions[0].bucket[0]+'.json'),legacy);
 const path=join(root,'oracle-facts.ndjson');writeFileSync(path,currentFacts.map(row=>JSON.stringify(row)+'\n').join(''));
 execFileSync(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'),['--project',oracle,'--facts-in',path,'--state',join(root,'oracle-state.yaml')],
  {input:JSON.stringify(currentSources.map(row=>row.record)),stdio:['pipe','ignore','pipe'],timeout:60000});
 const files=folder=>{
  const result={};for(const dir of [...new Set(native.contract.rulers.map(row=>row.scene))])if(existsSync(join(folder,dir)))
   for(const name of readdirSync(join(folder,dir)).filter(name=>name.endsWith('.json')&&name!=='meta.json'))result[(dir?dir+'/':'')+name]=readFileSync(join(folder,dir,name),'utf8');
  return result;
 };
 assert.deepEqual(files(join(root,'verified')),files(oracle),'independent split output differs from full native bytes');
}
try {
 let lost=true;
 await assert.rejects(prepareBoardCut(inputs,records,cut,native.call,{fault:async()=>{if(lost){lost=false;throw Error('lost prepared response');}}}),/lost prepared response/);
 let prepared;do{prepared=await prepareBoardCut(inputs,records,cut,native.call);}while(prepared.pending);
 const id=prepared.cut;
 assert(inputs.cut(id).input_count<facts.length+sources.length+queue.length+2,'queue rows became individual asynchronous jobs');
 const boardPath=join(root,'board.sqlite'),workPath=join(root,'work.sqlite');
 board=new PreparedBoardProjection(boardPath,sourcePath,contract,native.contract,root,{engine,create:true});
 inventory=new PreparedWorkInventory(workPath,sourcePath,contract,native.contract,{engine,create:true});
 assert.throws(()=>board.records.db.exec("INSERT INTO prepared.entity_inventory_records VALUES('forbidden',X'00')"),/readonly/);
 mkdirSync(join(root,'pub/data'),{recursive:true});writeFileSync(join(root,'pub/data/board_state.yaml'),'boards: {}\n');
 mkdirSync(join(root,'verified'));writeFileSync(join(root,'verified',descriptions[0].bucket[0]+'.json'),legacy);
 let enqueued=0;
 const enqueue=async({epoch,receipt})=>{
  assert.equal(receipt.complete,true);enqueued++;
  writeFileSync(join(root,'verified/meta.json'),JSON.stringify({source:receipt}));
  return outbox.enqueue({epoch,receipt,boardDir:join(root,'verified')});
 };
 let result;for(let tries=0;;tries++){assert(tries<1000);result=await board.step(native.call,{budgetMs:20,enqueue});if(!result.pending)break;}
 assert.equal(inventory.consumer.state().acknowledged,0);assert.equal(inventory.consumer.state().cursor,0);
 assert.equal(inventory.catalog.meta.complete,false);assert.equal(result.audit.complete,true);assert.equal(enqueued,1);
 assert.equal(outbox.status().publication.pending,1,'audited board did not hand off while inventory was paused');
 fullOracle(sources,facts);
 for(let n=0;n<2;n++) {
  const rows=JSON.parse(readFileSync(join(root,'verified',descriptions[n].bucket[0]+'.json'),'utf8'));
  assert(rows.some(row=>Number(row.score)===123+n));
 }
 const path=join(root,'verified',descriptions[0].bucket[0]+'.json'),bytes=readFileSync(path,'utf8');
 const mutated=JSON.parse(bytes);mutated.find(row=>row.score!=null).mode='broken-mode';writeFileSync(path,JSON.stringify(mutated));
 await assert.rejects(board.audit.run(inputs.cut(id),native.call,native.contract),/weapon rows differ/);writeFileSync(path,bytes);
 const missing=JSON.parse(bytes);missing.splice(missing.findIndex(row=>row.score!=null),1);writeFileSync(path,JSON.stringify(missing));
 await assert.rejects(board.audit.run(inputs.cut(id),native.call,native.contract),/weapon rows differ/);writeFileSync(path,bytes);
 const source=records.db.prepare("SELECT id,body FROM entity_prepared_board_versions WHERE kind='source' LIMIT 1").get();
 records.db.prepare('UPDATE entity_prepared_board_versions SET body=? WHERE id=?').run(Buffer.from('broken-source'),source.id);
 await assert.rejects(board.audit.run(inputs.cut(id),native.call,native.contract));records.db.prepare('UPDATE entity_prepared_board_versions SET body=? WHERE id=?').run(source.body,source.id);
 assert.equal((await board.audit.run(inputs.cut(id),native.call,native.contract)).complete,true);
 board.close();board=new PreparedBoardProjection(boardPath,sourcePath,contract,native.contract,root,{engine});
 assert.equal((await board.step(native.call,{enqueue})).pending,false);assert.equal(enqueued,1,'restart re-enqueued an acknowledged cut');
 let checked=0;
 for(let tries=0;;tries++) {
  assert(tries<1000);const step=await inventory.step(native.call,{budgetMs:20,audit:async(index,reader,held)=>{
   checked++;return workAudit.run(index,reader,held,{scratchRoot:root});
  }});if(!step.pending)break;
 }
 assert.equal(checked,1);
 assert(inventory.catalog.summary().rows>queue.length,'potential-work union collapsed to registered queue rows');
 assert.equal(inventory.consumer.state().acknowledged,id);
 inputs.observeAcknowledgement('projection',board.consumer.acknowledgement());
 assert.equal(inputs.collect().floor,0,'fast consumer erased slow inputs');
 inputs.observeAcknowledgement('inventory',inventory.consumer.acknowledgement());assert.equal(inputs.collect().floor,id);
 const changed={...facts[0],score:300,finished_at:'2026-11-14T00:00:02.000Z'},nextQueue={...queue[0],ruler:native.contract.rulers[1].id};
 const alias={id:'new-alias',record:{...raw[0],note:'same calculation, another source'}};
 const delta=[{kind:'reset',row:{id:queue[0].build_id}},{kind:'queue',row:nextQueue},{kind:'facts',row:changed},{kind:'builds',row:alias}];
 const deltaPath=join(root,'delta.ndjson');writeFileSync(deltaPath,delta.map(row=>JSON.stringify(row)+'\n').join(''));
 const second={bootstrap:false,delta:deltaPath,receipt:{...cut.receipt,epoch:'two',snapshot:'second'}};
 let prepared2;do{prepared2=await prepareBoardCut(inputs,records,second,native.call);}while(prepared2.pending);
 const id2=prepared2.cut;
 for(let tries=0;;tries++) {
  assert(tries<1000);const step=await inventory.step(native.call,{budgetMs:20,audit:(index,reader,held)=>workAudit.run(index,reader,held,{scratchRoot:root})});
  if(!step.pending)break;
 }
 assert.equal(inventory.consumer.state().acknowledged,id2);assert.equal(board.consumer.state().acknowledged,id,'paused board held up inventory');
 // The established projection supplies a separate compatibility oracle for the complete membership tuples.
 const oracle=openCatalog(join(root,'oracle.sqlite'),{engine,contract,create:true});
 try {
  const index=new InventoryRelations(oracle,native.contract);
  await index.sources([...sources,alias],native.call);
  const currentFacts=facts.map(row=>row===facts[0]?changed:row);
  for(let n=0;n<currentFacts.length;n+=256)await index.facts(currentFacts.slice(n,n+256),native.call);
  index.queue([nextQueue]);while(index.pending())await index.step(native.call,{budgetMs:100});
  assert.equal(inventory.catalog.digest(),oracle.digest(),'independent inventory changed the full potential-QID union');
 }finally{oracle.close();}
 const localOperate=async(op,items)=>{
  if(op==='describe')assert(items.every(item=>item.record.weapon!==raw[1].weapon),'warm audit visited an unrelated weapon');
  return native.call(op,items);
 };
 const finishWarm=async()=>{for(let tries=0;;tries++){assert(tries<1000);const step=await board.step(localOperate,{budgetMs:20,enqueue});if(!step.pending)return step;}};
 const weaponRows=board.reader.weaponRows;board.reader.weaponRows=function*(_weapon,id){yield* this.rows('source',id);};
 await assert.rejects(finishWarm(),/unrelated weapon/);board.reader.weaponRows=weaponRows;
 const warm=await finishWarm();assert.equal(warm.audit.checked_versions,inputs.cut(id2).input_count);assert(warm.audit.rendered<facts.length);
 assert.equal(board.consumer.state().acknowledged,id2);assert.equal(enqueued,2);
 // Exhaust only the work database, then commit another source cut and publish it through the separate board database.
 const pages=inventory.catalog.db.prepare('PRAGMA page_count').get().page_count;
 inventory.catalog.db.prepare('PRAGMA max_page_count='+(pages+4)).get();
 inventory.catalog.db.exec('CREATE TABLE own_limit_probe(body BLOB)');
 assert.throws(()=>inventory.catalog.db.exec('INSERT INTO own_limit_probe VALUES(zeroblob(1048576))'),/full/i);
 const thirdPath=join(root,'third.ndjson');writeFileSync(thirdPath,JSON.stringify({kind:'facts',row:{...changed,score:400}})+'\n');
 let third;do{third=await prepareBoardCut(inputs,records,{bootstrap:false,delta:thirdPath,receipt:{...cut.receipt,epoch:'three',snapshot:'third'}},native.call);}while(third.pending);
 await assert.rejects(async()=>{for(let tries=0;;tries++){assert(tries<1000);if(!(await board.step(localOperate,{budgetMs:20,
  enqueue:async()=>{throw Error('outbox temporarily full');}})).pending)break;}},/outbox temporarily full/);
 const oneFact=await board.step(async()=>{throw Error('completed package retry repeated native work');},{enqueue});
 assert.equal(oneFact.audit.checked_versions,1);assert(oneFact.audit.rendered<facts.length);
 assert.equal(board.consumer.state().acknowledged,third.cut);assert.equal(enqueued,3,'work database exhaustion blocked board handoff');
 const fourthPath=join(root,'fourth.ndjson');writeFileSync(fourthPath,JSON.stringify({kind:'facts',row:{...changed,score:500}})+'\n');
 let fourth;do{fourth=await prepareBoardCut(inputs,records,{bootstrap:false,delta:fourthPath,receipt:{...cut.receipt,epoch:'four',snapshot:'fourth'}},native.call);}while(fourth.pending);
 const publicationDirty=board.index.publicationDirty;board.index.publicationDirty=()=>{};
 await assert.rejects(async()=>{for(let tries=0;;tries++){assert(tries<1000);if(!(await board.step(native.call,{budgetMs:20,enqueue})).pending)break;}},/weapon rows differ/);
 assert.equal(board.consumer.state().acknowledged,third.cut,'a missing producer dirty marker passed independent audit');
 board.index.publicationDirty=publicationDirty;board.index.publicationDirty(descriptions[0].bucket[0]);
 board.board.setMeta({...board.board.meta,publication:{...board.board.meta.publication,round:'repair-missing-dirty'}});
 for(let tries=0;;tries++){assert(tries<1000);if(!(await board.step(native.call,{budgetMs:20,enqueue})).pending)break;}
 assert.equal(board.consumer.state().acknowledged,fourth.cut);assert.equal(enqueued,4);
 const auditedBytes=readFileSync(path,'utf8'),tampered=JSON.parse(auditedBytes);tampered.find(row=>row.score!=null).score++;
 writeFileSync(path,JSON.stringify(tampered));
 await assert.rejects(outbox.enqueue({epoch:'tampered-package',receipt:board.consumer.acknowledgement(),boardDir:join(root,'verified')}),/independent board audit/);
 writeFileSync(path,auditedBytes);
 fullOracle([...sources,alias],facts.map(row=>row===facts[0]?{...changed,score:500}:row));
 const fifthPath=join(root,'fifth.ndjson'),retired={...changed,score:500,retired_at:'2026-11-14T00:01:00.000Z'};
 writeFileSync(fifthPath,[{kind:'facts',row:retired},{kind:'builds',row:{id:sources[0].id,deleted:true}}].map(row=>JSON.stringify(row)+'\n').join(''));
 let fifth;do{fifth=await prepareBoardCut(inputs,records,{bootstrap:false,delta:fifthPath,receipt:{...cut.receipt,epoch:'five',snapshot:'fifth'}},native.call);}while(fifth.pending);
 await finishWarm();assert.equal(board.consumer.state().acknowledged,fifth.cut);
 assert.equal(inputs.cut(fifth.cut).source_count,inputs.cut(fourth.cut).source_count-1,'source deletion advanced an incorrect source counter');
 fullOracle([...sources.slice(1),alias],facts.map(row=>row===facts[0]?retired:row));
 const moved={id:alias.id,record:raw[2]},sixthPath=join(root,'sixth.ndjson');
 writeFileSync(sixthPath,JSON.stringify({kind:'builds',row:moved})+'\n');
 let sixth;do{sixth=await prepareBoardCut(inputs,records,{bootstrap:false,delta:sixthPath,receipt:{...cut.receipt,epoch:'six',snapshot:'sixth'}},native.call);}while(sixth.pending);
 await finishWarm();assert.equal(board.consumer.state().acknowledged,sixth.cut);
 fullOracle([...sources.slice(1),moved],facts.map(row=>row===facts[0]?retired:row));
 assert.equal(inventory.catalog.db.prepare('SELECT count(*) n FROM entity_inventory_records').get().n,0,'work consumer copied shared bodies');
 assert.equal(board.records.db.prepare('SELECT count(*) n FROM entity_inventory_records').get().n,0,'board consumer copied shared bodies');
 console.log('ok: separate databases/cursors and full potential QID union; cold/native bytes plus warm aliases, retirement, deletion and weapon move match; unrelated-history, missing producer dirty and post-audit corruption negatives reject; pause, database FULL, restart and slowest-ACK retention pass');
}finally{board?.close();inventory?.close();workAudit.close();outbox.close();records.close();await native.close();rmSync(root,{recursive:true,force:true});}
