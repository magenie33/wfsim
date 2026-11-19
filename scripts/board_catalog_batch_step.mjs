// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {registerCatalogScope} from './adopt_rank_catalog.mjs';
import {catalogPageDigest} from './board_rank_catalog_replay.mjs';
import {finishCatalogScan} from './board_rank_catalog_scan.mjs';
import {agreedPage,catalogHead,checkCanonicalHead,selectedAtOrigins} from './board_catalog_readback.mjs';
import {pageSummary,pinRows,transaction} from './board_catalog_batch_state.mjs';

function originalReservation(row,inventory){
 const w=inventory.reservations.find(x=>x.weapon===row.weapon&&x.ruler===row.ruler);
 if(row.request_position!==null)assert(w?.request&&String(w.request.position)===row.request_position&&
  String(w.partition_mail?.version)===row.request_version&&(w.rank?.cut??null)===row.original_cut,
  'original player reservation changed');
 return w??null;
}

function recordReading(journal,row,revision){
 journal.prepare("UPDATE scopes SET phase='reading',revision=? WHERE weapon=? AND ruler=?").run(revision,row.weapon,row.ruler);
 row.phase='reading';row.revision=revision;
}

async function checkPages(ctx,row,receipt,head){
 const {journal,sql,origins,sets}=ctx,keys=[row.weapon,row.ruler];
 checkCanonicalHead(head,receipt,ctx.scenes.get(row.ruler),row.manifest);
 const pin=journal.prepare('SELECT tag,rows FROM pins WHERE weapon=? AND ruler=? AND revision=? LIMIT 1').get(...keys,head.revision);
 assert(head.token||pin,'unqualified legacy public selection');
 if(pin)assert(pin.rows===Number(head.rows),'protected row count changed');
 if(row.revision!==head.revision||row.phase!=='reading'){
  if(!await selectedAtOrigins(origins,head,...keys))return 'awaiting-origins';
  recordReading(journal,row,head.revision);
 }
 const verified=journal.prepare('SELECT last FROM pages WHERE weapon=? AND ruler=? AND revision=? ORDER BY offset DESC LIMIT 1')
  .get(...keys,head.revision);
 let after=verified?.last??0;
 const count=Number(head.rows);
 if(!verified||after<count){
  for(let i=0;i<ctx.pageBudget;i++){
   const last=Math.min(count,after+128);
   const immutable=pin?pinRows(sets,pin.tag,...keys,after,last):null;
   const page=await agreedPage(sql,origins,head,...keys,after,immutable);
   if(!page)return 'awaiting-revision';
   journal.prepare('INSERT INTO pages VALUES(?,?,?,?,?,?)').run(...keys,head.revision,after,page.last,page.digest);
   after=page.last;if(after===count)break;
  }
  if(after<count)return 'reading';
 }
 const summary=pageSummary(journal,row,count);
 const initial=journal.prepare('SELECT pages_sha256 FROM initial_heads WHERE weapon=? AND ruler=? AND revision=?').get(...keys,head.revision);
 if(initial?.pages_sha256)assert.equal(summary,initial.pages_sha256,'initial selected pages changed');
 if(head.token){
  const digest=await catalogPageDigest(sql,{token:head.token,output_rows:count,output_sha256:head.output_sha256});
  assert.equal(summary,digest,'readback no longer matches the entire stored Native output');
  if(head.output_sha256===row.ordered)assert.equal(summary,receipt.pages_sha256,'frozen Native pages differ from qualification');
 }
 if(!await selectedAtOrigins(origins,head,...keys))return 'awaiting-origins';
 const now=await catalogHead(sql,...keys);
 if(now?.revision!==head.revision)return 'advanced-during-read';
 checkCanonicalHead(now,receipt,ctx.scenes.get(row.ruler),row.manifest);
 const disposition=row.manifest?'adopted':pin?'retained':'advanced',at=new Date().toISOString();
 transaction(journal,()=>{
  journal.prepare('INSERT OR REPLACE INTO verified VALUES(?,?,?,?,?,?)').run(...keys,head.revision,disposition,summary,at);
  journal.prepare(`UPDATE scopes SET phase='done',disposition=?,pages_sha256=?,verified_at=?
   WHERE weapon=? AND ruler=? AND revision=?`).run(disposition,summary,at,...keys,head.revision);
 });
 return disposition;
}

