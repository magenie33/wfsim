// SPDX-License-Identifier: AGPL-3.0-or-later
// Native rows from every ruler must fit a bounded scene pass without losing global per-ruler selection.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {openInventoryOperator} from './inventory_operator.mjs';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {PreparedBoardReader} from './prepared_board_reader.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {BoardProjectionAudit} from './audit_board_projection.mjs';
import {sqliteTransactionRecovery} from './sqlite_transaction_recovery.mjs';

const root=mkdtempSync(join(tmpdir(),'board-audit-scenes-')),binary=resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board');
const native=await openInventoryOperator(binary),engine='0123456789ab',contract={engine,normalizer:{v:1},record_storage:'interned-deflate-v1'};
const sourcePath=join(root,'source.sqlite'),records=openCatalog(sourcePath,{engine,contract,create:true}),inputs=new PreparedBoardInputs(records.db,contract);
let db,negative;
const rank=(id,n,max)=>n===max?id:id+'@'+n;
const seed=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[1].record,builds=[];
for(let n=0;n<300;n++)builds.push({id:'fixture-'+n,record:{...seed,mods:[rank('serration',n%11,10),rank('vital_sense',Math.floor(n/11)%6,5),
 rank('hellfire',Math.floor(n/66),5),'speed_trigger','galvanized_aptitude','hammer_shot','vigilante_fervor','magnetic_capacity']}});
const descriptions=[];for(let n=0;n<builds.length;n+=256)descriptions.push(...await native.call('describe',builds.slice(n,n+256)));
assert.equal(new Set(descriptions.map(row=>row.candidate)).size,builds.length);
const facts=descriptions.flatMap((d,n)=>native.contract.rulers.flatMap((r,i)=>d.modes.map(mode=>({identity:d.candidate,ruler:r.id,mode,
 score:n===0?500:n===1?100:n===2?99.99999:400+n%100+i*0.00001,measured_by:'verified:scene-fixture',
 started_at:'2026-11-09T00:00:00Z',finished_at:'2026-11-09T00:00:01Z',cost_seconds:1}))));
const raw={};for(const [kind,rows]of Object.entries({builds,facts,queue:[],tasks:[]})){raw[kind]=join(root,kind+'.ndjson');writeFileSync(raw[kind],rows.map(row=>JSON.stringify(row)+'\n').join(''));}
const work=join(root,'work');mkdirSync(join(work,'verified'),{recursive:true});mkdirSync(join(work,'pub/data'),{recursive:true});
writeFileSync(join(work,'pub/data/board_state.yaml'),'boards: {}\n');
function open(path,fault) {
 const db=sqliteTransactionRecovery(new DatabaseSync(path));db.exec('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA temp_store=FILE;PRAGMA cache_size=-4096');
 const reader=new PreparedBoardReader(db,sourcePath);reader.bindViews(1);
 const audit=new BoardProjectionAudit(db,reader,work,{fault});assert.equal(db.prepare('PRAGMA max_page_count=2048').get().max_page_count,2048);
 return {db,audit};
}
try {
 const capture={bootstrap:true,raw,meta:{counts:{library:builds.length,facts:facts.length,queue:0,tasks:0}},
 receipt:{engine,epoch:'scenes',snapshot:'frozen-fixture',started_at:'2026-11-09T00:00:00Z'}};
 let prepared;do{prepared=await prepareBoardCut(inputs,records,capture,native.call);}while(prepared.pending);
 execFileSync(binary,['--project',join(work,'verified'),'--facts-in',raw.facts,'--state',join(work,'pub/data/board_state.yaml')],
  {input:JSON.stringify(builds.map(row=>row.record)),stdio:['pipe','ignore','pipe'],timeout:60000});
 const cut=inputs.cut(1),path=join(root,'audit.sqlite');let audit;({db,audit}=open(path,async stage=>{if(stage==='after-board-audit-scene-receipt')throw Error('scene receipt crash');}));
 await assert.rejects(audit.run(cut,native.call,native.contract),/scene receipt crash/);
 const completed=db.prepare('SELECT scene FROM record_board_audit_scenes').all();assert.equal(completed.length,1);
 const heldScene=completed[0].scene,heldRulers=new Set(native.contract.rulers.filter(row=>row.scene===heldScene).map(row=>row.id));
 assert.equal(inputs.consumer('projection').acknowledged,0);db.close();({db,audit}=open(path));
 const proof=await audit.run(cut,async(op,items)=>{
  if(op==='render')assert(items.every(item=>!heldRulers.has(item.benchmark)),'completed scene was rendered again');
  return native.call(op,items);
 },native.contract);
 assert.equal(proof.complete,true);const peakBytes=db.prepare('PRAGMA main.page_count').get().page_count*4096;assert(peakBytes<=8*1024**2);
 const row=db.prepare('SELECT * FROM record_board_audit_scenes LIMIT 1').get();db.prepare('DELETE FROM record_board_audit_scenes WHERE cut=? AND weapon=? AND scene=?').run(row.cut,row.weapon,row.scene);
 await assert.rejects(audit.finish(cut,native.contract,{sources:proof.sources,rendered:proof.rendered}),/scene audit is incomplete/);
 assert.equal(inputs.consumer('projection').acknowledged,0,'incomplete audit advanced source acknowledgement');
 negative=open(join(root,'unpartitioned.sqlite'));
 const unpartitioned={...native.contract,rulers:native.contract.rulers.map(value=>({...value,scene:''}))};
 await assert.rejects(negative.audit.reconstructScene(descriptions[0].bucket[0],'',cut,native.call,unpartitioned),error=>error.errcode===13);
 console.log(JSON.stringify({check:'board audit scenes',status:'passed',sources:builds.length,facts:facts.length,sqlite_limit_bytes:8*1024**2,peak_bytes:peakBytes,
  native_all_scene_files_index_state_exact:true,durable_scene_resume:true,missing_scene_rejected:true,unpartitioned_full_negative:true,production_access:false}));
}finally{negative?.db.close();db?.close();records.close();await native.close();rmSync(root,{recursive:true,force:true});}
