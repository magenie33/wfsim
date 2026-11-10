// SPDX-License-Identifier: AGPL-3.0-or-later
// Frozen source transactions, durable progress and the real stateless engine.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, openSync, closeSync, readFileSync, renameSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { openCatalog } from './incremental_inventory.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { runRelationalInventory, relationalInventoryContext } from './inventory_relational_pipeline.mjs';
import { InventoryControl, InventoryActionRequired } from './inventory_control.mjs';
import { captureInventory } from './capture_compute_inventory.mjs';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { questionId } from '../worker/task_contract.js';
import { seedComputeWork,replaceComputeResult } from './pg/fixtures.mjs';
import { placeholders } from '../worker/db.js';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const root = mkdtempSync(join(tmpdir(), 'relational-pipeline-')), work = join(root, 'work'), bin = join(root, 'bin');
mkdirSync(work); mkdirSync(bin); copyFileSync(binary, join(bin, 'wfsim-board'));
const engine = '0ae1bf8edb0f';
writeFileSync(join(bin, 'ENGINE'), engine); writeFileSync(join(bin, 'VERSION'), 'relational-test');
const statePath = join(work, 'inventory/state.json'), load = p => JSON.parse(readFileSync(p, 'utf8'));
const pg = new PGlite(), query = (sql, args = []) => pg.query(sql, args).then(r => r.rows);
const fixtureDb={run:(text,...args)=>query(placeholders(text),args),get:async(text,...args)=>(await query(placeholders(text),args))[0]};
const err = openSync(join(root, 'engine.log'), 'w');
let native, operator, afterCapture, acknowledgments = 0, fullCaptures = 0, failed = true, failGate = false;
const described = [];
const open = async () => {
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
let generations = 0;
const unsafe = async (sql, args = []) => {
  if (sql.includes('SELECT inventory_begin(')) generations++;
  assert(!sql.includes('inventory_cohort_rows'), 'relational capture expanded changes to a weapon cohort');
  if (sql.includes('DECLARE inventory_scan') && sql.includes('FROM entity_weapon_builds')) fullCaptures++;
  if (sql.includes('inventory_ack(')) {
    current(db => {
      assert.equal(db.meta.complete, true);
      assert(!db.statement("SELECT 1 FROM state_catalog_groups UNION ALL SELECT 1 FROM state_catalog_ranges UNION ALL SELECT 1 FROM state_catalog_jobs LIMIT 1").get());
      for (const row of JSON.parse(args[1])) assert(db.statement("SELECT 1 FROM record_inventory_applied_changes WHERE scope=? AND key=? AND revision=?")
        .get(row.scope, row.key, row.revision) || db.statement("SELECT processed FROM state_inventory_epoch_tokens WHERE scope=? AND key=?").get(row.scope, row.key)?.processed);
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
const cycle = budgetMs => runRelationalInventory({ work, bin, sql, operator, budgetMs });
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
  for (const row of canonical) await query('INSERT INTO entity_weapon_builds(id,at,record) VALUES($1,\'day\',$2)', [row.candidate, JSON.stringify(row.record)]);
  const future = await questionId('future', { data: 'input' });
  await seedComputeWork(fixtureDb,{id:future,verb:'future',request:'{}',state:'open',due_at:'due',opened_at:'at'});
  await query("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('future','future',$1,'live','at','due')", [future]);
  const preserved = join(work, 'inventory/capture-1'); mkdirSync(preserved, { recursive: true });
  writeFileSync(join(preserved, 'protected'), 'original input');
  await assert.rejects(cycle(0), error => error instanceof InventoryActionRequired && error.stage === 'needs_bootstrap');
  assert.equal(generations, 0); assert.equal(fullCaptures, 0);
  assert.equal(readFileSync(join(preserved, 'protected'), 'utf8'), 'original input');
  const context = await relationalInventoryContext(bin, operator), control = new InventoryControl(join(work, 'inventory'));
  try { control.request({ mode: 'relational', ...context, reason: 'explicit stateless fixture', maxNewBytes: 1024 ** 3 }); }
  finally { control.close(); }
  console.log('ok: initial full capture is admitted only by an explicit durable task');
  let captured;
  afterCapture = async cut => {
    const dir = join(root, 'frozen'); mkdirSync(dir);
    captured = { meta: cut.meta, receipt: cut.receipt, raw: Object.fromEntries(Object.entries(cut.raw).map(([key, path]) => {
      const copy = join(dir, key); copyFileSync(path, copy); return [key, copy];
    })) };
    await replaceComputeResult(fixtureDb,future,{state:'fact',canon:'{}',fact_engine:engine,fact_at:'2026-11-01T00:00:01.000Z'});
  };
  let state = await cycle(0); assert(state.pending); assert.equal(acknowledgments, 0);
  const originalBuilds = readFileSync(load(statePath).pending.raw.builds), buildsPath = load(statePath).pending.raw.builds;
  writeFileSync(buildsPath, Buffer.concat([originalBuilds, Buffer.from('corrupt input\n')]));
  await assert.rejects(cycle(15000), error => error.stage === 'needs_restore');
  assert.equal(generations, 1); assert.equal(fullCaptures, 1); assert.equal(acknowledgments, 0);
  writeFileSync(buildsPath, originalBuilds);
  console.log('ok: changed sealed input is refused before ACK without starting a replacement capture');
  const normalized = described.length, generation = load(statePath).generation;
  await native.close(); await open();
  failGate = true;
  await assert.rejects(finish(), /injected stateless engine failure/);
  assert.equal(acknowledgments, 0);
  await native.close(); await open();
  state = await finish();
  assert.equal(load(statePath).generation, generation); assert.equal(fullCaptures, 1);
  assert.equal(described.length, 3); assert(normalized > 0);
  assert.equal(state.published.snapshot, captured.receipt.snapshot);
  assert.equal(state.published.started_at, captured.receipt.started_at);
  await oracle(captured.raw, captured.meta, 'bootstrap');
  assert.equal((await query('SELECT count(*) n FROM state_compute_inventory_invalidations WHERE key=$1', [future]))[0].n, 1);
  console.log('ok: two process failures resume durable inputs without recapture/re-normalization; frozen cut remains exact; later writes stay pending');

  mature(); const before = fullCaptures; await finish();
  assert.equal(fullCaptures, before); assert.equal(described.length, 3);
  await currentOracle('lifecycle');
  assert.equal((await query('SELECT count(*) n FROM state_compute_inventory_invalidations'))[0].n, 0);
  console.log('ok: lifecycle-only delta reads no library/cohort and matches the independent full oracle');

  const c = canonical[0];
  await query('INSERT INTO entity_weapon_builds(id,at,record) VALUES(\'alias\',\'day\',$1)', [JSON.stringify(c.record)]);
  mature(); const capturedBefore = fullCaptures; await finish(); assert.equal(fullCaptures, capturedBefore);
  await currentOracle('alias');
  await query('DELETE FROM entity_weapon_builds WHERE id=$1', [c.candidate]);
  mature(); await finish(); await currentOracle('canonical-delete');
  console.log('ok: changed sources and canonical deletion maintain exact membership without complete inventory reads');
  for (const [kind, hook] of [
    ['expired', async cut => query("UPDATE record_inventory_captures SET started_at=to_char((clock_timestamp()-interval '4 hours') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') WHERE epoch=$1", [cut.receipt.epoch])],
    ['superseded', async () => query("UPDATE state_factory_findings SET body=jsonb_set(body::jsonb,'{started_at}',to_jsonb(to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')))::text WHERE kind='task_inventory'")],
  ]) {
    mature(); afterCapture = hook;
    const before = fullCaptures, derived = described.length;
    const discarded = await cycle(15000);
    assert.equal(discarded.stage, `refresh-${kind}-cut`);
    assert.equal(load(statePath).generation, generation);
    await finish();
    assert.equal(fullCaptures, before); assert.equal(described.length, derived);
  }
  console.log('ok: expired or superseded publication refreshes the input cut while retaining its verified incremental baseline');
  const beforeState = readFileSync(statePath), beforeGeneration = generations, beforeCaptures = fullCaptures;
  const catalogPath = load(statePath).catalog;
  renameSync(catalogPath, catalogPath + '.preserved');
  try { await assert.rejects(cycle(15000), error => error.stage === 'needs_bootstrap'); }
  finally { renameSync(catalogPath + '.preserved', catalogPath); }
  writeFileSync(join(bin, 'ENGINE'), 'b'.repeat(12));
  try { await assert.rejects(cycle(15000), error => error.stage === 'needs_bootstrap'); }
  finally { writeFileSync(join(bin, 'ENGINE'), engine); }
  assert.equal(generations, beforeGeneration); assert.equal(fullCaptures, beforeCaptures);
  assert(readFileSync(statePath).equals(beforeState));
  assert.equal(readFileSync(join(preserved, 'protected'), 'utf8'), 'original input');
  console.log('ok: missing catalog and changed engine retain original inputs without a new generation or full capture');
  failed = false;
} catch (error) { console.error(`relational pipeline evidence: ${root}`); throw error; }
finally {
  await native?.close(); await pg.close(); closeSync(err);
  if (!failed && !process.env.WFSIM_INVENTORY_RELATIONS_EVIDENCE) rmSync(root, { recursive: true, force: true });
}
