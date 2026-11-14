// SPDX-License-Identifier: AGPL-3.0-or-later
// Matching native source components can be reused without changing their store or inheriting downstream completion.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,copyFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {openInventoryOperator} from './inventory_operator.mjs';
import {openCatalog} from './incremental_inventory.mjs';
import {InventoryRelations} from './inventory_relations.mjs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {PreparedBoardReader} from './prepared_board_reader.mjs';
import {prepareBoardCut} from './prepare_board_cut.mjs';
import {inspectNormalizedSourceBase,adoptPreparedSources} from './adopt_prepared_sources.mjs';
import {fileHash} from './inventory_publication_io.mjs';
import {canon} from '../worker/task_contract.js';

const root=mkdtempSync(join(tmpdir(),'source-adoption-')),native=await openInventoryOperator(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'));
const engine='0123456789ab',manifestSha='a'.repeat(64),codec={record_storage:'interned-deflate-v1'},basePath=join(root,'base.sqlite');
const raw=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url))).slice(0,2).map((row,n)=>({id:'source-'+n,record:row.record}));
raw.push({id:'invalid',record:{weapon:'unknown-weapon'}});
let old=openCatalog(basePath,{engine,contract:{...codec,algorithm:'offline-board-projection-v1',engine,manifest_sha:manifestSha,adapter:native.adapter,operator:native.contract},create:true});
const index=new InventoryRelations(old,native.contract);await index.sources(raw,native.call);
old.setMeta({...old.meta,input:{file:1,offset:0,count:0}});old.close();old=null;
const paths={};for(const [kind,rows]of Object.entries({builds:raw,facts:[],queue:[],tasks:[]})) {
 paths[kind]=join(root,kind+'.ndjson');writeFileSync(paths[kind],rows.map(row=>JSON.stringify(row)+'\n').join(''));
}
const cut={bootstrap:true,raw:paths,meta:{counts:{library:raw.length,facts:0,queue:0,tasks:0}},receipt:{epoch:'one',engine,snapshot:'frozen',started_at:'2026-11-14T00:00:00Z'}};
const normalizer={v:1,adapter:native.adapter,operator:native.contract},base=await inspectNormalizedSourceBase(basePath,{engine,manifestSha,native});
const adopted=openCatalog(join(root,'adopted.sqlite'),{engine,contract:codec,create:true}),fresh=openCatalog(join(root,'fresh.sqlite'),{engine,contract:codec,create:true});
const inputs=new PreparedBoardInputs(adopted.db,{engine,normalizer,record_base:base}),freshInputs=new PreparedBoardInputs(fresh.db,{engine,normalizer});
let readerDb;
try {
 const before=await fileHash(basePath);let lost=true;
 await assert.rejects(adoptPreparedSources(inputs,cut,base,{fault:async()=>{if(lost){lost=false;throw Error('lost adopted reply');}}}),/lost adopted reply/);
 await adoptPreparedSources(inputs,cut,base);
 let result;do{result=await prepareBoardCut(inputs,adopted,cut,native.call);}while(result.pending);
 let expected;do{expected=await prepareBoardCut(freshInputs,fresh,cut,native.call);}while(expected.pending);
 for(const row of raw)assert.equal(canon(inputs.value('source',row.id,result.cut).value),canon(freshInputs.value('source',row.id,expected.cut).value));
 assert.equal(adopted.db.prepare('SELECT count(*) n FROM entity_inventory_records').get().n,0,'source adoption copied shared raw/canonical bodies');
 readerDb=new DatabaseSync(join(root,'reader.sqlite'));const reader=new PreparedBoardReader(readerDb,join(root,'adopted.sqlite'));reader.bindViews(result.cut);
 for(const row of raw)assert.equal(reader.recordBody(inputs.value('source',row.id,result.cut).value.raw_record_ref),canon(row.record));
 assert.throws(()=>readerDb.exec("DELETE FROM record_base.entity_inventory_records"),/readonly/);
 assert.equal(await fileHash(basePath),before,'reuse modified the original source cache');
 await assert.rejects(inspectNormalizedSourceBase(basePath,{engine,manifestSha,native:{...native,adapter:'b'.repeat(64)}}),/matching source component/);
 const badPath=join(root,'bad.sqlite');copyFileSync(basePath,badPath);const bad=new DatabaseSync(badPath);
 bad.prepare('UPDATE entity_inventory_builds SET stored=? WHERE build_id=?').run('@'+'0'.repeat(64),raw[0].id);bad.close();
 const invalid=await inspectNormalizedSourceBase(badPath,{engine,manifestSha,native}),badRecords=openCatalog(join(root,'bad-target.sqlite'),{engine,contract:codec,create:true});
 try {const target=new PreparedBoardInputs(badRecords.db,{engine,normalizer,record_base:invalid});await assert.rejects(adoptPreparedSources(target,cut,invalid),/protected raw record/);}
 finally{badRecords.close();}
 console.log('ok: explicit source component reuse matches fresh native values, shares read-only bodies, preserves original bytes and resumes a lost response; changed native or raw binding rejects; no downstream completion inherited');
}finally{readerDb?.close();adopted.close();fresh.close();await native.close();rmSync(root,{recursive:true,force:true});}
