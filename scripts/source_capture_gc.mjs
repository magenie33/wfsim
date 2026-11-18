// SPDX-License-Identifier: AGPL-3.0-or-later
// Only explicitly owned transfers covered by audited consumers may retire; recovery baselines remain pinned.
import {createHash,randomUUID} from 'node:crypto';
import {existsSync,lstatSync,mkdirSync,readFileSync,realpathSync,unlinkSync,rmdirSync,opendirSync} from 'node:fs';
import {join,dirname,resolve} from 'node:path';
import {canon} from '../worker/task_contract.js';

const hash=body=>createHash('sha256').update(body).digest('hex');
const fileStamp=path=>{
 const value=lstatSync(path,{bigint:true});
  if(!value.isFile()||value.isSymbolicLink()||value.nlink!==1n)throw Error('capture file is not a regular owned file');
 return [value.dev,value.ino,value.size,value.mtimeNs,value.ctimeNs,value.uid].map(String);
};
const dirStamp=path=>{
 const value=lstatSync(path,{bigint:true});
 if(!value.isDirectory()||value.isSymbolicLink())throw Error('capture directory is linked or unavailable');
 return [value.dev,value.ino,value.uid].map(String);
};
const directoryNames=(path,limit)=>{
 const directory=opendirSync(path),names=[];
 try{for(let entry;(entry=directory.readSync());){if(names.length===limit)throw Error('capture directory exceeds metadata bound');names.push(entry.name);}}
 finally{directory.closeSync();}return names;
};
const schema=`CREATE TABLE IF NOT EXISTS record_source_capture_owners(id TEXT PRIMARY KEY,generation TEXT NOT NULL,directory TEXT NOT NULL UNIQUE,body TEXT NOT NULL);
 CREATE TRIGGER IF NOT EXISTS source_capture_owner_immutable BEFORE UPDATE ON record_source_capture_owners BEGIN SELECT RAISE(ABORT,'immutable capture owner');END;
 CREATE TRIGGER IF NOT EXISTS source_capture_owner_retained BEFORE DELETE ON record_source_capture_owners BEGIN SELECT RAISE(ABORT,'capture owner retained');END;
 CREATE TABLE IF NOT EXISTS record_source_capture_manifests(id TEXT PRIMARY KEY REFERENCES record_source_capture_owners(id),epoch TEXT NOT NULL,body TEXT NOT NULL);
 CREATE TRIGGER IF NOT EXISTS source_capture_manifest_immutable BEFORE UPDATE ON record_source_capture_manifests BEGIN SELECT RAISE(ABORT,'immutable capture manifest');END;
 CREATE TRIGGER IF NOT EXISTS source_capture_manifest_retained BEFORE DELETE ON record_source_capture_manifests BEGIN SELECT RAISE(ABORT,'capture manifest retained');END;
 CREATE TABLE IF NOT EXISTS state_source_capture_gc(id TEXT PRIMARY KEY REFERENCES record_source_capture_owners(id),generation TEXT NOT NULL,
  phase TEXT NOT NULL,directory_stamp TEXT,cut INTEGER,baseline INTEGER NOT NULL DEFAULT 0,cursor INTEGER NOT NULL DEFAULT 0,
  intent INTEGER NOT NULL DEFAULT 0,removed_bytes INTEGER NOT NULL DEFAULT 0,error TEXT);
 CREATE INDEX IF NOT EXISTS source_capture_gc_due ON state_source_capture_gc(generation,phase,cut);
 CREATE TABLE IF NOT EXISTS state_source_capture_refs(reference TEXT PRIMARY KEY,capture TEXT NOT NULL REFERENCES record_source_capture_owners(id));
 CREATE INDEX IF NOT EXISTS source_capture_refs ON state_source_capture_refs(capture);`;

