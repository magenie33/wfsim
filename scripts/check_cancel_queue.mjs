// SPDX-License-Identifier: AGPL-3.0-or-later
// Cancellation keeps other batches and non-board demands through the scripts' driver.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import postgres from './pg/node_modules/postgres/src/index.js';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { query, close } from './db.mjs';
import { seedComputeWork } from './pg/fixtures.mjs';

const connection = process.env.WFSIM_CANCEL_QUEUE_PG_URL;
if (connection) assert(['127.0.0.1', 'localhost'].includes(new URL(connection).hostname), 'use a local test database');
const directory = mkdtempSync(join(tmpdir(), 'cancel-queue-'));
const schema = 'cancel_queue_' + randomUUID().replaceAll('-', '');
const client = connection ? postgres(connection, { max: 1, fetch_types: false, onnotice: () => {} }) : null;
const ddl = ['schema', 'library', 'sync'].map(n => readFileSync(new URL(`../worker/pg/${n}.sql`, import.meta.url), 'utf8')).join('\n');
const script = fileURLToPath(new URL('./cancel_queue.sh', import.meta.url));
const original = process.env.PG_URL;
async function run(...args) {
  await close();
  const result = spawnSync('bash', [script, ...args], { cwd: directory, encoding: 'utf8', timeout: 30000,
    env: { ...process.env, CANCEL_SLICE: '1', CANCEL_PAUSE: '0', TMPDIR: directory } });
  return { status: result.status, output: `${result.stdout}${result.stderr}` };
}
async function success(...args) {
  const result = await run(...args);
  assert.equal(result.status, 0, result.output);
  return result.output;
}
const state = async () => ({
  queue: await query('SELECT batch,build_id,ruler,mode FROM state_board_pending_work ORDER BY batch,build_id,ruler,mode'),
  demands: await query('SELECT producer,ref,question,state FROM relation_task_consumers ORDER BY producer,ref'),
  questions: await query('SELECT id,state FROM state_compute_work ORDER BY id'),
});
try {
  console.log(client ? 'cancel queue: postgres.js TCP transport' : 'cancel queue: PGlite transport');
  if (client) {
    await client.unsafe(`CREATE SCHEMA ${schema}; SET search_path TO ${schema}`).simple();
    await client.unsafe(ddl).simple();
    const url = new URL(connection);
    url.searchParams.set('options', `-c search_path=${schema},pg_catalog,pg_temp`);
    process.env.PG_URL = url.href;
  } else {
    process.env.PG_URL = `pglite:${join(directory, 'db')}`;
    const pg = new PGlite(join(directory, 'db'));
    try { await pg.exec(ddl); } finally { await pg.close(); }
  }
  const prefix = "arrival_'雪\\%", batch = prefix + '-day', odd = 'b1"雪\\';
  for (const [name, id, ruler] of [['refresh', 'b2', 'sst'], [batch, odd, 'sst'], [batch, 'b2', 'sst'], [batch, 'b4', 'sst'],
    [batch, 'b5', 'sst'], [batch, 'b6', 'sst'], [batch, 'b3', 'other'], ['refresh', 'b3', 'sst']]) {
    await query("INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES(?,?,?,'base')", [name, id, ruler]);
  }
  for (const [id, kind, producers] of [[odd, 'open', ['new_build', 'sweep']], ['b2', 'open', ['new_build', 'refresh']],
    ['b3', 'open', ['refresh']], ['b5', 'fact', ['survey_build']], ['b6', 'open', ['new_build', 'user']]]) {
    await seedComputeWork({run:(sql,...args)=>query(sql,args),get:async(sql,...args)=>(await query(sql,args))[0]},
      {id,verb:'simulate',request:'{}',state:kind,due_at:'2026-10-02',opened_at:'2026-10-01'});
    for (const producer of producers) await query(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at,payload)
      VALUES(?,?,?,'live','2026-10-01','2026-10-02',?)`, [producer, `${id}|sst|base`, id, JSON.stringify({ identity: id, ruler: 'sst', mode: 'base' })]);
  }
  const before = await state();
  assert.match(await success('--batch', prefix, '--ruler', 'sst', '--dry-run'), /4 queue row\(s\) named/);
  assert.deepEqual(await state(), before, 'dry run changed state');
  assert.notEqual((await run()).status, 0, 'missing selector accepted');
  assert.notEqual((await run('--all', '--ruler', 'sst')).status, 0, 'mixed all selector accepted');
  assert.deepEqual(await state(), before, 'invalid selector changed state');
  await success('--batch', prefix, '--ruler', 'sst');
  const after = await state();
  assert.deepEqual(after.queue, before.queue.filter(r => r.batch !== batch || r.ruler !== 'sst'));
  assert.deepEqual(after.demands, before.demands.map(d => ({ ...d, state:
    [odd, 'b5', 'b6'].includes(d.question) && d.producer !== 'user' ? 'withdrawn' : 'live' })));
  assert.deepEqual(after.questions, before.questions.map(q => ({ ...q, state: q.id === odd ? 'withdrawn' : q.state })));
  console.log('ok: escaped selectors, one-row slices, shared batches, non-open questions and non-board demands retain their lifecycle');
  await success('--ruler', 'other');
  assert.deepEqual((await state()).queue, after.queue.filter(r => r.ruler !== 'other'));
  await success('--all');
  const empty = await state();
  assert.deepEqual(empty.queue, []);
  assert.deepEqual(empty.demands, before.demands.map(d => ({ ...d, state: d.producer === 'user' ? 'live' : 'withdrawn' })));
  assert.deepEqual(empty.questions, before.questions.map(q => ({ ...q, state:
    q.id === 'b6' ? 'open' : q.id === 'b5' ? 'fact' : 'withdrawn' })));
  await success('--all');
  assert.deepEqual(await state(), empty, 'empty cancellation changed state');
  console.log('ok: ruler-only, all and empty cancellation preserve unrelated user work and completed questions');
} finally {
  try { await close(); }
  finally {
    if (original === undefined) delete process.env.PG_URL; else process.env.PG_URL = original;
    try { if (client) await client.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`); }
    finally { if (client) await client.end({ timeout: 5 }); rmSync(directory, { recursive: true, force: true }); }
  }
}
