// SPDX-License-Identifier: AGPL-3.0-or-later
// The real source driver seals and acknowledges inputs before isolated consumers finish their own outputs.
import assert from 'node:assert/strict';
import {copyFileSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync,existsSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {randomUUID} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {PGlite} from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import postgres from './pg/node_modules/postgres/src/index.js';
import {InventoryControl} from './inventory_control.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardProjection} from './prepared_board_projection.mjs';
import {PreparedWorkInventory} from './prepared_work_inventory.mjs';
import {PreparedWorkAudit} from './audit_prepared_work.mjs';
import {BoardPipeline} from './board_pipeline.mjs';
import {runPrepareBoardCycle,splitBoardContext,boardConsumerContract,publishPreparedInventory} from './split_board_pipeline.mjs';

const root=mkdtempSync(join(tmpdir(),'split-capture-')),work=join(root,'work'),bin=join(root,'bin');mkdirSync(work);mkdirSync(bin);mkdirSync(join(work,'tmp'));
copyFileSync(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN||'target/debug/wfsim-board'),join(bin,'wfsim-board'));
writeFileSync(join(bin,'ENGINE'),'0123456789ab');writeFileSync(join(bin,'VERSION'),'split-capture-fixture');
const url=process.env.WFSIM_BOARD_PIPELINE_PG_URL;if(url)assert(['127.0.0.1','localhost'].includes(new URL(url).hostname));
const tcp=url?postgres(url,{max:1,fetch_types:false,onnotice:()=>{}}):null,pg=tcp?null:new PGlite(),schema='split_'+randomUUID().replaceAll('-','');
let active=tcp,acks=0;const path=join(work,'inventory/split-state.json');
const query=(text,args=[])=>tcp?active.unsafe(text,args):pg.query(text,args).then(result=>result.rows);
const unsafe=async(text,args=[])=>{
 if(text.includes('inventory_ack(')) {
  const state=JSON.parse(readFileSync(path)),catalog=openCatalog(state.catalog);
  try {
   const latest=catalog.db.prepare("SELECT state FROM record_prepared_board_cuts ORDER BY id DESC LIMIT 1").get();
   assert.equal(latest.state,'sealed');assert.equal(existsSync(state.inventory),false,'source ACK waited for work consumer');acks++;
  }finally{catalog.close();}
 }
 return query(text,args);
};
const sql={unsafe,begin:async(options,fn)=>{
 if(tcp)return tcp.begin(options,async tx=>{active=tx;try{return await fn({unsafe});}finally{active=tcp;}});
 await pg.exec('BEGIN '+options);try{const result=await fn({unsafe});await pg.exec('COMMIT');return result;}catch(error){await pg.exec('ROLLBACK');throw error;}
}};
const native=await openInventoryOperator(join(bin,'wfsim-board')),pipeline=new BoardPipeline(work);let board,inventory,audit;
try {
 const ddl=['schema','library','sync','inventory','build_workflows'].map(name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8')).join('\n');
 if(tcp){await tcp.unsafe('CREATE SCHEMA '+schema+';SET search_path TO '+schema+',pg_catalog,pg_temp').simple();await tcp.unsafe(ddl).simple();}else await pg.exec(ddl);
 await assert.rejects(runPrepareBoardCycle({work,bin,sql,operator:native}),/explicit rebuild task/);
 assert.equal((await query('SELECT inventory_status() status'))[0].status?.generation??null,null);
 const source=JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[0].record;
 const [description]=await native.call('describe',[{id:'seed',record:source}]);
 await query("INSERT INTO entity_weapon_builds(id,at,record) VALUES($1,'day',$2)",[description.candidate,JSON.stringify(description.record)]);
 const control=new InventoryControl(join(work,'inventory'));
 try{control.request({mode:'board',...await splitBoardContext(bin,native),reason:'split source fixture',maxNewBytes:1024**3});}finally{control.close();}
 let lost=true;
 await assert.rejects(runPrepareBoardCycle({work,bin,sql,operator:native,fault:async stage=>{
  if(stage==='after-prepared-input-commit'&&lost){lost=false;throw Error('lost durable source reply');}
 }}),/lost durable source reply/);
 let result;for(let n=0;;n++){assert(n<100);result=await runPrepareBoardCycle({work,bin,sql,operator:native});if(!result.pending)break;}
 assert(acks>0);const state=JSON.parse(readFileSync(path)),generation=state.generation;
 assert.equal(Number((await query('SELECT count(*) n FROM state_compute_inventory_invalidations'))[0].n),0);
 const context=await splitBoardContext(bin,native);
 inventory=new PreparedWorkInventory(state.inventory,state.catalog,boardConsumerContract('inventory',state),native.contract,{...context,generation,create:true});
 audit=new PreparedWorkAudit(state.audit);
 board=new PreparedBoardProjection(state.board,state.catalog,boardConsumerContract('projection',state),native.contract,work,{...context,generation,create:true});
 mkdirSync(join(work,'pub/data'),{recursive:true});writeFileSync(join(work,'pub/data/board_state.yaml'),'boards: {}\n');
 for(let n=0;;n++){assert(n<100);const step=await board.step(native.call,{budgetMs:20,enqueue:async({epoch,receipt})=>{
  const receiptPath=join(work,'input-receipt.json');writeFileSync(receiptPath,JSON.stringify(receipt));
  execFileSync(process.execPath,['scripts/board_heads.mjs',join(work,'verified')],{stdio:['ignore','pipe','pipe']});
  execFileSync('python3',['scripts/board_meta.py',join(work,'verified'),join(work,'pub/data/board_state.yaml'),join(work,'verified/meta.json'),receiptPath],{stdio:['ignore','pipe','pipe']});
  const stamp=JSON.parse(readFileSync(join(work,'verified/meta.json')));assert.equal(stamp.inputs.snapshot,receipt.snapshot);
  assert.equal(stamp.inputs.engine,receipt.engine);assert.equal(stamp.inputs.input_digest,receipt.input_digest);
  return pipeline.enqueue({epoch,receipt,boardDir:join(work,'verified')});
 }});if(!step.pending)break;}
 assert.equal(inventory.consumer.state().acknowledged,0);assert.equal(pipeline.status().publication.pending,1);
 for(let n=0;;n++){assert(n<100);const step=await inventory.step(native.call,{budgetMs:20,audit:(index,reader,cut,options)=>audit.run(index,reader,cut,{...options,scratchRoot:join(work,'tmp')})});if(!step.pending)break;}
 await publishPreparedInventory(sql,inventory,state,context.source,()=>{});
 const published=(await query("SELECT body::jsonb body FROM state_factory_findings WHERE kind='task_inventory'"))[0].body;
 assert.equal(published.engine,context.engine);assert.equal(published.snapshot,inventory.consumer.acknowledgement().snapshot);
 assert(published.total>0);assert.equal(published.registered,0,'potential QIDs were replaced by registered task rows');
 assert.equal((await runPrepareBoardCycle({work,bin,sql,operator:native})).pending,false);
 console.log('ok: '+(tcp?'actual TCP PostgreSQL':'PGlite')+' explicit generation, source crash/restart and early durable ACK; independent audited board/outbox; full potential-work publication with original snapshot/engine');
}finally{board?.close();inventory?.close();audit?.close();pipeline.close();await native.close();
 if(tcp){await tcp.unsafe('RESET search_path;DROP SCHEMA '+schema+' CASCADE').simple();await tcp.end();}else await pg.close();rmSync(root,{recursive:true,force:true});}
