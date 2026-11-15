// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {mkdtempSync,rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {sqliteTransactionRecovery} from './sqlite_transaction_recovery.mjs';

const root=mkdtempSync(join(tmpdir(),'wfsim-full-recovery-'));
try {
 for(const adapted of [false,true]) {
  const path=join(root,String(adapted)+'.sqlite');let db=new DatabaseSync(path);
  db.exec('PRAGMA page_size=4096;PRAGMA journal_mode=WAL;CREATE TABLE state(cursor INTEGER);INSERT INTO state VALUES(0);CREATE TABLE rows(body BLOB);PRAGMA max_page_count=8');
  if(adapted)sqliteTransactionRecovery(db);
  let failure;
  try {
   db.exec('BEGIN IMMEDIATE');db.prepare('UPDATE state SET cursor=1').run();
   try{db.prepare('INSERT INTO rows VALUES(?)').run(Buffer.alloc(128*1024));db.exec('COMMIT');}
   catch(error){db.exec('ROLLBACK');throw error;}
  }catch(error){failure=error;}
  assert(failure);
  if(adapted)assert.equal(failure.errcode,13);else assert.match(failure.message,/cannot rollback/);
  assert.equal(db.prepare('SELECT cursor FROM state').get().cursor,0);
  assert.equal(db.prepare('SELECT count(*) n FROM rows').get().n,0);
  assert.throws(()=>db.exec('ROLLBACK'),/no transaction is active/);
  db.close();db=sqliteTransactionRecovery(new DatabaseSync(path));
  assert.deepEqual(db.prepare('PRAGMA quick_check').all().map(row=>row.quick_check),['ok']);
  db.exec('PRAGMA max_page_count=128;BEGIN IMMEDIATE');db.prepare('INSERT INTO rows VALUES(?)').run(Buffer.alloc(128*1024));
  db.prepare('UPDATE state SET cursor=1').run();db.exec('COMMIT');
  assert.equal(db.prepare('SELECT cursor FROM state').get().cursor,1);db.close();
 }
 console.log('sqlite FULL original-error preservation, rollback, cold reopen and bounded resume PASS; original handler negative reproduced');
}finally{rmSync(root,{recursive:true,force:true});}
