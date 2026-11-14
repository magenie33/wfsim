// SPDX-License-Identifier: AGPL-3.0-or-later
// Fold every retained input through one pinned snapshot; only a full audit may acknowledge the span.
import {createHash} from 'node:crypto';
import {canon} from '../worker/task_contract.js';
import {PreparedBoardConsumer} from './prepared_board_consumer.mjs';

const sha=value=>createHash('sha256').update(value).digest('hex');
const initial=sha('prepared-work-span-v1');
const binding=cut=>({cut:cut.id,input_count:cut.input_count,input_digest:cut.sealed_digest,
 snapshot:cut.receipt.snapshot,engine:cut.receipt.engine,started_at:cut.receipt.started_at});
const extend=(digest,cut)=>sha(digest+'\n'+canon(binding(cut)));

export function workSpanReceipt(reader,after,target) {
 if(!Number.isSafeInteger(after)||after<0||!Number.isSafeInteger(target)||target<=after)throw Error('invalid work input span');
 let previous=after,digest=initial,inputs=0,cuts=0;
 while(previous<target) {
  const next=reader.db.prepare("SELECT id FROM record_prepared_board_cuts WHERE id>? AND state='sealed' ORDER BY id LIMIT 1").get(previous);
  const cut=next?reader.cut(next.id):null;
  if(!cut||cut.id!==previous+1||cut.id>target||cut.state!=='sealed')throw Error('work input span has a missing cut');
  digest=extend(digest,cut);inputs+=cut.input_count;cuts++;previous=cut.id;
 }
 return {v:1,after,first:after+1,last:target,cuts,inputs,digest};
}
export function validateWorkSpan(reader,span,target,{after}={}) {
 if(!span||span.v!==1||span.last!==target||after!==undefined&&(span.after>after||after>target))throw Error('invalid work input span');
 const receipt=workSpanReceipt(reader,span.after,target);
 if(canon(receipt)!==canon(Object.fromEntries(Object.keys(receipt).map(key=>[key,span[key]]))))throw Error('work input span receipt differs');
 return receipt;
}

export class PreparedWorkConsumer extends PreparedBoardConsumer {
 constructor(...args) {
  super(...args);
  this.db.exec(`CREATE TABLE IF NOT EXISTS state_work_input_span(name TEXT PRIMARY KEY,body TEXT NOT NULL);
   CREATE TABLE IF NOT EXISTS record_work_input_cuts(name TEXT,cut INTEGER,body TEXT NOT NULL,PRIMARY KEY(name,cut)) WITHOUT ROWID;`);
 }
 span(){return JSON.parse(this.db.prepare('SELECT body FROM state_work_input_span WHERE name=?').get(this.name)?.body||'null');}
 save(span){this.db.prepare('INSERT OR REPLACE INTO state_work_input_span VALUES(?,?)').run(this.name,canon(span));}
 begin() {
  const state=this.state();
  if(state.cut>state.acknowledged) {
   if(!this.span()||this.span().last!==state.cut)throw Error('active work input span is missing');
   return this.reader.cut(state.cut);
  }
  const latest=this.reader.db.prepare("SELECT id FROM record_prepared_board_cuts WHERE id>? AND state='sealed' ORDER BY id DESC LIMIT 1").get(state.acknowledged);
  if(!latest)return null;
  const first=this.reader.next(state.acknowledged);
  if(first.id!==state.acknowledged+1)throw Error('work input span has a missing cut');
  this.save({v:1,after:state.acknowledged,first:first.id,last:latest.id,cut:first.id,cursor:0,cuts:0,inputs:0,digest:initial});
  this.db.prepare('UPDATE state_prepared_input_consumption SET cut=?,cursor=0,audit=NULL WHERE name=?').run(latest.id,this.name);
  return this.reader.cut(latest.id);
 }
 advance() {
  let span=this.span();
  for(let n=0;span&&span.cuts<span.last-span.after&&n<256;n++) {
   const cut=this.reader.cut(span.cut);
   if(span.cursor!==cut.input_count)break;
   this.db.prepare('INSERT OR REPLACE INTO record_work_input_cuts VALUES(?,?,?)').run(this.name,cut.id,canon(binding(cut)));
   span={...span,cuts:span.cuts+1,inputs:span.inputs+cut.input_count,digest:extend(span.digest,cut)};
   if(cut.id<span.last) {
    const next=this.reader.next(cut.id);
    if(!next||next.id!==cut.id+1||next.id>span.last)throw Error('work input span has a missing cut');
    span={...span,cut:next.id,cursor:0};
   } else this.db.prepare('UPDATE state_prepared_input_consumption SET cursor=? WHERE name=?').run(cut.input_count,this.name);
   this.save(span);
  }
  return span;
 }
 page(limit=256) {
  const span=this.advance();
  if(!span||span.cuts===span.last-span.after)return [];
  const page=this.reader.readPage(span.cut,span.cursor,limit);
  this.pending={cut:span.cut,cursor:span.cursor,end:page.at(-1)?.ordinal+1};
  return page;
 }
 progress(cursor) {
  const span=this.span(),pending=this.pending;
  if(!pending||pending.cut!==span.cut||pending.cursor!==span.cursor||!Number.isSafeInteger(cursor)
    ||cursor<=span.cursor||cursor>pending.end)throw Error('invalid work span progress');
  this.save({...span,cursor});this.pending=null;this.advance();
 }
 inputsPending(){const span=this.span();return span.cuts!==span.last-span.after;}
 auditSpan() {
  const span=this.span(),observed=this.reader.db.prepare('SELECT acknowledged FROM state_prepared_board_consumers WHERE name=?').get(this.name)?.acknowledged;
  if(!Number.isSafeInteger(observed)||observed>span.after)throw Error('source acknowledgement is ahead of local work');
  return workSpanReceipt(this.reader,observed,span.last);
 }
 complete(audit) {
  const span=this.span();
  if(this.inputsPending()||!audit?.input_span)throw Error('work span audit is incomplete');
  const receipt=validateWorkSpan(this.reader,audit.input_span,span.last,{after:span.after});
  for(let cut=receipt.first;cut<=receipt.last;cut++) {
   const held=this.db.prepare('SELECT body FROM record_work_input_cuts WHERE name=? AND cut=?').get(this.name,cut);
   if(held?.body!==canon(binding(this.reader.cut(cut))))throw Error('work span progress receipt differs');
  }
  return super.complete({...audit,input_span:receipt});
 }
}
