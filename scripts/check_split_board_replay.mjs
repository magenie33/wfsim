// SPDX-License-Identifier: AGPL-3.0-or-later
// The offline driver resumes isolated source and consumer checkpoints and keeps protected inputs unchanged.
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,copyFileSync,readdirSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,basename} from 'node:path';
import {createHash} from 'node:crypto';
import {openInventoryOperator} from './inventory_operator.mjs';
import {replaySplitBoard} from './replay_split_board.mjs';
import {openCatalog} from './incremental_inventory.mjs';
import {InventoryRelations} from './inventory_relations.mjs';
import {fileHash} from './inventory_publication_io.mjs';

const root=mkdtempSync(join(tmpdir(),'split-replay-')),capture=join(root,'capture'),bin=join(root,'bin');for(const dir of[capture,bin])mkdirSync(dir);
copyFileSync(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'),join(bin,'wfsim-board'));
const engine='0123456789ab';writeFileSync(join(bin,'ENGINE'),engine);
const native=await openInventoryOperator(join(bin,'wfsim-board')),sha=body=>createHash('sha256').update(body).digest('hex');
try {
 const rawRecords=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url))).slice(0,3).map(row=>row.record);
 const descriptions=await native.call('describe',rawRecords.map((record,n)=>({id:String(n),record}))),builds=rawRecords.map((record,n)=>({id:descriptions[n].candidate,record}));
 const facts=descriptions.slice(0,2).flatMap((d,n)=>native.contract.rulers.flatMap(ruler=>d.modes.map(mode=>({identity:d.candidate,ruler:ruler.id,mode,score:120+n,
  measured_by:'verified:fixture',started_at:'2026-11-14T00:00:00Z',finished_at:'2026-11-14T00:00:01Z',cost_seconds:1}))));
 const queue=[{build_id:descriptions[2].candidate,ruler:native.contract.rulers[0].id,mode:descriptions[2].modes[0]}],raw={};
 for(const [kind,rows]of Object.entries({builds,facts,queue,tasks:[]})){raw[kind]=join(capture,kind+'.ndjson');writeFileSync(raw[kind],rows.map(row=>JSON.stringify(row)+'\n').join(''));}
 raw.meta=join(capture,'snapshot.json');writeFileSync(raw.meta,JSON.stringify({engine,snapshot:'frozen',started_at:'2026-11-14T00:00:00Z',counts:{library:builds.length,facts:facts.length,queue:1,tasks:0}}));
 const manifest={bootstrap:true,raw,files:Object.values(raw).map(path=>({path:'/protected/'+basename(path),sha256:sha(readFileSync(path))}))};
 const manifestPath=join(capture,'capture.json');writeFileSync(manifestPath,JSON.stringify(manifest));const manifestSha=await fileHash(manifestPath);
 const options={capture,bin,manifestSha,diskBudgetBytes:10*1024**3,report:()=>{}};
 const work=join(root,'fresh');let lost=true;
 await assert.rejects(replaySplitBoard({...options,work,fault:async stage=>{if(stage==='after-prepared-input-commit'&&lost){lost=false;throw Error('lost durable source response');}}}),/durable source response/);
 await assert.rejects(replaySplitBoard({...options,work,fault:async stage=>{if(stage==='after-independent-board-package')throw Error('inventory still unstarted');}}),/inventory still unstarted/);
 const middle=JSON.parse(readFileSync(join(work,'split-replay.json')));assert.equal(middle.board_complete,true);assert.equal(middle.inventory_initialized,undefined);
 const complete=await replaySplitBoard({...options,work});assert.equal(complete.phase,'complete');assert.equal(complete.database_epoch,null);assert.equal(complete.publication_remote,false);assert.equal(complete.published,false);
 assert.equal(complete.board_audit.complete,true);assert.equal(complete.inventory_audit.complete,true);
 const repeated=await replaySplitBoard({...options,work});assert.equal(repeated.inventory_audit.digest,complete.inventory_audit.digest);
 const basePath=join(root,'base.sqlite'),base=openCatalog(basePath,{engine,contract:{algorithm:'offline-board-projection-v1',manifest_sha:manifestSha,record_storage:'interned-deflate-v1',adapter:native.adapter,operator:native.contract},create:true});
 const index=new InventoryRelations(base,native.contract);await index.sources(builds,native.call);base.setMeta({...base.meta,input:{file:1,offset:0,count:0}});base.close();
 const before=await fileHash(basePath),reused=await replaySplitBoard({...options,work:join(root,'reused'),reuseSources:basePath});
 assert.equal(reused.inventory_audit.digest,complete.inventory_audit.digest);assert.equal(reused.board_audit.digest,complete.board_audit.digest);assert.equal(await fileHash(basePath),before);
 for(const file of manifest.files)assert.equal(await fileHash(join(capture,basename(file.path))),file.sha256);
 const stamp=readFileSync(join(work,'verified/meta.json'),'utf8');assert.equal(JSON.parse(stamp).inputs.snapshot,'frozen');
 writeFileSync(raw.facts,readFileSync(raw.facts,'utf8')+'\n');await assert.rejects(replaySplitBoard({...options,work}),/protected capture input changed/);
 console.log('ok: independent offline source crash/restart, board package before inventory, complete exact consumer outputs and immutable source reuse; original inputs/cache unchanged and source tampering rejects; no PostgreSQL or remote publication');
}finally{await native.close();rmSync(root,{recursive:true,force:true});}
