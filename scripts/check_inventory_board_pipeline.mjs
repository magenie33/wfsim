// SPDX-License-Identifier: AGPL-3.0-or-later
// The production classic controller resumes disk state and publishes actual native rows.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, openSync, closeSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import postgres from './pg/node_modules/postgres/src/index.js';
import { randomUUID } from 'node:crypto';
import { openCatalog } from './incremental_inventory.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { captureInventory } from './capture_compute_inventory.mjs';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { questionId } from '../worker/task_contract.js';
import { InventoryControl } from './inventory_control.mjs';
import { seedComputeWork } from './pg/fixtures.mjs';
import { placeholders } from '../worker/db.js';
const { runBoardInventoryCycle: runInventoryCycle, boardInventoryContext } = await import(process.env.WFSIM_BOARD_PIPELINE_MODULE
  ? pathToFileURL(resolve(process.env.WFSIM_BOARD_PIPELINE_MODULE)).href : './inventory_board_pipeline.mjs');

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const root = mkdtempSync(join(tmpdir(), 'board-pipeline-')), work = join(root, 'work'), bin = join(root, 'bin');
mkdirSync(work); mkdirSync(bin); copyFileSync(binary, join(bin, 'wfsim-board'));
const engine = '0ae1bf8edb0f';
writeFileSync(join(bin, 'ENGINE'), engine); writeFileSync(join(bin, 'VERSION'), 'relational-test');
const statePath = join(work, 'inventory/state.json'), load = p => JSON.parse(readFileSync(p, 'utf8'));
const connection = process.env.WFSIM_BOARD_PIPELINE_PG_URL;
if (connection) assert(['127.0.0.1','localhost'].includes(new URL(connection).hostname), 'use a local fixture database');
const tcp = connection ? postgres(connection, { max: 1, fetch_types: false, onnotice: () => {} }) : null;
const pg = tcp ? null : new PGlite(), schema = 'board_pipeline_' + randomUUID().replaceAll('-', '');
let active = tcp;
const query = (sql, args = []) => tcp ? active.unsafe(sql, args) : pg.query(sql, args).then(r => r.rows);
const fixtureDb={run:(text,...args)=>query(placeholders(text),args),get:async(text,...args)=>(await query(placeholders(text),args))[0]};
const err = openSync(join(root, 'engine.log'), 'w');
let native, operator, afterCapture, acknowledgments = 0, fullCaptures = 0, failed = true, failGate = false;
let context = {}, lostInput = true, boardBeforeAudit = 0, clockOffset = 0, lostAck = false, closedConnection = false;
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
  if (closedConnection && sql.includes('SELECT inventory_inputs(')) {
    closedConnection = false; throw Object.assign(Error('injected physical connection loss before seal'), { code: 'CONNECTION_CLOSED' });
  }
  assert(!sql.includes('inventory_cohort_rows'), 'relational capture expanded changes to a weapon cohort');
  if (sql.includes('DECLARE inventory_scan') && sql.includes('FROM entity_weapon_builds')) fullCaptures++;
  if (sql.includes('inventory_ack(')) {
    current(db => {
      assert.equal(db.meta.complete, true);
      assert(!db.statement('SELECT 1 FROM state_catalog_groups UNION ALL SELECT 1 FROM state_catalog_ranges UNION ALL SELECT 1 FROM state_catalog_jobs LIMIT 1').get());
      for (const row of JSON.parse(args[1]).flat()) assert(db.statement('SELECT 1 FROM record_inventory_applied_changes WHERE scope=? AND key=? AND revision=?')
        .get(row.scope, row.key, row.revision) || db.statement('SELECT processed FROM state_inventory_epoch_tokens WHERE scope=? AND key=?').get(row.scope, row.key)?.processed);
    });
    acknowledgments++;
  }
  return query(sql, args);
};
const sql = { unsafe, begin: async (options, fn) => {
  let value;
  if (tcp) value = await tcp.begin(options, async transaction => {
    active = transaction; try { return await fn({ unsafe }); } finally { active = tcp; }
  });
  else {
    await pg.exec(`BEGIN ${options}`);
    try { value = await fn({ unsafe }); await pg.exec('COMMIT'); }
    catch (error) { await pg.exec('ROLLBACK'); throw error; }
  }
  if (afterCapture && value?.receipt?.status === 'sealed') { const hook = afterCapture; afterCapture = null; await hook(value); }
  return value;
} };
const cycle = budgetMs => runInventoryCycle({ work, bin, sql, operator, context, budgetMs,
  now: () => Date.now() + clockOffset,
  onBoard: async () => current(db => { assert.equal(db.meta.publication.phase, 'complete'); if (!db.meta.complete) boardBeforeAudit++; }),
  fault: async stage => {
    if (stage === 'after-input-commit-before-reply' && lostInput) { lostInput = false; throw Error('injected committed input reply loss'); }
    if (stage === 'after-ack-before-offset' && lostAck) { lostAck = false; throw Error('injected committed ACK reply loss'); }
  },
});
const mature = () => { const state = load(statePath); state.published_at = '2000-01-01T00:00:00.000Z'; writeFileSync(statePath, JSON.stringify(state)); };
async function finish() {
  for (let tries = 0; tries < 100; tries++) { const value = await cycle(15000); if (!value.pending) return value; }
  throw Error('relational pipeline did not converge');
}
async function oracle(raw, meta, tag) {
  const dir = join(root, tag); mkdirSync(dir);
  const library = raw.library || join(dir,'library.json');
  if (!raw.library) writeFileSync(library,JSON.stringify(readFileSync(raw.builds,'utf8').split('\n').filter(Boolean).map(line => JSON.parse(line).record)));
  const rulers = native.contract.rulers.map(r => r.id);
  execFileSync(binary, ['--each-ruler', '--dry-run', '--gate', '--facts-in', raw.facts, '--queue-in', raw.queue,
    '--inventory-builds', join(dir, 'canonical'), '--queue-missing', join(dir, '{id}.missing'), '--queue-orphans', join(dir, '{id}.orphans')],
  { input: readFileSync(library), stdio: ['pipe', 'pipe', err], timeout: 60000 });
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
  const ddl = ['schema', 'library', 'sync', 'inventory','build_workflows'].map(n => readFileSync(new URL(`../worker/pg/${n}.sql`, import.meta.url), 'utf8')).join('\n');
  if (tcp) { await tcp.unsafe(`CREATE SCHEMA ${schema};SET search_path TO ${schema},pg_catalog,pg_temp`).simple(); await tcp.unsafe(ddl).simple(); }
  else await pg.exec(ddl);
  console.log('classic disk pipeline: ' + (tcp ? 'postgres.js TCP transport' : 'PGlite transport'));
  await open();
  await assert.rejects(runInventoryCycle({ work, bin, sql, operator, context, diskBudgetBytes: 1 }), /cannot hold/);
  assert.equal((await query('SELECT inventory_status() status'))[0].status?.generation ?? null, null);
  assert.equal(existsSync(statePath), false, 'an invalid explicit disk budget must fail before generation writes');
  const seed = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(r => r.record);
  const canonical = await native.call('describe', seed.map((record, i) => ({ id: String(i), record })));
  for (const row of canonical) await query('INSERT INTO entity_weapon_builds(id,at,record) VALUES($1,\'day\',$2)', [row.candidate, JSON.stringify(row.record)]);
  const future = await questionId('future', { data: 'input' });
  await seedComputeWork(fixtureDb,{ id:future,verb:'future',request:'{}',state:'open',due_at:'due',opened_at:'at' });
  await query("INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES('future','future',$1,'live','at','due')", [future]);
  await assert.rejects(cycle(0), /explicit rebuild task/);
  const control = new InventoryControl(join(work,'inventory'));
  try { control.request({ mode:'board',...await boardInventoryContext(bin,operator),reason:'explicit bounded board fixture',maxNewBytes:1024 ** 3 }); }
  finally { control.close(); }
  let captured;
  afterCapture = async cut => {
    const dir = join(root, 'frozen'); mkdirSync(dir);
    captured = { meta: cut.meta, receipt: cut.receipt, raw: Object.fromEntries(Object.entries(cut.raw).map(([key, path]) => {
      const copy = join(dir, key); copyFileSync(path, copy); return [key, copy];
    })) };
    await seedComputeWork(fixtureDb,{ id:future,verb:'future',request:'{}',state:'fact',fact_engine:engine,fact_at:new Date().toISOString() });
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
  assert.equal(described.length, 3); assert(normalized > 0); assert.equal(boardBeforeAudit, 0);
  assert.equal(state.published.snapshot, captured.receipt.snapshot);
  assert.equal(state.published.started_at, captured.receipt.started_at);
  await oracle(captured.raw, captured.meta, 'bootstrap');
  await runInventoryCycle({ work, bin, sql, operator, context, diskBudgetBytes: 9 * 1024 ** 3 });
  assert.equal(load(statePath).generation, generation, 'a resource budget must not change the input generation');
  assert.equal(Number((await query('SELECT count(*) n FROM state_compute_inventory_invalidations WHERE key=$1', [future]))[0].n), 1);
  console.log('ok: two process failures resume durable inputs without recapture/re-normalization; frozen cut remains exact; later writes stay pending');

  mature(); const before = fullCaptures, nativePid = native.pid; closedConnection = true;
  await assert.rejects(cycle(15000), /physical connection loss before seal/);
  assert.equal(load(statePath).generation, generation); assert.equal(load(statePath).pending, null);
  current(db => assert.equal(db.meta.complete, true)); lostAck = true;
  await assert.rejects(finish(), /committed ACK reply loss/);
  await finish(); assert.equal(native.pid, nativePid, 'transient SQL failures must not restart the native operator');
  assert.equal(fullCaptures, before); assert.equal(described.length, 3);
  await currentOracle('lifecycle');
  assert.equal(Number((await query('SELECT count(*) n FROM state_compute_inventory_invalidations'))[0].n), 0);
  console.log('ok: unsealed connection loss gets a new honest receipt without dropping the baseline/native operator; lost committed ACK replays safely; lifecycle-only delta matches every full-oracle tuple');

  const c = canonical[0];
  await query('INSERT INTO entity_weapon_builds(id,at,record) VALUES(\'alias\',\'day\',$1)', [JSON.stringify(c.record)]);
  mature(); const capturedBefore = fullCaptures; await finish(); assert.equal(fullCaptures, capturedBefore);
  await currentOracle('alias');
  await query('DELETE FROM entity_weapon_builds WHERE id=$1', [c.candidate]);
  mature(); await finish(); await currentOracle('canonical-delete');
  console.log('ok: changed sources and canonical deletion maintain exact membership without complete inventory reads');
  mature(); clockOffset = 4 * 60 * 60000;
  const historyBefore = fullCaptures, normalizedBefore = described.length;
  let expired;
  for (let n = 0; n < 100; n++) { expired = await cycle(15000); if (expired.stage === 'refresh-expired-cut') break; }
  assert.equal(expired.stage, 'refresh-expired-cut'); clockOffset = 0;
  assert.equal(load(statePath).generation, generation); await finish();
  assert.equal(fullCaptures, historyBefore); assert.equal(described.length, normalizedBefore);
  mature(); afterCapture = async () => query("UPDATE state_factory_findings SET body=jsonb_set(body::jsonb,'{started_at}',to_jsonb(to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"')))::text WHERE kind='task_inventory'");
  let superseded;
  for (let n = 0; n < 100; n++) { superseded = await cycle(15000); if (superseded.stage === 'refresh-superseded-cut') break; }
  assert.equal(superseded.stage, 'refresh-superseded-cut'); await finish();
  assert.equal(fullCaptures, historyBefore); assert.equal(described.length, normalizedBefore);
  console.log('ok: expired or superseded publication refreshes the input cut while retaining its verified incremental baseline');
  failed = false;
} catch (error) { console.error(`classic disk pipeline evidence: ${root}`); throw error; }
finally {
  await native?.close();
  if (tcp) { try { await tcp.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`).simple(); } finally { await tcp.end({ timeout: 5 }); } }
  else await pg.close(); closeSync(err);
  if (!failed && !process.env.WFSIM_INVENTORY_RELATIONS_EVIDENCE) rmSync(root, { recursive: true, force: true });
}
