// SPDX-License-Identifier: AGPL-3.0-or-later
// A new audit database may reuse rendered output, but never a previous audit's completion or newly frozen legacy input.
import {DatabaseSync} from 'node:sqlite';
import {existsSync,lstatSync,statSync,openSync,closeSync,renameSync,writeFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {canon} from '../worker/task_contract.js';
import {boardConsumerContract} from './split_board_pipeline.mjs';
import {fileHash,syncDirectory} from './inventory_publication_io.mjs';
import {sqliteTransactionRecovery} from './sqlite_transaction_recovery.mjs';

const original='fdb9277cb94ba9e23da556ff9a99554336bbcde5bcee548c9d3480d958bde15f';
const scratch=new Set(['state_board_publication_rows','state_board_publication_shapes','state_board_publication_maxima','state_board_publication_selection']);
const keep=name=>!scratch.has(name)&&(!name.startsWith('state_board_audit_')||['state_board_audit_baselines','state_board_audit_setup'].includes(name))
 &&!name.startsWith('record_board_audit_');
const digest=rows=>createHash('sha256').update(canon(rows)).digest('hex');
const identifier=name=>{if(!/^[a-z0-9_]+$/.test(name))throw Error('unsupported migration table');return '"'+name+'"';};
export async function migrateBoardAudit({oldPath,newPath,sourcePath,state,fromContract,databaseBytes,check=()=>{}}) {
 oldPath=resolve(oldPath);newPath=resolve(newPath);sourcePath=resolve(sourcePath);check();
 const target=boardConsumerContract('projection',state),expected={...target,code:original};
 if(canon(fromContract)!==canon(expected)||target.code===original)throw Error('unsupported board audit contract migration');
 if(oldPath===newPath||dirname(oldPath)!==dirname(newPath)||existsSync(newPath)||existsSync(newPath+'.partial')
  ||[oldPath,sourcePath,dirname(newPath)].some(path=>lstatSync(path).isSymbolicLink()))throw Error('migration requires separate unlinked owned paths');
 if(existsSync(oldPath+'-wal')&&statSync(oldPath+'-wal').size)throw Error('migration requires a quiescent recovered checkpoint');
 const old=new DatabaseSync(oldPath,{readOnly:true}),source=new DatabaseSync(sourcePath,{readOnly:true});let db;
 try {
  const held=old.prepare("SELECT * FROM state_prepared_input_consumption WHERE name='projection'").get();
  const generation=JSON.parse(old.prepare('SELECT body FROM state_inventory_generation WHERE id=1').get().body);
  const projection=JSON.parse(old.prepare('SELECT body FROM state_board_projection_generation WHERE id=1').get().body);
  const cut=source.prepare("SELECT * FROM record_prepared_board_cuts WHERE id=1 AND state='sealed'").get();
  if(!cut||held?.cut!==1||held.acknowledged!==0||held.cursor!==cut.input_count||held.audit!=null||held.contract!==canon(fromContract)
   ||held.source!==canon(state.contract)||generation.engine!==state.engine||generation.generation!==state.generation
   ||canon(generation.contract)!==canon(fromContract)||canon(projection.contract)!==canon(fromContract)
   ||projection.prepared_cut!==1||projection.publication?.phase!=='complete'||projection.complete
   ||!old.prepare('SELECT 1 FROM state_board_audit_setup WHERE id=1').get()
   ||source.prepare("SELECT acknowledged FROM state_prepared_board_consumers WHERE name='projection'").get()?.acknowledged!==0
   ||source.prepare('SELECT body FROM state_prepared_board_generation WHERE id=1').get()?.body!==canon(state.contract))
   throw Error('only an unacknowledged first cut with completed producer output and preserved legacy setup can migrate');
  const baselines=old.prepare('SELECT * FROM state_board_audit_baselines ORDER BY dir,weapon').all(),setup=old.prepare('SELECT * FROM state_board_audit_setup ORDER BY id').all();
  const receipt={v:1,from:fromContract,to:target,old_path:oldPath,new_path:newPath,source_path:sourcePath,old_sha256:await fileHash(oldPath),
   generation:state.generation,cut:1,input_count:cut.input_count,input_digest:cut.sealed_digest,legacy_baselines_sha:digest(baselines),legacy_setup_sha:digest(setup),
   input_cursor_reused:held.cursor,old_audit_reused:false,new_audit_required:true,producer_output_reused:true,new_legacy_freeze:false};
  const partial=newPath+'.partial';writeFileSync(newPath+'.migration.json',canon({...receipt,phase:'creating'})+'\n',{flag:'wx'});
  closeSync(openSync(partial,'wx'));db=sqliteTransactionRecovery(new DatabaseSync(partial));
  db.exec('PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA foreign_keys=OFF');
  const pages=Math.floor(databaseBytes/db.prepare('PRAGMA page_size').get().page_size);
  if(!Number.isSafeInteger(pages)||pages<1||db.prepare('PRAGMA max_page_count='+pages).get().max_page_count!==pages)throw Error('invalid migration database limit');
  db.prepare('ATTACH DATABASE ? AS previous').run(pathToFileURL(oldPath).href+'?mode=ro');
  const schema=old.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name").all();
  db.exec('BEGIN IMMEDIATE');try {
   for(const row of schema.filter(row=>row.type==='table')){check();db.exec(row.sql);}
   for(const row of schema.filter(row=>row.type==='table'&&keep(row.name))){check();db.exec('INSERT INTO '+identifier(row.name)+' SELECT * FROM previous.'+identifier(row.name));}
   db.prepare('UPDATE state_inventory_generation SET body=? WHERE id=1').run(canon({...generation,contract:target}));
   db.prepare('UPDATE state_board_projection_generation SET body=? WHERE id=1').run(canon({...projection,contract:target,complete:false,audit_ready:null}));
   db.prepare("UPDATE state_prepared_input_consumption SET contract=?,acknowledged=0,audit=NULL WHERE name='projection'").run(canon(target));
   for(const row of schema.filter(row=>row.type!=='table')){check();db.exec(row.sql);}
   db.exec('CREATE TABLE record_board_audit_migrations(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL)');
   db.prepare('INSERT INTO record_board_audit_migrations VALUES(1,?)').run(canon(receipt));db.exec('COMMIT');
  }catch(error){db.exec('ROLLBACK');throw error;}
  if(digest(db.prepare('SELECT * FROM state_board_audit_baselines ORDER BY dir,weapon').all())!==receipt.legacy_baselines_sha
   ||digest(db.prepare('SELECT * FROM state_board_audit_setup ORDER BY id').all())!==receipt.legacy_setup_sha)throw Error('migration legacy baseline differs');
  if(db.prepare('SELECT count(*) n FROM state_board_audit_round').get().n!==0)throw Error('migration retained old audit progress');
  if(db.prepare('PRAGMA quick_check').get().quick_check!=='ok')throw Error('migrated database integrity failed');
  db.exec('DETACH DATABASE previous;PRAGMA wal_checkpoint(TRUNCATE)');db.close();db=null;
  renameSync(partial,newPath);syncDirectory(newPath);writeFileSync(newPath+'.migration.json',canon({...receipt,phase:'complete'})+'\n');
  if(await fileHash(oldPath)!==receipt.old_sha256)throw Error('original board changed during migration');check();return receipt;
 }finally{db?.close();old.close();source.close();}
}
