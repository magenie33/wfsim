// SPDX-License-Identifier: AGPL-3.0-or-later
// A missing archive partition is an outstanding obligation even when it has no selected facts.
import {createHash} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import {guardCatalogFile} from './board_rank_storage.mjs';
import {pathToFileURL} from 'node:url';
const state=(db,k)=>JSON.parse(db.prepare('SELECT v FROM progress WHERE k=?').get(k)?.v||'null');
function recordEvidence(ledger,scope,r,body){
 ledger.exec(`CREATE TABLE IF NOT EXISTS transfer(weapon TEXT,ruler TEXT,body TEXT,file TEXT,ordinal INTEGER,
  PRIMARY KEY(weapon,ruler)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS outcomes(weapon TEXT,ruler TEXT,file TEXT,ordinal INTEGER,fact TEXT,destination TEXT,detail TEXT,
   PRIMARY KEY(weapon,ruler,file,ordinal)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS page_hashes(weapon TEXT,ruler TEXT,first INTEGER,last INTEGER,sha TEXT,
   PRIMARY KEY(weapon,ruler,first)) WITHOUT ROWID`);
 const keys=[r.weapon,r.ruler];
 ledger.prepare("INSERT OR IGNORE INTO transfer VALUES(?,?,?,'',-1)").run(...keys,body);
 const prior=ledger.prepare('SELECT * FROM transfer WHERE weapon=? AND ruler=?').get(...keys);
 if(prior.body!==body)throw Error('scope evidence cut changed');
 const put=ledger.prepare('INSERT INTO outcomes VALUES(?,?,?,?,?,?,?)');
 let {file,ordinal}=prior;
 const next=scope.prepare('SELECT file,ordinal,fact,destination,detail FROM dispositions WHERE (file,ordinal)>(?,?) ORDER BY file,ordinal LIMIT 256');
 const end=Date.now()+20000;
 for(;;){
  const batch=next.all(file,ordinal);
  if(!batch.length)break;
  ledger.exec('BEGIN IMMEDIATE');
  try{
   for(const d of batch)put.run(...keys,d.file,d.ordinal,d.fact,d.destination,d.detail);
   ({file,ordinal}=batch.at(-1));
   ledger.prepare('UPDATE transfer SET file=?,ordinal=? WHERE weapon=? AND ruler=?').run(file,ordinal,...keys);
   ledger.exec('COMMIT');
  }catch(e){ledger.exec('ROLLBACK');throw e;}
  if(Date.now()>=end)return false;
 }
 const rows=ledger.prepare('SELECT count(*) n FROM outcomes WHERE weapon=? AND ruler=?').get(...keys).n;
 if(rows!==r.members)throw Error('copied evidence does not cover the archive members');
 const store=ledger.prepare('INSERT OR REPLACE INTO page_hashes VALUES(?,?,?,?,?)');
 const hash=createHash('sha256');let last=0;
 ledger.exec('BEGIN IMMEDIATE');
 try{
  for(const page of scope.prepare('SELECT * FROM pages ORDER BY first').iterate()){
   if(page.first!==last+1||page.last<page.first||page.last-page.first>=128||!/^[a-f0-9]{64}$/.test(page.sha))
    throw Error('invalid ordered evidence page');
   store.run(...keys,page.first,page.last,page.sha);
   hash.update(page.first+' '+page.last+' '+page.sha+'\n');last=page.last;
  }
  if(last!==r.rows||(r.pages_sha256&&hash.digest('hex')!==r.pages_sha256))throw Error('public page evidence differs from receipt');
  ledger.prepare('INSERT OR REPLACE INTO receipts VALUES(?,?,?)').run(...keys,body);
  ledger.prepare('INSERT OR REPLACE INTO evidenced VALUES(?,?)').run(...keys);
  ledger.exec('COMMIT');
 }catch(e){ledger.exec('ROLLBACK');throw e;}
 return true;
}

