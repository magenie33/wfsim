// SPDX-License-Identifier: AGPL-3.0-or-later
// Normalize bounded pages before a short commit; durable cursors advance with their shared references.
import {createHash} from 'node:crypto';
import {canon} from '../worker/task_contract.js';
import {readInventoryPage} from './inventory_projection.mjs';

const sha=value=>createHash('sha256').update(canon(value)).digest('hex');
export async function prepareBoardCut(inputs,records,cut,operate,{budgetMs=1000,check=()=>{},fault=async()=>{}}={}) {
  if(records.db!==inputs.db||records.meta.contract.record_storage!=='interned-deflate-v1')throw Error('prepared inputs require their atomic shared record pool');
  const receipt={...cut.receipt,normalizer:inputs.contract.normalizer,capture:JSON.parse(JSON.stringify(cut))};
  const id=inputs.begin(receipt),held=inputs.cut(id);
  if(held.state==='sealed')return {pending:false,cut:id};
  let cursor=held.checkpoint?JSON.parse(held.checkpoint):{file:0,offset:0,count:0};
  const files=cut.bootstrap?['builds','facts','queue','tasks'].map(kind=>[kind,cut.raw[kind]]):[['delta',cut.delta]];
  const until=performance.now()+budgetMs;
  while(cursor.file<files.length) {
    check();const [kind,path]=files[cursor.file],page=await readInventoryPage(path,cursor.offset,255);
    const rows=kind==='delta'?page.rows:page.rows.map(row=>({kind,row}));
    const items=[],intern=[];
    for(let from=0;from<rows.length;) {
      const group=rows[from].kind;let to=from+1;while(to<rows.length&&rows[to].kind===group)to++;
      const batch=rows.slice(from,to).map(item=>item.row);
      if(group==='builds') {
        const descriptions=await operate('describe',batch.map(row=>({id:row.id,record:row.deleted?null:row.record})));
        if(descriptions.length!==batch.length)throw Error('incomplete prepared source descriptions');
        for(let n=0;n<batch.length;n++) {
          const row=batch[n],value={...descriptions[n]};if(value.id!==row.id)throw Error('prepared source order changed');
          if(row.deleted||row.record==null)items.push({kind:'source',key:row.id,value:null});
          else {
            value.raw_record_ref='@'+sha(row.record);intern.push(row.record);
            if(value.record){value.record_ref='@'+sha(value.record);intern.push(value.record);delete value.record;}
            items.push({kind:'source',key:row.id,value});
          }
        }
      } else if(group==='facts') {
        const normalized=await operate('facts',batch);
        if(normalized.length!==batch.length)throw Error('incomplete prepared facts');
        for(const value of normalized)if(value.mode && Number.isFinite(Number(value.score)))
          items.push({kind:'fact',key:canon([value.identity,value.ruler,value.mode]),value});
      } else if(group==='reset')for(const value of batch)items.push({kind:'reset',key:value.id,value});
      else if(!['queue','tasks'].includes(group))throw Error('unknown prepared input kind');
      from=to;
    }
    // Work-only pages remain references to the sealed capture rather than a second queue body store.
    if(rows.some(item=>['queue','tasks','reset'].includes(item.kind)))items.push({kind:'work',key:canon([id,cursor.file,cursor.offset]),
      value:{path,offset:cursor.offset,count:page.rows.length,kind,sha:sha(page.rows)}});
    let next={...cursor,offset:page.offset,count:cursor.count+page.rows.length};
    if(page.done) {
      const expected=cut.bootstrap&&cut.meta.counts[kind==='builds'?'library':kind];
      if(cut.bootstrap&&next.count!==expected)throw Error('incomplete prepared input '+kind);
      next={file:cursor.file+1,offset:0,count:0};
    }
    const ordinal=inputs.cut(id).input_count;
    inputs.append(id,items.map((item,n)=>({...item,ordinal:ordinal+n})),
      {beforeWrite:()=>{for(const record of intern)records.internRecord(record);},checkpoint:next});
    await fault('after-prepared-input-commit',{cut:id,cursor:next});cursor=next;
    if(performance.now()>=until)return {pending:true,cut:id,cursor};
  }
  const digest=createHash('sha256');let offset=0;
  if(cut.bootstrap&&inputs.cut(id).source_count!==cut.meta.counts.library)throw Error('incomplete prepared source identity set');
  for(;;) {
    const page=inputs.db.prepare('SELECT ordinal,count,input_sha FROM record_prepared_board_changes WHERE cut=? AND ordinal>=? ORDER BY ordinal LIMIT 4096').all(id,offset);
    if(!page.length)break;for(const row of page)digest.update(row.input_sha+'\n');offset=page.at(-1).ordinal+page.at(-1).count;
  }
  inputs.seal(id,{inputCount:inputs.cut(id).input_count,digest:digest.digest('hex')});
  return {pending:false,cut:id};
}

export async function preparedWorkRows(value) {
  const page=await readInventoryPage(value.path,value.offset,value.count);
  if(page.rows.length!==value.count||sha(page.rows)!==value.sha)throw Error('prepared work input changed');
  return value.kind==='delta'?page.rows.filter(item=>['queue','tasks','reset'].includes(item.kind))
    :page.rows.map(row=>({kind:value.kind,row}));
}
