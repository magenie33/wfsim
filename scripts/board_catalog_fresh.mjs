// SPDX-License-Identifier: AGPL-3.0-or-later
// A scope which first appeared after the cut is captured at one PostgreSQL snapshot.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {existsSync,mkdirSync,readFileSync,renameSync,rmSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {deflateSync} from 'node:zlib';
import {qualify} from './qualify_rank_catalog.mjs';
import {limitRankScratch} from './board_rank_storage.mjs';
import {captureRankBaselineAt} from './board_rank_baseline.mjs';
import {rankFilePage,syncRankDirectory} from './board_rank_files.mjs';
import {sha256} from './board_catalog_readback.mjs';

const packed=value=>deflateSync(JSON.stringify(value),{level:1});
export const freshDirectory=(state,w,r)=>join(state,'fresh-'+sha256(JSON.stringify([w,r])));

export async function qualifyRetainedScope({state,capture,archive,native},weapon,ruler) {
 const directory=freshDirectory(state,weapon,ruler);
 mkdirSync(directory,{recursive:true,mode:0o700});
 const paths=join(directory,`retained-${native.policyAdapter}.sqlite`);
 const q=new DatabaseSync(paths);
 try{
  limitRankScratch(q);
  return await qualify(capture,archive,q,q,native,weapon,ruler,1);
 }finally{q.close();}
}

export function assertFreshOrigin(capture,archive,weapon,ruler) {
 const keys=[weapon,ruler];
 assert(!capture.prepare('SELECT 1 FROM facts WHERE weapon=? AND ruler=? LIMIT 1').get(...keys)&&
  !archive.prepare('SELECT 1 FROM scopes WHERE weapon=? AND ruler=?').get(...keys)&&
  !archive.prepare('SELECT 1 FROM members WHERE weapon=? AND ruler=? LIMIT 1').get(...keys),
  'retained scope requires its original qualification');
}

export function freshScopeLimits(scope) {
 assert(scope.fresh===true&&scope.members===0&&Number.isSafeInteger(scope.rows)&&scope.rows>=0&&
  ['identities','facts','manifest_bytes','scanned_builds'].every(k=>
  Number.isSafeInteger(scope[k])&&scope[k]>0)&&scope.scanned_builds<Number.MAX_SAFE_INTEGER,'fresh scope requires a bounded current forecast');
 return {rows:scope.rows,identities:scope.identities,facts:scope.facts,manifest_bytes:scope.manifest_bytes,scanned_builds:scope.scanned_builds};
}

export function checkFreshManifest(m,contract,weapon,ruler,limits) {
 assert(m.weapon===weapon&&m.ruler===ruler&&m.source===contract.source&&m.native===contract.native&&
  m.mailbox_floor===contract.floor&&typeof m.snapshot==='string'&&m.snapshot&&/^[a-f0-9]{64}$/.test(m.sha256),
  'fresh scope capture boundary changed');
 assert([m.total,m.facts,m.bytes,m.scanned_builds].every(n=>Number.isSafeInteger(n)&&n>0)&&
  m.total<=limits.identities&&m.facts<=limits.facts&&m.bytes<=limits.manifest_bytes&&
  m.total<=m.scanned_builds&&m.scanned_builds<=limits.scanned_builds,
  'fresh scope capture exceeds its qualified forecast');
}

export function checkFreshReceipt(r,m,contract,policy,limits) {
 assert(r?.complete&&r.eligible&&r.unresolved===0&&r.members===0&&r.weapon===m.weapon&&r.ruler===m.ruler&&
  r.source===m.source&&r.native===m.native&&r.snapshot===m.snapshot&&r.archive===contract.archive&&
  r.policy===policy&&Number.isSafeInteger(r.rows)&&r.rows>=0&&r.rows<=limits.rows&&/^[a-f0-9]{64}$/.test(r.pages_sha256),
  'fresh scope qualification differs from its bounded capture');
}

