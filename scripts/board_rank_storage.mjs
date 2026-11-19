// SPDX-License-Identifier: AGPL-3.0-or-later
// Preparation stops before consuming the mounted Data reserve or system WAL headroom.
import {dirname,resolve,sep} from 'node:path';
import {existsSync,lstatSync,realpathSync,statfsSync} from 'node:fs';
import {boardStorage} from './board_storage.mjs';
export function guardRankSpace(work,{env=process.env,filesystem=statfsSync,storage=boardStorage}={}) {
 const selected=storage(work,{env});
 if(env.WFSIM_BOARD_STORAGE_REQUIRED!=='1')return selected;
 for(const [path,reserve] of [[selected,32*1024**3],['/',1024**3]]){
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
