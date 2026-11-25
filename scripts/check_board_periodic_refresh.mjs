// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { testDb } from './pg/test_db.mjs';
import { drainBuildAdmission } from './pg/build_admission_fixture.mjs';
import { refreshCutoff, refreshPage } from './board_periodic_refresh.mjs';
import { prepareIntakeBuild } from './build_batches.mjs';
import { boardReviewsRoute } from '../worker/board_reviews.js';
import { BUILD_WORKFLOW_PRODUCER } from '../worker/build_workflows.js';
import { factory, canon } from '../worker/tasks.js';

const db = await testDb('library');
const now = Date.now(), day = 86_400_000, old = new Date(now - 15 * day).toISOString();
const fresh = new Date(now - 2 * day).toISOString(), cutoff = refreshCutoff(now);
const record = { weapon: 'braton', mods: ['serration'] };
const ref = 'a'.repeat(32), changed = 'b'.repeat(32), current = 'c'.repeat(32);
const ruler = 'standard_single_target', mode = 'base';
const request = id => ({ weapon: 'braton', mods: ['serration'], duration: 2, runs: 2,
  metric: 'kpm', seed: id === ref ? 12 : id === changed ? 13 : 14 });
const flow = id => ({ schema: 1, scope: { boards: [{ id: ruler }],
  modes: { braton: [mode] } }, builds: [{ id, record }],
  templates: [request(id)], patches: [{ set: {}, remove: [] }],
  nodes: [{ kind: 'measure', identity: id, ruler, mode, template: 0, patch: 0 }] });
const capture = async rows => rows.map(({id, at}) => ({id, at, record, flow: flow(id)}));
const read = (sql, params) => db.all(sql, ...params);
const env = { LIBRARY: db, ASSETS: { fetch: async () => Response.json({engine:'e1'}) } };
const f = factory({ verbs: { simulate: { cap: 2, lease_ms: 60000, canon } },
  producers: { build_workflow: BUILD_WORKFLOW_PRODUCER } });
const score = id => db.get('SELECT * FROM state_board_measurement_selection WHERE identity=?', id);
const page = () => refreshPage({ read, capture, admit: read, cutoff, now });

try {
  assert.equal(cutoff, new Date(now - 14 * day).toISOString());
  assert.equal(refreshCutoff(now, 3), new Date(now - 3 * day).toISOString());
  assert.throws(() => refreshCutoff(now, 0), /positive/);
  for (const [id, measured] of [[ref, old], [changed, new Date(now - 16 * day).toISOString()], [current, fresh]]) {
    await db.run('INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?)',id,old,JSON.stringify(record));
    await db.run('INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES(?,?,?,?,?,?,?,?,?)',
      id,ruler,mode,'older',10,'kpm',1,measured,measured);
  }
  const first = await page();
  assert.equal(first.asked, 2);
  const ordered = await db.all("SELECT i.build_id FROM state_build_admission_work w JOIN entity_build_intake_intents i ON i.id=w.key WHERE w.kind='intake' ORDER BY w.position");
  assert.deepEqual(ordered.map(row=>row.build_id),[changed,ref],'older measurements reserve FIFO positions first');
  assert.equal((await page()).asked, 0);
  assert.equal((await score(ref)).finished_at, old);
  await db.run('UPDATE state_board_measurement_selection SET finished_at=? WHERE identity=?',fresh,changed);
  await drainBuildAdmission(db);
  const tasks = await db.all('SELECT * FROM state_compute_work WHERE state<>\'withdrawn\'');
  assert.equal(tasks.length, 1, 'a changed baseline is skipped before task creation');
  const held = tasks[0];
  assert.equal(held.zone, 4);
  assert.equal((await score(ref)).score, 10);
  assert.equal((await score(ref)).finished_at, old);
  assert.equal((await db.get('SELECT retired_at FROM board_facts WHERE identity=?',ref)).retired_at,null);
  console.log('old scores stay visible, fresh scores and races are skipped, and only one periodic task enters zone 04');

  const response = await boardReviewsRoute(new Request('https://wfsim.test/api/board/review',{
    method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({
      identity:ref,ruler,mode,engine:'e1',request:request(ref),observed_score:11,baseline_score:10,metric:'kpm'
    })}),env,'/api/board/review');
  const review = await response.json();
  assert.equal(review.accepted,true);
  const inbox = await db.get('SELECT * FROM record_build_submissions WHERE id=?','review:'+review.id);
  const input = await prepareIntakeBuild({ id:ref,at:inbox.at,record,initiator:'anonymous',from:[inbox.id],
    observation:{...JSON.parse(inbox.observation),valid:true},flow:flow(ref) });
  await db.run('SELECT build_intake_batch(?::text)',JSON.stringify([input]));
  await db.run('SELECT build_ack_submissions(?::text)',JSON.stringify([inbox]));
  await drainBuildAdmission(db);
  const promoted = await db.get('SELECT * FROM state_compute_work WHERE id=?',held.id);
  assert.equal(promoted.zone,1);
  assert.equal(promoted.task_id,held.task_id,'promotion must preserve the independent computation round');
  await f.answerOfficial(env,{device:'official:refresh',owner:'official:refresh'},
    {question:held.id,engine:'e1',result:{score:10,metric:'kpm'},work:1});
  const after = await score(ref);
  assert.equal(after.score,10);
  assert.ok(after.finished_at > old);
  assert.equal((await page()).asked,0);
  console.log('a user correction joins the same task in zone 01 and equal independent results renew the measurement clock');
  const retry = 'd'.repeat(32);
  await db.run('INSERT INTO entity_weapon_builds(id,at,record) VALUES(?,?,?)',retry,old,JSON.stringify(record));
  await db.run('INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES(?,?,?,?,?,?,?,?,?)',
    retry,ruler,mode,'older',10,'kpm',1,old,old);
  assert.equal((await page()).asked,1);
  await drainBuildAdmission(db);
  await db.run("UPDATE state_build_workflows SET state='failed',completed_at=? WHERE build_id=?",fresh,retry);
  assert.equal((await page()).asked,1,'the next scan retries a failed periodic workflow');
  assert.equal((await page()).asked,0,'its replacement is already retained');
  console.log('failed refresh rounds can retry without duplicating an active replacement');

} finally {
  await db.close();
}
