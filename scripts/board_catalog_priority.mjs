// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {copyFileSync,renameSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {registerCatalogScope} from './adopt_rank_catalog.mjs';
import {catalogHead} from './board_catalog_readback.mjs';
import {freshDirectory,freshPublication} from './board_catalog_fresh.mjs';
import {transaction} from './board_catalog_batch_state.mjs';

export function preparePriority(db,contract){
 db.exec(`CREATE TABLE IF NOT EXISTS priority_cursor(feed TEXT PRIMARY KEY,value TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS priority_work(weapon TEXT,ruler TEXT,priority INTEGER NOT NULL,phase TEXT NOT NULL,
  due INTEGER NOT NULL,reason TEXT,arrival TEXT NOT NULL,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS priority_ready ON priority_work(priority,due,weapon,ruler) WHERE phase<>'selected' AND phase<>'retired';
 CREATE INDEX IF NOT EXISTS priority_pending ON priority_work(phase,priority);`);
 const pin=JSON.stringify([contract.source,contract.cluster,contract.native,contract.archive,contract.epoch]);
 db.prepare("INSERT OR IGNORE INTO priority_cursor VALUES('contract',?)").run(pin);
 assert.equal(db.prepare("SELECT value FROM priority_cursor WHERE feed='contract'").get().value,pin,'priority publication boundary changed');
}

async function intake(ctx,turn){
 const {journal:db,sql,ledger}=ctx;
 let boundary=JSON.parse(db.prepare("SELECT value FROM priority_cursor WHERE feed='arrival_boundary'").get()?.value||'null');
 if(!boundary){
  const [r]=await sql.unsafe("SELECT board_input_timestamp(clock_timestamp()) at,coalesce((SELECT max(position) FROM state_board_requested_partitions WHERE activated_at IS NULL),0)::text position");
  boundary=[r.at,r.position];db.prepare("INSERT INTO priority_cursor VALUES('arrival_boundary',?)").run(JSON.stringify(boundary));
 }
 const feed=['latest','request','accepted','history','latest','request','sparse','history'][turn%8];
 const cursor=JSON.parse(db.prepare('SELECT value FROM priority_cursor WHERE feed=?').get(feed)?.value||'null');
 let rows,tail,latest=[];
 if(feed==='latest'){
  const stamp='coalesce(last_measured_at,queued_at)';
  const select='SELECT weapon,ruler,queued_at first_queued,'+stamp+' queued_at FROM state_board_rank_new_scopes';
  const order=stamp+',weapon,ruler';
  rows=await sql.unsafe(select+' WHERE ('+order+')>($1,$2,$3) ORDER BY '+order+' LIMIT 32',cursor||[boundary[0],'','']);
  latest=await sql.unsafe(select+' WHERE '+stamp+'>=$1 ORDER BY '+stamp+' DESC,weapon DESC,ruler DESC LIMIT 8',[boundary[0]]);
  const r=rows.at(-1);tail=r&&[r.queued_at,r.weapon,r.ruler];
 }else if(feed==='accepted'){
  const select='SELECT weapon,ruler,queued_at FROM state_board_rank_new_scopes';
  rows=await sql.unsafe(select+' WHERE (queued_at,weapon,ruler)>($1,$2,$3) ORDER BY queued_at,weapon,ruler LIMIT 256',cursor||['','','']);

  const r=rows.at(-1);tail=r&&[r.queued_at,r.weapon,r.ruler];
 }else if(feed==='request'){
  rows=await sql.unsafe('SELECT weapon,ruler,position FROM state_board_requested_partitions WHERE position>$1 AND activated_at IS NULL ORDER BY position LIMIT 32',[cursor||'0']);
  latest=await sql.unsafe('SELECT weapon,ruler,position FROM state_board_requested_partitions WHERE activated_at IS NULL AND position>$1 ORDER BY position DESC LIMIT 8',[boundary[1]]);
  tail=rows.at(-1)?.position;
 }else if(feed==='sparse'){
  // Seek the next scope, skipping all identities already captured for its predecessor.
  rows=await sql.unsafe("SELECT weapon,ruler FROM state_board_rank_catalog_inputs WHERE (weapon,ruler)>($1,$2) AND ruler<>'' ORDER BY weapon,ruler LIMIT 1",cursor||['','']);
  const r=rows.at(-1);tail=r&&[r.weapon,r.ruler];
 }else{
  rows=ledger.prepare('SELECT weapon,ruler FROM receipts WHERE (weapon,ruler)>(?,?) ORDER BY weapon,ruler LIMIT 64').all(...(cursor||['','']));
  const r=rows.at(-1);tail=r&&[r.weapon,r.ruler];
 }
 const priority=feed==='history'?2:feed==='sparse'?1:0;
 transaction(db,()=>{
  const known=ledger.prepare('SELECT 1 FROM receipts WHERE weapon=? AND ruler=?');
  const put=db.prepare(`INSERT INTO priority_work VALUES(?,?,?,'ready',0,NULL,?) ON CONFLICT(weapon,ruler)
   DO UPDATE SET priority=min(priority_work.priority,excluded.priority),arrival=CASE
    WHEN excluded.priority=-2 THEN excluded.arrival WHEN priority_work.priority=-2 THEN priority_work.arrival
    ELSE max(priority_work.arrival,excluded.arrival) END
   WHERE priority_work.priority>excluded.priority OR priority_work.arrival<excluded.arrival`);
  for(const row of [...rows,...latest]){
   const urgent=feed==='latest'||(feed==='request'&&BigInt(row.position)>BigInt(boundary[1]));
   const first=(feed==='latest'||feed==='accepted')&&!known.get(row.weapon,row.ruler);
   const arrival=first?(row.first_queued||row.queued_at):(row.queued_at||String(row.position||'').padStart(20,'0'));
   put.run(row.weapon,row.ruler,first?-2:urgent?-1:priority,arrival);
   if(['latest','request','accepted'].includes(feed))db.prepare("UPDATE priority_work SET phase='ready',due=0 WHERE weapon=? AND ruler=? AND phase='selected'").run(row.weapon,row.ruler);
  }
  db.prepare('INSERT OR REPLACE INTO priority_cursor VALUES(?,?)').run(feed,JSON.stringify(tail??null));
  db.prepare("INSERT OR REPLACE INTO priority_cursor VALUES('turn',?)").run(String((turn+1)%120));
 });
}

