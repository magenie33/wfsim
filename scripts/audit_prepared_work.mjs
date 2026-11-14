// SPDX-License-Identifier: AGPL-3.0-or-later
// An independent raw-input cursor reconstructs queue and lifecycle state before classifying the complete QID union.
import {DatabaseSync} from 'node:sqlite';
import {inflateRawSync,deflateRawSync} from 'node:zlib';
import {canon} from '../worker/task_contract.js';
import {classifyInventory} from './classify_compute_inventory.mjs';
import {readInventoryPage} from './inventory_projection.mjs';
import {fileHash} from './inventory_publication_io.mjs';
import {readFileSync,statSync} from 'node:fs';

export class PreparedWorkAudit {
 constructor(path,{databaseBytes=4*1024**3,scratchBytes=databaseBytes}={}) {
  this.scratchBytes=scratchBytes;
  this.db=new DatabaseSync(path);this.db.exec(`PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA cache_size=-16384;
   PRAGMA temp_store=FILE;PRAGMA mmap_size=0;PRAGMA wal_autocheckpoint=256;
   CREATE TABLE IF NOT EXISTS state_work_audit_cursor(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);
   CREATE TABLE IF NOT EXISTS state_work_audit_queue(identity TEXT,ruler TEXT,mode TEXT,PRIMARY KEY(identity,ruler,mode)) WITHOUT ROWID;
   CREATE TABLE IF NOT EXISTS state_work_audit_tasks(id TEXT PRIMARY KEY,body BLOB NOT NULL) WITHOUT ROWID;`);
  const pages=Math.floor(databaseBytes/this.db.prepare('PRAGMA page_size').get().page_size);
  if(!Number.isSafeInteger(pages)||pages<1||this.db.prepare('PRAGMA max_page_count='+pages).get().max_page_count!==pages)throw Error('invalid work audit database bound');
 }
 state(){return JSON.parse(this.db.prepare('SELECT body FROM state_work_audit_cursor WHERE id=1').get()?.body||'{"cut":0,"file":0,"offset":0,"rows":0}');}
 save(state){this.db.prepare('INSERT OR REPLACE INTO state_work_audit_cursor VALUES(1,?)').run(canon(state));}
 async prepare(reader,cut,{check=()=>{}}={}) {
  for(;;) {
   let state=this.state();if(state.complete>=cut.id)return;
   const id=state.cut>Number(state.complete||0)?state.cut:reader.next(Number(state.complete||0))?.id;
   if(!id||id>cut.id)throw Error('work audit input cut is missing');
   const held=reader.cut(id),capture=held.receipt.capture;
   if(state.cut!==id){state={cut:id,complete:state.complete||0,file:0,offset:0,rows:0};this.save(state);}
   if(capture.capture) {
    if(await fileHash(capture.capture)!==capture.manifest_sha)throw Error('work audit capture manifest changed');
    const manifest=JSON.parse(readFileSync(capture.capture,'utf8'));
    for(const file of manifest.files) {
     check();const stamp=statSync(file.path);
     if(await fileHash(file.path)!==file.sha256||statSync(file.path).mtimeMs!==stamp.mtimeMs)throw Error('work audit immutable input changed');
    }
   }
   const files=capture.bootstrap?[['queue',capture.raw.queue],['tasks',capture.raw.tasks]]:[['delta',capture.delta]];
   while(state.file<files.length) {
    check();const [kind,path]=files[state.file],page=await readInventoryPage(path,state.offset,256);
    const rows=kind==='delta'?page.rows:page.rows.map(row=>({kind,row}));
    this.db.exec('BEGIN IMMEDIATE');
    try {
     for(const {kind,row} of rows) {
      if(kind==='reset')this.db.prepare('DELETE FROM state_work_audit_queue WHERE identity=?').run(row.id);
      else if(kind==='queue')this.db.prepare('INSERT OR IGNORE INTO state_work_audit_queue VALUES(?,?,?)').run(row.build_id,row.ruler,row.mode);
      else if(kind==='tasks') {
       const body=deflateRawSync(canon(row.deleted?{id:row.id}:row),{level:1});
       if(capture.bootstrap&&this.db.prepare('SELECT 1 FROM state_work_audit_tasks WHERE id=?').get(row.id))throw Error('duplicate work audit lifecycle input');
       this.db.prepare('INSERT OR REPLACE INTO state_work_audit_tasks VALUES(?,?)').run(row.id,body);
      }
     }
     state={...state,offset:page.offset,rows:state.rows+page.rows.length};
     if(page.done) {
      if(capture.bootstrap&&state.rows!==capture.meta.counts[kind])throw Error('incomplete work audit raw input');
      state={...state,file:state.file+1,offset:0,rows:0};
     }
     this.save(state);this.db.exec('COMMIT');
    }catch(error){this.db.exec('ROLLBACK');throw error;}
   }
   this.save({...state,complete:id});
  }
 }
 async run(index,reader,cut,{check=()=>{},scratchRoot}={}) {
  await this.prepare(reader,cut,{check});
  const queue=this.db.prepare('SELECT identity build_id,ruler,mode FROM state_work_audit_queue ORDER BY identity,ruler,mode');
  const queueRows=function*(){for(const row of this.db.prepare('SELECT identity build_id,ruler,mode FROM state_work_audit_queue ORDER BY identity,ruler,mode').iterate())yield row;}.bind(this);
  const builds=function*(){for(const row of reader.rows('source',cut.id))yield {id:row.key,record:JSON.parse(reader.recordBody(row.value.raw_record_ref))};};
  const canonical=function*(){for(let after='';;) {
   const page=index.q('SELECT id,body FROM entity_catalog_candidates WHERE id>? ORDER BY id LIMIT 256').all(after);
   if(!page.length)return;
   for(const row of page){const d=JSON.parse(row.body);yield {id:row.id,record:JSON.parse(index.catalog.recordBody(d.record_ref))};}
   after=page.at(-1).id;
  }};
  const tasks=function*(){for(const row of this.db.prepare('SELECT body FROM state_work_audit_tasks ORDER BY id').iterate())yield JSON.parse(inflateRawSync(row.body));}.bind(this);
  const members=function*(){for(const row of index.q(`SELECT r.build_id,ru.name ruler,mo.name mode FROM relation_inventory_owner_members m
   JOIN entity_inventory_builds r ON r.id=m.candidate JOIN entity_inventory_symbols ru ON ru.id=m.ruler
   JOIN entity_inventory_symbols mo ON mo.id=m.mode WHERE m.queued=0`).iterate())yield row;};
  const queued=index.q(`SELECT 1 FROM relation_inventory_owners o JOIN relation_inventory_owner_members m ON m.owner=o.id
   WHERE o.owner=? AND m.ruler=(SELECT id FROM entity_inventory_symbols WHERE category='ruler' AND name=?)
   AND m.mode=(SELECT id FROM entity_inventory_symbols WHERE category='mode' AND name=?) AND m.queued=1 LIMIT 1`);
  const orphans=function*(){for(const row of queueRows())if(!queued.get(row.build_id,row.ruler,row.mode))yield row;};
  // Independently reconstructed pending input must equal the enumerator's compact queue in both directions.
  let expected=0;
  for(const row of queue.iterate()) {
   if(!index.q('SELECT 1 FROM state_catalog_pending_work WHERE identity=? AND ruler=? AND mode=?').get(row.build_id,row.ruler,row.mode))throw Error('work audit queue input differs');
   if(++expected%256===0)check();
  }
  if(index.q('SELECT count(*) n FROM state_catalog_pending_work').get().n!==expected)throw Error('work audit queue has extra input');
  const meta={engine:cut.receipt.engine,snapshot:cut.receipt.snapshot,started_at:cut.receipt.started_at,counts:{
   library:reader.raw.prepare('SELECT count(*) n FROM entity_catalog_sources').get().n,
   queue:expected,tasks:this.db.prepare('SELECT count(*) n FROM state_work_audit_tasks').get().n}};
  const plan=await classifyInventory({meta,builds:builds(),canonical:canonical(),tasks:tasks(),queue:queueRows(),
   missing:members(),orphans:orphans(),rulers:index.contract.rulers.map(row=>row.id),questions:true,check,scratchRoot,databaseBytes:this.scratchBytes});
  const summary=index.catalog.summary();
  if(summary.rows!==plan.rows||summary.queued!==plan.queued||index.catalog.digest()!==plan.question_digest)throw Error('work audit QID union differs');
  index.catalog.setMeta({...index.catalog.meta,complete:true,snapshot:meta.snapshot,started_at:meta.started_at,audit_digest:plan.question_digest});
  return {complete:true,plan,digest:plan.question_digest};
 }
 close(){this.db.close();}
}
