// SPDX-License-Identifier: AGPL-3.0-or-later
import {randomUUID} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {chmod,copyFile,mkdir,open} from 'node:fs/promises';
import {basename,dirname,join,resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
import {beginRankBaseline,verifyRankBaseline} from './board_rank_baseline.mjs';
import {rankFilePage,syncRankDirectory} from './board_rank_files.mjs';
import {guardRankSpace} from './board_rank_storage.mjs';
import {requireCatalogScan} from './board_rank_catalog_scan.mjs';
import {requestedReservation,preserveCatalogPredecessor,replaceCatalogPredecessor} from './board_rank_catalog_reservation.mjs';
import {openInventoryOperator} from './inventory_operator.mjs';

async function localCut(m,work){
 const root=guardRankSpace(work),dir=join(root,'rank-cut-'+randomUUID()),path=join(dir,'inputs.ndjson');
 await mkdir(dir,{mode:0o750});await copyFile(m.path,path);await chmod(path,0o440);
 const file=await open(path,'r');try{await file.sync();}finally{await file.close();}
 syncRankDirectory(dir);syncRankDirectory(root);
 await verifyRankBaseline({path,bytes:m.bytes,cut:m.sha256},root);
 return {path,bytes:m.bytes,sha256:m.sha256,meta:rankFilePage(path,0,1).rows[0]};
}

export async function registerCatalogScope(sql,{work,manifest:m,receipt:r,contract,expectedRevision=null,reservation=null,epoch=r?.snapshot,
 installed=null,verifyPredecessor=verifyRankBaseline}){
 const {source,native,weapon,ruler}=m,keys=[weapon,ruler],scene=contract.rulers.find(row=>row.id===ruler)?.scene;
 if(!r?.eligible||r.unresolved!==0||r.weapon!==weapon||r.ruler!==ruler||r.source!==source||
    r.snapshot!==m.snapshot||r.native!==native||!/^[a-f0-9]{64}$/.test(r.archive))throw Error('catalog qualification boundary changed');
 if(scene===undefined&&r.members===0&&r.retained>0&&r.rows===0&&r.reason==='ruler absent from pinned Native')
  return {status:'retired-ruler',weapon,ruler};
 if(scene===undefined||!Number.isSafeInteger(r.rows)||r.rows<0||!/^[a-f0-9]{64}$/.test(r.pages_sha256)||!/^[a-f0-9]{64}$/.test(m.sha256))
  throw Error('invalid catalog publication proof');
 const frozen={scene,snapshot:r.snapshot,archive:r.archive,pages_sha256:r.pages_sha256,rows:r.rows};
 return sql.begin(async tx=>{
  await tx.unsafe('SELECT board_rank_source_lock($1::text)',[source]);
  await tx.unsafe('SELECT pg_advisory_xact_lock(hashtextextended($1,0))',[JSON.stringify([source,'catalog',...keys])]);
  const [selected]=await tx.unsafe('SELECT * FROM state_board_read_partitions WHERE weapon=$1 AND ruler=$2 FOR UPDATE',keys);
  const [part]=await tx.unsafe('SELECT * FROM state_board_rank_partitions WHERE weapon=$1 AND ruler=$2 FOR UPDATE',keys);
  if(selected?.enabled&&selected.canonical_sources&&selected.revision&&selected.scene===scene){
   const [head]=await tx.unsafe('SELECT source FROM record_board_read_snapshots WHERE weapon=$1 AND ruler=$2 AND revision=$3',[...keys,selected.revision]);
   if(head?.source!==source||part?.source!==source||part.native!==native||!part.complete)throw Error('existing public head has no matching canonical index');
   if(['frozen','replay'].includes(part.catalog_phase))throw Error('public head advanced during catalog handoff');
   return {status:selected.revision===expectedRevision?'retained':'advanced',weapon,ruler,revision:selected.revision};
  }
  const [publicHead]=await tx.unsafe(`SELECT h.revision,s.source,s.inputs_sha256,i.archive,f.token
   FROM state_board_public_heads h LEFT JOIN record_board_read_snapshots s USING(weapon,ruler,revision)
   LEFT JOIN record_board_rank_snapshot_refs f USING(weapon,ruler,revision)
   LEFT JOIN state_board_public_import i ON i.id=1 WHERE h.weapon=$1 AND h.ruler=$2 FOR UPDATE OF h`,keys);
  if(publicHead&&(!publicHead.revision||publicHead.archive!==r.archive||publicHead.inputs_sha256!==r.archive||
     publicHead.source!=='archive:'+r.archive||publicHead.token))throw Error('current public head is outside the pinned archive');
  await requireCatalogScan(tx,m,epoch);
  if(expectedRevision!==null)throw Error('pinned public catalog head missing');
  if(part?.catalog_phase){
   if(!selected||selected.enabled||!selected.canonical_sources||selected.revision!==null||selected.scene!==scene||
      !['frozen','replay'].includes(part?.catalog_phase)||part.cut!==m.sha256||part.native!==native||part.source!==source||
      !Object.entries(frozen).every(([key,v])=>part.catalog_proof[key]===v))throw Error('catalog reservation changed');
   return {status:part.catalog_phase,weapon,ruler};
  }
  if(part||selected){
   Object.assign(frozen,await requestedReservation(tx,keys,scene,selected,part,reservation,publicHead?.revision??null));
   if(part&&(part.source!==source||part.native!==native))throw Error('occupied catalog epoch changed');
  }else if(reservation)throw Error('pinned requested reservation missing');
  const root=installed?resolve(work):guardRankSpace(work);
  const file=installed??await localCut(m,root),{path,meta}=file;
  if(file.bytes!==m.bytes||file.sha256!==m.sha256||basename(path)!=='inputs.ndjson'||
     !/^rank-cut-[a-f0-9-]{36}$/.test(basename(dirname(path)))||dirname(dirname(path))!==root)
   throw Error('catalog staged file differs from the manifest');
  if(meta?.kind!=='meta'||!['weapon','ruler','source','native','snapshot','captured_at','mailbox_floor'].every(k=>meta[k]===m[k]))
   throw Error('catalog manifest metadata changed');
  if(part){
   frozen.previous=await preserveCatalogPredecessor(tx,part,reservation,root,verifyPredecessor);
   await replaceCatalogPredecessor(tx,{...m,path});
  }else await beginRankBaseline(tx,{...m,path});
  if(selected)await tx.unsafe('UPDATE state_board_read_partitions SET canonical_sources=true WHERE weapon=$1 AND ruler=$2',keys);
  else await tx.unsafe('INSERT INTO state_board_read_partitions(weapon,ruler,scene,canonical_sources) VALUES($1,$2,$3,true)',[...keys,scene]);
  await tx.unsafe("UPDATE state_board_rank_partitions SET catalog_phase='frozen',catalog_proof=$3::text::jsonb WHERE weapon=$1 AND ruler=$2",[...keys,JSON.stringify(frozen)]);
  return {status:'frozen',weapon,ruler};
 });
}

if(process.argv[1]&&pathToFileURL(process.argv[1]).href===import.meta.url){
 const [ledgerPath,manifestPath,binary,work,...args]=process.argv.slice(2);
 const flag=args.indexOf('--requested');
 const inventory=flag<0?null:JSON.parse(readFileSync(args.splice(flag,2)[1]));
 if(args.length>1)throw Error('unexpected catalog adoption arguments');
 const [pin]=args;
 if(!process.env.PG_URL)throw Error('PG_URL is required');
 if(!work)throw Error('usage: adopt_rank_catalog.mjs LEDGER MANIFEST_JSON NATIVE WORK [PINNED_REVISION] [--requested INVENTORY_JSON]');
 const m=JSON.parse(readFileSync(manifestPath)),ledger=new DatabaseSync(ledgerPath,{readOnly:true});
 const native=await openInventoryOperator(binary);
 const {default:postgres}=await import('./pg/node_modules/postgres/src/index.js');
 const sql=postgres(process.env.PG_URL,{max:1,fetch_types:false,connection:{lock_timeout:2000,statement_timeout:10000}});
 try{
  const epoch=ledger.prepare('SELECT v FROM epoch WHERE id=1').get().v;
  const [source,,digest,archive,snapshot,at]=JSON.parse(epoch);
  const row=ledger.prepare('SELECT body FROM receipts JOIN evidenced USING(weapon,ruler) WHERE weapon=? AND ruler=?').get(m.weapon,m.ruler);
  if(!row)throw Error('missing qualified catalog receipt');
  const receipt=JSON.parse(row.body);
  if(m.source!==source||digest!==native.adapter||m.native!==digest||receipt.archive!==archive||m.snapshot!==snapshot||m.captured_at!==at)
   throw Error('qualified catalog epoch changed');
  console.log(JSON.stringify(await registerCatalogScope(sql,{work,manifest:m,receipt,contract:native.contract,expectedRevision:pin??null,epoch,
   reservation:inventory?.reservations.find(row=>row.weapon===m.weapon&&row.ruler===m.ruler)})));
 }finally{ledger.close();await native.close();await sql.end();}
}
