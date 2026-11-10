// SPDX-License-Identifier: AGPL-3.0-or-later
// The production classic controller resumes disk state and publishes actual native rows.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, openSync, closeSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { openCatalog } from './incremental_inventory.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { runInventoryCycle } from './inventory_pipeline.mjs';
import { captureInventory } from './capture_compute_inventory.mjs';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { questionId } from '../worker/task_contract.js';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const root = mkdtempSync(join(tmpdir(), 'board-pipeline-')), work = join(root, 'work'), bin = join(root, 'bin');
mkdirSync(work); mkdirSync(bin); copyFileSync(binary, join(bin, 'wfsim-board'));
const engine = '0ae1bf8edb0f';
writeFileSync(join(bin, 'ENGINE'), engine); writeFileSync(join(bin, 'VERSION'), 'relational-test');
const statePath = join(work, 'inventory/state.json'), load = p => JSON.parse(readFileSync(p, 'utf8'));
const pg = new PGlite(), query = (sql, args = []) => pg.query(sql, args).then(r => r.rows);
const err = openSync(join(root, 'engine.log'), 'w');
let native, operator, afterCapture, acknowledgments = 0, fullCaptures = 0, failed = true, failGate = false;
let context = {}, lostInput = true, boardBeforeAudit = 0, clockOffset = 0;
const described = [];
const open = async () => {
  context.writer?.close(); context = {};
  native = await openInventoryOperator(binary, { stderr: err });
  operator = { contract: native.contract, call: async (op, rows) => {
    if (op === 'describe') described.push(...rows.map(r => r.id));
    if (op === 'gate' && failGate) { failGate = false; throw Error('injected stateless engine failure'); }
    return native.call(op, rows);
  } };
};
const current = fn => {
  const db = openCatalog(load(statePath).catalog);
  try { return fn(db); } finally { db.close(); }
};
const unsafe = async (sql, args = []) => {
  assert(!sql.includes('inventory_cohort_rows'), 'relational capture expanded changes to a weapon cohort');
  if (sql.includes('DECLARE inventory_scan') && sql.includes('FROM builds')) fullCaptures++;
  if (sql.includes('inventory_ack(')) {
    current(db => {
      assert.equal(db.meta.complete, true);
      assert(!db.statement('SELECT 1 FROM rel_groups UNION ALL SELECT 1 FROM rel_ranges UNION ALL SELECT 1 FROM rel_jobs LIMIT 1').get());
      for (const row of JSON.parse(args[1]).flat()) assert(db.statement('SELECT 1 FROM receipts WHERE scope=? AND key=? AND revision=?')
        .get(row.scope, row.key, row.revision) || db.statement('SELECT processed FROM epoch_tokens WHERE scope=? AND key=?').get(row.scope, row.key)?.processed);
    });
    acknowledgments++;
  }
  return query(sql, args);
};
const sql = { unsafe, begin: async (options, fn) => {
  await pg.exec(`BEGIN ${options}`);
  let value;
  try { value = await fn({ unsafe }); await pg.exec('COMMIT'); }
  catch (error) { await pg.exec('ROLLBACK'); throw error; }
  if (afterCapture && value?.receipt?.status === 'sealed') { const hook = afterCapture; afterCapture = null; await hook(value); }
  return value;
} };
const cycle = budgetMs => runInventoryCycle({ work, bin, sql, operator, context, budgetMs,
  now: () => Date.now() + clockOffset,
  onBoard: async () => current(db => { assert.equal(db.meta.publication.phase, 'complete'); if (!db.meta.complete) boardBeforeAudit++; }),
  fault: async stage => { if (stage === 'after-input-commit-before-reply' && lostInput) { lostInput = false; throw Error('injected committed input reply loss'); } },
});
const mature = () => { const state = load(statePath); state.published_at = '2000-01-01T00:00:00.000Z'; writeFileSync(statePath, JSON.stringify(state)); };
async function finish() {
  for (let tries = 0; tries < 100; tries++) { const value = await cycle(15000); if (!value.pending) return value; }
  throw Error('relational pipeline did not converge');
}
async function oracle(raw, meta, tag) {
  const dir = join(root, tag); mkdirSync(dir);
  const rulers = native.contract.rulers.map(r => r.id);
  execFileSync(binary, ['--each-ruler', '--dry-run', '--gate', '--facts-in', raw.facts, '--queue-in', raw.queue,
    '--inventory-builds', join(dir, 'canonical'), '--queue-missing', join(dir, '{id}.missing'), '--queue-orphans', join(dir, '{id}.orphans')],
  { input: readFileSync(raw.library), stdio: ['pipe', 'pipe', err], timeout: 60000 });
  for (const suffix of ['missing', 'orphans']) writeFileSync(join(dir, suffix), rulers.map(r => readFileSync(join(dir, `${r}.${suffix}`), 'utf8')).join(''));
  const expected = await classifyInventory({ ...raw, meta, rulers, canonical: join(dir, 'canonical'),
    missing: join(dir, 'missing'), orphans: join(dir, 'orphans'), questions: join(dir, 'questions') });
  current(db => assert.equal(db.digest(), expected.question_digest, `full oracle mismatch: ${tag}`));
}
async function currentOracle(tag) {
  const dir = join(root, `capture-${tag}`); mkdirSync(dir);
  const raw = Object.fromEntries(['library', 'builds', 'facts', 'queue', 'tasks', 'meta'].map(n => [n, join(dir, n)]));
  const meta = await captureInventory(sql, raw, engine); await oracle(raw, meta, tag);
}
try {
  await pg.exec(['schema', 'library', 'sync', 'inventory'].map(n => readFileSync(new URL(`../worker/pg/${n}.sql`, import.meta.url), 'utf8')).join('\n'));
  await open();
  const seed = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(r => r.record);
  const canonical = await native.call('describe', seed.map((record, i) => ({ id: String(i), record })));
  for (const row of canonical) await query('INSERT INTO builds(id,at,record) VALUES($1,\'day\',$2)', [row.candidate, JSON.stringify(row.record)]);
  const future = await questionId('future', { data: 'input' });
  await query("INSERT INTO questions(id,verb,request,state,due_at,opened_at) VALUES($1,'future','{}','open','due','at')", [future]);
  await query("INSERT INTO demands(producer,ref,question,state,asked_at,due_at) VALUES('future','future',$1,'live','at','due')", [future]);
  let captured;
  afterCapture = async cut => {
    const dir = join(root, 'frozen'); mkdirSync(dir);
    captured = { meta: cut.meta, receipt: cut.receipt, raw: Object.fromEntries(Object.entries(cut.raw).map(([key, path]) => {
      const copy = join(dir, key); copyFileSync(path, copy); return [key, copy];
    })) };
    await query("UPDATE questions SET state='fact',fact_engine=$2 WHERE id=$1", [future, engine]);
  };
  await assert.rejects(cycle(0), /committed input reply loss/);
  let state; assert.equal(acknowledgments, 0);
  const normalized = described.length, generation = load(statePath).generation;
  await native.close(); await open();
  failGate = true;
  await assert.rejects(finish(), /injected stateless engine failure/);
  assert.equal(acknowledgments, 0);
  await native.close(); await open();
  state = await finish();
  assert.equal(load(statePath).generation, generation); assert.equal(fullCaptures, 1);
  assert.equal(described.length, 3); assert(normalized > 0); assert.equal(boardBeforeAudit, 1);
  assert.equal(state.published.snapshot, captured.receipt.snapshot);
  assert.equal(state.published.started_at, captured.receipt.started_at);
  await oracle(captured.raw, captured.meta, 'bootstrap');
  assert.equal((await query('SELECT count(*) n FROM inventory_dirty WHERE key=$1', [future]))[0].n, 1);
  console.log('ok: two process failures resume durable inputs without recapture/re-normalization; frozen cut remains exact; later writes stay pending');

  mature(); const before = fullCaptures; await finish();
  assert.equal(fullCaptures, before); assert.equal(described.length, 3);
  await currentOracle('lifecycle');
  assert.equal((await query('SELECT count(*) n FROM inventory_dirty'))[0].n, 0);
  console.log('ok: lifecycle-only delta reads no library/cohort and matches the independent full oracle');

  const c = canonical[0];
  await query('INSERT INTO builds(id,at,record) VALUES(\'alias\',\'day\',$1)', [JSON.stringify(c.record)]);
  mature(); const capturedBefore = fullCaptures; await finish(); assert.equal(fullCaptures, capturedBefore);
  await currentOracle('alias');
  await query('DELETE FROM builds WHERE id=$1', [c.candidate]);
  mature(); await finish(); await currentOracle('canonical-delete');
  console.log('ok: changed sources and canonical deletion maintain exact membership without complete inventory reads');
  mature(); clockOffset = 4 * 60 * 60000;
  const historyBefore = fullCaptures, normalizedBefore = described.length;
  let expired;
  for (let n = 0; n < 100; n++) { expired = await cycle(15000); if (expired.stage === 'refresh-expired-cut') break; }
  assert.equal(expired.stage, 'refresh-expired-cut'); clockOffset = 0;
  assert.equal(load(statePath).generation, generation); await finish();
  assert.equal(fullCaptures, historyBefore); assert.equal(described.length, normalizedBefore);
  mature(); afterCapture = async () => query("UPDATE factory_findings SET body=jsonb_set(body::jsonb,'{started_at}',to_jsonb(to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')))::text WHERE kind='task_inventory'");
  let superseded;
  for (let n = 0; n < 100; n++) { superseded = await cycle(15000); if (superseded.stage === 'refresh-superseded-cut') break; }
  assert.equal(superseded.stage, 'refresh-superseded-cut'); await finish();
  assert.equal(fullCaptures, historyBefore); assert.equal(described.length, normalizedBefore);
  console.log('ok: expired or superseded publication refreshes the input cut while retaining its verified incremental baseline');
  failed = false;
} catch (error) { console.error(`classic disk pipeline evidence: ${root}`); throw error; }
finally {
  await native?.close(); await pg.close(); closeSync(err);
  if (!failed && !process.env.WFSIM_INVENTORY_RELATIONS_EVIDENCE) rmSync(root, { recursive: true, force: true });
}
