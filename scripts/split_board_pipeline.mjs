// SPDX-License-Identifier: AGPL-3.0-or-later
// One source owner seals inputs; isolated consumers own enumeration, audited rendering and remote delivery.
import postgres from './pg/node_modules/postgres/src/index.js';
import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {existsSync,mkdirSync,readFileSync,statfsSync,writeFileSync} from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {spawn} from 'node:child_process';
import {canon} from '../worker/task_contract.js';
import {BOARD_KINDS} from '../worker/board_kinds.js';
import {inventoryWrite,INVENTORY_MAX_MS} from '../worker/task_inventory.js';
import {openCatalog} from './incremental_inventory.mjs';
import {InventoryControl,needsAction} from './inventory_control.mjs';
import {captureEpoch,acknowledgeInventory,saveInventoryState as save,hashInventoryFile as hash} from './inventory_pipeline.mjs';
import {readInventoryPage} from './inventory_projection.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {PreparedBoardProjection} from './prepared_board_projection.mjs';
import {PreparedWorkInventory} from './prepared_work_inventory.mjs';
import {PreparedWorkAudit} from './audit_prepared_work.mjs';
import {BoardPipeline} from './board_pipeline.mjs';

const load=path=>JSON.parse(readFileSync(path,'utf8')),here=dirname(fileURLToPath(import.meta.url));
const GiB=1024**3;
const sourceFiles=['prepared_board_inputs.mjs','prepare_board_cut.mjs','inventory_pipeline.mjs','inventory_projection.mjs',
 'capture_compute_inventory.mjs','incremental_inventory.mjs','inventory_operator.mjs','../worker/task_contract.js'];
const consumerFiles={inventory:['prepared_work_inventory.mjs','audit_prepared_work.mjs','inventory_relations.mjs','incremental_inventory.mjs','classify_compute_inventory.mjs'],
 projection:['prepared_board_projection.mjs','board_projection_state.mjs','audit_board_projection.mjs','inventory_publication.mjs','inventory_publication_io.mjs','board_heads.mjs','board_meta.py']};
const digest=files=>{const value=createHash('sha256');for(const file of files)value.update(readFileSync(new URL(file,import.meta.url)));return value.digest('hex');};
const bound=(name,fallback)=>{const value=Number(process.env[name]??fallback);if(!Number.isSafeInteger(value)||value<1024**2)throw Error('invalid '+name);return value;};
const limits=()=>({source:bound('WFSIM_BOARD_SOURCE_BYTES',8*GiB),inventory:bound('WFSIM_BOARD_WORK_BYTES',8*GiB),
 audit:bound('WFSIM_BOARD_AUDIT_BYTES',4*GiB),projection:bound('WFSIM_BOARD_PROJECTION_BYTES',4*GiB)});
export async function splitBoardContext(bin,operator) {
 const engine=readFileSync(join(bin,'ENGINE'),'utf8').trim(),source=readFileSync(join(bin,'VERSION'),'utf8').trim();
 const adapter=await hash(join(bin,'wfsim-board')),normalizer={v:1,adapter,code:digest(sourceFiles),operator:operator.contract};
 if(operator.adapter&&operator.adapter!==adapter)throw Object.assign(Error('loaded inventory operator binary changed'),{code:'INVENTORY_OPERATOR_CHANGED'});
 return {engine,source,contract:{v:1,algorithm:'board-inputs-v1',covers:BOARD_KINDS,engine,normalizer,record_storage:'interned-deflate-v1'}};
}
export function boardConsumerContract(stage,state) {
 return {v:1,stage,source_generation:state.generation,source:state.contract,record_storage:'interned-deflate-v1',
  code:digest([...consumerFiles[stage],'prepared_board_reader.mjs','prepared_board_consumer.mjs'])};
}
const disk=work=>{const s=statfsSync(work);if(s.bavail*s.bsize<8*GiB)throw Error('board pipeline reached the filesystem reserve');};
function acknowledgement(path,name,source) {
 if(!existsSync(path))return null;const db=new DatabaseSync(path,{readOnly:true});
 try {
  if(!db.prepare("SELECT 1 FROM sqlite_schema WHERE name='state_prepared_input_consumption'").get())return null;
  const row=db.prepare('SELECT source,audit FROM state_prepared_input_consumption WHERE name=?').get(name);
  if(row&&row.source!==canon(source))throw Error('consumer acknowledgement source changed');
  return row?.audit?JSON.parse(row.audit):null;
 }finally{db.close();}
}

