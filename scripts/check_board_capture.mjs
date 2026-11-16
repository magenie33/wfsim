// Canonical source writes and removals create captured, transactional ranking inputs.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
import {placeholders} from '../worker/placeholders.js';

const ddl=name=>readFileSync(new URL('../worker/pg/'+name+'.sql',import.meta.url),'utf8');
export async function boardCaptureCases(db,{breakCapture=false}={}) {
 await db.exec(ddl('schema')+'\n'+ddl('library')+'\n'+ddl('board_mailbox')+'\n'+ddl('board_capture'));
 if(breakCapture)await db.exec('DROP TRIGGER board_input_changed ON entity_weapon_builds');
 const row=(scope,key)=>db.get('SELECT * FROM state_board_input_mailbox WHERE scope=? AND key=?',scope,key);
 const id='capture-build',at='2026-01-01T00:00:00.000Z',record=JSON.stringify({weapon:'laetum',mods:['serration']});
 const put=()=>db.run('INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?)',id,at,record);
 const key=(ruler='standard_single_target',mode='base')=>JSON.stringify([id,ruler,mode]).replaceAll(',',', ');
 let passed=0;
 const check=async(name,run)=>{await run();passed++;console.log('ok: '+name);};
 await check('actual build writes capture their canonical row in the same transaction',async()=>{
  await db.exec('BEGIN');await put();assert.equal((await row('build',id)).body.record,record);await db.exec('ROLLBACK');
  assert.equal(await row('build',id),null);assert.equal(await db.get('SELECT * FROM entity_weapon_builds WHERE id=?',id),null);
  await put();const captured=await row('build',id),build=await db.get('SELECT * FROM entity_weapon_builds WHERE id=?',id);
  assert.deepEqual(captured.body,build);assert.equal(captured.body.weapon,'laetum');
  await db.run('UPDATE entity_weapon_builds SET record=record WHERE id=?',id);
  assert.equal(String((await row('build',id)).version),String(captured.version));
 });
 const score=()=>db.run(`INSERT INTO state_board_measurement_selection
   (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
   VALUES(?,?,?,?,?,?,?,?,?)`,id,'standard_single_target','base','verified:engine',10,'kpm',1,at,at);
 await check('measurement identity has unambiguous build, ruler and mode components',async()=>{
  await db.exec('BEGIN');await score();assert.equal((await row('measurement',key())).body.score,10);await db.exec('ROLLBACK');
  assert.equal(await row('measurement',key()),null);await score();
  const captured=await row('measurement',key()),source=await db.get('SELECT * FROM state_board_measurement_selection WHERE identity=?',id);
  assert.deepEqual(captured.body,source);
  await db.run('UPDATE state_board_measurement_selection SET score=11 WHERE identity=?',id);
  assert.equal((await row('measurement',key())).body.score,11);
  await db.run('UPDATE state_board_measurement_selection SET mode=? WHERE identity=?','base|alternate',id);
  assert.equal((await row('measurement',key())).removed,true);
  assert.equal((await row('measurement',key('standard_single_target','base|alternate'))).body.score,11);
 });
 await check('review retirement and recovery travel independently of measurement timing',async()=>{
  await db.run(`INSERT INTO entity_measurement_reviews(id,ref,round_at,identity,ruler,mode,engine,question,record,state,
    baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`,
   'capture-review','capture-ref',at,id,'standard_single_target','base','engine','capture-question',record,'waiting',10,'kpm',1,'old',at,at,at);
  assert.equal((await row('review','capture-review')).body.state,'waiting');
  await db.run('UPDATE entity_measurement_reviews SET invalidated_at=?,invalidation_reason=? WHERE id=?',at,'invalid proof','capture-review');
  assert.equal((await row('review','capture-review')).body.invalidated_at,at);
  await db.run("UPDATE entity_measurement_reviews SET state='succeeded' WHERE id='capture-review'");
  assert.equal((await row('review','capture-review')).body.state,'succeeded');
 });
 await check('deletions retain tombstones and pending execution does not feed ranking',async()=>{
  const scopes=await db.get('SELECT count(*) n FROM state_board_input_mailbox');
  await db.run("INSERT INTO entity_compute_batches(id,at,total) VALUES('capture-batch',?,1)",at);
  await db.run('INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES(?,?,?,?)','capture-batch',id,'standard_single_target','base');
  assert.equal(String((await db.get('SELECT count(*) n FROM state_board_input_mailbox')).n),String(scopes.n));
  await db.run('DELETE FROM entity_measurement_reviews WHERE id=?','capture-review');assert.equal((await row('review','capture-review')).removed,true);
  await db.run('DELETE FROM state_board_measurement_selection WHERE identity=?',id);
  assert.equal((await row('measurement',key('standard_single_target','base|alternate'))).removed,true);
  await db.run('DELETE FROM entity_weapon_builds WHERE id=?',id);assert.equal((await row('build',id)).removed,true);
  await put();assert.equal((await row('build',id)).removed,false);
 });
 await check('source reinstall preserves identity and watermarks; silent truncate is refused',async()=>{
  const identity=await db.get('SELECT source FROM state_board_input_source WHERE id=1'),before=await row('build',id);
  await db.exec(ddl('board_mailbox')+'\n'+ddl('board_capture'));
  assert.deepEqual(await db.get('SELECT source FROM state_board_input_source WHERE id=1'),identity);
  assert.deepEqual(await row('build',id),before);
  await assert.rejects(db.exec('TRUNCATE entity_weapon_builds'),/explicit replacement projection/);
  assert.deepEqual(await row('build',id),before);
  await db.exec('BEGIN');
  try {
   await db.exec('DELETE FROM state_board_input_source');
   await assert.rejects(db.exec(ddl('board_mailbox')),/source metadata requires restore/);
  }finally{await db.exec('ROLLBACK');}
 });
 console.log(`${passed} canonical board capture checks passed`);return passed;
}

if(process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href) {
 const {PGlite}=await import('./pg/node_modules/@electric-sql/pglite/dist/index.js'),pg=new PGlite();
 const q=(sql,args)=>pg.query(placeholders(sql),args).then(r=>r.rows);
 try{await boardCaptureCases({exec:sql=>pg.exec(sql),run:(sql,...args)=>q(sql,args),get:async(sql,...args)=>(await q(sql,args))[0]??null},{breakCapture:process.argv.includes('--break-capture')});}
 finally{await pg.close();}
}