async function advance(ctx,job,now){
 const {sql,contract,journal:db,ledger,archive,native}=ctx,{weapon,ruler}=job,keys=[weapon,ruler];
 const result=(phase,status,delay=0,reason=null)=>{
  db.prepare('UPDATE priority_work SET phase=?,due=?,reason=? WHERE weapon=? AND ruler=?').run(phase,now+delay,reason,...keys);
  return {status,weapon,ruler,...(reason?{reason}:{})};
 };
 try{
  const head=await catalogHead(sql,...keys);
  if(head?.enabled&&head.revision){
   assert(head.complete&&head.rank_source===contract.source&&head.source===contract.source&&head.native===contract.native,'existing Native selection changed source');
   await sql.begin(async tx=>{
    await tx.unsafe('SELECT board_rank_source_lock($1)',[contract.source]);
    const [selected]=await tx.unsafe(`SELECT 1 FROM state_board_read_partitions n JOIN state_board_public_heads p USING(weapon,ruler,revision)
     WHERE n.weapon=$1 AND n.ruler=$2 AND n.enabled AND n.canonical_sources AND n.revision=$3 FOR SHARE OF n,p`,[...keys,head.revision]);
    assert(selected,'public selection advanced');
    await tx.unsafe('DELETE FROM state_board_rank_new_scopes WHERE weapon=$1 AND ruler=$2',keys);
    await tx.unsafe('UPDATE state_board_requested_partitions SET enqueued=true,activated_at=board_input_timestamp(clock_timestamp()) WHERE weapon=$1 AND ruler=$2 AND activated_at IS NULL',keys);
   });
   rmSync(freshDirectory(ctx.state,...keys),{recursive:true,force:true});
   return result('selected','selected');
  }
  if(head?.catalog_phase){
   const held=(head.error&&!head.available_at&&head.error)||(head.publication_error&&!head.publication_available&&head.publication_error);
   return held?result('held','held',60000,held):result('pending','awaiting-reader',2500);
  }
  const rows=ledger.prepare('SELECT body FROM receipts WHERE weapon=? AND ruler=?').get(...keys);
  let receipt=rows&&JSON.parse(rows.body),manifest;
  if(!ctx.scenes.has(ruler)){
   if(receipt?.eligible&&receipt.reason==='ruler absent from pinned Native'&&!receipt.rows&&!receipt.members)return result('retired','retired-ruler');
   return result('held','held',60000,'ruler absent from pinned Native');
  }
  const cap=ctx.maxPending??6;
  const slots=db.prepare(`SELECT count(*) total,count(*) FILTER(WHERE priority=-2) first,
   count(*) FILTER(WHERE priority=-1) recent,count(*) FILTER(WHERE priority>=0) older FROM priority_work
   WHERE phase IN('pending','preparing') AND (weapon,ruler)<>(?,?)`).get(...keys);
  const lane=job.priority===-2?'first':job.priority===-1?'recent':'older';
  const reserved=['first','recent','older'].filter(k=>k!==lane&&!slots[k]).length;
  if(slots.total>=cap-reserved)return result('ready','deferred',1500);
  if(receipt){
   assert(receipt.eligible&&receipt.unresolved===0&&receipt.archive===contract.archive&&receipt.snapshot===contract.snapshot,'local scope qualification incomplete');
   manifest=await ctx.emit(weapon,ruler);
  }else{
   const newScope=await freshPublication(ctx,...keys);
   if(!newScope)return result('preparing','preparing-fresh',100);
   ({manifest,receipt}=newScope);
   copyFileSync(manifest.path,join(ctx.target,'inputs.partial'));renameSync(join(ctx.target,'inputs.partial'),join(ctx.target,'inputs.ndjson'));
   manifest={...manifest,path:join(ctx.target,'inputs.ndjson')};
  }
  assert(receipt.members===archive.prepare('SELECT count(*) n FROM members WHERE weapon=? AND ruler=?').get(...keys).n,'archived member count changed');
  assert.equal(manifest.mailbox_floor,contract.floor);
  let witness=head?ctx.inventory().reservations.find(r=>r.weapon===weapon&&r.ruler===ruler):null;
  if(head&&!witness&&!head.cut){
   const [request]=await sql.unsafe('SELECT position FROM state_board_requested_partitions WHERE weapon=$1 AND ruler=$2 AND enqueued AND activated_at IS NULL',keys);
   const [mail]=await sql.unsafe("SELECT version,applied_version AS applied FROM state_board_input_mailbox WHERE scope='partition' AND key=$1",[JSON.stringify(keys)]);
   if(request&&mail)witness={weapon,ruler,request,partition_mail:mail};
  }
  await ctx.capacity();
  const installed=await ctx.files.stage(manifest);
  const r=await registerCatalogScope(sql,{work:ctx.work,manifest,receipt,contract:native.contract,epoch:contract.epoch,
   reservation:witness,installed,verifyPredecessor:ctx.files.verify});
  return result('pending',r.status,500);
 }catch(e){
  if(/capacity|headroom|allocation|storage reserve|supervisor disconnected/i.test(e.message))throw e;
  return result('held','held',30000,e.message);
 }
}

