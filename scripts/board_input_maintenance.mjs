// SPDX-License-Identifier: AGPL-3.0-or-later
// Existing audited outboxes advance retention independently of admitting new normalization work.
import {DatabaseSync} from 'node:sqlite';
import {createHash} from 'node:crypto';
import {existsSync,lstatSync,readFileSync,realpathSync,statfsSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {canon} from '../worker/task_contract.js';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {validateWorkSpan} from './prepared_work_consumer.mjs';
import {InventoryControl} from './inventory_control.mjs';
import {SourceCaptureGC} from './source_capture_gc.mjs';
import {boardStorage} from './board_storage.mjs';

const sha=value=>createHash('sha256').update(value).digest('hex');
const methods=['transaction','cut','consumer','observeAcknowledgement','collect'];
const fail=(reason)=>{throw Object.assign(Error(reason),{code:'BOARD_MAINTENANCE_HOLD'});};
export function maintenanceIdentity(path,directory=false) {
 const p=resolve(path),s=lstatSync(p,{bigint:true});
 if(s.isSymbolicLink()||realpathSync(p)!==p||(directory?!s.isDirectory():!s.isFile()||s.nlink!==1n))fail('linked or unavailable maintenance state');
 return [String(s.dev),String(s.ino),String(s.uid)];
}
export function maintenanceSchema(db) {
 const bound=db.prepare(`SELECT count(*) n,coalesce(sum(octet_length(type)+octet_length(name)+octet_length(tbl_name)+octet_length(sql)),0) bytes
  FROM (SELECT type,name,tbl_name,sql FROM held.sqlite_schema WHERE sql IS NOT NULL LIMIT 257)`).get();
 if(bound.n>256||bound.bytes>256*1024)fail('maintenance schema exceeds metadata bound');
 const rows=db.prepare("SELECT type,name,tbl_name,sql FROM held.sqlite_schema WHERE sql IS NOT NULL ORDER BY type,name LIMIT 257").all();
 if(rows.length>256||Buffer.byteLength(canon(rows))>256*1024)fail('maintenance schema exceeds metadata bound');
 return sha(canon(rows));
}
export function maintenanceMethods() {
 return sha(canon([['maintenance',sha(readFileSync(new URL(import.meta.url)))],
  ...['prepared_board_inputs.mjs','prepared_work_consumer.mjs','source_capture_gc.mjs','inventory_control.mjs','../worker/task_contract.js']
   .map(path=>[path,sha(readFileSync(new URL(path,import.meta.url)))]),
  ...methods.map(name=>[name,String(PreparedBoardInputs.prototype[name])]),['validateWorkSpan',String(validateWorkSpan)],
  ['captureCollection',String(SourceCaptureGC.prototype.collect)],['controllerOpen',String(InventoryControl.openExisting)]]));
}
export function openExistingMaintenanceDatabase(path,{readOnly=false,beforeAttach=()=>{}}={}) {
 const identity=maintenanceIdentity(path),db=new DatabaseSync(':memory:');
 try {
  const uri=pathToFileURL(resolve(path));uri.searchParams.set('mode',readOnly?'ro':'rw');
  beforeAttach();db.prepare('ATTACH DATABASE ? AS held').run(uri.href);
  if(canon(maintenanceIdentity(path))!==canon(identity))fail('maintenance database changed while opening');
  db.exec('PRAGMA busy_timeout=0;PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF');
  return db;
 }catch(error){db.close();throw error;}
}
const boundedJSON=(path,limit)=>{
 if(lstatSync(path).size>limit)fail('maintenance JSON exceeds metadata bound');
 maintenanceIdentity(path);return JSON.parse(readFileSync(path,'utf8'));
};
const walBytes=path=>existsSync(path+'-wal')?lstatSync(path+'-wal').size:0;
function sourceAdapter(db,contract) {
 const value={db,contract};
 for(const name of methods)value[name]=PreparedBoardInputs.prototype[name].bind(value);
 const cut=value.cut;value.cut=id=>{
  const size=db.prepare(`SELECT coalesce(octet_length(receipt),0)+coalesce(octet_length(epoch),0)+coalesce(octet_length(state),0)
   +coalesce(octet_length(checkpoint),0)+coalesce(octet_length(sealed_digest),0) bytes FROM record_prepared_board_cuts WHERE id=?`).get(id)?.bytes;
  if(size>64*1024)fail('prepared cut exceeds maintenance metadata bound');return cut(id);
 };
 const consumer=value.consumer;value.consumer=name=>{
  const size=db.prepare('SELECT length(CAST(audit AS BLOB)) bytes FROM state_prepared_board_consumers WHERE name=?').get(name)?.bytes;
  if(size>64*1024)fail('stored acknowledgement exceeds metadata bound');return consumer(name);
 };
 return value;
}
function retentionBudget(inputs,limit) {
 const floor=inputs.db.prepare('SELECT min(acknowledged) floor FROM state_prepared_board_consumers').get().floor;
 if(!floor)return;
 const size=columns=>columns.map(name=>'coalesce(length(CAST('+name+' AS BLOB)),0)').join('+');
 const versions=inputs.db.prepare(`SELECT ${size(['body','key','kind','sha','identity','candidate','weapon','modes','ruler','mode','at','gate_ruler','scene'])} bytes
  FROM entity_prepared_board_versions WHERE last_cut IS NOT NULL AND last_cut<=? ORDER BY last_cut,id LIMIT ?`).all(floor,limit);
 const changes=inputs.db.prepare(`SELECT ${size(['versions','input_sha'])} bytes FROM record_prepared_board_changes
  WHERE cut<=? ORDER BY cut,ordinal LIMIT ?`).all(floor,limit);
 if([...versions,...changes].reduce((sum,row)=>sum+row.bytes,0)>64*1024)fail('retention batch requires a smaller qualified byte budget');
}
export async function runBoardInputMaintenance({work,owner,bindingFile=process.env.WFSIM_BOARD_MAINTENANCE_BINDING,
 paused=process.env.WFSIM_BOARD_MAINTENANCE_PAUSED==='1',check=()=>{},budgetMs=1000,fault=async()=>{}}) {
 if(paused)return {paused:true};
 if(!bindingFile)return {held:true,reason:'explicit existing-state binding required'};
 if(!Number.isSafeInteger(budgetMs)||budgetMs<1||budgetMs>1000)fail('invalid maintenance deadline');
 const until=performance.now()+budgetMs,root=join(boardStorage(work),'inventory'),binding=boundedJSON(bindingFile,64*1024);
 if(binding.v!==1||binding.work!==resolve(work)||binding.root?.path!==root||binding.methods!==maintenanceMethods())fail('maintenance binding differs');
 if(!owner?.isTransaction||owner.prepare("SELECT file FROM pragma_database_list WHERE name='main'").get()?.file!==join(resolve(work),'normalization-consumer-lock.sqlite'))
  fail('normalization owner lock is required');
 const statePath=join(root,'split-state.json'),state=boundedJSON(statePath,64*1024),opened=[];
 const {limits}=binding;
 if(!limits||![limits.reserveBytes,limits.walBytes,limits.maxAckBytes,limits.maxSpanCuts,limits.maxVersions].every(Number.isSafeInteger)
  ||limits.reserveBytes<8*1024**3||limits.walBytes<4096||limits.walBytes>4*1024**2
  ||limits.maxAckBytes<1||limits.maxAckBytes>64*1024||limits.maxSpanCuts<1||limits.maxSpanCuts>256
  ||limits.maxVersions<1||limits.maxVersions>256)fail('invalid explicit maintenance budgets');
 if(state.generation!==binding.generation||state.rebuild_task!==binding.task||state.engine!==binding.engine
  ||canon(state.contract)!==canon(binding.contract)||!Array.isArray(binding.consumers)||binding.consumers.length!==2
  ||canon(binding.consumers.map(x=>x.name).sort())!==canon(['inventory','projection']))fail('maintenance generation or consumers changed');
 const files=binding.files;
 if(!files||state.catalog!==join(root,'inputs-'+state.generation+'.sqlite')||state.inventory!==join(root,'work-'+state.generation+'.sqlite')
  ||state.board!==join(root,'board-'+state.generation+'.sqlite'))fail('maintenance database paths differ');
 const beforeWal={};let control,source;
 const guard=()=>{
  check();if(performance.now()>=until)fail('maintenance deadline reached');boardStorage(work);
  if(canon(maintenanceIdentity(root,true))!==canon(binding.root.identity))fail('maintenance root changed');
  const current=boundedJSON(statePath,64*1024);if(canon(current)!==canon(state))fail('normalization state changed');
  const fs=statfsSync(root);if(fs.bavail*fs.bsize<limits.reserveBytes+3*limits.walBytes)fail('maintenance reserve reached');
  for(const [path,before]of Object.entries(beforeWal)) {
   if(canon(maintenanceIdentity(path))!==canon(files[path]?.identity))fail('maintenance database identity changed');
   if(Math.max(0,walBytes(path)-before)>limits.walBytes||walBytes(path)+limits.walBytes>files[path].walCeilingBytes)fail('maintenance WAL budget reached');
  }
 };
 const open=(path,readOnly=false)=>{
  guard();const expected=files[path];if(!expected||canon(maintenanceIdentity(path))!==canon(expected.identity))fail('unbound maintenance database');
  const probe=openExistingMaintenanceDatabase(path,{readOnly:true});opened.push(probe);
  if(maintenanceSchema(probe)!==expected.schema)fail('maintenance database schema differs');
  if(!readOnly&&(!Number.isSafeInteger(expected.walCeilingBytes)||walBytes(path)+limits.walBytes>expected.walCeilingBytes))fail('maintenance WAL allocation is unqualified');
  const db=readOnly?probe:openExistingMaintenanceDatabase(path);if(!readOnly)opened.push(db);
  if(!readOnly)beforeWal[path]=walBytes(path);return db;
 };
 try {
  guard();control=InventoryControl.openExisting(root,path=>open(path));
  const selected=control.current({maxBytes:65536});if(selected?.id!==binding.task||selected.generation!==binding.generation||!['active','complete'].includes(selected.phase))fail('selected maintenance task changed');
  source=open(state.catalog);const held=source.prepare('SELECT CASE WHEN octet_length(body)<=65536 THEN body END body FROM state_prepared_board_generation WHERE id=1').get();
  if(held?.body!==canon(state.contract))fail('prepared source contract differs');
  const rows=source.prepare('SELECT name FROM state_prepared_board_consumers ORDER BY name LIMIT 3').all();
  if(canon(rows.map(row=>row.name))!==canon(['inventory','projection']))fail('registered maintenance consumers differ');
  const inputs=sourceAdapter(source,state.contract),result={acknowledged:[],retention:null,retiredFileBytes:0,filesystemBytesReclaimed:null};
  for(const row of rows)inputs.consumer(row.name);
  const gc=SourceCaptureGC.openExisting(control.db,root,state,{guard});
  const meta=source.prepare('SELECT CASE WHEN length(CAST(body AS BLOB))<=65536 THEN body END body FROM state_inventory_generation WHERE id=1').get();
  if(!meta?.body)fail('normalized source metadata is missing or oversized');
  const normalized=JSON.parse(meta.body),normalizedCut=Number(normalized.normalized_cut||0);
  if(normalized.v!==1||normalized.generation!==state.generation||normalized.engine!==state.engine||canon(normalized.contract)!==canon(state.contract))
   fail('normalized source binding differs');
  for(const consumer of binding.consumers) {
   const path=consumer.name==='inventory'?state.inventory:state.board;
   if(consumer.path!==path||canon(consumer.source)!==canon(state.contract))fail('historical consumer binding differs');
   const db=open(path,true);db.exec('BEGIN');let row;
   try {
    row=db.prepare(`SELECT CASE WHEN length(CAST(source AS BLOB))<=65536 THEN source END source,
     CASE WHEN length(CAST(contract AS BLOB))<=65536 THEN contract END contract,
     CASE WHEN length(CAST(audit AS BLOB))<=? THEN audit ELSE NULL END audit,length(CAST(audit AS BLOB)) bytes
     FROM state_prepared_input_consumption WHERE name=?`).get(limits.maxAckBytes,consumer.name);
   }finally{db.exec('ROLLBACK');}
   if(!row)fail('historical consumer outbox is missing');
   if(row.source!==canon(consumer.source)||row.contract!==canon(consumer.contract))fail('historical consumer contract differs');
   if(row.bytes>limits.maxAckBytes)fail('consumer acknowledgement exceeds metadata bound');
   if(!row.audit)continue;
   const audit=JSON.parse(row.audit),previous=inputs.consumer(consumer.name),prior=previous.acknowledged;
   if(audit.cut===prior){if(previous.audit!==canon(audit))fail('acknowledged consumer receipt changed');continue;}
   guard();source.exec('BEGIN IMMEDIATE');
   try {
    guard();
    if(consumer.name==='inventory') {
     if(!audit.input_span||audit.cut-audit.input_span.after>limits.maxSpanCuts)fail('work span requires a separately qualified bounded validation cursor');
     validateWorkSpan(inputs,audit.input_span,audit.cut,{after:prior});
    }
    inputs.observeAcknowledgement(consumer.name,audit);await fault('before-maintenance-ack-commit',{name:consumer.name});
    guard();source.exec('COMMIT');
   }catch(error){try{source.exec('ROLLBACK');}catch{}throw error;}
   await fault('after-maintenance-ack-commit',{name:consumer.name});result.acknowledged.push({name:consumer.name,cut:audit.cut});
  }
  guard();retentionBudget(inputs,limits.maxVersions);result.retention=inputs.collect(limits.maxVersions);guard();
  const pending=state.pending,references=pending?[pending.capture,pending.changes,pending.applies,pending.delta,...Object.values(pending.raw||{})].filter(x=>typeof x==='string'):[];
  result.captures=await gc.collect(inputs,{normalizedCut,references,maxCaptures:1,maxFiles:1,maxBytes:256*1024,
   maxManifestBytes:64*1024,maxDirectoryEntries:16,fault});
  result.retiredFileBytes=result.captures.bytes;
  return result;
 }finally{
  control?.close();
  for(const db of opened)try{db.close();}catch{}
 }
}
