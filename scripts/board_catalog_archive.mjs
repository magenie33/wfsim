// SPDX-License-Identifier: AGPL-3.0-or-later
// The archive defines the identities whose fate must be accounted for at the cut.
import {createHash} from 'node:crypto';
import {readFileSync,statSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
import {guardCatalogFile} from './board_rank_storage.mjs';
import {deflateSync} from 'node:zlib';
import {headlessBoardRecord} from '../mcp/headless.js';
import {openInventoryOperator} from './inventory_operator.mjs';

const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const valid=name=>/^[a-z0-9_]{1,160}$/.test(name);
function shard(path){
 if(statSync(path).size>32*1024*1024)throw Error('archive shard exceeds byte budget');
 return readFileSync(path);
}

export function archiveInventory(root){
 const raw=shard(join(root,'board.meta.json')),meta=JSON.parse(raw),files=[];
 const base=Object.entries(meta.files).sort(([a],[b])=>a<b?-1:1);
 if(sha(base.map(([name,digest])=>name+' '+digest+'\n').join(''))!==meta.digest)throw Error('archive stamp mismatch');
 const names=[['',base.map(([name])=>name)],...Object.entries(meta.scene_weapons).sort()];
 for(const [scene,weapons] of names){
  if(scene&&!valid(scene))throw Error('invalid archive scene');
  for(const name of new Set(['index',...weapons].sort())){
   if(!valid(name))throw Error('invalid archive file');
   const file=(scene?scene+'/':'')+name+'.json';
   const digest=sha(shard(join(root,'board',file)));
   if(!scene&&meta.files[name]&&meta.files[name]!==digest)throw Error('archive file differs from stamp: '+file);
   files.push({scene,name,file,digest});
  }
 }
 return {v:1,stamp:sha(raw),digest:meta.digest,files};
}

export async function importArchive(db,native,inventory,root,seconds){
 const contract=sha(JSON.stringify(inventory));
 db.exec(`CREATE TABLE IF NOT EXISTS contract(id INTEGER PRIMARY KEY,archive TEXT,native TEXT);
 CREATE TABLE IF NOT EXISTS sealed(id INTEGER PRIMARY KEY,archive TEXT);
 CREATE TABLE IF NOT EXISTS shards(file TEXT PRIMARY KEY,cursor INTEGER,complete INTEGER);
 CREATE TABLE IF NOT EXISTS scopes(weapon TEXT,ruler TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
 CREATE TABLE IF NOT EXISTS members(weapon TEXT,ruler TEXT,id TEXT,mode TEXT,file TEXT,ordinal INTEGER,wire BLOB,
   PRIMARY KEY(file,ordinal)) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS member_scope ON members(weapon,ruler,id,mode);
 CREATE INDEX IF NOT EXISTS member_ordinal ON members(weapon,ruler,ordinal);
 CREATE TABLE IF NOT EXISTS heads(weapon TEXT,ruler TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;`);
 db.prepare('INSERT OR IGNORE INTO contract VALUES(1,?,?)').run(contract,native.adapter);
 const state=db.prepare('SELECT * FROM contract').get();
 if(state.archive!==contract||state.native!==native.adapter)throw Error('archive recovery boundary changed');
 const end=Date.now()+seconds*1000,scenes=new Map(native.contract.rulers.map(r=>[r.id,r.scene]));
 const get=db.prepare('SELECT * FROM shards WHERE file=?'),set=db.prepare('INSERT OR REPLACE INTO shards VALUES(?,?,?)');
 const member=db.prepare('INSERT INTO members VALUES(?,?,?,?,?,?,?)');
 const scope=db.prepare('INSERT OR IGNORE INTO scopes VALUES(?,?)'),head=db.prepare('INSERT OR IGNORE INTO heads VALUES(?,?)');
 for(const {name,scene,file,digest} of inventory.files){
  const prior=get.get(file);
  if(prior?.complete)continue;
  if(Date.now()>=end)break;
  const bytes=shard(join(root,'board',file));
  if(sha(bytes)!==digest)throw Error('archive changed: '+file);
  const rows=JSON.parse(bytes);
  if(name==='index'){
   db.exec('BEGIN');
   try{
    for(const [weapon,entries] of Object.entries(rows)){
     if(!valid(weapon))throw Error('invalid indexed weapon');
     for(const row of entries){
      if(scenes.get(row.benchmark)!==scene)throw Error('index ruler/scene mismatch');
      head.run(weapon,row.benchmark);
     }
    }
    set.run(file,0,1);db.exec('COMMIT');
   }catch(e){db.exec('ROLLBACK');throw e;}
   continue;
  }
  if(!Array.isArray(rows))throw Error('archive rows required');
  let cursor=prior?.cursor||0;
  while(cursor<rows.length&&Date.now()<end){
   const page=rows.slice(cursor,cursor+64);
   const descriptors=await native.call('describe',page.map((row,i)=>({id:String(cursor+i),record:headlessBoardRecord(name,row)})));
   db.exec('BEGIN');
   try{
    for(const [i,row] of page.entries()){
     if(scenes.get(row.benchmark)!==scene||descriptors[i].id!==String(i+cursor))throw Error('member archive context changed');
     scope.run(name,row.benchmark);
     member.run(name,row.benchmark,descriptors[i].candidate,row.mode||'base',file,cursor+i,deflateSync(JSON.stringify(row)));
    }
    cursor+=page.length;
    set.run(file,cursor,Number(cursor===rows.length));
    db.exec('COMMIT');
   }catch(e){db.exec('ROLLBACK');throw e;}
  }
  if(!rows.length)set.run(file,0,1);
 }
 const complete=db.prepare('SELECT count(*) n FROM shards WHERE complete=1').get().n===inventory.files.length;
 if(complete&&db.prepare('SELECT * FROM heads EXCEPT SELECT * FROM scopes LIMIT 1').get())throw Error('published index has a partition without members');
 if(complete)db.prepare('INSERT OR REPLACE INTO sealed VALUES(1,?)').run(contract);
 return {archive:contract,complete,scopes:db.prepare('SELECT count(*) n FROM scopes').get().n};
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const [action,root,inventoryPath,database,binary]=process.argv.slice(2);
 if(action==='inventory')console.log(JSON.stringify(archiveInventory(root)));
 else if(action==='import'){
  const inventory=JSON.parse(readFileSync(inventoryPath)),db=new DatabaseSync(guardCatalogFile(database)),native=await openInventoryOperator(binary);
  try{console.log(JSON.stringify(await importArchive(db,native,inventory,root,30)));}
  finally{db.close();await native.close();}
 }else throw Error('use inventory ROOT or import ROOT INVENTORY DATABASE NATIVE');
}
