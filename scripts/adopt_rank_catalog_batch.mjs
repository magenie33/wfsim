// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {DatabaseSync} from 'node:sqlite';
import {createReadStream,existsSync,lstatSync,mkdirSync,realpathSync,statSync} from 'node:fs';
import {basename,dirname,join,resolve,sep} from 'node:path';
import {setTimeout as pause} from 'node:timers/promises';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {promisify} from 'node:util';
import {guardRankSpace} from './board_rank_storage.mjs';
import {boardStorage} from './board_storage.mjs';
import {scanCatalogPage,startCatalogScan} from './board_rank_catalog_scan.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {initializeJournal,jsonFile,openProtectedSets,planBatch} from './board_catalog_batch_state.mjs';
import {preparePriority,priorityStep} from './board_catalog_priority.mjs';
import {batchStep,closeBatch} from './board_catalog_batch_step.mjs';
import {sampleCapacity} from './board_catalog_batch_capacity.mjs';
import {connectFileOwner} from './board_catalog_file_owner.mjs';
import {readerCut} from './board_catalog_cut_files.mjs';
import {captureSelected} from './board_catalog_readback.mjs';
import {coverage} from './rank_catalog_coverage.mjs';

const execute=promisify(execFile);
const ro={readOnly:true};
const official=['https://wfsim.app','https://cn.wfsim.app'];
function localOrOfficial(origins){
 const loopback=origins.every(o=>{const u=new URL(o);return u.origin===o&&u.protocol==='http:'&&['127.0.0.1','[::1]'].includes(u.hostname);});
 assert(origins.length===2&&new Set(origins).size===2&&
  (origins.every(o=>official.includes(o))||(loopback&&process.env.WFSIM_BOARD_STORAGE_REQUIRED!=='1')),'two independent public origins required');
}
function directory(path,readOnly=false){
 const exact=resolve(path),env=process.env.WFSIM_BOARD_WORK_ROOT?{...process.env,WFSIM_BOARD_WORK_ROOT:exact}:process.env;
 assert.equal(readOnly?boardStorage(exact,{env}):guardRankSpace(exact,{env}),exact);
 assert(realpathSync(exact)===exact,'linked operator directory');return exact;
}
function catalogFile(root,path){
 const file=resolve(path);
 assert(file.startsWith(root+sep)&&realpathSync(dirname(file))===dirname(file)&&
  (!existsSync(file)||lstatSync(file).isFile()),'linked or misplaced catalog file');
 return file;
}

