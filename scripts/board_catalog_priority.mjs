// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {copyFileSync,renameSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {reconcileSelectedInputs} from './board_catalog_delivery.mjs';
import {registerCatalogScope} from './adopt_rank_catalog.mjs';
import {catalogHead} from './board_catalog_readback.mjs';
import {freshDirectory,freshPublication,qualifyRetainedScope,readFreshQualification} from './board_catalog_fresh.mjs';
import {transaction} from './board_catalog_batch_state.mjs';

export function preparePriority(db,contract){
 db.exec(`CREATE TABLE IF NOT EXISTS priority_cursor(feed TEXT PRIMARY KEY,value TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS priority_work(weapon TEXT,ruler TEXT,priority INTEGER NOT NULL,phase TEXT NOT NULL,
  due INTEGER NOT NULL,reason TEXT,arrival TEXT NOT NULL,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
 CREATE INDEX IF NOT EXISTS priority_ready ON priority_work(priority,due,weapon,ruler) WHERE phase<>'selected' AND phase<>'retired';
 CREATE INDEX IF NOT EXISTS priority_pending ON priority_work(phase,priority);`);
 if(!db.prepare("SELECT 1 FROM pragma_table_info('priority_work') WHERE name='position'").get()){
  transaction(db,()=>{
   db.exec(`ALTER TABLE priority_work ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
    WITH ordering AS MATERIALIZED (
     SELECT weapon,ruler,row_number() OVER(ORDER BY priority,arrival,weapon,ruler) position FROM priority_work
    ) UPDATE priority_work SET position=ordering.position,priority=CASE
     WHEN priority<=0 AND length(arrival)=20 AND arrival NOT GLOB '*[^0-9]*' THEN 0
     WHEN priority<=0 THEN 3 ELSE 99 END
    FROM ordering WHERE (priority_work.weapon,priority_work.ruler)=(ordering.weapon,ordering.ruler);
    CREATE UNIQUE INDEX priority_position ON priority_work(position);
    CREATE INDEX priority_zone_fifo ON priority_work(priority,position) WHERE phase NOT IN('selected','retired');`);
  });
 }
 const pin=JSON.stringify([contract.source,contract.cluster,contract.native,contract.archive,contract.epoch,...(contract.priority_scope?[contract.priority_scope]:[]),...(contract.priority_fresh?[contract.priority_fresh]:[])]);
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
  const select='SELECT weapon,ruler,queued_at,zone,'+stamp+' changed_at FROM state_board_rank_new_scopes';
  const order=stamp+',weapon,ruler';
  rows=await sql.unsafe(select+' WHERE ('+order+')>($1,$2,$3) ORDER BY '+order+' LIMIT 32',cursor||[boundary[0],'','']);
  latest=await sql.unsafe(select+' ORDER BY zone,coalesce(zone_position,0),queued_at,weapon,ruler LIMIT 32');
  const r=rows.at(-1);tail=r&&[r.changed_at,r.weapon,r.ruler];
 }else if(feed==='accepted'){
  const select='SELECT weapon,ruler,queued_at,zone FROM state_board_rank_new_scopes';
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
 transaction(db,()=>{
  let position=db.prepare('SELECT coalesce(max(position),0) n FROM priority_work').get().n;
  const known=db.prepare('SELECT phase FROM priority_work WHERE weapon=? AND ruler=?');
  const put=db.prepare(`INSERT INTO priority_work(weapon,ruler,priority,phase,due,reason,arrival,position)
   VALUES(?,?,?,'ready',0,NULL,?,?)
   ON CONFLICT(weapon,ruler) DO UPDATE SET
    priority=CASE WHEN priority_work.phase='selected' THEN excluded.priority ELSE min(priority_work.priority,excluded.priority) END,
    arrival=CASE WHEN priority_work.phase='selected' OR priority_work.arrival NOT GLOB '*[^0-9]*'
     THEN excluded.arrival ELSE min(priority_work.arrival,excluded.arrival) END,
    position=CASE WHEN priority_work.phase='selected' OR excluded.priority<priority_work.priority THEN excluded.position ELSE priority_work.position END,
    phase=CASE WHEN priority_work.phase='selected' THEN 'ready' ELSE priority_work.phase END,
    due=CASE WHEN priority_work.phase='selected' THEN 0 ELSE priority_work.due END`);
  const changed=feed==='latest'||feed==='request'||feed==='accepted';
  for(const row of [...latest,...rows]){
   const existing=known.get(row.weapon,row.ruler);
   if(existing?.phase==='retired'||(!changed&&existing?.phase==='selected'))continue;
   const zone=row.zone??(feed==='request'?0:99);
   if(!Number.isInteger(zone)||zone<0||zone>99)throw Error('invalid publication zone');
   const arrival=row.queued_at??new Date().toISOString();
   put.run(row.weapon,row.ruler,zone,arrival,++position);
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
  if(contract.priority_fresh&&head?.catalog_phase){
   const {receipt,manifest}=readFreshQualification(ctx,...keys),proof=head.catalog_proof;
   assert(head.cut===manifest.sha256&&head.rank_source===contract.source&&head.native===contract.native&&
    proof?.snapshot===receipt.snapshot&&proof.archive===receipt.archive&&proof.pages_sha256===receipt.pages_sha256&&
    proof.rows===receipt.rows&&proof.policy===receipt.policy,
    'fresh scope registration differs from its qualified capture');
   if(head.enabled&&head.revision)return result('pending','awaiting-completion',2500);
  }
  if(head?.enabled&&head.revision){
   assert(!contract.priority_fresh,'fresh scope already has a public head');
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
  if(contract.priority_fresh&&!head?.catalog_phase){
   const [published]=await sql.unsafe('SELECT revision FROM state_board_public_heads WHERE weapon=$1 AND ruler=$2',keys);
   assert(!published,'fresh scope already has a public head');
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
  if(receipt){
   assert(receipt.eligible&&receipt.unresolved===0&&receipt.archive===contract.archive&&receipt.snapshot===contract.snapshot,'local scope qualification incomplete');
   if(native.policyAdapter){
    receipt=await qualifyRetainedScope(ctx,weapon,ruler);
    if(!receipt.complete)return result('preparing','preparing-retained',100);
    assert(receipt.eligible&&receipt.unresolved===0&&receipt.policy===native.policyAdapter,'riven policy qualification incomplete');
   }
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
   reservation:witness,installed,verifyPredecessor:ctx.files.verify,requireFresh:!!contract.priority_fresh});
  return result('pending',r.status,500);
 }catch(e){
  if(/capacity|headroom|allocation|storage reserve|supervisor disconnected/i.test(e.message))throw e;
  return result('held','held',30000,e.message);
 }
}

export async function nextPriority(ctx,now=Date.now()){
 if(ctx.priorityScope){
  const {journal:db,sql,priorityScope:{weapon,ruler}}=ctx;
  const keys=[weapon,ruler];
  const held=db.prepare('SELECT * FROM priority_work WHERE weapon=? AND ruler=?').get(...keys);
  if(held)return held.phase!=='selected'&&held.phase!=='retired'&&held.due<=now?held:null;
  const [next]=await sql.unsafe(`SELECT weapon,ruler,zone,queued_at FROM state_board_rank_new_scopes
   ORDER BY zone,coalesce(zone_position,0),queued_at,weapon,ruler LIMIT 1`);
  if(!next)return null;
  if(next.weapon!==weapon||next.ruler!==ruler)throw Error('approved scope is not the current priority head');
  transaction(db,()=>db.prepare(`INSERT INTO priority_work(weapon,ruler,priority,phase,due,reason,arrival,position)
   VALUES(?,?,?,'ready',0,NULL,?,1)`).run(...keys,next.zone,next.queued_at));
  return db.prepare('SELECT * FROM priority_work WHERE weapon=? AND ruler=?').get(...keys);
 }
 const db=ctx.journal,turn=Number(db.prepare("SELECT value FROM priority_cursor WHERE feed='turn'").get()?.value||0);
 await intake(ctx,turn);
 const active=db.prepare("SELECT count(*) n FROM priority_work WHERE phase IN('pending','preparing')").get().n;
 return db.prepare(`SELECT * FROM priority_work WHERE phase NOT IN('selected','retired') AND due<=?
  AND (? OR phase IN('pending','preparing')) ORDER BY priority,position LIMIT 1`).get(now,active<(ctx.maxPending??6)?1:0);
}

export async function priorityStep(ctx,now=Date.now()){
 const job=await nextPriority(ctx,now);
 if(!ctx.priorityScope){
  const turn=Number(ctx.journal.prepare("SELECT value FROM priority_cursor WHERE feed='turn'").get().value);
  if(turn%8===1)await reconcileSelectedInputs(ctx,now);
 }
 return job?advance(ctx,job,now):{status:'idle'};
}
