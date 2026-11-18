// SPDX-License-Identifier: AGPL-3.0-or-later
// Temporary SQLite fixtures cover maintenance ownership and committed ACKs without new source work.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,existsSync,rmSync,symlinkSync,realpathSync,renameSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {canon} from '../worker/task_contract.js';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {workSpanReceipt} from './prepared_work_consumer.mjs';
import {InventoryControl} from './inventory_control.mjs';
import {SourceCaptureGC} from './source_capture_gc.mjs';
import {BoardPipeline} from './board_pipeline.mjs';
import {serveBoardStage} from './split_board_pipeline.mjs';
import {runBoardInputMaintenance,maintenanceIdentity,maintenanceSchema,maintenanceMethods,openExistingMaintenanceDatabase} from './board_input_maintenance.mjs';

const work=realpathSync(mkdtempSync(join(tmpdir(),'board-maintenance-'))),root=join(work,'inventory');mkdirSync(root);
const generation='fixture-generation',task='fixture-task',engine='0123456789ab',contract={engine,normalizer:{v:1}};
const state={generation,rebuild_task:task,engine,contract,catalog:join(root,'inputs-'+generation+'.sqlite'),
 inventory:join(root,'work-'+generation+'.sqlite'),board:join(root,'board-'+generation+'.sqlite'),pending:null};
const statePath=join(root,'split-state.json'),bindingFile=join(work,'maintenance-binding.json');writeFileSync(statePath,canon(state));
const source=new DatabaseSync(state.catalog);source.exec('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL');
const inputs=new PreparedBoardInputs(source,contract),control=new InventoryControl(root);
control.db.prepare('INSERT INTO entity_inventory_rebuild_tasks VALUES(?,?)').run(task,canon({mode:'board',engine,contract}));
control.db.prepare('INSERT INTO state_inventory_rebuild_tasks VALUES(?,?,?,NULL)').run(task,'active',generation);
control.db.prepare('INSERT INTO state_inventory_rebuild_selection VALUES(1,?)').run(task);
new SourceCaptureGC(control.db,root,state);control.close();
source.exec('CREATE TABLE state_inventory_generation(id INTEGER PRIMARY KEY,body TEXT NOT NULL)');
source.prepare('INSERT INTO state_inventory_generation VALUES(1,?)').run(canon({v:1,generation,engine,contract,normalized_cut:1}));
const cut=inputs.begin({epoch:'fixture-epoch',engine,normalizer:contract.normalizer,snapshot:'fixture-snapshot',started_at:'2026-11-18T00:00:00Z'});
inputs.seal(cut,{inputCount:0,digest:createHash('sha256').update('').digest('hex')});
const audit={complete:true,cut,engine,snapshot:'fixture-snapshot',input_count:0,input_digest:inputs.cut(cut).sealed_digest};
const historical={v:1,source_generation:generation,code:'historical-audited-fixture'};
const consumers=[{name:'inventory',path:state.inventory,source:contract,contract:historical},
 {name:'projection',path:state.board,source:contract,contract:historical}];
const writeAudit=(name,value)=>{
 const file=consumers.find(c=>c.name===name).path,db=new DatabaseSync(file);
 try{db.exec('CREATE TABLE IF NOT EXISTS state_prepared_input_consumption(name TEXT PRIMARY KEY,source TEXT,contract TEXT,audit TEXT)');
  db.prepare('INSERT OR REPLACE INTO state_prepared_input_consumption VALUES(?,?,?,?)').run(name,canon(contract),canon(historical),value?canon(value):null);
 }finally{db.close();}
};
writeAudit('inventory',{...audit,input_span:workSpanReceipt(inputs,0,cut)});writeAudit('projection',audit);
let owner=new DatabaseSync(join(work,'normalization-consumer-lock.sqlite'));owner.exec('BEGIN IMMEDIATE');
const binding={v:1,work,root:{path:root,identity:maintenanceIdentity(root,true)},generation,task,engine,contract,
 methods:maintenanceMethods(),consumers,files:{},limits:{reserveBytes:8*1024**3,walBytes:4*1024**2,maxAckBytes:64*1024,maxSpanCuts:256,maxVersions:32}};
