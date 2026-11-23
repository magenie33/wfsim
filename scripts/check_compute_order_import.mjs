// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import postgres from './pg/node_modules/postgres/src/index.js';
import { boardCoverageSchemaSql } from './board_coverage_schema.mjs';
import { boardCoverageOrderAdapter } from './board_coverage_orders.mjs';
import { boardCoverageGaps } from './board_coverage_gaps.mjs';
import { importComputeOrders } from './compute_order_import.mjs';
import { computeOrderImportSchema } from './compute_order_import_schema.mjs';
import { computationId } from '../worker/computation_input.js';

const url = process.env.WFSIM_COVERAGE_TEST_URL;
assert(url && ['127.0.0.1', 'localhost', '[::1]'].includes(new URL(url).hostname), 'use an isolated local PostgreSQL');
const schema = 'order_import_' + randomBytes(6).toString('hex');
const connect = () => postgres(url, { max: 1, fetch_types: false, onnotice() {},
  connection: { search_path: schema+',pg_catalog,pg_temp', statement_timeout: 15_000 } });
const writer = connect(), reader = connect(), peerWriter = connect(), peerReader = connect();
const root = mkdtempSync(join(tmpdir(), 'wfsim-order-import-check-'));
const binary = resolve(process.env.WFSIM_REQUESTED_INTAKE_BIN || 'target/debug/wfsim-intake');
const native = createHash('sha256').update(readFileSync(binary)).digest('hex');
const at = '2026-11-20T00:00:00.000Z', engine = 'a'.repeat(12);
const loadSql = name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), 'utf8');
const file = (name, orders) => {
  const path = join(root, name+'.ndjson');
  writeFileSync(path, orders.map(row => JSON.stringify(row)+'\n').join(''));
  return path;
};
const adapters = new Map([['board_coverage', boardCoverageOrderAdapter]]);
const run = options => importComputeOrders({ writer, reader, engine, adapters, maxOpen: 10_000, ...options });
const count = async () => (await reader.unsafe('SELECT count(*)::int n FROM entity_compute_tasks'))[0].n;
const demand = (ref, seed, zone = 3) => ({ kind: 'demand', ask: { producer: 'user_calculation', ref,
  trigger_id: ref, verb: 'simulate_report', request: { seed }, zone, due_in_ms: 0, payload: { ref } } });