export class SourceCaptureGC {
 static openExisting(db,root,context,{guard=()=>{}}={}) {
  if(!context?.generation||!context.engine||!context.contract)throw Error('capture ownership context is missing');
  const value=Object.create(SourceCaptureGC.prototype);
  dirStamp(root);value.db=db;value.root=realpathSync(root);value.context=context;value.guard=guard;value.rootStamp=canon(dirStamp(value.root));
  for(const name of ['record_source_capture_owners','record_source_capture_manifests','state_source_capture_gc','state_source_capture_refs'])
   db.prepare('SELECT 1 FROM '+name+' LIMIT 0').all();
  return value;
 }
 constructor(db,root,context,{guard=()=>{}}={}) {
  if(!context?.generation||!context.engine||!context.contract)throw Error('capture ownership context is missing');
  dirStamp(root);this.db=db;this.root=realpathSync(root);this.context=context;this.guard=guard;this.rootStamp=canon(dirStamp(this.root));
  db.exec(schema);
 }
 q(sql){return this.db.prepare(sql);}
 transaction(fn){this.db.exec('BEGIN IMMEDIATE');try{const result=fn();this.db.exec('COMMIT');return result;}catch(error){try{this.db.exec('ROLLBACK');}catch{}throw error;}}
 checkRoot(){this.guard();if(realpathSync(this.root)!==this.root||canon(dirStamp(this.root))!==this.rootStamp)throw Error('capture root identity changed');}
 reserve() {
  this.checkRoot();const id=randomUUID(),directory=join(this.root,'capture-'+id);
  if(existsSync(directory))throw Error('capture ownership cannot adopt an existing directory');
  const body=canon({v:1,producer:'normalization',generation:this.context.generation,root:this.root,root_stamp:this.rootStamp,created_at:new Date().toISOString()});
  this.transaction(()=>{
   this.q('INSERT INTO record_source_capture_owners VALUES(?,?,?,?)').run(id,this.context.generation,directory,body);
   this.q("INSERT INTO state_source_capture_gc(id,generation,phase) VALUES(?,?,'reserved')").run(id,this.context.generation);
  });
  mkdirSync(directory);this.q("UPDATE state_source_capture_gc SET phase='capturing',directory_stamp=? WHERE id=?").run(canon(dirStamp(directory)),id);
  return {id,directory};
 }
 seal(id,cut) {
  this.checkRoot();const row=this.q(`SELECT o.*,g.phase,g.directory_stamp FROM record_source_capture_owners o JOIN state_source_capture_gc g USING(id) WHERE o.id=?`).get(id);
  if(!row||row.generation!==this.context.generation||row.phase!=='capturing'||row.directory_stamp!==canon(dirStamp(row.directory)))throw Error('capture creator identity differs');
  if(cut.receipt?.generation!==this.context.generation||cut.receipt.engine!==this.context.engine||cut.receipt.status!=='sealed'
   ||canon(cut.receipt.contract)!==canon(this.context.contract)||resolve(cut.capture)!==join(row.directory,'capture.json'))throw Error('capture sealed receipt differs');
  const text=readFileSync(cut.capture),manifest=JSON.parse(text);
  if(hash(text)!==cut.manifest_sha||!Array.isArray(manifest.files))throw Error('capture sealed manifest differs');
  const seen=new Set(),files=[];
  for(const file of [...manifest.files,{path:cut.capture,sha256:cut.manifest_sha}]) {
   const path=resolve(file.path);
   if(dirname(path)!==row.directory||seen.has(path)||! /^[a-f0-9]{64}$/.test(file.sha256))throw Error('capture file ownership differs');
   const stamp=fileStamp(path);if(stamp[0]!==JSON.parse(row.directory_stamp)[0]||stamp[5]!==JSON.parse(row.directory_stamp)[2])throw Error('capture file creator differs');
   files.push({path,sha256:file.sha256,stamp,bytes:Number(stamp[2])});seen.add(path);
  }
  this.transaction(()=>{
   this.q('INSERT INTO record_source_capture_manifests VALUES(?,?,?)').run(id,cut.receipt.epoch,canon({v:1,receipt:cut.receipt,manifest_sha:cut.manifest_sha,files}));
   this.q("UPDATE state_source_capture_gc SET phase='sealed',baseline=? WHERE id=?").run(Number(!!cut.bootstrap),id);
  });
 }
 bind(epoch,cut) {
  const row=this.q(`SELECT g.id,g.cut FROM state_source_capture_gc g JOIN record_source_capture_manifests m USING(id) WHERE g.generation=? AND m.epoch=?`).get(this.context.generation,epoch);
  if(!row)return;
  if(row.cut!=null&&row.cut!==cut)throw Error('capture cut binding changed');
  this.q('UPDATE state_source_capture_gc SET cut=? WHERE id=?').run(cut,row.id);
 }
 pin(reference,id) {
  this.transaction(()=>{
   const row=this.q('SELECT phase,generation FROM state_source_capture_gc WHERE id=?').get(id);
   if(!row||row.generation!==this.context.generation||!['capturing','sealed'].includes(row.phase))throw Error('capture is unavailable for a new reference');
   this.q('INSERT INTO state_source_capture_refs VALUES(?,?) ON CONFLICT(reference) DO UPDATE SET capture=excluded.capture').run(reference,id);
  });
 }
 unpin(reference){this.q('DELETE FROM state_source_capture_refs WHERE reference=?').run(reference);}
 status(){return this.q('SELECT phase,count(*) n,sum(removed_bytes) removed_bytes FROM state_source_capture_gc WHERE generation=? GROUP BY phase ORDER BY phase').all(this.context.generation);}
 async collect(inputs,{normalizedCut,references=[],maxCaptures=4,maxFiles=8,maxBytes=8*1024**3,maxManifestBytes=1024**2,maxDirectoryEntries=64,fault=async()=>{}}={}) {
  if(!Number.isSafeInteger(normalizedCut)||normalizedCut<0||![maxCaptures,maxFiles,maxBytes,maxManifestBytes,maxDirectoryEntries].every(value=>Number.isSafeInteger(value)&&value>0))throw Error('invalid capture collection bound');
  this.checkRoot();
  if(canon(inputs.contract)!==canon(this.context.contract))throw Error('capture collection source contract differs');
  const consumers=inputs.db.prepare('SELECT * FROM state_prepared_board_consumers ORDER BY name LIMIT 65').all();
  if(consumers.length>64)throw Error('capture consumer count exceeds metadata bound');
  if(!['inventory','projection'].every(name=>consumers.some(row=>row.name===name)))throw Error('capture consumers are missing');
  for(const row of consumers)if(row.acknowledged) {
   const audit=JSON.parse(row.audit||'null'),cut=inputs.cut(row.acknowledged);
   if(!audit?.complete||audit.cut!==row.acknowledged||audit.engine!==cut.receipt.engine||audit.snapshot!==cut.receipt.snapshot
    ||audit.input_count!==cut.input_count||audit.input_digest!==cut.sealed_digest)throw Error('capture acknowledgement was not audited');
  }
  const floor=Math.min(normalizedCut,...consumers.map(row=>row.acknowledged)),result={floor,files:0,bytes:0,retired:0,blocked:[]};
  const rows=this.q(`SELECT o.directory,CASE WHEN length(CAST(o.body AS BLOB))<=? THEN o.body END owner,g.*,m.epoch,CASE WHEN length(CAST(m.body AS BLOB))<=? THEN m.body END manifest FROM state_source_capture_gc g JOIN record_source_capture_owners o USING(id)
   JOIN record_source_capture_manifests m USING(id) WHERE g.generation=? AND g.phase IN('sealed','deleting') AND g.baseline=0 AND g.cut<=?
   AND NOT EXISTS(SELECT 1 FROM state_source_capture_refs r WHERE r.capture=g.id) ORDER BY g.cut,g.id LIMIT ?`).all(maxManifestBytes,maxManifestBytes,this.context.generation,floor,maxCaptures);
  for(const row of rows) {
   const blocked=reason=>{result.blocked.push({id:row.id,reason});this.q('UPDATE state_source_capture_gc SET error=? WHERE id=?').run(reason,row.id);};
   if(!row.manifest||!row.owner){blocked('capture manifest exceeds metadata bound');continue;}
   const manifest=JSON.parse(row.manifest),owner=JSON.parse(row.owner),cut=inputs.cut(row.cut),directory=row.directory;
   if(!Array.isArray(manifest.files)||manifest.files.length>maxDirectoryEntries){blocked('capture file count exceeds metadata bound');continue;}
   if(owner.v!==1||owner.producer!=='normalization'||owner.root!==this.root||owner.root_stamp!==this.rootStamp
    ||owner.generation!==this.context.generation||dirname(directory)!==this.root||cut.state!=='sealed'||cut.receipt.epoch!==row.epoch
    ||cut.receipt.generation!==this.context.generation||cut.receipt.engine!==this.context.engine||canon(cut.receipt.contract)!==canon(this.context.contract)
    ||cut.receipt.capture.capture!==join(directory,'capture.json')||cut.receipt.capture.manifest_sha!==manifest.manifest_sha) {blocked('capture ownership or sealed binding differs');continue;}
   if(references.some(path=>path&&(resolve(path)===directory||resolve(path).startsWith(directory+'/')))){blocked('pending or recovery reference');continue;}
   if(!existsSync(directory)) {
    if(row.phase==='deleting'&&row.cursor===manifest.files.length&&row.intent){this.q("UPDATE state_source_capture_gc SET phase='retired',intent=0,error=NULL WHERE id=?").run(row.id);result.retired++;}
    else blocked('capture directory is missing');
    continue;
   }
   let unsafe=null;
   try {
    if(canon(dirStamp(directory))!==row.directory_stamp)throw Error('capture directory identity differs');
    const names=new Set(manifest.files.map(file=>file.path));
    if(directoryNames(directory,maxDirectoryEntries).some(name=>!names.has(join(directory,name))))throw Error('capture contains an unknown file');
    for(let n=row.cursor;n<manifest.files.length;n++) {
     const file=manifest.files[n];
     if(!existsSync(file.path)) {if(n===row.cursor&&row.phase==='deleting'&&row.intent)continue;throw Error('capture file is missing');}
     if(dirname(file.path)!==directory||canon(fileStamp(file.path))!==canon(file.stamp))throw Error('capture file identity differs');
    }
   }catch(error){unsafe=error.message;}
   if(unsafe){blocked(unsafe);continue;}
   if(row.phase!=='deleting') {this.q("UPDATE state_source_capture_gc SET phase='deleting',error=NULL WHERE id=?").run(row.id);await fault('after-capture-gc-claim',{id:row.id});}
   for(let n=row.cursor;n<manifest.files.length;n++) {
    const file=manifest.files[n];
    if(file.bytes>maxBytes){blocked('capture file exceeds the configured collection byte bound');return result;}
    if(result.files>=maxFiles||result.bytes+file.bytes>maxBytes)return result;
    this.checkRoot();
    if(this.q('SELECT 1 FROM state_source_capture_refs WHERE capture=?').get(row.id))throw Error('capture gained a reference after collection claim');
    this.q('UPDATE state_source_capture_gc SET intent=1 WHERE id=?').run(row.id);await fault('before-capture-file-unlink',{id:row.id,path:file.path});
    this.checkRoot();
    if(existsSync(file.path)) {
     if(canon(dirStamp(directory))!==row.directory_stamp||canon(fileStamp(file.path))!==canon(file.stamp))throw Error('capture identity changed before unlink');
     unlinkSync(file.path);
    }
    await fault('after-capture-file-unlink',{id:row.id,path:file.path});
    this.q('UPDATE state_source_capture_gc SET cursor=?,intent=0,removed_bytes=removed_bytes+?,error=NULL WHERE id=?').run(n+1,file.bytes,row.id);
    result.files++;result.bytes+=file.bytes;
   }
   if(directoryNames(directory,maxDirectoryEntries).length){blocked('capture contains an unknown file');continue;}
   this.q('UPDATE state_source_capture_gc SET intent=1 WHERE id=?').run(row.id);this.checkRoot();
   if(canon(dirStamp(directory))!==row.directory_stamp)throw Error('capture directory identity changed before removal');
   rmdirSync(directory);await fault('after-capture-directory-remove',{id:row.id});
   this.q("UPDATE state_source_capture_gc SET phase='retired',intent=0,error=NULL WHERE id=?").run(row.id);result.retired++;
  }
  return result;
 }
}