export async function batchStep(ctx){
 const {journal,sql,contract,scenes}=ctx;
 const row=journal.prepare(`SELECT * FROM scopes WHERE phase IN ('preparing','pending','reading')
  ORDER BY priority,weapon,ruler LIMIT 1`).get()||
  journal.prepare("SELECT * FROM scopes WHERE phase='ready' ORDER BY priority,weapon,ruler LIMIT 1").get();
 if(!row)return {status:'covered'};
 const keys=[row.weapon,row.ruler],receipt=JSON.parse(row.receipt);
 const result=status=>({status,weapon:row.weapon,ruler:row.ruler});
 const head=await catalogHead(sql,...keys);
 if(head?.enabled&&head.revision){
  if(row.phase==='preparing'&&row.manifest&&!head.catalog_phase){
   journal.prepare('UPDATE scopes SET manifest=NULL WHERE weapon=? AND ruler=?').run(...keys);row.manifest=null;
  }
  try{return result(await checkPages(ctx,row,receipt,head));}
  catch(e){
   // A superseded revision may be collected while an origin serves its last cached catalog.
   if((await catalogHead(sql,...keys))?.revision!==head.revision)return result('advanced-during-read');
   throw e;
  }
 }
 if(!scenes.has(row.ruler)){
  assert(!head&&receipt.reason==='ruler absent from pinned Native'&&receipt.rows===0&&receipt.members===0&&receipt.retained>0,
   'retired ruler has live or protected rows');
  journal.prepare("UPDATE scopes SET phase='retired',disposition='retired-ruler',verified_at=? WHERE weapon=? AND ruler=?")
   .run(new Date().toISOString(),...keys);return result('retired-ruler');
 }
 if(head?.catalog_phase){
  const p=head.catalog_proof;
  assert(row.manifest&&head.cut===row.manifest&&head.rank_source===contract.source&&head.native===contract.native&&
   ['frozen','replay'].includes(head.catalog_phase)&&head.revision===null&&!head.enabled&&head.canonical_sources&&
   p?.pages_sha256===receipt.pages_sha256&&p.snapshot===receipt.snapshot&&p.archive===receipt.archive,
   'registered partition differs from the batch checkpoint');
  if((head.error&&!head.available_at)||(head.publication_error&&!head.publication_available))
   throw Error('catalog reader held '+keys.join('/')+': '+(head.error||head.publication_error));
  journal.prepare("UPDATE scopes SET phase='pending' WHERE weapon=? AND ruler=?").run(...keys);
  return result('awaiting-reader');
 }
 assert(row.phase!=='pending'&&row.phase!=='reading','a registered or selected partition disappeared');
 const m=await ctx.emit(row.weapon,row.ruler);
 assert(m.source===contract.source&&m.native===contract.native&&m.snapshot===contract.snapshot&&m.captured_at===contract.at&&
  m.mailbox_floor===contract.floor&&m.weapon===row.weapon&&m.ruler===row.ruler&&(!row.manifest||row.manifest===m.sha256),
  'scope manifest changed');
 const reservation=originalReservation(row,ctx.inventory());
 journal.prepare("UPDATE scopes SET phase='preparing',manifest=? WHERE weapon=? AND ruler=?").run(m.sha256,...keys);
 await ctx.capacity();
 const installed=ctx.files?await ctx.files.stage(m):null;
 await ctx.capacity();
 const response=await registerCatalogScope(sql,{work:ctx.work,manifest:m,receipt,contract:ctx.native.contract,epoch:contract.epoch,
  reservation,installed,verifyPredecessor:ctx.files?.verify});
 if(response.status==='advanced')return result('advanced-during-registration');
 assert(['frozen','replay'].includes(response.status),'unexpected registration status');
 journal.prepare("UPDATE scopes SET phase='pending' WHERE weapon=? AND ruler=?").run(...keys);
 return result(response.status);
}

export async function closeBatch(ctx){
 const {sql,journal,contract}=ctx;
 const incomplete=journal.prepare("SELECT count(*) n FROM scopes WHERE phase NOT IN ('done','retired')").get().n;
 assert(!incomplete,'public readback is incomplete');
 const changed=[];
 const result=await sql.begin(async tx=>{
  // Lock only the selected pointer rows for the local receipt comparison and the close.
  const read=journal.prepare("SELECT weapon,ruler,revision FROM scopes WHERE phase='done' ORDER BY weapon,ruler");
  let batch=[];
  async function compare(){
   const live=await tx.unsafe(`SELECT k.weapon,k.ruler,s.revision,s.enabled,s.canonical_sources
    FROM state_board_read_partitions s JOIN jsonb_to_recordset($1::text::jsonb) k(weapon text,ruler text)
    USING(weapon,ruler) ORDER BY k.weapon COLLATE "C",k.ruler COLLATE "C" FOR SHARE OF s`,[JSON.stringify(batch)]);
   assert(live.length===batch.length,'a verified public selection disappeared');
   live.forEach((r,i)=>{
    const b=batch[i];assert(r.weapon===b.weapon&&r.ruler===b.ruler&&r.enabled&&r.canonical_sources&&r.revision,'invalid selected scope');
    if(r.revision!==b.revision)changed.push(b);
   });batch=[];
  }
  for(const row of read.iterate()){batch.push(row);if(batch.length===128)await compare();}
  if(batch.length)await compare();
  if(changed.length)return {status:'advanced',scopes:changed.length};
  const closed=await finishCatalogScan({begin:fn=>fn(tx)},contract.source,contract.native,read.iterate(),contract.epoch);
  assert(closed.closed);return {status:'closed',scopes:journal.prepare("SELECT count(*) n FROM scopes WHERE phase='done'").get().n};
 });
 transaction(journal,()=>{
  for(const r of changed)journal.prepare("UPDATE scopes SET phase='reading' WHERE weapon=? AND ruler=?").run(r.weapon,r.ruler);
  if(result.status==='closed')journal.prepare('UPDATE run SET closed=1 WHERE id=1').run();
 });
 return result;
}
