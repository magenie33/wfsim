// SPDX-License-Identifier: AGPL-3.0-or-later
// Confirm an already committed scope without reserving another publication budget.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createReadStream,lstatSync} from 'node:fs';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {freshDirectory,readFreshQualification} from './board_catalog_fresh.mjs';
import {agreedPage,catalogHead,checkCanonicalHead,selectedAtOrigins} from './board_catalog_readback.mjs';
import {catalogPageDigest} from './board_rank_catalog_replay.mjs';
import {pageSummary,transaction} from './board_catalog_batch_state.mjs';

async function fileDigest(path){
 assert(lstatSync(path).isFile(),'scope manifest is not a regular file');
 const hash=createHash('sha256');
 for await(const chunk of createReadStream(path))hash.update(chunk);
 return hash.digest('hex');
}

function selectedReceipt({state,ledger,native,contract},weapon,ruler){
 const row=ledger.prepare('SELECT body FROM receipts WHERE weapon=? AND ruler=?').get(weapon,ruler);
 if(contract.priority_fresh){
  assert(!row,'retained scope requires its original qualification');
  return readFreshQualification({state,contract,native},weapon,ruler);
 }
 const original=JSON.parse(row.body);
 if(!native.policyAdapter)return {receipt:original};
 const db=new DatabaseSync(join(freshDirectory(state,weapon,ruler),`retained-${native.policyAdapter}.sqlite`),{readOnly:true});
 try{
  const current=JSON.parse(db.prepare("SELECT v FROM progress WHERE k='receipt'").get().v);
  assert(current.policy===native.policyAdapter&&current.archive===original.archive&&current.snapshot===original.snapshot&&
   current.source===original.source&&current.native===original.native&&current.members===original.members,
   'retained policy receipt differs from the archived scope');
  return {receipt:current};
 }finally{db.close();}
}

export async function completePriorityScope(ctx,revision){
 const {journal,sql,contract,origins,scenes,priorityScope,pageBudget,target}=ctx;
 const {weapon,ruler}=priorityScope;
 const keys=[weapon,ruler],job=journal.prepare('SELECT * FROM priority_work WHERE weapon=? AND ruler=?').get(...keys);
 assert(job&&['pending','selected'].includes(job.phase),'scope has no pending published registration');
 const {receipt,manifest:fresh}=selectedReceipt(ctx,...keys);
 assert(receipt.eligible&&receipt.unresolved===0&&receipt.weapon===weapon&&receipt.ruler===ruler&&
  receipt.source===contract.source&&receipt.native===contract.native&&receipt.snapshot===(fresh?.snapshot??contract.snapshot)&&receipt.archive===contract.archive,
  'scope completion requires its qualified receipt');
 const head=await catalogHead(sql,...keys);
 assert(head?.revision===revision&&head.token,'pinned Native publication changed');
 const manifest=await fileDigest(join(target,'inputs.ndjson'));
 if(fresh)assert(manifest===fresh.sha256,'fresh scope manifest differs from the registered capture');
 const check=selected=>{
  checkCanonicalHead(selected,receipt,scenes.get(ruler),manifest);
  if(fresh)assert(selected.catalog_proof.policy===receipt.policy,'fresh scope policy proof changed');
 };
 check(head);
 if(!await selectedAtOrigins(origins,head,...keys))return {status:'awaiting-origins',...priorityScope};
 const count=Number(head.rows);
 const verified=journal.prepare('SELECT last FROM pages WHERE weapon=? AND ruler=? AND revision=? ORDER BY offset DESC LIMIT 1')
  .get(...keys,revision);
 let after=verified?.last??0;
 if(!verified||after<count){
  for(let i=0;i<pageBudget;i++){
   const page=await agreedPage(sql,origins,head,...keys,after,null);
   if(!page)return {status:'awaiting-revision',...priorityScope};
   journal.prepare('INSERT INTO pages VALUES(?,?,?,?,?,?)').run(...keys,revision,after,page.last,page.digest);
   after=page.last;if(after===count)break;
  }
 }
 if(after<count)return {status:'reading',...priorityScope,after};
 const summary=pageSummary(journal,{weapon,ruler,revision},count);
 assert.equal(summary,await catalogPageDigest(sql,{token:head.token,output_rows:count,output_sha256:head.output_sha256}),
  'public pages differ from the committed Native generation');
 if(!await selectedAtOrigins(origins,head,...keys))return {status:'awaiting-origins',...priorityScope};
 await sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1)',[contract.source]);
  const [locked]=await tx.unsafe(`SELECT 1 FROM state_board_read_partitions s JOIN state_board_public_heads p USING(weapon,ruler,revision)
   JOIN state_board_rank_partitions r USING(weapon,ruler)
   WHERE s.weapon=$1 AND s.ruler=$2 AND s.revision=$3 AND s.enabled AND s.canonical_sources
   AND r.source=$4 AND r.native=$5 AND r.complete AND r.catalog_phase='active'
   FOR SHARE OF s,p,r`,[...keys,revision,contract.source,contract.native]);
  assert(locked,'public selection advanced');
  check(await catalogHead(tx,...keys));
  await tx.unsafe('DELETE FROM state_board_rank_new_scopes WHERE weapon=$1 AND ruler=$2',keys);
  await tx.unsafe('UPDATE state_board_requested_partitions SET enqueued=true,activated_at=board_input_timestamp(clock_timestamp()) WHERE weapon=$1 AND ruler=$2 AND activated_at IS NULL',keys);
 });
 transaction(journal,()=>{
  journal.prepare('INSERT OR REPLACE INTO verified VALUES(?,?,?,?,?,?)').run(...keys,revision,'adopted',summary,new Date().toISOString());
  journal.prepare("UPDATE priority_work SET phase='selected',due=0,reason=NULL WHERE weapon=? AND ruler=?").run(...keys);
 });
 return {status:'selected',...priorityScope,revision,rows:count,pages_sha256:summary};
}