export function coverage(capture,archive,ledger,scope=null){
 const spec=state(capture,'spec'),cut=state(capture,'replay'),sealed=archive.prepare('SELECT archive FROM sealed WHERE id=1').get();
 if(!cut||spec.archive!==sealed?.archive)throw Error('complete archive and coherent replay required');
 ledger.exec('CREATE TABLE IF NOT EXISTS evidenced(weapon TEXT,ruler TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID');
 const epoch=JSON.stringify([spec.source,spec.cluster,spec.native,spec.archive,cut.snapshot,cut.captured_at]);
 ledger.exec('CREATE TABLE IF NOT EXISTS epoch(id INTEGER PRIMARY KEY,v TEXT); CREATE TABLE IF NOT EXISTS receipts(weapon TEXT,ruler TEXT,body TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID');
 ledger.prepare('INSERT OR IGNORE INTO epoch VALUES(1,?)').run(epoch);
 if(ledger.prepare('SELECT v FROM epoch').get().v!==epoch)throw Error('catalog receipt cut changed');
 if(scope){
  const r=state(scope,'receipt'),contract=JSON.parse(state(scope,'contract')||'null');
  if(!r||r.source!==spec.source||r.snapshot!==cut.snapshot||r.archive!==spec.archive||r.native!==spec.native||
   contract?.cut.captured_at!==cut.captured_at||contract?.weapon!==r.weapon||contract?.ruler!==r.ruler)throw Error('stale scope receipt');
  const count=archive.prepare('SELECT count(*) n FROM members WHERE weapon=? AND ruler=?').get(r.weapon,r.ruler).n;
  if(r.members!==count)throw Error('scope receipt member count mismatch');
  const old=ledger.prepare('SELECT body FROM receipts WHERE weapon=? AND ruler=?').get(r.weapon,r.ruler),body=JSON.stringify(r);
  if(old&&old.body!==body)throw Error('conflicting qualification receipts');
  return recordEvidence(ledger,scope,r,body)?r:{complete:false,weapon:r.weapon,ruler:r.ruler};
 }
 ledger.exec('CREATE TEMP TABLE IF NOT EXISTS expected(weapon TEXT,ruler TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID');
 ledger.exec('DELETE FROM expected');
 const add=ledger.prepare('INSERT OR IGNORE INTO expected VALUES(?,?)');
 for(const {weapon,ruler} of archive.prepare('SELECT weapon,ruler FROM scopes').iterate())add.run(weapon,ruler);
 const next=capture.prepare('SELECT weapon,ruler FROM facts WHERE (weapon,ruler)>(?,?) ORDER BY weapon,ruler LIMIT 1');
 let scopeKey=['',''];
 for(let row;(row=next.get(...scopeKey));){scopeKey=[row.weapon,row.ruler];add.run(...scopeKey);}
 const count=ledger.prepare('SELECT count(*) n FROM expected').get().n;
 let qualified=0,members=0,held=0,missing=0;const examples=[];
 const get=ledger.prepare('SELECT body FROM receipts JOIN evidenced USING(weapon,ruler) WHERE weapon=? AND ruler=?');
 for(const {weapon,ruler} of ledger.prepare('SELECT * FROM expected ORDER BY weapon,ruler').iterate()){
  const key=weapon+'/'+ruler;
  const v=get.get(weapon,ruler);
  if(!v){missing++;if(examples.length<10)examples.push([key,'pending']);continue;}
  const r=JSON.parse(v.body);members+=r.members;
  if(r.eligible)qualified++;
  else{held++;if(examples.length<10)examples.push([key,r.unresolved]);}
 }
 const orphans=capture.prepare('SELECT count(*) n FROM facts WHERE weapon IS NULL').get().n;
 const original=archive.prepare('SELECT count(*) n FROM members').get().n;
 return {scopes:count,qualified,held,missing,orphans,members,original,examples,
  eligible:count>0&&qualified===count&&members===original&&!orphans};
}
if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href){
 const [capturePath,archivePath,ledgerPath,scopePath]=process.argv.slice(2),ro={readOnly:true};
 const c=new DatabaseSync(capturePath,ro),a=new DatabaseSync(archivePath,ro),l=new DatabaseSync(guardCatalogFile(ledgerPath));
 const s=scopePath?new DatabaseSync(scopePath,ro):null;
 try{c.exec('BEGIN');a.exec('BEGIN');console.log(JSON.stringify(coverage(c,a,l,s)));}
 finally{s?.close();l.close();a.close();c.close();}
}
