// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {coverage} from './rank_catalog_coverage.mjs';
import {initializeJournal,planBatch,transaction} from './board_catalog_batch_state.mjs';
import {sha256} from './board_catalog_readback.mjs';

const size=16384,empty=sha256(''),source='batch-scale',native=sha256('native'),original=sha256('archive'),ruler='standard_single_target';
const [capture,archive,ledger,journal]=Array.from({length:4},()=>new DatabaseSync(':memory:'));
const epoch=JSON.stringify([source,'cluster',native,original,'snapshot','time']);
capture.exec('CREATE TABLE progress(k TEXT PRIMARY KEY,v TEXT);CREATE TABLE facts(weapon TEXT,ruler TEXT);CREATE INDEX by_scope ON facts(weapon,ruler)');
for(const [k,v] of Object.entries({spec:{source,cluster:'cluster',native,archive:original},replay:{snapshot:'snapshot',captured_at:'time'}}))
 capture.prepare('INSERT INTO progress VALUES(?,?)').run(k,JSON.stringify(v));
archive.exec('CREATE TABLE sealed(id INTEGER PRIMARY KEY,archive TEXT);CREATE TABLE scopes(weapon TEXT,ruler TEXT);CREATE TABLE members(id TEXT)');
archive.prepare('INSERT INTO sealed VALUES(1,?)').run(original);
coverage(capture,archive,ledger);
ledger.exec('CREATE TABLE verified_outputs(weapon TEXT,ruler TEXT,rows INTEGER,ordered TEXT,PRIMARY KEY(weapon,ruler)) WITHOUT ROWID');
const put=ledger.prepare('INSERT INTO receipts VALUES(?,?,?)'),prove=ledger.prepare('INSERT INTO evidenced VALUES(?,?)');
const out=ledger.prepare('INSERT INTO verified_outputs VALUES(?,?,0,?)'),scope=archive.prepare('INSERT INTO scopes VALUES(?,?)');
transaction(ledger,()=>{
 for(let i=0;i<size;i++){
  const weapon='w'+String(i).padStart(5,'0');
  scope.run(weapon,ruler);
  put.run(weapon,ruler,JSON.stringify({eligible:true,unresolved:0,source,weapon,ruler,native,archive:original,snapshot:'snapshot',members:0,rows:0,pages_sha256:empty}));
  prove.run(weapon,ruler);out.run(weapon,ruler,empty);
 }
});
const final=coverage(capture,archive,ledger,null,{readOnly:true});assert.equal(final.scopes,size);
const options={journal,capture,archive,ledger,sets:new Map(),final,budget:{preliminary_capacity_pass:true,scope_count:size},
 contract:{source,native,archive:original,snapshot:'snapshot',epoch},reservations:[],selected:[],scenes:new Map([[ruler,'']])};
initializeJournal(journal);
ledger.prepare("UPDATE verified_outputs SET weapon='extra' WHERE weapon='w00001'").run();
assert.throws(()=>planBatch(options),/verified|cover every scope/);
assert.equal(journal.prepare('SELECT count(*) n FROM run').get().n,0);
ledger.prepare("UPDATE verified_outputs SET weapon='w00001' WHERE weapon='extra'").run();
const start=performance.now();planBatch(options);
assert.equal(journal.prepare('SELECT count(*) n FROM scopes').get().n,size);
planBatch(options);assert.equal(journal.prepare('SELECT count(*) n FROM scopes').get().n,size);
console.log('ok — '+size+' qualified scopes, rejected a same-count substitution, resumed the same plan in '+Math.round(performance.now()-start)+' ms');
for(const db of [capture,archive,ledger,journal])db.close();