export async function runBatch(config,action,{available,observe=()=>{},pageBudget=8}={}){
 const priority=['priority-run','priority-step','priority-status'].includes(action),status=action==='status'||action==='priority-status';
 assert((priority||['plan','step','run','status'].includes(action))&&Number.isInteger(pageBudget)&&pageBudget>0&&pageBudget<=32,'invalid batch action');
 const state=directory(config.state,status),path=catalogFile(state,join(state,priority?'priority.sqlite':'adoption.sqlite'));
 if(!priority&&action!=='plan')assert(existsSync(path),'plan the complete catalog first');
 const journal=new DatabaseSync(path,{readOnly:status});
 const opened=[],openRO=p=>{const db=new DatabaseSync(p,ro);opened.push(db);return db;};
 let native,sql,monitor,lock,sets,files,sampler,ongoing,capacityError;
 try{
  journal.exec('PRAGMA busy_timeout=5000');
  if(!status)journal.exec('PRAGMA journal_mode=WAL');
  if(action==='priority-status')return {scopes:journal.prepare('SELECT phase,count(*) n FROM priority_work GROUP BY phase').all(),
   queues:journal.prepare("SELECT priority,phase,count(*) n,min(arrival) oldest FROM priority_work WHERE phase NOT IN('selected','retired') GROUP BY priority,phase ORDER BY priority,phase").all(),
   held:journal.prepare("SELECT weapon,ruler,reason,due FROM priority_work WHERE phase='held' ORDER BY due LIMIT 32").all()};
  if(action==='status')return {run:journal.prepare('SELECT closed FROM run WHERE id=1').get()||null,
   scopes:journal.prepare('SELECT phase,count(*) n FROM scopes GROUP BY phase ORDER BY phase').all(),
   storage:journal.prepare('SELECT * FROM high_water WHERE id=1').get()||null};
  initializeJournal(journal);
  const stage=directory(config.stage),work=resolve(config.work);
  if(process.env.WFSIM_CATALOG_FILE_FD){
   files=await connectFileOwner(Number(process.env.WFSIM_CATALOG_FILE_FD),work,statSync(stage,{bigint:true}).dev);
  }else assert(guardRankSpace(config.work)===work&&realpathSync(work)===work,'reader work path changed');
  const target=join(state,'manifest');mkdirSync(target,{recursive:true,mode:0o700});directory(target);
  const origins=config.origins||official;localOrOfficial(origins);
  const captured=name=>openRO(catalogFile(stage,join(stage,name)));
  const capture=captured('catalog.sqlite'),archive=captured('archive.sqlite'),ledger=captured('qualification.sqlite');
  const read=k=>JSON.parse(capture.prepare('SELECT v FROM progress WHERE k=?').get(k)?.v||'null');
  const spec=read('spec'),cut=read('replay'),epoch=ledger.prepare('SELECT v FROM epoch WHERE id=1').get()?.v;
  assert(spec&&cut?.snapshot&&JSON.stringify([spec.source,spec.cluster,spec.native,spec.archive,cut.snapshot,cut.captured_at])===epoch,
   'real coherent cut and matching qualification ledger required');
  const budget=jsonFile(config.budget),final=priority?{scopes:budget.scope_count}:jsonFile(config.coverage).final;
  if(!priority)assert(final?.eligible&&final.scopes===final.qualified&&final.missing===0&&final.held===0&&final.orphans===0,
   'historical qualification is incomplete');
  const contract={source:spec.source,native:spec.native,archive:spec.archive,cluster:String(spec.cluster),epoch,
   snapshot:cut.snapshot,at:cut.captured_at,floor:String(read('floor')??read('start')?.version),scopes:final.scopes,
   work,volume:process.env.WFSIM_BOARD_DATA_UUID||null,origins,
   pins:[...(final.current_pins?[final.current_pins.sha256]:[]),...(final.additional_heads||[]).map(p=>p.pins)].sort()};
  const previous=journal.prepare('SELECT contract,closed FROM run WHERE id=1').get();
  if(previous)assert.equal(previous.contract,JSON.stringify(contract),'batch recovery boundary changed');
  if(priority)preparePriority(journal,contract);
  if(!priority&&action!=='plan')assert(previous,'no prepared catalog');
  if(previous?.closed&&action!=='plan')return {status:'closed'};
  assert(process.env.PG_URL,'PG_URL is required');
  const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
  const options={max:3,fetch_types:false,connection:{lock_timeout:2000,statement_timeout:10000}};
  if(config.pg_socket)options.path=config.pg_socket;
  const role=config.database_role||(files?'wfsim':null);
  monitor=role?postgres(process.env.PG_URL,{...options,max:1}):null;
  sql=postgres(process.env.PG_URL,{...options,connection:{...options.connection,...(role?{role}:{})}});
  const [cluster]=await (monitor||sql).unsafe('SELECT system_identifier::text cluster FROM pg_control_system()');
  const [live]=await sql.unsafe('SELECT source::text, current_user AS actor FROM state_board_input_source WHERE id=1');
  assert(cluster.cluster===contract.cluster&&live.source===contract.source&&(!role||live.actor===role),'live source, role or database cluster differs from the cut');
  lock=await sql.reserve();
  const [actor]=await lock.unsafe('SELECT pg_try_advisory_lock(hashtextextended($1,0)) owner',[contract.source+'/catalog-batch']);
  assert(actor?.owner,'another catalog adoption driver is active');
  native=await openInventoryOperator(config.native);assert.equal(native.adapter,contract.native,'Native digest changed');
  const scenes=new Map(native.contract.rulers.map(r=>[r.id,r.scene]));
  for(const p of priority?[]:config.pins){catalogFile(stage,p.members);catalogFile(stage,p.proof);}
  if(priority){
   const local=archive.prepare('SELECT archive,native FROM contract JOIN sealed USING(id,archive)').get();
   const [pin]=await sql.unsafe('SELECT archive FROM state_board_public_import WHERE id=1');
   assert(local?.archive===contract.archive&&local.native===native.adapter&&pin?.archive===contract.archive,'first public inventory changed');
  }else sets=openProtectedSets(config,final,native.adapter);
  const inventory=()=>jsonFile(config.reservations);
  const checkCapacity=async()=>{
   if(capacityError)throw capacityError;
   if(files&&!files.alive)throw Error('reader file supervisor disconnected');
   if(!ongoing)ongoing=sampleCapacity(journal,monitor||sql,{work:stage,budget,contract},available).finally(()=>{ongoing=null;});
   return ongoing;
  };
  const owned=files||{stage:async m=>{
   const stream=createReadStream(m.path);
   try{return await readerCut(work,{action:'stage',sha256:m.sha256,bytes:m.bytes},stream);}finally{stream.destroy();}
  },verify:async p=>{
   const verified=await readerCut(work,{action:'verify',name:basename(dirname(p.path)),sha256:p.cut,bytes:Number(p.bytes)});
   assert.equal(verified.path,p.path);
  }};
  const ctx={state,target,journal,capture,archive,ledger,sets,contract,scenes,native,work,sql,origins,budget,inventory,pageBudget,files:owned,
   capacity:checkCapacity,
   emit:async(weapon,ruler)=>{
    for(const file of ['inputs.partial','inputs.ndjson'])catalogFile(target,join(target,file));
    const helper=fileURLToPath(new URL('./board_catalog_manifest.py',import.meta.url));
    const out=await execute('python3',[helper,join(stage,'catalog.sqlite'),target,weapon,ruler],{maxBuffer:65536});
    const m=JSON.parse(out.stdout);assert.equal(m.path,join(target,'inputs.ndjson'));return m;
   }};
  if(action==='plan'){
   const selected=previous?[]:await captureSelected(sql,sets,contract);
   planBatch({...ctx,final,reservations:inventory().reservations,selected});
   await ctx.capacity();
   return {status:'planned',scopes:contract.scopes,protected:journal.prepare('SELECT count(*) n FROM pins').get().n};
  }
  await ctx.capacity();
  sampler=setInterval(()=>{if(!capacityError)checkCapacity().catch(e=>{capacityError=e;});},1000);
  let scan=await startCatalogScan(sql,contract,{resumeClosed:priority}),result;
  do{
   await ctx.capacity();
   if(!scan.complete){scan=await scanCatalogPage(sql,contract.source);result={status:'scanning',scope:scan.last_scope,key:scan.last_key};}
   else{
    result=priority?await priorityStep(ctx):await batchStep(ctx);
    if(result.status==='covered'){
     assert(coverage(capture,archive,ledger,null,{readOnly:true}).eligible,'historical coverage changed before closeout');
     result=await closeBatch(ctx);
    }
   }
   await ctx.capacity();observe(result);
   if(action==='step'||action==='priority-step'||result.status==='closed')return result;
   if(priority&&['idle','deferred','awaiting-reader'].includes(result.status))await pause(250);
   else if(result.status.startsWith('awaiting-'))await pause(3000);
  }while(true);
 }finally{
  clearInterval(sampler);if(ongoing)await ongoing.catch(()=>{});
  files?.close();lock?.release();if(sql)await sql.end();if(monitor)await monitor.end();
  if(native)await native.close();
  for(const s of sets?.values()||[])s.db.close();
  for(const db of opened)db.close();
  journal.close();
 }
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 const [action,file]=process.argv.slice(2);assert(file,'usage: adopt_rank_catalog_batch.mjs ACTION CONFIG.json');
 const result=await runBatch(jsonFile(file),action,{observe:r=>{if(action.endsWith('run')&&!['closed','idle','deferred','awaiting-reader'].includes(r.status))console.log(JSON.stringify(r));}});
 console.log(JSON.stringify(result));
}
