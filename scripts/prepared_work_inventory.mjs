// SPDX-License-Identifier: AGPL-3.0-or-later
// Potential-question enumeration owns its database and never gates the board consumer.
import {openCatalog} from './incremental_inventory.mjs';
import {InventoryRelations} from './inventory_relations.mjs';
import {PreparedBoardReader} from './prepared_board_reader.mjs';
import {PreparedBoardConsumer} from './prepared_board_consumer.mjs';
import {preparedWorkRows} from './prepare_board_cut.mjs';

export class PreparedWorkInventory {
  constructor(path,source,contract,native,{engine,generation,databaseBytes,create=false}={}) {
    this.catalog=openCatalog(path,{engine,generation,contract,create,databaseBytes});
    try {
      this.index=new InventoryRelations(this.catalog,native);
      this.reader=new PreparedBoardReader(this.catalog.db,source);
      this.catalog.recordPool=this.reader;this.catalog.sourceRecords=this.reader.sourceRecords();
      this.consumer=new PreparedBoardConsumer(this.catalog.db,this.reader,'inventory',contract);
    } catch(error){this.catalog.close();throw error;}
  }
  invalidate(items,previous,cut) {
    const candidates=new Set(),index=this.index;
    for(const item of items) {
      if(item.kind==='source') {
        const old=this.reader.value('source',item.key,previous)?.value;
        if(old?.candidate)candidates.add(old.candidate);
        if(item.value?.candidate)candidates.add(item.value.candidate);
        this.catalog.statement('INSERT OR IGNORE INTO entity_inventory_builds(build_id) VALUES(?)').run(item.key);
        index.dirty(item.key);
      } else if(['fact','reset'].includes(item.kind)) {
        const identity=item.value?.identity||item.value?.id;
        if(!identity)throw Error('prepared measurement has no identity');
        index.touchGroups(identity);
        if(item.kind==='reset')index.q('DELETE FROM state_catalog_measurements WHERE identity=?').run(identity);
        else {
          const visible=item.first_cut<=cut&&(item.last_cut==null||item.last_cut>cut);
          const value=visible?item.value:this.reader.value('fact',item.key,cut)?.value;
          if(value?.gate_ruler) {
            const bucket=index.q('SELECT bucket FROM entity_catalog_candidates WHERE id=?').get(identity)?.bucket??null;
            index.q('INSERT OR REPLACE INTO state_catalog_measurements VALUES(?,?,?,?,?,?,?,?,?)')
              .run(identity,value.ruler,value.mode,Number(value.score),value.scene,bucket,value.retired_at??value.finished_at??'',Number(value.retired_at!=null),value.gate_ruler);
          }
        }
        index.touchGroups(identity);index.dirty(identity);
      }
    }
    for(const id of candidates)index.candidate(id);
  }
  async step(operate,{budgetMs=1000,check=()=>{},audit,fault=async()=>{}}={}) {
    const cut=this.catalog.transaction(()=>{
      const next=this.consumer.begin();
      if(next&&this.catalog.meta.prepared_cut!==next.id)this.catalog.setMeta({...this.catalog.meta,complete:false,prepared_cut:next.id});
      return next;
    });if(!cut)return {pending:false};
    this.reader.bindViews(cut.id);
    const until=performance.now()+budgetMs;
    do {
      check();const available=this.consumer.page();if(!available.length)break;
      const workAt=available.findIndex(item=>item.kind==='work');
      const page=workAt<0?available:available.slice(0,workAt||1);
      const work=[];
      for(const item of page)if(item.kind==='work')work.push(...await preparedWorkRows(item.value));
      this.catalog.transaction(()=>{
        this.invalidate(page,this.consumer.state().acknowledged,cut.id);
        for(const {kind,row} of work) {
          if(kind==='queue')this.index.queue([row]);
          else if(kind==='reset')this.index.queue([],{replace:[row.id]});
          else this.catalog.applyLifecycle([row],{bootstrap:cut.receipt.capture.bootstrap});
        }
        this.consumer.progress(page.at(-1).ordinal+1);
      });
      await fault('after-inventory-input-commit',{cut:cut.id});
    } while(performance.now()<until);
    if(this.consumer.state().cursor<cut.input_count)return {pending:true,stage:'inputs',cut:cut.id};
    const result=await this.index.step(operate,{budgetMs:Math.max(0,until-performance.now())});
    if(result.pending)return {pending:true,stage:'enumerate',cut:cut.id,metrics:result.metrics};
    if(!audit)throw Error('an independent full work inventory audit is required');
    const proof=await audit(this.index,this.reader,cut,{check});
    if(proof?.complete!==true||proof.plan?.snapshot!==cut.receipt.snapshot||proof.plan.engine!==cut.receipt.engine)
      throw Error('independent work inventory audit did not complete');
    this.consumer.complete({...proof,complete:true,cut:cut.id,snapshot:cut.receipt.snapshot,engine:cut.receipt.engine});
    return {pending:!!this.reader.next(cut.id),stage:'complete',cut:cut.id,audit:proof};
  }
  close(){this.catalog.close();}
}