export async function runPrepareBoardCycle({work,bin,sql,operator,budgetMs=1000,report=()=>{},fault=async()=>{}}) {
 const root=join(work,'inventory');mkdirSync(root,{recursive:true});disk(work);
 const control=new InventoryControl(root),path=join(root,'split-state.json'),context=await splitBoardContext(bin,operator),quota=limits();
 let records;
 try {
  const [{status}]=await sql.unsafe('SELECT inventory_status() status');
  let state=existsSync(path)?load(path):null;
  const same=state&&state.engine===context.engine&&canon(state.contract)===canon(context.contract)&&state.generation===status?.generation;
  if(control.current()?.phase==='requested'||!same||status?.engine!==context.engine||canon(status?.contract)!==canon(context.contract)) {
   const request={mode:'board',...context,status},begun=control.bound(request)||await control.begin(request,sql);
   state={...context,generation:begun.generation,rebuild_task:begun.task,pending:null,
    catalog:join(root,'inputs-'+begun.generation+'.sqlite'),inventory:join(root,'work-'+begun.generation+'.sqlite'),
    board:join(root,'board-'+begun.generation+'.sqlite'),audit:join(root,'work-audit-'+begun.generation+'.sqlite')};
   save(path,state);
  }
  control.checkTask(state);
  control.budget(control.current());
  const create=control.current()?.phase==='created';
  if(!existsSync(state.catalog)&&!create)needsAction('needs_restore','shared input database is missing; an explicit new rebuild is required');
  records=openCatalog(state.catalog,{...context,generation:state.generation,create,databaseBytes:quota.source});
  const inputs=new PreparedBoardInputs(records.db,state.contract);
  if(create)control.phase(state.rebuild_task,'active',state.generation);
  for(const [name,file] of [['inventory',state.inventory],['projection',state.board]]) {
   const ack=acknowledgement(file,name,state.contract);if(ack)inputs.observeAcknowledgement(name,ack);
  }
  inputs.collect();
  const complete=records.db.prepare('SELECT min(acknowledged) n FROM state_prepared_board_consumers').get().n;
  if(complete)control.complete(state);
  if(!state.pending&&state.normalized_at&&Date.now()-Date.parse(state.normalized_at)<60000)return {pending:false,state};
  if(!state.pending) {
   const capture=()=>captureEpoch({sql,catalog:records,generation:state.generation,dir:join(root,'capture-'+Date.now()),
    bootstrap:!records.meta.normalized_cut,relational:true,recordArray:false,budgetCheck:()=>control.checkTask(state),report});
   if(!records.meta.normalized_cut)await control.capture(state,capture,()=>save(path,state));
   else {state.pending=await capture();save(path,state);}
  }
  const cut=state.pending;await control.verifyCapture(cut,state);
  const [{receipt}]=await sql.unsafe('SELECT inventory_epoch_status($1::text) receipt',[cut.receipt.epoch]);
  if(canon(receipt)!==canon(cut.receipt))throw Error('sealed source receipt changed');
  records.beginEpoch(cut.receipt);
  const result=await prepareBoardCut(inputs,records,cut,operator.call,{budgetMs,check:()=>{disk(work);control.checkTask(state);},fault});
  if(result.pending)return result;
  // Source invalidations are acknowledged after their durable handoff, independently of both consumers.
  let offset=records.meta.source_ack?.epoch===cut.receipt.epoch?records.meta.source_ack.offset:0;
  const until=performance.now()+budgetMs;
  do {
   const groups=[];let next=offset,done=false,bytes=0;
   while(groups.length<128) {
    const page=await readInventoryPage(cut.changes,next,256),size=Buffer.byteLength(JSON.stringify(page.rows));
    if(groups.length&&bytes+size>4*1024**2)break;
    if(page.rows.length){groups.push(page.rows);bytes+=size;}
    next=page.offset;done=page.done;if(done)break;
   }
   await acknowledgeInventory(sql,records,state.generation,groups);
   await fault('after-source-ack-before-cursor',{cut:result.cut});
   offset=next;records.setMeta({...records.meta,source_ack:{epoch:cut.receipt.epoch,offset}});
   if(done)break;if(performance.now()>until)return {pending:true,stage:'acknowledging'};
  }while(true);
  if(records.meta.epoch.tokens_processed!==cut.receipt.change_count)throw Error('source acknowledgement count differs');
  records.setMeta({...records.meta,normalized_cut:result.cut,epoch:{...records.meta.epoch,phase:'complete'}});
  state.pending=null;state.normalized_at=new Date().toISOString();save(path,state);
  return {pending:false,cut:result.cut,state};
 }finally{records?.close();control.close();}
}

