// SPDX-License-Identifier: AGPL-3.0-or-later
// An audited native board consumes fixed inputs independently of potential-work enumeration.
import {openCatalog} from './incremental_inventory.mjs';
import {PreparedBoardReader} from './prepared_board_reader.mjs';
import {PreparedBoardConsumer} from './prepared_board_consumer.mjs';
import {BoardProjectionState} from './board_projection_state.mjs';
import {BoardProjectionAudit} from './audit_board_projection.mjs';
import {InventoryPublication} from './inventory_publication.mjs';
import {createHash} from 'node:crypto';
import {canon} from '../worker/task_contract.js';

export class PreparedBoardProjection {
 constructor(path,source,contract,native,work,{engine,generation,databaseBytes,create=false,context={},fault=async()=>{}}={}) {
  this.records=openCatalog(path,{engine,generation,contract,create,databaseBytes});this.context=context;this.work=work;this.fault=fault;
  try {
   this.reader=new PreparedBoardReader(this.records.db,source);this.records.recordPool=this.reader;
   this.board=new BoardProjectionState(this.records,contract);this.index=this.board.index(native);
   this.audit=new BoardProjectionAudit(this.records.db,this.reader,work);
   context.freezeBaseline=value=>this.audit.freezeBaseline(value);
   this.consumer=new PreparedBoardConsumer(this.records.db,this.reader,'projection',contract);
   this.publication=new InventoryPublication(this.index,work,context,fault);
  }catch(error){this.records.close();throw error;}
 }
 async step(operate,{budgetMs=1000,check=()=>{},enqueue}={}) {
  const cut=this.board.transaction(()=>{
   const next=this.consumer.begin();
   if(next&&this.board.meta.prepared_cut!==next.id)this.board.setMeta({...this.board.meta,complete:false,prepared_cut:next.id});
   return next;
  });if(!cut)return {pending:false};
  this.reader.bindViews(cut.id);const until=performance.now()+budgetMs;
  do {
   check();const page=this.consumer.page();if(!page.length)break;
   this.board.transaction(()=>{
    for(const item of page) {
     if(item.kind==='source') {
      const old=this.reader.value('source',item.key,this.consumer.state().acknowledged)?.value;
      if(old?.bucket)this.index.publicationDirty(old.bucket[0]);
      if(item.value?.bucket)this.index.publicationDirty(item.value.bucket[0]);
     } else if(['fact','reset'].includes(item.kind)) {
      const identity=item.value?.identity||item.value?.id;
      for(const row of this.records.db.prepare('SELECT DISTINCT weapon FROM state_catalog_publication_sources WHERE candidate=?').all(identity))
       this.index.publicationDirty(row.weapon);
     }
    }
    this.consumer.progress(page.at(-1).ordinal+1);
   });
   await this.fault('after-board-input-commit',{cut:cut.id});
  }while(performance.now()<until);
  if(this.consumer.state().cursor<cut.input_count)return {pending:true,stage:'inputs',cut:cut.id};
  check();const output=await this.publication.step(cut.receipt,operate,{budgetMs:Math.max(0,until-performance.now())});
  if(output.pending)return {pending:true,stage:'render',cut:cut.id};
  check();const proof={...await this.audit.run(cut,operate,this.index.contract,{check}),input_digest:cut.sealed_digest,
   normalizer_sha:createHash('sha256').update(canon(this.reader.contract.normalizer)).digest('hex'),
   consumer_sha:createHash('sha256').update(canon(this.board.meta.contract)).digest('hex')};
  await this.fault('after-board-audit-before-package',{cut:cut.id,proof});
  if(!enqueue)throw Error('board completion requires an immutable publication handoff');
  const publication=await enqueue({epoch:cut.receipt.epoch,work:this.work,receipt:proof});
  await this.fault('after-board-package-before-ack',{cut:cut.id,publication});
  this.board.transaction(()=>{
   this.consumer.complete({...proof,publication});
   this.board.setMeta({...this.board.meta,complete:true,snapshot:proof.snapshot,started_at:proof.started_at,audit_digest:proof.digest});
  });
  return {pending:!!this.reader.next(cut.id),stage:'complete',cut:cut.id,audit:proof,publication};
 }
 close(){this.context.writer?.close();this.records.close();}
}