export function readFreshQualification({state,contract,native},weapon,ruler) {
 const directory=freshDirectory(state,weapon,ruler),manifest=JSON.parse(readFileSync(join(directory,'cut.json'),'utf8'));
 checkFreshManifest(manifest,contract,weapon,ruler,contract.priority_fresh);
 assert(manifest.path===join(directory,'inputs.ndjson'),'fresh scope manifest path changed');
 const ranking=native.policyAdapter?`ranking-${native.policyAdapter}.sqlite`:'ranking.sqlite';
 const db=new DatabaseSync(join(directory,ranking),{readOnly:true});
 try{
  const receipt={complete:true,...JSON.parse(db.prepare("SELECT v FROM progress WHERE k='receipt'").get().v)};
  checkFreshReceipt(receipt,manifest,contract,native.policyAdapter,contract.priority_fresh);
  return {receipt,manifest};
 }finally{db.close();}
}

export async function freshPublication(ctx,weapon,ruler) {
 const {contract,sql,native,archive,state}=ctx,keys=[weapon,ruler];
 assertFreshOrigin(ctx.capture,archive,...keys);
 const limits=contract.priority_fresh;
 const directory=freshDirectory(state,...keys),saved=join(directory,'cut.json');
 mkdirSync(directory,{recursive:true,mode:0o700});
 let m;
 if(existsSync(saved))m=JSON.parse(readFileSync(saved,'utf8'));
 else{
  const file=join(directory,'inputs.ndjson');rmSync(file,{force:true});
  m=await captureRankBaselineAt(sql,{weapon,ruler,source:contract.source,native:contract.native,
   work:state,mailboxFloor:contract.floor,limits},directory);
  writeFileSync(saved+'.partial',JSON.stringify(m),{flush:true});renameSync(saved+'.partial',saved);syncRankDirectory(directory);
 }
 assert(m.path===join(directory,'inputs.ndjson')&&m.source===contract.source&&m.native===native.adapter&&m.weapon===weapon&&m.ruler===ruler);
 if(limits)checkFreshManifest(m,contract,...keys,limits);
 const ranking=native.policyAdapter?`ranking-${native.policyAdapter}.sqlite`:'ranking.sqlite';
 const cap=new DatabaseSync(join(directory,'capture.sqlite')),q=new DatabaseSync(join(directory,ranking));
 try{
  limitRankScratch(q);limitRankScratch(cap);
  cap.exec(`CREATE TABLE IF NOT EXISTS progress(k text PRIMARY KEY,v text);
   CREATE TABLE IF NOT EXISTS builds(id text PRIMARY KEY,body blob);
   CREATE TABLE IF NOT EXISTS facts(id text,ruler text,mode text,weapon text,body blob,verified integer,PRIMARY KEY(weapon,ruler,id,mode)) WITHOUT ROWID;
   CREATE TABLE IF NOT EXISTS removals(id text,ruler text,mode text,version text,PRIMARY KEY(id,ruler,mode)) WITHOUT ROWID;`);
  const get=k=>cap.prepare('SELECT v FROM progress WHERE k=?').get(k)?.v;
  const save=cap.prepare('INSERT OR REPLACE INTO progress VALUES(?,?)');
  if(!get('spec')){
   cap.exec('BEGIN');
   save.run('spec',JSON.stringify({archive:contract.archive,native:contract.native,source:contract.source}));
   save.run('replay',JSON.stringify({snapshot:m.snapshot,captured_at:m.captured_at}));cap.exec('COMMIT');
  }
  const offset=Number(get('offset')||0);
  if(offset<m.bytes){
   const page=rankFilePage(m.path,offset,64);
   const build=cap.prepare('INSERT INTO builds VALUES(?,?)'),fact=cap.prepare('INSERT INTO facts VALUES(?,?,?,?,?,0)');
   cap.exec('BEGIN');
   try{
    for(const row of page.rows)if(row.kind==='source'){
     build.run(row.source.id,packed(row.source));
     for(const f of row.facts){assert.equal(f.identity,row.source.id);assert.equal(f.ruler,ruler);fact.run(f.identity,ruler,f.mode,weapon,packed(f));}
    }
    save.run('offset',String(page.offset));cap.exec('COMMIT');
   }catch(e){cap.exec('ROLLBACK');throw e;}
   if(!page.done)return null;
  }
  const r=await qualify(cap,archive,q,q,native,weapon,ruler,1);
  if(!r.complete)return null;
  assert(r.eligible&&r.members===0&&r.unresolved===0);
  if(limits)checkFreshReceipt(r,m,contract,native.policyAdapter,limits);
  return {manifest:m,receipt:r};
 }finally{cap.close();q.close();}
}
