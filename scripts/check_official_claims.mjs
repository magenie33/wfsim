// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { factory, canon } from '../worker/tasks.js';
import { seedComputeWork, seedComputeWorkMany } from './pg/fixtures.mjs';

export async function officialSourceIndexCases(db) {
  await seedComputeWorkMany(db,Array.from({length:2000},(_,i)=>({id:'source-'+i,state:'open',due_at:'2026-01-01',opened_at:'2026-01-01'})));
  await db.exec(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
    SELECT CASE WHEN task_id%100=0 THEN 'sparse' ELSE 'common' END,id,id,'live',opened_at,due_at FROM state_compute_work`);
  await db.exec('VACUUM ANALYZE relation_task_consumers');
  const row=await db.get(`EXPLAIN(ANALYZE,BUFFERS,TIMING OFF,FORMAT JSON)
    SELECT question FROM relation_task_consumers WHERE producer='sparse'`);
  const reads=[];
  const walk=p=>{if(p['Relation Name']==='relation_task_consumers')reads.push(p);for(const child of p.Plans || [])walk(child);};
  walk(row['QUERY PLAN'][0].Plan);
  assert.equal(reads.length,1);
  assert.equal(reads[0]['Node Type'],'Index Only Scan');
  assert.equal(reads[0]['Index Name'],'demands_producer_question');
  assert.equal(reads[0]['Actual Rows'],20);
}

export async function officialClaimGuardCases(db) {
  const now=Date.parse('2026-11-14T08:00:00.000Z'),at=new Date(now).toISOString();
  const old='2026-11-13T08:00:00.000Z',future='2026-11-15T08:00:00.000Z';
  const f=factory({verbs:{simulate:{cap:2,canon:r=>canon(r)}},producers:{common:{},selected:{}}});
  const ids=['unrelated','leased','answered','incapable','future','multi','withdrawn-source','duplicate','spot'];
  for(const [position,id] of ids.entries()) {
    await seedComputeWork(db,{id,verb:'simulate',producer:'common',state:id==='spot'?'spot':'open',
      due_at:id==='future'?future:old,opened_at:old,lease_until:id==='leased'?future:null});
    await db.run('UPDATE state_compute_scheduling SET zone=1,zone_position=? WHERE id=?',position,id);
    await db.run(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      VALUES('common',?,?,'live',?,?)`,id,id,old,old);
    if(id!=='unrelated')await db.run(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
      VALUES('selected',?,?,?,?,?)`,id,id,id==='withdrawn-source'?'withdrawn':'live',old,old);
  }
  await db.run("UPDATE state_compute_work SET required_capability='other-capability' WHERE id='incapable'");
  await db.run(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at)
    VALUES('selected','duplicate-again','duplicate','live',?,?)`,old,old);
  for(const engine of ['7bd3c7f00f18','84dd66d22de7']) {
    await db.run(`INSERT INTO record_candidate_answers(question,engine,device,owner,net,canon,result,work,at,official)
      VALUES('answered',?,'official:guard','official:guard','','{}','{}',1,?,1)`,engine,at);
    await db.run(`UPDATE state_compute_work SET lease=NULL,lease_until=NULL,leased_to=NULL WHERE id IN('multi','withdrawn-source','duplicate','spot')`);
    const jobs=await f.claim({LIBRARY:db},{device:'official:guard',capabilities:['fixed-board-input-v1']},
      {states:['open','spot'],producers:['selected'],max:4},engine,now);
    assert.deepEqual(jobs.map(job=>job.question),['multi','withdrawn-source','duplicate','spot']);
    assert.equal(new Set(jobs.map(job=>job.question)).size,4);
    assert(jobs.every(job=>job.lease));
    assert.equal((await db.get("SELECT lease_until FROM state_compute_work WHERE id='leased'")).lease_until,future);
  }
}

if(process.argv[1] && pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {
  const {testDb}=await import('./pg/test_db.mjs');
  for(const run of [officialSourceIndexCases,officialClaimGuardCases]) {
    const db=await testDb('library');
    try{await run(db);console.log(run.name+': pass');}finally{await db.close();}
  }
}
