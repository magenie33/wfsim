// SPDX-License-Identifier: AGPL-3.0-or-later
// A protected-source replay qualifies each isolated consumer without a database epoch or remote publication.
import {existsSync,readFileSync,writeFileSync,mkdirSync,readdirSync,statSync} from 'node:fs';
import {join,resolve,basename} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {spawn} from 'node:child_process';
import {canon} from '../worker/task_contract.js';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {inspectNormalizedSourceBase,adoptPreparedSources} from './adopt_prepared_sources.mjs';
import {PreparedBoardProjection} from './prepared_board_projection.mjs';
import {PreparedWorkInventory} from './prepared_work_inventory.mjs';
import {PreparedWorkAudit} from './audit_prepared_work.mjs';
import {boardInputContract,boardConsumerContract} from './split_board_pipeline.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {BoardPipeline} from './board_pipeline.mjs';
import {inventoryDiskBudget} from './inventory_disk_budget.mjs';
import {saveInventoryState as save} from './inventory_pipeline.mjs';
import {fileHash} from './inventory_publication_io.mjs';
import {offlineCapacityGuard} from './offline_capacity_guard.mjs';

const GiB=1024**3,here=fileURLToPath(new URL('.',import.meta.url));
const hash=body=>createHash('sha256').update(body).digest('hex');
const load=path=>JSON.parse(readFileSync(path,'utf8'));
const bytes=paths=>[...new Set(paths)].reduce((n,path)=>n+statSync(path).size,0);
const command=(program,args,work)=>new Promise((accept,reject)=>{
 const child=spawn(program,args,{stdio:['ignore','inherit','inherit'],env:{...process.env,TMPDIR:join(work,'tmp')}});
 child.once('error',reject);child.once('exit',code=>code===0?accept():reject(Error('offline derived artifact failed: '+program)));
});
function allocation(total,protectedBytes,override,reserveBytes) {
 const usable=total-protectedBytes-reserveBytes;
 const result=override||Object.fromEntries(Object.entries({source:.4,inventory:.3,audit:.2,projection:.07,outbox:.03}).map(([key,share])=>[key,Math.floor(usable*share)]));
 if(Object.keys(result).sort().join(',')!=='audit,inventory,outbox,projection,source'||!Number.isSafeInteger(usable)||usable<5*1024**2||Object.values(result).some(value=>!Number.isSafeInteger(value)||value<1024**2)
  ||Object.values(result).reduce((a,b)=>a+b,0)>usable)throw Error('split replay database allocations exceed the explicit source and scratch budget');
 return result;
}
export async function replaySplitBoard({capture,bin,work,manifestSha,diskBudgetBytes,reuseSources,allocations,
 minimumFreeBytes=8*GiB,allocationPath,fault=async()=>{},stopAfterPasses=Infinity,report=body=>console.log(JSON.stringify(body))}={}) {
 capture=resolve(capture);bin=resolve(bin);work=resolve(work);
 if(!/^[a-f0-9]{64}$/.test(manifestSha||'')||!Number.isSafeInteger(diskBudgetBytes))throw Error('protected manifest SHA and explicit total disk bytes are required');
 if(work===capture||work.startsWith(capture+'/')||work===bin||work.startsWith(bin+'/'))throw Error('an isolated replay directory is required');
 mkdirSync(work,{recursive:true});const statePath=join(work,'split-replay.json');
 if(!existsSync(statePath)&&readdirSync(work).length)throw Error('new split replay directory must be empty');
 const manifestPath=join(capture,'capture.json');if(await fileHash(manifestPath)!==manifestSha)throw Error('protected capture manifest changed');
 const manifest=load(manifestPath),names=new Set(),files=[];
 for(const file of manifest.files) {
  const name=basename(file.path),path=join(capture,name);if(names.has(name))throw Error('ambiguous protected input filename');names.add(name);
  report({event:'split_replay',phase:'verify-input',name});
  if(await fileHash(path)!==file.sha256)throw Error('protected capture input changed: '+name);files.push({path,sha256:file.sha256});
 }
 const raw=Object.fromEntries(Object.entries(manifest.raw).map(([key,path])=>[key,join(capture,basename(path))]));
 if(Object.values(raw).some(path=>!names.has(basename(path))))throw Error('capture input is absent from its protected manifest');
 const meta=load(raw.meta),engine=readFileSync(join(bin,'ENGINE'),'utf8').trim();
 if(!manifest.bootstrap||meta.engine!==engine)throw Error('split replay requires the matching complete captured source baseline');
 const native=await openInventoryOperator(join(bin,'wfsim-board'));let records,board,inventory,audit,outbox;
 try {
  if(!['describe','facts','gate','render','floor','carry','state'].every(op=>native.contract.operators.includes(op)))throw Error('native projection operations unavailable');
  const base=reuseSources?await inspectNormalizedSourceBase(reuseSources,{engine,manifestSha,native}):null;
  const sources=[manifestPath,...files.map(file=>file.path),...(base?.files.map(file=>file.path)||[])];
  const quota=allocation(diskBudgetBytes,bytes(sources),allocations,minimumFreeBytes),disk=inventoryDiskBudget({work,sources,totalBytes:diskBudgetBytes,reserveBytes:minimumFreeBytes});
  const capacity=offlineCapacityGuard(allocationPath,work);capacity.check(true);
  const contract={...boardInputContract(engine,native),manifest_sha:manifestSha,...(base?{record_base:base,
   adoption_code:hash(readFileSync(new URL('adopt_prepared_sources.mjs',import.meta.url)))}:{})};
  const sourceId=hash(canon(contract));let state=existsSync(statePath)?load(statePath):null;
  if(state&&state.generation!==sourceId)throw Error('split replay normalization, native, protected input or source-reuse fence changed');
  if(!state)state={v:1,mode:'offline_split_replay',generation:sourceId,contract,engine,source_snapshot:meta.snapshot,source_started_at:meta.started_at,
   database_epoch:null,publication_remote:false,published:false,phase:'normalization',source_initialized:false,
   catalog:join(work,'inputs.sqlite'),inventory:join(work,'inventory.sqlite'),board:join(work,'board.sqlite'),audit:join(work,'inventory-audit.sqlite')};
  const referenced={bootstrap:true,raw,meta,files,...(base?{record_base:base}:{}),receipt:{engine,snapshot:meta.snapshot,started_at:meta.started_at,epoch:sourceId}};
  const capturePath=join(work,'source-reference.json'),captureText=JSON.stringify(referenced),captureSha=hash(captureText);
  save(capturePath,referenced);const cut={...referenced,capture:capturePath,manifest_sha:captureSha,receipt:{...referenced.receipt,manifest_sha:captureSha}};
  state.quota=quota;save(statePath,state);mkdirSync(join(work,'tmp'),{recursive:true});mkdirSync(join(work,'pub/data'),{recursive:true});
  const stateYaml=join(work,'pub/data/board_state.yaml');if(!existsSync(stateYaml))writeFileSync(stateYaml,'boards: {}\n');
  records=openCatalog(state.catalog,{engine,contract,generation:sourceId,create:!state.source_initialized,databaseBytes:quota.source});
  const inputs=new PreparedBoardInputs(records.db,contract);state.source_initialized=true;save(statePath,state);
  let passes=0,last=0;
  const progress=(phase,fields={})=>{
   if(Date.now()-last<1000&&phase!=='complete')return;last=Date.now();report({event:'split_replay',phase,...fields,memory:process.memoryUsage(),disk:disk.report(),native_pid:native.pid});
  };
  const check=()=>{
   capacity.check();disk.check();if(Date.now()-last<1000)return;
   const cut=records.db.prepare('SELECT id,state,input_count,source_count,checkpoint FROM record_prepared_board_cuts ORDER BY id DESC LIMIT 1').get();
   progress(state.phase,{source_cut:cut?.id??null,source_state:cut?.state,input_count:cut?.input_count,source_count:cut?.source_count,
    source_checkpoint:cut?.checkpoint?JSON.parse(cut.checkpoint):null,
    board_cursor:board?.consumer.state(),inventory_cursor:inventory?.consumer.state(),capacity:capacity.report()});
  };
  const stopped=()=>++passes>=stopAfterPasses;
  if(base) {
   let adopted;do{adopted=await adoptPreparedSources(inputs,cut,base,{check,fault});state.cut=adopted.cut;progress('adopt-source',adopted);if(adopted.pending&&stopped())return {...state,pending:true};}while(adopted.pending);
  }
  let normalized;do {
   normalized=await prepareBoardCut(inputs,records,cut,native.call,{check,fault});state.cut=normalized.cut;progress('normalization',normalized);
   if(normalized.pending&&stopped())return {...state,pending:true};
  }while(normalized.pending);
  state.phase='projection';save(statePath,state);await fault('after-sealed-source',{cut:state.cut});
  outbox=new BoardPipeline(work,{maxBytes:quota.outbox});
  board=new PreparedBoardProjection(state.board,state.catalog,boardConsumerContract('projection',state),native.contract,work,
   {engine,generation:sourceId,create:!state.board_initialized,databaseBytes:quota.projection,fault});
  state.board_initialized=true;save(statePath,state);
  let projected;do {
   projected=await board.step(native.call,{budgetMs:1000,check,enqueue:async({epoch,receipt})=>{
    const receiptPath=join(work,'board-input-receipt.json');save(receiptPath,receipt);
    await command(process.execPath,[join(here,'board_heads.mjs'),join(work,'verified')],work);
    await command('python3',[join(here,'board_meta.py'),join(work,'verified'),stateYaml,join(work,'verified/meta.json'),receiptPath],work);
    return outbox.enqueue({epoch,receipt,boardDir:join(work,'verified')});
   }});progress('projection',projected);if(projected.pending&&stopped())return {...state,pending:true};
  }while(projected.pending);
  state.board_complete=true;state.board_audit=board.consumer.acknowledgement();inputs.observeAcknowledgement('projection',state.board_audit);
  state.phase='inventory';save(statePath,state);await fault('after-independent-board-package',{cut:state.cut});
  inventory=new PreparedWorkInventory(state.inventory,state.catalog,boardConsumerContract('inventory',state),native.contract,
   {engine,generation:sourceId,create:!state.inventory_initialized,databaseBytes:quota.inventory});
  const auditStateBytes=Math.floor(quota.audit/4);audit=new PreparedWorkAudit(state.audit,{databaseBytes:auditStateBytes,scratchBytes:quota.audit-auditStateBytes});
  state.inventory_initialized=true;save(statePath,state);
  let counted;do {
   counted=await inventory.step(native.call,{budgetMs:1000,check,fault,audit:(index,reader,held,options)=>audit.run(index,reader,held,{...options,scratchRoot:join(work,'tmp')})});
   progress('inventory',counted);if(counted.pending&&stopped())return {...state,pending:true};
  }while(counted.pending);
  state.inventory_complete=true;state.inventory_audit=inventory.consumer.acknowledgement();inputs.observeAcknowledgement('inventory',state.inventory_audit);
  state.retention=inputs.collect();capacity.check(true);disk.check(true);state.phase='complete';state.pending=false;
  state.disk=disk.report();state.capacity=capacity.report();state.memory=process.memoryUsage();save(statePath,state);progress('complete',state);
  return state;
 }catch(error){if(existsSync(statePath)){const state=load(statePath);save(join(work,'split-replay-failure.json'),{phase:state.phase,source_cut:state.cut??null,error:error.message,code:error.code||null,partial_inputs_retained:true});}throw error;}
 finally{board?.close();inventory?.close();audit?.close();outbox?.close();records?.close();await native.close();}
}
if(process.argv[1]&&pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
  const [capture,bin,work,manifestSha,total,reuseSources,minimumFree]=process.argv.slice(2);
 if(!capture||!bin||!work||!total)throw Error('usage: replay_split_board.mjs <capture> <bin> <empty-work> <manifest-sha> <total-disk-bytes> [readonly-normalized-source-db] [minimum-free-bytes]');
 await replaySplitBoard({capture,bin,work,manifestSha,diskBudgetBytes:Number(total),reuseSources,
  minimumFreeBytes:minimumFree?Number(minimumFree):8*GiB,allocationPath:process.env.WFSIM_CAPACITY_ALLOCATION});
}
