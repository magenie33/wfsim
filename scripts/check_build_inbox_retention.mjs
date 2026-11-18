// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import {mkdtempSync,readFileSync,rmSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {intakeBatches} from './intake_batches.mjs';
import {buildBatches,prepareIntakeBuild} from './build_batches.mjs';
import {testDb} from './pg/test_db.mjs';

const db=await testDb('library');
const at=new Date().toISOString();
const submit=(id,initiator='anonymous',family=null,observation=null)=>db.run(
  'INSERT INTO record_build_submissions(id,at,record,initiator,family_source,observation) VALUES(?,?,?,?,?,?)',
  id,at,'{}',initiator,family,observation);
const expired=id=>db.get('SELECT * FROM record_build_inbox_expirations WHERE id=?',id);
const open=id=>db.get('SELECT id FROM record_build_submissions WHERE id=?',id);
const since=async id=>Number((await db.get('SELECT extract(epoch FROM expires_at-clock_timestamp()) AS seconds FROM state_build_inbox_expiry WHERE id=?',id)).seconds);
const directory=mkdtempSync(join(tmpdir(),'wfsim-inbox-expiration-'));

try {
  await submit('player','user:original');
  await submit('generated','system','family:generator');
  await submit('formal','system','workflow|registered|out');
  await submit('review:accepted','anonymous',null,'{}');
  await submit('legacy');
  await db.run("DELETE FROM state_build_inbox_expiry WHERE id='legacy'");
  await db.run("INSERT INTO record_equipment_submissions(id,at,record) VALUES('equipment:old',?,'{}')",at);
  await db.run("INSERT INTO record_equipment_submissions(id,at,record) VALUES('equipment:accepted',?,'{}')",at);
  assert.ok(await since('player')>7*86400-5 && await since('player')<=8*86400);
  assert.ok(await since('generated')>86400-5 && await since('generated')<=86400);
  assert.ok(await since('equipment:old')>7*86400-5);
  const samePlayerDay=await db.get("SELECT count(DISTINCT expires_at) AS n FROM state_build_inbox_expiry WHERE id IN('player','equipment:old')");
  assert.equal(samePlayerDay.n,1);
  assert.deepEqual(await db.all("SELECT id FROM state_build_inbox_expiry WHERE id IN('formal','review:accepted','legacy')"),[]);
  const intake=await prepareIntakeBuild({id:'accepted-build',at,initiator:'user:original',from:['player'],
    record:{weapon:'braton'},flow:{schema:1,scope:{},nodes:[]}});
  await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([intake]));
  const equipment=join(directory,'equipment.ndjson');
  writeFileSync(equipment,JSON.stringify({id:'equipment-build',at,kind:'warframe',record:{kind:'warframe'},from:['equipment:accepted']})+'\n');
  for await(const batch of buildBatches(equipment,{retention:true})) {
    assert.match(batch.sql,/build_equipment_intake_batch/);
    await db.run(batch.sql,...batch.params);
  }
  await db.exec("UPDATE state_build_inbox_expiry SET expires_at=clock_timestamp()-interval '1 second'");
  assert.equal((await db.get('SELECT build_expire_inbox(128) AS n')).n,2);
  assert.ok(await open('player'),'accepted source retained until its ordinary ACK');
  assert.ok(await open('formal'));
  assert.ok(await open('review:accepted'));
  assert.ok(await open('legacy'));
  assert.ok(await expired('generated'));
  assert.ok(await expired('equipment:old'));
  assert.ok(await db.get("SELECT id FROM entity_equipment_builds WHERE id='equipment-build'"));
  assert.ok(await db.get("SELECT id FROM record_equipment_submissions WHERE id='equipment:accepted'"));
  assert.ok(!(await expired('equipment:accepted')));
  assert.ok(!(await open('generated')));
  assert.deepEqual(await db.all('SELECT id FROM build_inbox_expired_ids(?::text) ORDER BY id',
    JSON.stringify(['generated','equipment:old','player'])),[{id:'equipment:old'},{id:'generated'}]);
  await submit('generated','system');
  assert.ok(!(await open('generated')),'expired source was not replayed');
  await assert.rejects(db.run('SELECT build_intake_batch(?::text)',JSON.stringify([{...intake,id:'stale',from:['generated']}])),/expired during intake/);
  await assert.rejects(db.run('SELECT build_equipment_intake_batch(?::text)',JSON.stringify([
    {from:['equipment:old'],id:'stale',at,record:'{}',position:1}])),/expired during intake/);
  assert.deepEqual(await db.all("SELECT id FROM entity_weapon_builds WHERE id='stale'"),[]);
  assert.deepEqual(await db.all("SELECT id FROM entity_equipment_builds WHERE id='stale'"),[]);
  await db.run('SELECT build_spend_submissions(?::text)',JSON.stringify(['player']));
  assert.ok(!(await open('player')));
  assert.ok(!(await expired('player')));
  assert.equal((await db.get('SELECT build_expire_inbox(128) AS n')).n,0);
  await assert.rejects(db.run('SELECT build_expire_inbox(129)'),/1\.\.128/);

  await db.run("INSERT INTO record_build_submissions(id,at,record) SELECT 'bytes:'||n,?,repeat('x',5*1024*1024) FROM generate_series(1,2)n",at);
  await db.exec("UPDATE state_build_inbox_expiry SET expires_at=clock_timestamp()-interval '1 second'");
  assert.equal((await db.get('SELECT build_expire_inbox(128) AS n')).n,1);
  assert.equal((await db.get('SELECT build_expire_inbox(128) AS n')).n,1);

  const inbox=join(directory,'inbox.ndjson'),done=join(directory,'done.txt');
  writeFileSync(inbox,JSON.stringify({id:'generated'})+'\n');
  const result=await intakeBatches(inbox,{retention:true,doneFile:done,
    expiredIds:ids=>db.all('SELECT id FROM build_inbox_expired_ids(?::text)',JSON.stringify(ids)),
    run:()=>{throw Error('an expired capture reached normalization');}});
  assert.equal(readFileSync(done,'utf8'),'generated\n');
  assert.equal(result.arrivals,1);
  console.log('raw submission lifetimes, protected and accepted sources, replay fences and expired capture ACK pass');
} finally {
  rmSync(directory,{recursive:true,force:true});
  await db.close();
}
