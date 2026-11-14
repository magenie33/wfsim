// SPDX-License-Identifier: AGPL-3.0-or-later
// Explicit offline component reuse preserves the old cache and checks every raw source binding.
import {DatabaseSync} from 'node:sqlite';
import {existsSync,lstatSync,statSync} from 'node:fs';
import {resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {inflateRawSync} from 'node:zlib';
import {canon} from '../worker/task_contract.js';
import {fileHash} from './inventory_publication_io.mjs';
import {readInventoryPage} from './inventory_projection.mjs';

const sha=body=>createHash('sha256').update(body).digest('hex');
const stamp=path=>{const s=statSync(path,{bigint:true});return [s.dev,s.ino,s.size,s.mtimeNs,s.ctimeNs].map(String).join(':');};
export async function inspectNormalizedSourceBase(path,{engine,manifestSha,native}) {
 path=resolve(path);if(lstatSync(path).isSymbolicLink())throw Error('normalized source base cannot be linked');
 const files=[];
 for(const name of [path,path+'-wal'])if(existsSync(name)) {
  const before=stamp(name),hash=await fileHash(name);if(stamp(name)!==before)throw Error('normalized source base is still changing');
  files.push({path:name,stamp:before,sha256:hash});
 }
 const db=new DatabaseSync(path,{readOnly:true});
 try {
  const meta=JSON.parse(db.prepare('SELECT body FROM state_inventory_generation WHERE id=1').get().body),c=meta.contract;
  if(meta.engine!==engine||c.algorithm!=='offline-board-projection-v1'||c.manifest_sha!==manifestSha
    ||c.record_storage!=='interned-deflate-v1'||c.adapter!==native.adapter||canon(c.operator)!==canon(native.contract)
    ||!meta.input||meta.input.file<1)throw Error('normalized source base is not a complete matching source component');
  const counts={sources:db.prepare('SELECT count(*) n FROM entity_catalog_sources').get().n,
   records:db.prepare('SELECT count(*) n FROM entity_inventory_builds WHERE stored IS NOT NULL').get().n};
  if(counts.sources!==counts.records)throw Error('normalized source base has incomplete raw records');
  return {codec:'interned-deflate-v1',path,files,meta_sha:sha(canon(meta)),producer_contract:c,source_rows:counts.sources,
   reuse_scope:'normalized source component only; no workload, facts or publication completion inherited'};
 }finally{db.close();}
}
export async function adoptPreparedSources(inputs,cut,base,{budgetMs=1000,check=()=>{},fault=async()=>{}}={}) {
 if(inputs.contract.record_base?.meta_sha!==base.meta_sha)throw Error('source adoption was not explicitly pinned');
 const receipt={...cut.receipt,normalizer:inputs.contract.normalizer,capture:JSON.parse(JSON.stringify(cut))};
 const id=inputs.begin(receipt),held=inputs.cut(id);
 if(held.state==='sealed')return {pending:false,cut:id};
 let cursor=held.checkpoint?JSON.parse(held.checkpoint):{file:0,offset:0,count:0};
 if(cursor.file>=1)return {pending:false,cut:id};
 if(!cut.bootstrap||cut.meta.counts.library!==base.source_rows)throw Error('source adoption requires the exact complete captured baseline');
 const old=new DatabaseSync(base.path,{readOnly:true}),until=performance.now()+budgetMs;
 try {
  const lookup=old.prepare(`SELECT s.candidate,s.body,r.stored,p.modes FROM entity_catalog_sources s
   JOIN entity_inventory_builds r ON r.build_id=s.id LEFT JOIN state_catalog_publication_sources p ON p.id=s.id WHERE s.id=?`);
  do {
   check();for(const file of base.files)if(stamp(file.path)!==file.stamp)throw Error('normalized source base changed during adoption');
   const page=await readInventoryPage(cut.raw.builds,cursor.offset,255),items=[];
   const ordinal=inputs.cut(id).input_count;
   for(let n=0;n<page.rows.length;n++) {
    const row=page.rows[n],cached=lookup.get(row.id),rawRef='@'+sha(canon(row.record));
    if(!cached||cached.stored!==rawRef)throw Error('cached source does not match its protected raw record');
    const d=JSON.parse(typeof cached.body==='string'?cached.body:inflateRawSync(cached.body).toString());
    const value={...d,id:row.id,candidate:cached.candidate,raw_record_ref:rawRef};
    if(cached.modes!=null)value.publication_modes=JSON.parse(cached.modes);
    if(value.record||value.record_ref&&!/^@[a-f0-9]{64}$/.test(value.record_ref))throw Error('cached source record codec changed');
    items.push({kind:'source',key:row.id,value,ordinal:ordinal+n});
   }
   let next={file:0,offset:page.offset,count:cursor.count+page.rows.length};
   if(page.done) {
    if(next.count!==base.source_rows)throw Error('incomplete protected source adoption');next={file:1,offset:0,count:0};
   }
   inputs.append(id,items,{checkpoint:next});await fault('after-adopted-source-commit',{cut:id,cursor:next});cursor=next;
   if(page.done)return {pending:false,cut:id};
  }while(performance.now()<until);
  return {pending:true,cut:id,cursor};
 }finally{old.close();}
}
