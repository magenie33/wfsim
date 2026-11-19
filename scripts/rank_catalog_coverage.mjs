// SPDX-License-Identifier: AGPL-3.0-or-later
// A missing archive partition is an outstanding obligation even when it has no selected facts.
import {DatabaseSync} from 'node:sqlite';
import {guardCatalogFile} from './board_rank_storage.mjs';
import {pathToFileURL} from 'node:url';
const state=(db,k)=>JSON.parse(db.prepare('SELECT v FROM progress WHERE k=?').get(k)?.v||'null');
export function coverage(capture,archive,ledger,scope=null){
 const spec=state(capture,'spec'),cut=state(capture,'replay'),sealed=archive.prepare('SELECT archive FROM sealed WHERE id=1').get();
 if(!cut||spec.archive!==sealed?.archive)throw Error('complete archive and coherent replay required');
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
  ledger.prepare('INSERT OR IGNORE INTO receipts VALUES(?,?,?)').run(r.weapon,r.ruler,body);
  return r;
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
 const get=ledger.prepare('SELECT body FROM receipts WHERE weapon=? AND ruler=?');
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
