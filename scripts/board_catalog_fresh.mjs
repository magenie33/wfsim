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

export async function freshPublication(ctx,weapon,ruler) {
 const {contract,sql,native,archive,state}=ctx,keys=[weapon,ruler];
 if(ctx.capture.prepare('SELECT 1 FROM facts WHERE weapon=? AND ruler=? LIMIT 1').get(...keys)||
    archive.prepare('SELECT 1 FROM scopes WHERE weapon=? AND ruler=?').get(...keys))throw Error('retained scope requires its original qualification');
 const directory=freshDirectory(state,...keys),saved=join(directory,'cut.json');
 mkdirSync(directory,{recursive:true,mode:0o700});
 let m;
 if(existsSync(saved))m=JSON.parse(readFileSync(saved,'utf8'));
 else{
  const file=join(directory,'inputs.ndjson');rmSync(file,{force:true});
  m=await captureRankBaselineAt(sql,{weapon,ruler,source:contract.source,native:contract.native,
   work:state,mailboxFloor:contract.floor},directory);
  writeFileSync(saved+'.partial',JSON.stringify(m),{flush:true});renameSync(saved+'.partial',saved);syncRankDirectory(directory);
 }
 assert(m.path===join(directory,'inputs.ndjson')&&m.source===contract.source&&m.native===native.adapter&&m.weapon===weapon&&m.ruler===ruler);
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
  return {manifest:m,receipt:r};
 }finally{cap.close();q.close();}
}