export async function nextPriority(ctx,now=Date.now()){
 const db=ctx.journal,turn=Number(db.prepare("SELECT value FROM priority_cursor WHERE feed='turn'").get()?.value||0);
 await intake(ctx,turn);
 const eligible="phase NOT IN('selected','retired') AND due<=?";
 const pick=(clause,order='due,weapon,ruler')=>db.prepare('SELECT * FROM priority_work WHERE '+eligible+clause+' ORDER BY '+order+' LIMIT 1').get(now);
 const lane=turn%8;
 const retry=lane===3&&pick(" AND phase='held'");
 const checking=[1,5].includes(lane)&&pick(" AND phase IN ('pending','preparing')");
 const type=[-2,-1,0,-1,1,-2,-1,2][lane];
 const order=type===-2||(type===-1&&lane===6)?'arrival,due,weapon,ruler':type===-1?'arrival DESC,due,weapon,ruler':undefined;
 const job=retry||checking||pick(' AND priority='+type,order)||pick(' AND priority=-2','arrival,due,weapon,ruler')||
  pick(' AND priority=-1')||pick(' AND priority=0')||pick('');
 return job;
}

export async function priorityStep(ctx,now=Date.now()){
 const job=await nextPriority(ctx,now);
 return job?advance(ctx,job,now):{status:'idle'};
}
