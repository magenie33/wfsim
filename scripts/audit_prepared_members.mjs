// SPDX-License-Identifier: AGPL-3.0-or-later
// Expected memberships derive from sealed sources and measurements, independently of enumeration jobs.
import {createHash} from 'node:crypto';
import {canon} from '../worker/task_contract.js';

const bit=target=>BigInt.asIntN(64,1n<<BigInt(target));
const mask=hex=>BigInt.asIntN(64,BigInt('0x'+hex));
export class PreparedMembershipAudit {
 constructor(db) {
  this.db=db;
  db.exec(`CREATE TABLE IF NOT EXISTS state_work_member_cursor(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);
   CREATE TABLE IF NOT EXISTS state_work_member_dirty(id TEXT PRIMARY KEY,source INTEGER NOT NULL DEFAULT 0,fact INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;
   CREATE TABLE IF NOT EXISTS state_work_member_candidates(id TEXT PRIMARY KEY,bucket TEXT NOT NULL,admit INTEGER NOT NULL,
    default_id TEXT,modes TEXT NOT NULL,record_ref TEXT NOT NULL) WITHOUT ROWID;
   CREATE TABLE IF NOT EXISTS state_work_member_scores(identity TEXT,ruler TEXT,mode TEXT,score REAL,scene TEXT,bucket TEXT,retired INTEGER,gate_ruler TEXT,
    PRIMARY KEY(identity,ruler,mode)) WITHOUT ROWID;
   CREATE INDEX IF NOT EXISTS work_member_leaders ON state_work_member_scores(bucket,gate_ruler,mode,retired,score DESC,identity DESC);
   CREATE TABLE IF NOT EXISTS state_work_member_leaders(bucket TEXT,ruler TEXT,mode TEXT,target INTEGER,score REAL,
    PRIMARY KEY(bucket,ruler,mode,target)) WITHOUT ROWID;`);
 }
 q(sql){return this.db.prepare(sql);}
 state(){return JSON.parse(this.q('SELECT body FROM state_work_member_cursor WHERE id=1').get()?.body||'{"complete":0,"cut":0,"cursor":0}');}
 save(state){this.q('INSERT OR REPLACE INTO state_work_member_cursor VALUES(1,?)').run(canon(state));}
 transaction(fn){this.db.exec('BEGIN IMMEDIATE');try{const value=fn();this.db.exec('COMMIT');return value;}catch(error){this.db.exec('ROLLBACK');throw error;}}
 async prepare(reader,target,contract,check) {
  let state=this.state();if(state.complete===target.id)return;
  if(state.target&&state.target!==target.id)throw Error('membership audit snapshot changed');
  if(!state.target){state={...state,target:target.id,phase:'inputs'};this.save(state);}
  while(state.phase==='inputs') {
   check();
   const id=state.cut>state.complete?state.cut:reader.next(state.complete)?.id;
   if(!id||id>target.id)throw Error('membership audit sealed input is missing');
   if(id!==state.cut){state={...state,cut:id,cursor:0};this.save(state);}
   const cut=reader.cut(id),page=reader.readPage(id,state.cursor);
   this.transaction(()=>{
    for(const item of page) {
     if(item.kind==='source') {
      const old=reader.value('source',item.key,id-1)?.value?.candidate;
      for(const candidate of [old,item.value?.candidate])if(candidate)this.q(`INSERT INTO state_work_member_dirty(id,source) VALUES(?,1)
       ON CONFLICT(id) DO UPDATE SET source=1`).run(candidate);
     } else if(item.kind==='fact'||item.kind==='reset') {
      const identity=item.value?.identity||item.value?.id;
      if(!identity)throw Error('membership audit measurement identity is missing');
      this.q(`INSERT INTO state_work_member_dirty(id,fact) VALUES(?,1) ON CONFLICT(id) DO UPDATE SET fact=1`).run(identity);
     }
    }
    state={...state,cursor:page.length?page.at(-1).ordinal+1:state.cursor};
    if(state.cursor===cut.input_count)state=id===target.id?{...state,phase:'candidates',after:''}:{...state,complete:id};
    this.save(state);
   });
  }
  while(state.phase==='candidates') {
   check();const page=this.q('SELECT id FROM state_work_member_dirty WHERE source=1 AND id>? ORDER BY id LIMIT 256').all(state.after);
   this.transaction(()=>{
    for(const {id}of page) {
     let after=0,first=null,admit=0n;const modes={};
     for(;;) {
      const aliases=reader.db.prepare(`SELECT key,source_order FROM entity_prepared_board_versions WHERE kind='source' AND candidate=?
       AND source_order>? AND body IS NOT NULL AND first_cut<=? AND (last_cut IS NULL OR last_cut>?) ORDER BY source_order LIMIT 256`)
       .all(id,after,target.id,target.id);
      if(!aliases.length)break;
      for(const alias of aliases) {
       const value=reader.value('source',alias.key,target.id).value;first??=value;admit|=mask(value.admit);
       if(canon(value.bucket)!==canon(first.bucket)||value.default!==first.default)throw Error('membership audit candidate dependencies differ');
       for(const mode of value.modes)modes[mode]=BigInt.asUintN(64,mask(modes[mode]||'0')|mask(value.admit)).toString(16);
      }
      after=aliases.at(-1).source_order;
     }
     if(first)this.q('INSERT OR REPLACE INTO state_work_member_candidates VALUES(?,?,?,?,?,?)')
      .run(id,canon(first.bucket),admit,first.default,canon(modes),first.record_ref);
     else this.q('DELETE FROM state_work_member_candidates WHERE id=?').run(id);
     this.q('UPDATE state_work_member_scores SET bucket=? WHERE identity=?').run(first?canon(first.bucket):null,id);
    }
    state=page.length?{...state,after:page.at(-1).id}:{...state,phase:'facts',after:''};this.save(state);
   });
  }
  while(state.phase==='facts') {
   check();const page=this.q('SELECT id FROM state_work_member_dirty WHERE fact=1 AND id>? ORDER BY id LIMIT 256').all(state.after);
   this.transaction(()=>{
    for(const {id}of page) {
     this.q('DELETE FROM state_work_member_scores WHERE identity=?').run(id);
     const bucket=this.q('SELECT bucket FROM state_work_member_candidates WHERE id=?').get(id)?.bucket??null;
     const keys=reader.db.prepare(`SELECT key FROM entity_prepared_board_versions WHERE kind='fact' AND identity=? AND body IS NOT NULL
      AND first_cut<=? AND (last_cut IS NULL OR last_cut>?) ORDER BY ruler,mode`).all(id,target.id,target.id);
     for(const {key}of keys) {
      const value=reader.value('fact',key,target.id).value;
      if(value.gate_ruler)this.q('INSERT OR REPLACE INTO state_work_member_scores VALUES(?,?,?,?,?,?,?,?)')
       .run(id,value.ruler,value.mode,Number(value.score),value.scene,bucket,Number(value.retired_at!=null),value.gate_ruler);
     }
    }
    if(page.length)state={...state,after:page.at(-1).id};
    else {this.q('DELETE FROM state_work_member_leaders').run();state={...state,phase:'leaders',after:['','','']};}
    this.save(state);
   });
  }
  while(state.phase==='leaders') {
   check();const page=this.q(`SELECT DISTINCT bucket,gate_ruler,mode FROM state_work_member_scores WHERE bucket IS NOT NULL
    AND (bucket,gate_ruler,mode)>(?,?,?) ORDER BY bucket,gate_ruler,mode LIMIT 256`).all(...state.after);
   this.transaction(()=>{
    for(const group of page)for(let target=0;target<contract.rulers.length;target++) {
     const score=this.q(`SELECT f.score FROM state_work_member_scores f JOIN state_work_member_candidates c ON c.id=f.identity
      WHERE f.bucket=? AND f.gate_ruler=? AND f.mode=? AND f.retired=0 AND (c.admit & ?)<>0 ORDER BY f.score DESC,f.identity DESC LIMIT 1`)
      .get(group.bucket,group.gate_ruler,group.mode,bit(target))?.score??null;
     this.q('INSERT INTO state_work_member_leaders VALUES(?,?,?,?,?)').run(group.bucket,group.gate_ruler,group.mode,target,score);
    }
    if(page.length){const last=page.at(-1);state={...state,after:[last.bucket,last.gate_ruler,last.mode]};}
    else {this.q('DELETE FROM state_work_member_dirty').run();state={complete:target.id,cut:target.id,cursor:target.input_count};}
    this.save(state);
   });
  }
 }
 share(id,target,scene) {
  const value=this.q(`SELECT max(CASE WHEN l.score>0 THEN f.score/l.score ELSE 1.0 END) value FROM state_work_member_scores f
   JOIN state_work_member_leaders l ON l.bucket=f.bucket AND l.ruler=f.gate_ruler AND l.mode=f.mode AND l.target=?
   WHERE f.identity=? AND f.retired=0 AND f.scene=?`).get(target,id,scene).value;
  return value??(scene?this.share(id,target,''):null);
 }
 async run(index,reader,cut,operate,check) {
  if(!operate)throw Error('membership audit requires the native gate');
  await this.prepare(reader,cut,index.contract,check);
  const member=index.q(`SELECT m.queued,lower(hex(m.qid)) qid FROM relation_inventory_owners o JOIN relation_inventory_owner_members m ON m.owner=o.id
   JOIN entity_inventory_builds b ON b.id=m.candidate JOIN entity_inventory_symbols r ON r.id=m.ruler JOIN entity_inventory_symbols mo ON mo.id=m.mode
   WHERE o.owner=? AND b.build_id=? AND r.name=? AND mo.name=?`);
  let candidates=0,members=0;
  for(let after='';;) {
   check();const page=this.q("SELECT id,bucket,printf('%016llx',admit) admit,default_id,modes,record_ref FROM state_work_member_candidates WHERE id>? ORDER BY id LIMIT 16").all(after);
   if(!page.length)break;
   const requests=[],held=[];
   for(const candidate of page) {
    const actual=index.q('SELECT body FROM entity_catalog_candidates WHERE id=?').get(candidate.id),body=actual?JSON.parse(actual.body):null;
    if(!body||body.record_ref!==candidate.record_ref||canon(body.bucket)!==candidate.bucket||body.default!==candidate.default_id||mask(body.admit)!==mask(candidate.admit))
     throw Error('membership audit canonical candidate differs');
    const owed=!!this.q('SELECT 1 FROM state_work_audit_queue WHERE identity=? LIMIT 1').get(candidate.id),targets=[];
    for(let target=0;target<index.contract.rulers.length;target++)if((mask(candidate.admit)&bit(target))!==0n) {
     const present=candidate.default_id!=null&&!!this.q('SELECT 1 FROM state_work_member_candidates WHERE id=? AND (admit & ?)<>0').get(candidate.default_id,bit(target));
     targets.push({target,request:requests.length});const ruler=index.contract.rulers[target];
     requests.push({share:this.share(candidate.id,target,ruler.scene),owed,default_present:present,
      default_share:present?this.share(candidate.default_id,target,ruler.scene):null});
    }
    held.push({candidate,targets});
   }
   const decisions=[];for(let from=0;from<requests.length;from+=256)decisions.push(...await operate('gate',requests.slice(from,from+256)));
   for(const {candidate,targets}of held) {
    const stored=reader.value('source',candidate.id,cut.id)?.value?.raw_record_ref;
    const record=reader.recordBody(stored||candidate.record_ref),prefixes=new Map();
    for(const [mode,admit]of Object.entries(JSON.parse(candidate.modes)))for(const {target,request}of targets) {
     if((mask(admit)&bit(target))===0n)continue;
     const ruler=index.contract.rulers[target].id,queued=!!this.q('SELECT 1 FROM state_work_audit_queue WHERE identity=? AND ruler=? AND mode=?').get(candidate.id,ruler,mode);
     if(typeof decisions[request]?.eligible!=='boolean')throw Error('membership audit native gate is incomplete');
     if(!queued&&(!decisions[request].eligible||this.q('SELECT 1 FROM state_work_member_scores WHERE identity=? AND ruler=? AND mode=? AND retired=0').get(candidate.id,ruler,mode)))continue;
     if(!prefixes.has(mode))prefixes.set(mode,createHash('sha256').update(`simulate\n{"mode":${JSON.stringify(mode)},"record":${record},"ruler":`));
     const qid=prefixes.get(mode).copy().update(JSON.stringify(ruler)+'}').digest('hex').slice(0,32),actual=member.get(candidate.id,candidate.id,ruler,mode);
     if(!actual||actual.queued!==Number(queued)||actual.qid!==qid)throw Error('membership audit expected member differs');
     members++;
    }
    candidates++;
   }
   after=page.at(-1).id;
  }
  if(index.q('SELECT count(*) n FROM entity_catalog_candidates').get().n!==candidates
   ||index.q('SELECT count(*) n FROM relation_inventory_owner_members').get().n!==members)throw Error('membership audit has extra members or candidates');
  return {complete:true,cut:cut.id,candidates,members};
 }
}
