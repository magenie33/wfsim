// SPDX-License-Identifier: AGPL-3.0-or-later
// Claimed inputs stay small even when the unrelated queue grows.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { captureScoreInputs } from './fetch_score_inputs.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';

const directory = mkdtempSync(join(tmpdir(), 'score-inputs-')), pg = new PGlite();
const queries = [], query = async (text, args = []) => { queries.push([text, args]); return (await pg.query(text, args)).rows; };
const sql = { begin: async (options, fn) => {
  await pg.exec(`BEGIN ${options}`);
  try { const result = await fn({ unsafe: query }); await pg.exec('COMMIT'); return result; }
  catch (error) { await pg.exec('ROLLBACK'); throw error; }
} };
const row = (id, mode = 'base') => ({ producer: 'sweep', ref: `${id}|r|${mode}`,
  payload: { identity: id, ruler: 'r', mode, batch: 'stale-payload-batch' } });
const tasks = [{ question: 'a', verb: 'simulate', demands: [row('a'), row('a')] },
  { question: 'b', verb: 'simulate', demands: [row('b'), row('b', 'cycle')] },
  { question: 'cancelled', verb: 'simulate', demands: [row('cancelled'), row('missing')] }];
const load = name => readFileSync(join(directory, name), 'utf8');
const lines = name => load(name).trim().split('\n').filter(Boolean).map(JSON.parse);
try {
  await pg.exec(['schema', 'library', 'sync'].map(n => readFileSync(new URL(`../worker/pg/${n}.sql`, import.meta.url), 'utf8')).join('\n'));
  await pg.exec(`INSERT INTO batches(id,at,total) VALUES('old','1',0),('new','2',0);
    INSERT INTO builds(id,at,record) SELECT 'unrelated-'||n,'day','{"weapon":"unrelated"}' FROM generate_series(1,20000) n;
    INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
      SELECT id,'r','base','engine',1,'fixture',1,'a','b' FROM builds;
    INSERT INTO builds(id,at,record) VALUES('a','day','{"weapon":"a","precise":0.1234567890123456789}'),
      ('b','day','{"weapon":"b"}'),('cancelled','day','{"weapon":"cancelled"}');
    INSERT INTO queue(batch,build_id,ruler,mode) VALUES('new','a','r','base'),('old','b','r','base'),
      ('old','b','r','cycle'),('gone','cancelled','r','base'),('old','missing','r','base');
    INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
      VALUES('b','r','base','previous',8,'fixture',2,'a','b'); ANALYZE;`);
  for (let start = 1; start <= 20000; start += 500) await pg.exec(`INSERT INTO queue(batch,build_id,ruler,mode)
    SELECT 'old','unrelated-'||n,'r','base' FROM generate_series(${start},${start + 499}) n`);
  await pg.exec('ANALYZE');
  let calls = 0;
  const claim = async body => { calls++; assert.equal(body.max, 3); assert.deepEqual(body.states, ['open']); return tasks; };
  const result = await captureScoreInputs({ sql, claim, directory, engine: 'test', max: 3 });
  assert.equal(calls, 1); assert.equal(result.leased, 3); assert.equal(result.records, 2);
  assert.equal(result.rows, 3); assert.equal(result.facts, 1);
  assert(load('library.json').includes('0.1234567890123456789'), 'source numbers lost precision');
  assert.deepEqual(JSON.parse(load('library.json')).map(r => r.weapon), ['a', 'b']);
  assert.deepEqual(lines('queue.ndjson').map(r => [r.batch, r.build_id, r.mode]),
    [['old', 'b', 'base'], ['old', 'b', 'cycle'], ['new', 'a', 'base']]);
  assert.equal(lines('facts-known.ndjson')[0].measured_by, 'previous');
  const reads = queries.filter(([s]) => s.includes('jsonb_to_recordset'));
  assert.equal(reads.length, 1); assert.equal(JSON.parse(reads[0][1][0]).length, 5);
  const [text, args] = reads[0];
  for (const mode of ['force_generic_plan', 'force_custom_plan']) {
    await pg.exec(`SET plan_cache_mode=${mode}`);
    const plan = await query('EXPLAIN (ANALYZE,FORMAT JSON) ' + text, args);
    const nodes = []; const walk = n => { nodes.push(n); for (const child of n.Plans || []) walk(child); };
    walk(plan[0]['QUERY PLAN'][0].Plan);
    for (const table of ['queue', 'builds', 'scores']) {
      const scans = nodes.filter(n => n['Relation Name'] === table);
      assert(scans.length && scans.every(n => /Index/.test(n['Node Type']) && n['Actual Rows'] <= 1), `${mode}: ${table} scanned history`);
    }
  }
  console.log('ok: three claimed questions seek five keys in 20,000 unrelated rows; ordering, deduplication, cancellation and exact source bytes preserved');
  const names = ['library.json', 'queue.ndjson', 'facts-known.ndjson'];
  for (const name of names) writeFileSync(join(directory, name), 'previous');
  await assert.rejects(captureScoreInputs({ sql: { begin: async () => { throw Error('connection lost'); } },
    claim, directory, engine: 'test', max: 3 }), /connection lost/);
  assert(names.every(name => load(name) === 'previous'));
  await assert.rejects(captureScoreInputs({ sql, claim: async () => [...tasks, tasks[0]], directory, engine: 'test', max: 3 }), /claim response/);
  assert(names.every(name => load(name) === 'previous'));
  const empty = await captureScoreInputs({ sql, claim: async () => [], directory, engine: 'test' });
  assert.equal(empty.rows, 0); assert.equal(load('library.json'), '[]'); assert.equal(load('queue.ndjson'), '');
  console.log('ok: failed capture cannot publish partial inputs; oversized claims fail; empty claims read no source rows');

  const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
  const operator = await openInventoryOperator(binary);
  let builds;
  try {
    const records = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 4).map(r => r.record);
    builds = await operator.call('describe', records.map((record, i) => ({ id: String(i), record })));
  } finally { await operator.close(); }
  assert(builds.every(b => b.candidate));
  const ruler = 'standard_single_target';
  for (const b of builds) await query("INSERT INTO builds(id,at,record) VALUES($1,'day',$2)", [b.candidate, JSON.stringify(b.record)]);
  const nativeTasks = builds.slice(0, 2).map(b => ({ question: b.candidate, verb: 'simulate', demands: [{ producer: 'sweep',
    ref: `${b.candidate}|${ruler}|base`, payload: { identity: b.candidate, ruler, mode: 'base' } }] }));
  for (const b of builds.slice(0, 2)) await query("INSERT INTO queue(batch,build_id,ruler,mode) VALUES('old',$1,$2,'base')", [b.candidate, ruler]);
  await captureScoreInputs({ sql, claim: async () => nativeTasks, directory, engine: 'test', max: 2 });
  const full = JSON.stringify(builds.map(b => b.record)), limited = load('library.json');
  const dry = (library, shard) => {
    const result = spawnSync(binary, [ruler, '--dry-run', '--new-limit', '10', '--deadline', '60', '--shard', shard,
      '--queue-in', join(directory, 'queue.ndjson'), '--facts-in', join(directory, 'facts-known.ndjson')],
    { input: library, encoding: 'utf8', timeout: 30000, stdio: ['pipe', 'pipe', 'pipe'] });
    assert.equal(result.status, 0, result.stderr);
    return result.stderr.match(/dry-run: todo=\d+ work=\d+/)?.[0];
  };
  for (const shard of ['0/1', '0/2', '1/2']) {
    const expected = dry(full, shard); assert(expected, 'native dry-run did not report work');
    if (shard === '0/1') assert(expected.includes('todo=2 '), expected);
    assert.equal(dry(limited, shard), expected, `bounded native scoring changed shard ${shard}`);
  }
  console.log('ok: actual native scorer and both shards select identical work from complete and claimed-only libraries');
} finally { await pg.close(); rmSync(directory, { recursive: true, force: true }); }