function command(binary,args) {
 const process=spawn(binary,args,{stdio:['ignore','inherit','inherit']});
 const done=new Promise((resolve,reject)=>{process.once('error',reject);process.once('exit',(code,signal)=>code===0?resolve():reject(Error(binary+' exited '+(signal||code))));});
 return {process,done};
}
export async function publishPreparedInventory(sql,inventory,state,source,report) {
 const audit=inventory.consumer.acknowledgement();if(!audit)return;
 inventory.catalog.db.exec('CREATE TABLE IF NOT EXISTS state_work_inventory_delivery(cut INTEGER PRIMARY KEY,body TEXT NOT NULL)');
 if(inventory.catalog.db.prepare('SELECT 1 FROM state_work_inventory_delivery WHERE cut=?').get(audit.cut))return;
 let receipt;
 if(Date.now()-Date.parse(audit.plan.started_at)>=INVENTORY_MAX_MS)receipt={expired:true,started_at:audit.plan.started_at};
 else {
  const body=JSON.parse(inventoryWrite({...audit.plan,source}).params[1]),cut=inventory.reader.cut(audit.cut);
  try {const [row]=await sql.unsafe('SELECT inventory_publish_cut($1::text,$2::text) published',
   [cut.receipt.epoch,JSON.stringify({...body,generation:state.generation,contract:state.contract})]);receipt=row.published;}
  catch(error) {if(error.code==='P0001'&&['inventory cut expired','a newer inventory cut is already published'].includes(error.message))receipt={expired:true,reason:error.message};else throw error;}
 }
 inventory.catalog.db.prepare('INSERT INTO state_work_inventory_delivery VALUES(?,?)').run(audit.cut,canon(receipt));report({cut:audit.cut,delivery:receipt});
}
export async function serveBoardStage(stage,work,bin) {
 if(['normalization','inventory'].includes(stage)&&!process.env.PG_URL)throw Error('PG_URL required');
 const pipeline=new BoardPipeline(work),quota=limits(),statePath=join(work,'inventory/split-state.json');
 const sql=['normalization','inventory'].includes(stage)?postgres(process.env.PG_URL,{max:1,fetch_types:false,connect_timeout:15,onnotice:()=>{},
  connection:{application_name:'wfsim-board-'+stage,statement_timeout:5000,lock_timeout:500}}):null;
 const lock=new DatabaseSync(join(work,stage+'-consumer-lock.sqlite'));let operator,consumer,audit,selected,stopping=false;
 lock.exec('PRAGMA busy_timeout=0;BEGIN IMMEDIATE');
 const stop=()=>{stopping=true;};process.on('SIGTERM',stop);process.on('SIGINT',stop);
 const report=body=>pipeline.progress(stage,body);
 try {
  while(!stopping) {
   let pending=false;
   try {
    if(pipeline.paused(stage)){report({paused:true});}
    else if(stage==='normalization') {
     operator||=await openInventoryOperator(join(bin,'wfsim-board'));
     const result=await runPrepareBoardCycle({work,bin,sql,operator,report:(step,fields)=>report({step,...fields})});pending=result.pending;report(result);
    } else if(existsSync(statePath)) {
     const state=load(statePath);
     if(existsSync(state.catalog)) {
      operator||=await openInventoryOperator(join(bin,'wfsim-board'));
      const context=await splitBoardContext(bin,operator);
      if(canon(state.contract)!==canon(context.contract))throw Error('consumer source generation differs from installed normalizer');
      if(selected!==state.generation) {
       consumer?.close();audit?.close();consumer=null;audit=null;
       const contract=boardConsumerContract(stage,state),file=stage==='inventory'?state.inventory:state.board,marker=file+'.initialized';
       if(!existsSync(file)&&existsSync(marker))needsAction('needs_restore','consumer database is missing; its cursor cannot restart silently');
       consumer=stage==='inventory'?new PreparedWorkInventory(file,state.catalog,contract,operator.contract,{...context,generation:state.generation,
        create:!existsSync(marker),databaseBytes:quota.inventory}):new PreparedBoardProjection(file,state.catalog,contract,operator.contract,work,
        {...context,generation:state.generation,create:!existsSync(marker),databaseBytes:quota.projection});
       if(stage==='inventory')audit=new PreparedWorkAudit(state.audit,{databaseBytes:quota.audit});
       writeFileSync(marker,canon({generation:state.generation,contract}));selected=state.generation;
      }
      const check=()=>{disk(work);if(stopping||pipeline.paused(stage))throw Error(stage+' paused during its bounded pass');};
      if(stage==='inventory') {
       await publishPreparedInventory(sql,consumer,state,context.source,report);
       const result=await consumer.step(operator.call,{check,audit:(index,reader,cut,options)=>audit.run(index,reader,cut,{...options,scratchRoot:join(work,'tmp')})});
       pending=result.pending;report(result);await publishPreparedInventory(sql,consumer,state,context.source,report);
      } else {
       const result=await consumer.step(operator.call,{check,enqueue:async({epoch,receipt})=>{
        await command(process.execPath,[join(here,'board_heads.mjs'),join(work,'verified')]).done;
        const receiptPath=join(work,'board-input-receipt.json');save(receiptPath,receipt);
        await command('python3',[join(here,'board_meta.py'),join(work,'verified'),join(work,'pub/data/board_state.yaml'),join(work,'verified/meta.json'),receiptPath]).done;
        return pipeline.enqueue({epoch,receipt,boardDir:join(work,'verified')});
       }});pending=result.pending;report(result);
      }
     }
    }
   }catch(error){pending=true;report({failed:true,error:error.message,code:error.code||null,stage:error.stage||null});
    if(operator?.healthy===false||error.code==='INVENTORY_OPERATOR_CHANGED'){consumer?.close();audit?.close();consumer=null;audit=null;selected=null;await operator.close();operator=null;}}
   if(process.env.ONCE==='1')break;
   if(!stopping)await new Promise(resolve=>setTimeout(resolve,pending?1000:2000));
  }
 }finally{process.off('SIGTERM',stop);process.off('SIGINT',stop);consumer?.close();audit?.close();await operator?.close();await sql?.end({timeout:5});pipeline.close();lock.exec('ROLLBACK');lock.close();}
}
export async function serveSplitBoardPipeline(work,bin) {
 if(!process.env.PG_URL)throw Error('PG_URL required');
 mkdirSync(join(work,'tmp'),{recursive:true});mkdirSync(join(work,'pub/data'),{recursive:true});
 if(!existsSync(join(work,'pub/data/board_state.yaml')))writeFileSync(join(work,'pub/data/board_state.yaml'),'boards: {}\n');
 const children=new Set();let stopping=false;
 const stop=()=>{stopping=true;for(const child of children)child.process.kill('SIGTERM');};
 process.on('SIGTERM',stop);process.on('SIGINT',stop);
 try {
  if(process.env.ONCE==='1') {
   for(const stage of ['normalization','inventory','projection'])await serveBoardStage(stage,work,bin);
   return;
  }
  const running=new Map();
  while(!stopping) {
   for(const stage of ['normalization','inventory','projection','publication'])if(!running.has(stage)) {
    const args=stage==='publication'?['--max-old-space-size=64',join(here,'board_pipeline.mjs'),'serve',work]
     :['--max-old-space-size=128',fileURLToPath(import.meta.url),stage,work,bin];
    const child=command(process.execPath,args);children.add(child);running.set(stage,child);
    child.done.catch(error=>console.error(error.message)).finally(()=>{running.delete(stage);children.delete(child);});
   }
   await new Promise(resolve=>setTimeout(resolve,1000));
  }
 }finally{stop();await Promise.allSettled([...children].map(child=>child.done));process.off('SIGTERM',stop);process.off('SIGINT',stop);}
}
if(process.argv[1]&&pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
 const [stage,work,bin]=process.argv.slice(2);if(!['normalization','inventory','projection'].includes(stage)||!work||!bin)throw Error('usage: split_board_pipeline.mjs normalization|inventory|projection <work> <bin>');
 await serveBoardStage(stage,resolve(work),resolve(bin));
}