try {
  await writer.unsafe('CREATE SCHEMA '+schema);
  for (const name of ['schema', 'library', 'sync', 'inventory', 'build_workflows', 'board_admission', 'queue_counts'])
    await writer.unsafe(loadSql(name));
  const install = sql => writer.unsafe(sql.replace('SET LOCAL ROLE wfsim;', ''));
  await install(boardCoverageSchemaSql('pg_default', schema));
  const getCoverage = () => writer.unsafe(`SELECT oid::text,to_jsonb(p) AS definition FROM pg_proc p
    WHERE pronamespace=current_schema()::regnamespace AND proname LIKE 'board_coverage_%' ORDER BY oid`);
  const original = await getCoverage();
  await install(computeOrderImportSchema('pg_default', schema));
  assert.equal(original.length, 6);
  assert.deepEqual(await getCoverage(), original);
  await writer.unsafe(loadSql('order_cut_indexes').replaceAll('INDEX CONCURRENTLY', 'INDEX'));
  await writer.unsafe(`INSERT INTO state_task_pipeline_controls(name,paused,changed_at,changed_by) VALUES('admission',true,'test','test');
    INSERT INTO state_factory_parameters(name,value,set_at,set_by) VALUES('board_admission',0,'test','test') ON CONFLICT(name) DO UPDATE SET value=0;
    SELECT compute_order_cut_stage('{"id":"order-import-cut","cut_at":"2026-11-16T01:53:48.726Z",
      "task_cutoff":1,"keep_boundary":[3,1,"fixture"],"source":{}}');
    SELECT compute_order_cut_capture('order-import-cut');
    SELECT compute_order_cut_activate('order-import-cut',0,'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
    SELECT setval(pg_get_serial_sequence('entity_compute_tasks','id'),1,true);`);
  const manual = file('manual', [demand('first', 1), demand('same-input', 1), demand('second', 2, 0), demand('extra', 3)]);
  assert.equal((await run({ path: manual, id: 'manual', target: 2, rows: 2 })).result, 'queue_unavailable');
  assert.equal(await count(), 0);
  await writer.unsafe("INSERT INTO state_queue_zone_seed(epoch,zones,sampled_at,workload) VALUES(coalesce(compute_order_cut_epoch(),''),'[]',now(),'[]')");

  const controller = new AbortController();
  let lost = false;
  const interrupted = { unsafe: (...args) => writer.unsafe(...args), begin: async fn => {
    const result = await writer.begin(fn);
    if (!lost) { lost = true; throw Error('fixture dropped a successful commit response'); }
    return result;
  } };
  await assert.rejects(run({ writer: interrupted, path: manual, id: 'manual', target: 2, rows: 2,
    signal: controller.signal, onProgress: () => controller.abort() }), { name: 'AbortError' });
  assert.equal(await count(), 1);
  await writer.unsafe("UPDATE state_compute_order_imports SET paused=true WHERE id='manual'");
  assert.equal((await run({ path: manual, id: 'manual', target: 2 })).result, 'paused');
  await writer.unsafe("UPDATE state_compute_order_imports SET paused=false WHERE id='manual'");
  assert.equal((await run({ path: manual, id: 'manual', target: 2, maxOpen: 1 })).result, 'queue_full');
  const complete = await run({ path: manual, id: 'manual', target: 2 });
  assert.deepEqual([complete.result, complete.admitted, complete.processed, complete.without_new_task], ['complete', 2, 3, 1]);
  assert.equal(await count(), 2);
  assert.equal((await run({ path: manual, id: 'manual', target: 2 })).admitted, 2);
  await assert.rejects(run({ path: file('changed', [demand('changed', 4)]), id: 'manual', target: 2 }), /immutable inputs/);
  const occupied = await reader.unsafe('SELECT zone,count(*)::int n FROM state_compute_scheduling GROUP BY zone ORDER BY zone');
  assert.deepEqual(occupied.map(row => [Number(row.zone), row.n]), [[0,1],[3,1]]);
  const repeated = await run({ path: file('duplicates', [demand('first', 1)]), id: 'duplicates', target: 2 });
  assert.deepEqual([repeated.result, repeated.admitted, repeated.remaining], ['input_exhausted', 0, 2]);

  const sample = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url), 'utf8'))[0].record;
  const [build] = execFileSync(binary, ['--deadline', '30'], { encoding: 'utf8', timeout: 35000,
    input: JSON.stringify({ id: 'capture', at, record: sample, initiator: 'system', observation: {} })+'\n' }).trim().split('\n').map(JSON.parse);
  const scopes = build.flow.nodes.filter(n => n.kind === 'measure' && n.identity === build.id);
  assert(scopes.length > 2);
  const anchor = scopes[0];
  await writer.unsafe('INSERT INTO entity_weapon_builds(id,at,record) VALUES($1,$2,$3),($4,$2,$3)',
    [build.id, at, JSON.stringify(build.record), 'f'.repeat(32)]);
  await writer.unsafe(`INSERT INTO state_board_measurement_selection
    (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
    VALUES($1,$2,$3,'verified:test',1,'kpm',0,$4,$4)`, [build.id, anchor.ruler, anchor.mode, at]);
  const page = await boardCoverageGaps({ reader, binary, native });
  assert.equal(page.scanned, 2);
  assert.equal(page.measured, 1);
  assert.equal(page.orders.length, scopes.length - 1);
  const selected = page.orders.slice(0, 2);
  const duplicate = { ...selected[0], plan: { ...selected[0].plan,
    deadlines: { ...selected[0].plan.deadlines, system: at } } };
  const coverageFile = file('coverage', [selected[0], duplicate, selected[1]]);
  await assert.rejects(run({ path: coverageFile, id: 'coverage', target: 2,
    adapters: new Map([['board_coverage', { ...boardCoverageOrderAdapter, async admit(...args) {
      await boardCoverageOrderAdapter.admit(...args);
      throw Error('fixture aborted the transaction');
    } }]]) }), /aborted the transaction/);
  assert.equal(await count(), 2, 'failed coverage write did not strand jobs');
  const coverage = await run({ path: coverageFile, id: 'coverage', target: 2 });
  assert.deepEqual([coverage.result, coverage.admitted, coverage.processed], ['complete', 2, 3]);
  const newer = await boardCoverageGaps({ reader, binary, native, build: build.id });
  assert.equal(newer.orders.length, page.orders.length - selected.length);
  const resent = await run({ path: coverageFile, id: 'manual-coverage', target: 2 });
  assert.deepEqual([resent.result, resent.admitted], ['input_exhausted', 0]);

  const concurrent = { path: file('concurrent', [demand('one', 3), demand('two', 4)]), id: 'concurrent', target: 2, rows: 1 };
  const results = await Promise.all([run(concurrent), run({ ...concurrent, writer: peerWriter, reader: peerReader })]);
  assert(results.every(result => result.result === 'complete' && result.admitted === 2));
  const [physical] = await reader.unsafe(`SELECT count(*)::int pages, sum(cardinality(task_ids))::int new
    FROM record_compute_order_import_batches WHERE import_id='concurrent'`);
  assert.deepEqual({ ...physical }, { pages: 2, new: 2 });

  const contendedFile = file('contended', [demand('contended', 5)]);
  for (const retryRollbacks of [false, true]) {
    let timedOut = false;
    const locked = Promise.withResolvers(), release = Promise.withResolvers();
    const hold = peerWriter.begin(async tx => {
      await tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('compute-order-import',0))");
      locked.resolve();
      await release.promise;
    });
    const contendedWriter = { unsafe: (...args) => writer.unsafe(...args), begin: async fn => {
      try { return await writer.begin(fn); }
      catch (error) {
        if (error.code === '55P03') { timedOut = true; release.resolve(); }
        throw error;
      }
    } };
    try {
      await Promise.race([locked.promise, hold]);
      const result = await run({ writer: contendedWriter, path: contendedFile, id: 'contended', target: 1, retryRollbacks });
      assert.equal(timedOut, true);
      assert.deepEqual([result.result, result.admitted], retryRollbacks ? ['complete', 1] : ['rolled_back', 0]);
    } finally { release.resolve(); await hold; }
  }

  const cliUrl = new URL(url);
  cliUrl.searchParams.set('search_path', schema+',pg_catalog,pg_temp');
  const env = { ...process.env, WFSIM_COMPUTE_ORDER_DATABASE_URL: cliUrl.href };
  const scan = execFileSync(process.execPath, ['scripts/board_coverage_gaps.mjs', `--binary=${binary}`, `--native=${native}`,
    `--build=${build.id}`], { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 40_000 });
  const discovered = scan.trim().split('\n').map(JSON.parse);
  assert.deepEqual(discovered.map(order => order.target), newer.orders.map(order => order.target));
  const status = JSON.parse(execFileSync(process.execPath, ['scripts/compute_order_import.mjs', 'status', 'concurrent'],
    { env, encoding: 'utf8', timeout: 10_000 }));
  assert.deepEqual([Number(status.admitted), Number(status.processed)], [2, 2]);
  const cliImport = JSON.parse(execFileSync(process.execPath, ['scripts/compute_order_import.mjs', 'run', 'cli-coverage',
    file('cli-coverage', discovered), '--target=1', `--engine=${engine}`, '--max-open=10000', '--once'],
    { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 15_000 }));
  assert.deepEqual([cliImport.result, cliImport.admitted, cliImport.processed], ['complete', 1, 1]);

  const template = selected[0];
  const benchmark = process.argv.includes('--benchmark');
  const widths = benchmark ? [32, 256, 512, 1024] : [96], candidateCount = benchmark ? 1024 : 96;
  let filled = 0;
  for (const width of widths) {
    const name = 'wide-' + width;
    const wide = await Promise.all(Array.from({length: candidateCount}, async (_, index) => {
      const ruler = `${name}_${index}`, request = { ...sample, seed: width * 4096 + index }, mode = template.target.mode;
      const computation = await computationId('simulate', request);
      return { ...template, target: { ...template.target, ruler, computation }, plan: {
        ...template.plan, flow: { ...template.plan.flow, scope: {
          boards: [{ id: ruler, scenario: { kind: 'test' } }], modes: { [sample.weapon]: [mode] } }, templates: [request],
        patches: [{ set: {}, remove: [] }], nodes: [{ ...template.plan.flow.nodes[0], ruler, computation }] } } };
    }));
    const path = file(name, wide), started = performance.now();
    const result = await run({ path, id: name, target: wide.length, rows: width });
    const seconds = (performance.now() - started) / 1000;
    assert.deepEqual([result.result, result.admitted], ['complete', wide.length]);
    const [proof] = await reader.unsafe(`SELECT sum(orders)::int as orders,sum(cardinality(task_ids))::int as new,
      count(*)::int as pages FROM record_compute_order_import_batches WHERE import_id=$1`, [name]);
    assert.deepEqual({ ...proof }, { orders: wide.length, new: wide.length, pages: Math.ceil(wide.length / width) });
    filled += wide.length;
    console.log(`  ok  ${wide.length} independently verified tasks across ${proof.pages} batch(es) of up to ${width} in ${seconds.toFixed(3)}s`);
  }
  const [flows] = await reader.unsafe(`SELECT count(*)::int n FROM relation_task_consumers
    WHERE producer='build_workflow' AND state='live'`);
  assert(flows.n >= filled + selected.length);
  console.log('  ok  sealed input, lost reply, pause and resume, queue cap, coverage rollback, lock retry, duplicate replay, concurrent runners and Native gaps');
} finally {
  await Promise.all([reader.end(), peerReader.end(), peerWriter.end()]);
  await writer.unsafe('DROP SCHEMA '+schema+' CASCADE').catch(() => {});
  await writer.end();
  rmSync(root, { recursive: true, force: true });
}
