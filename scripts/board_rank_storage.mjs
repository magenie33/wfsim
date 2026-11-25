// SPDX-License-Identifier: AGPL-3.0-or-later
// Preparation stops before consuming the mounted Data reserve or system WAL headroom.
import {dirname,resolve,sep} from 'node:path';
import {existsSync,lstatSync,realpathSync,statfsSync} from 'node:fs';
import {boardDataReserve,boardStorage} from './board_storage.mjs';
export function guardRankSpace(work,{env=process.env,filesystem=statfsSync,storage=boardStorage}={}) {
 const selected=storage(work,{env});
 if(env.WFSIM_BOARD_STORAGE_REQUIRED!=='1')return selected;
 for(const [path,reserve] of [[selected,boardDataReserve(env)],['/',1024**3]]){
  const state=filesystem(path,{bigint:true});
  if(state.bavail*state.bsize<BigInt(reserve))throw Error('ranking storage headroom is below its reserve');
 }
 return selected;
}

export function guardCatalogFile(path){
 const file=resolve(path),parent=dirname(file),root=guardRankSpace(parent);
 if(process.env.WFSIM_BOARD_STORAGE_REQUIRED==='1'&&
    ((parent!==root&&!parent.startsWith(root+sep))||realpathSync(parent)!==parent||
      (existsSync(file)&&!lstatSync(file).isFile())))throw Error('catalog file outside guarded ranking work directory');
 return file;
}

// SQLite can roll back up to one copy of its main file during a scope write.
export function limitRankScratch(db,bytes=process.env.WFSIM_BOARD_SCOPE_DB_BYTES){
 if(bytes==null)return;
 const cap=Number(bytes),page=db.prepare('PRAGMA page_size').get().page_size;
 if(!Number.isSafeInteger(cap)||cap<page)throw Error('invalid ranking scope database budget');
 const pages=Math.floor(cap/page);
 if(db.prepare('PRAGMA max_page_count='+pages).get().max_page_count>pages)throw Error('existing ranking scope exceeds its database budget');
}
