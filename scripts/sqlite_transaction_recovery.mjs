// SPDX-License-Identifier: AGPL-3.0-or-later
// SQLite FULL may end its transaction before a caller's cleanup rollback.
const installed=new WeakSet();
export function sqliteTransactionRecovery(db) {
 if(installed.has(db))return db;
 const exec=db.exec.bind(db),prepare=db.prepare.bind(db);let failedFull=false;
 const failed=error=>{failedFull=Number(error.errcode)%256===13;throw error;};
 db.prepare=function(...args) {
  const statement=prepare(...args);
  for(const name of ['run','get','all']) {
   const call=statement[name].bind(statement);
   statement[name]=function(...values){try{return call(...values);}catch(error){return failed(error);}};
  }
  return statement;
 };
 db.exec=function(sql) {
  const rollback=/^\s*ROLLBACK\s*;?\s*$/i.test(sql),recover=rollback&&failedFull;
  try{const result=exec(sql);failedFull=false;return result;}
  catch(error) {
   if(recover&&error.errcode===1&&error.message==='cannot rollback - no transaction is active'){failedFull=false;return;}
   return failed(error);
  }
 };
 installed.add(db);return db;
}
