// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { testDb } from './pg/test_db.mjs';
import { DEADLINE_MS, REFRESH_WITHIN_MS } from '../worker/deadlines.js';
import { REFRESH_WITHIN_MS as factoryRefresh } from '../worker/factory.js';
import { calculationId, rowAsk } from '../worker/board_tasks.js';
import { questionId } from '../worker/task_contract.js';

assert.equal(REFRESH_WITHIN_MS, 14 * 86_400_000);
assert.equal(DEADLINE_MS.refresh, REFRESH_WITHIN_MS);
assert.equal(factoryRefresh, REFRESH_WITHIN_MS);
const original = rowAsk('refresh-first', { identity: 'build', ruler: 'r', mode: 'base' }, '{}', 'old', null, 0);
const repeated = rowAsk('refresh-later', { identity: 'build', ruler: 'r', mode: 'base' }, '{}', 'old', null, REFRESH_WITHIN_MS);
assert.equal(original.ref, calculationId('build', 'r', 'base'));
assert.equal(original.ref, repeated.ref);
assert.equal(await questionId(original.verb, original.request), await questionId(repeated.verb, repeated.request));
const db = await testDb('library');
try {
  const put = (batch, ruler = 'r', mode = 'base', who = null) => db.run(
    'INSERT INTO queue(batch,build_id,ruler,mode,initiator) VALUES (?,\'b\',?,?,?) ON CONFLICT DO NOTHING', batch, ruler, mode, who);
  await put('arrivals-first');
  await put('refresh-next-week', 'r', 'base', 'user:first');
  await put('rescore-later', 'r', 'base', 'user:second');
  await put('arrivals-first', 'r', 'base', 'user:third');
  assert.deepEqual(await db.all('SELECT batch,initiator FROM queue'), [{ batch: 'arrivals-first', initiator: 'user:first' }]);
  await put('refresh-next-week', 'other');
  await put('refresh-next-week', 'r', 'other');
  assert.equal((await db.get('SELECT count(*) n FROM queue')).n, 3, 'different rulers and modes must remain computable');
  await db.run("ALTER TABLE queue DISABLE TRIGGER queue_once");
  await assert.rejects(db.run("INSERT INTO queue(batch,build_id,ruler,mode) VALUES ('bypass','b','r','base')"), /duplicate key/);
  await db.run("ALTER TABLE queue ENABLE TRIGGER queue_once");
  await db.run("DELETE FROM queue WHERE build_id='b' AND ruler='r' AND mode='base'");
  await put('refresh-after-completion');
  assert.equal((await db.get("SELECT count(*) n FROM queue WHERE ruler='r' AND mode='base'")).n, 1);
  assert.equal((await db.get("SELECT batch FROM queue WHERE ruler='r' AND mode='base'")).batch, 'refresh-after-completion');
  console.log('Queue calculations coalesce across batches and refresh rounds, preserve initiators and other scenes, and can recur after completion; the unique index rejects a bypass.');
} finally { await db.close(); }