for(const file of [join(root,'controller-lock.sqlite'),join(root,'controller.sqlite'),state.catalog,state.inventory,state.board]) {
 const db=openExistingMaintenanceDatabase(file,{readOnly:true});
 try{binding.files[file]={identity:maintenanceIdentity(file),schema:maintenanceSchema(db),walCeilingBytes:32*1024**2};}finally{db.close();}
}
const save=()=>writeFileSync(bindingFile,canon(binding));save();
const run=options=>runBoardInputMaintenance({work,owner,bindingFile,...options});
const reset=()=>source.exec('UPDATE state_prepared_board_consumers SET cut=0,cursor=0,acknowledged=0,audit=NULL;UPDATE state_prepared_board_retention SET floor=0');
try {
 const missing=join(work,'missing.sqlite');assert.throws(()=>openExistingMaintenanceDatabase(missing));assert(!existsSync(missing));
 const raced=join(work,'raced.sqlite'),away=join(work,'raced-away.sqlite');new DatabaseSync(raced).close();
 assert.throws(()=>openExistingMaintenanceDatabase(raced,{beforeAttach:()=>renameSync(raced,away)}));assert(!existsSync(raced));assert(existsSync(away));
 const linked=join(work,'linked.sqlite');symlinkSync(state.catalog,linked);assert.throws(()=>openExistingMaintenanceDatabase(linked),/linked/);
 assert.deepEqual(await run({paused:true}),{paused:true});assert.equal(inputs.consumer('inventory').acknowledged,0);
 assert((await run({bindingFile:null})).held);assert.equal(inputs.consumer('inventory').acknowledged,0);
 await assert.rejects(run({owner:null}),/owner lock/);
 const controllerFile=join(root,'controller.sqlite'),metadataController=new DatabaseSync(controllerFile);
 const originalBody=metadataController.prepare('SELECT body FROM entity_inventory_rebuild_tasks WHERE id=?').get(task).body;
 const immutable=metadataController.prepare("SELECT sql FROM sqlite_schema WHERE name='inventory_task_immutable'").get().sql;
 const largeMetadata=async(field)=>{
  if(field==='body') {
   metadataController.exec('DROP TRIGGER inventory_task_immutable');
   metadataController.prepare('UPDATE entity_inventory_rebuild_tasks SET body=? WHERE id=?').run(canon({mode:'board',engine,contract,reason:'x'.repeat(65536)}),task);
   metadataController.exec(immutable);
  } else metadataController.prepare('UPDATE state_inventory_rebuild_tasks SET error=? WHERE task=?').run('x'.repeat(65536),task);
  try{await assert.rejects(run(),error=>error.code==='INVENTORY_ACTION_REQUIRED'&&error.stage==='maintenance_metadata_bound');}
  finally {
   if(field==='body'){metadataController.exec('DROP TRIGGER inventory_task_immutable');metadataController.prepare('UPDATE entity_inventory_rebuild_tasks SET body=? WHERE id=?').run(originalBody,task);metadataController.exec(immutable);}
   else metadataController.prepare('UPDATE state_inventory_rebuild_tasks SET error=NULL WHERE task=?').run(task);
  }
 };
 try{for(const field of process.env.WFSIM_MAINTENANCE_METADATA_FIELD?[process.env.WFSIM_MAINTENANCE_METADATA_FIELD]:['body','error'])await largeMetadata(field);}
 finally{metadataController.close();}
 const schemaBefore=canon(binding.files),result=await run();
 assert.deepEqual(result.acknowledged.map(x=>x.name),['inventory','projection']);assert.equal(inputs.consumer('inventory').acknowledged,cut);
 assert.equal(result.retention.floor,cut);assert.equal(result.retiredFileBytes,0);assert.equal(result.filesystemBytesReclaimed,null);
 assert.equal(canon(binding.files),schemaBefore);assert.deepEqual((await run()).acknowledged,[]);
 reset();binding.consumers[0].contract={code:'wrong-current-binary-contract'};save();
 await assert.rejects(run(),/historical consumer contract/);assert.equal(inputs.consumer('inventory').acknowledged,0);
 binding.consumers[0].contract=historical;save();
 const ceiling=binding.files[state.catalog].walCeilingBytes;binding.files[state.catalog].walCeilingBytes=0;save();
 await assert.rejects(run(),/WAL allocation/);assert.equal(inputs.consumer('inventory').acknowledged,0);
 binding.files[state.catalog].walCeilingBytes=ceiling;save();
 writeAudit('inventory',{...audit,input_span:workSpanReceipt(inputs,0,cut),extra:'x'.repeat(65536)});
 await assert.rejects(run(),/acknowledgement exceeds metadata bound/);assert.equal(inputs.consumer('inventory').acknowledged,0);
 binding.limits.maxSpanCuts=1;save();writeAudit('inventory',{...audit,cut:2,input_span:{after:0,last:2}});
 await assert.rejects(run(),/bounded validation cursor/);assert.equal(inputs.consumer('inventory').acknowledged,0);
 binding.limits.maxSpanCuts=256;save();
 const bad={...audit,input_span:{...workSpanReceipt(inputs,0,cut),digest:'0'.repeat(64)}};writeAudit('inventory',bad);
 await assert.rejects(run(),/span receipt differs/);assert.equal(inputs.consumer('inventory').acknowledged,0);
 writeAudit('inventory',{...audit,input_span:workSpanReceipt(inputs,0,cut)});
 await assert.rejects(run({fault:async stage=>{if(stage==='before-maintenance-ack-commit')throw Error('before commit');}}),/before commit/);
 assert.equal(inputs.consumer('inventory').acknowledged,0);
 await assert.rejects(run({fault:async stage=>{if(stage==='after-maintenance-ack-commit')throw Error('after commit');}}),/after commit/);
 assert.equal(inputs.consumer('inventory').acknowledged,cut);assert.equal(inputs.consumer('projection').acknowledged,0);
 assert.deepEqual((await run()).acknowledged.map(x=>x.name),['projection']);
 reset();let stop=false;
 await assert.rejects(run({check:()=>{if(stop)throw Error('stopped');},fault:async stage=>{if(stage==='before-maintenance-ack-commit')stop=true;}}),/stopped/);
 assert.equal(inputs.consumer('inventory').acknowledged,0);
 const held=new DatabaseSync(join(root,'controller-lock.sqlite'));held.exec('BEGIN IMMEDIATE');
 await assert.rejects(run(),/locked/);held.exec('ROLLBACK');held.close();
 binding.methods='0'.repeat(64);save();await assert.rejects(run(),/binding differs/);binding.methods=maintenanceMethods();save();
 binding.files[state.catalog].schema='0'.repeat(64);save();await assert.rejects(run(),/schema differs/);
 const db=openExistingMaintenanceDatabase(state.catalog,{readOnly:true});binding.files[state.catalog].schema=maintenanceSchema(db);db.close();save();
 assert.equal((await run()).acknowledged.length,2);
 reset();owner.exec('ROLLBACK');owner.close();owner=null;
 const pipeline=new BoardPipeline(work);pipeline.control('normalization',true);pipeline.close();
 const saved=Object.fromEntries(['PG_URL','ONCE','WFSIM_BOARD_MAINTENANCE_BINDING'].map(key=>[key,process.env[key]]));
 try {
  process.env.PG_URL='postgres://fixture:fixture@127.0.0.1:1/unused';process.env.ONCE='1';process.env.WFSIM_BOARD_MAINTENANCE_BINDING=bindingFile;
  await serveBoardStage('normalization',work,join(work,'absent-native-bin'));
 }finally{for(const [key,value]of Object.entries(saved))if(value===undefined)delete process.env[key];else process.env[key]=value;}
 assert.equal(inputs.consumer('inventory').acknowledged,cut,'paused actual service must import inventory ACK');
 assert.equal(inputs.consumer('projection').acknowledged,cut,'paused actual service must import projection ACK');
 assert(!existsSync(join(work,'absent-native-bin')));
 reset();const allPaused=new BoardPipeline(work);
 for(const stage of ['normalization','inventory','projection','publication'])allPaused.control(stage,true);allPaused.close();
 const shell=readFileSync(new URL('./live_board.sh',import.meta.url),'utf8'),command=shell.match(/^  node --max-old-space-size=192 .*$/m)?.[0].trim();
 assert(command,'actual production command is required');
 execFileSync('bash',['-c','exec '+command.replace(/^node /,'"$REAL_NODE" ')],{timeout:10000,encoding:'utf8',stdio:['ignore','pipe','pipe'],
  env:{PATH:process.env.PATH,REAL_NODE:process.execPath,HERE:new URL('.',import.meta.url).pathname.replace(/\/$/,''),WORK:work,BIN:join(work,'absent-native-bin'),
   ONCE:'1',PG_URL:'postgres://fixture:fixture@127.0.0.1:1/unused',WFSIM_BOARD_MAINTENANCE_BINDING:bindingFile}});
 assert.equal(inputs.consumer('inventory').acknowledged,cut,'production supervisor must schedule maintenance through stage pauses');
 assert.equal(inputs.consumer('projection').acknowledged,cut);
 assert.equal(source.prepare('SELECT count(*) n FROM record_prepared_board_cuts').get().n,1,'maintenance cannot create a source cut');
 assert.equal(source.prepare('SELECT count(*) n FROM state_prepared_board_consumers').get().n,2,'maintenance cannot register a consumer');
 const original=readFileSync(process.env.WFSIM_MAINTENANCE_CHECK_SOURCE||new URL('./split_board_pipeline.mjs',import.meta.url),'utf8');
 assert(original.indexOf('maintenance=await runBoardInputMaintenance')>=0,'maintenance must run independently of new normalization work');
 assert(original.indexOf('maintenance=await runBoardInputMaintenance')<original.indexOf('if(pipeline.paused(stage))'));
 assert(!original.slice(original.indexOf('maintenance=await runBoardInputMaintenance'),original.indexOf('if(pipeline.paused(stage))')).includes('openInventoryOperator'));
 console.log('board input maintenance PASS: actual production supervisor through stage pauses, existing-only open, owner/controller locks, historical contracts, bounded source ACK, rollback/restart, stop, schema/method fences and no physical-space claim');
}finally{source.close();if(owner){owner.exec('ROLLBACK');owner.close();}rmSync(work,{recursive:true,force:true});}
