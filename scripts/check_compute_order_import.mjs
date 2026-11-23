// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import postgres from './pg/node_modules/postgres/src/index.js';
import { boardCoverageSchemaSql } from './board_coverage_schema.mjs';
import { boardCoverageOrderAdapter } from './board_coverage_orders.mjs';
import { boardCoverageGaps } from './board_coverage_gaps.mjs';
import { importComputeOrders, orderImportAtEnd, orderImportConnection, verifyRefillConnection } from './compute_order_import.mjs';
import { acceptOrderStock, readAcceptedStock, runOrderStock } from './compute_order_stock.mjs';
import { admitStockPage, MAX_ACTIVE_ORDERS, publishStockWindow, reserveStockWindow, stockWindowBounds } from './compute_order_stock_window.mjs';
import { belowReadyOrderStock, orderRefill } from './compute_order_refill.mjs';
import { computeOrderImportSchema } from './compute_order_import_schema.mjs';
import { computationId } from '../worker/computation_input.js';
import { queueOrders, queueZonesSql } from '../worker/queue_counts.js';

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
  await assert.rejects(run({ path: manual, id: 'over-cap', target: 2, maxOpen: MAX_ACTIVE_ORDERS + 1 }), /max-open/);
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

  const spaced = Array.from({ length: 160 }, (_, i) => demand('stock-window-' + i, 10_000 + i));
  const stockWindow = await run({ path: file('stock-window', spaced), id: 'stock-window', target: spaced.length, rows: 32 });
  assert.equal(stockWindow.admitted, spaced.length);
  const tick = Date.now(), now = new Date(tick).toISOString(), otherEngine = 'b'.repeat(12);
  const [{ order_epoch: cut, zones }] = await reader.unsafe(queueZonesSql);
  const maxOpen = queueOrders(zones).total + 1;
  await writer.unsafe('UPDATE state_compute_scheduling SET required_engine=$1', [otherEngine]);
  const stock = await reader.unsafe(`SELECT id FROM state_compute_effective_open
    WHERE verb='simulate' AND waits=0 AND required_capability='fixed-board-input-v1' AND lanes IS NULL LIMIT 4`);
  assert.equal(stock.length, 4);
  await writer.unsafe(`UPDATE state_compute_scheduling SET required_engine=$1,
    waits=CASE WHEN id=$3 THEN 1 ELSE waits END,
    required_capability=CASE WHEN id=$4 THEN 'fixture' ELSE required_capability END,
    lease_until=CASE WHEN id=$5 THEN $6 ELSE lease_until END
    WHERE id IN ($2,$3,$4,$5)`,
  [engine, ...stock.map(({ id }) => id), new Date(tick + 60000).toISOString()]);
  assert.equal(await belowReadyOrderStock(reader, engine, maxOpen, maxOpen + 1, tick), true);
  assert.equal(await belowReadyOrderStock(reader, engine, 2, maxOpen, tick), true);
  assert.equal(await belowReadyOrderStock(reader, engine, 1, maxOpen, tick), false);
  assert.equal(await belowReadyOrderStock(reader, engine, 1, maxOpen - 1, tick), false);
  await writer.unsafe(`INSERT INTO record_compute_order_cancellations
    SELECT s.id,s.task_id,c.id,$2 FROM state_compute_scheduling s
    CROSS JOIN compute_order_cut() c WHERE s.id=$1`, [stock[0].id, now]);
  assert.equal(await belowReadyOrderStock(reader, engine, 1, maxOpen, tick), true);
  await writer.unsafe('DELETE FROM record_compute_order_cancellations WHERE question=$1', [stock[0].id]);

  const path = file('refill', [demand('refill-a', 501), demand('refill-b', 502)]);
  const inputSha = createHash('sha256').update(readFileSync(path)).digest('hex');
  const settings = { low: 1, maxHours: 2, now: () => tick,
    stock: (current, limit) => belowReadyOrderStock(reader, current, limit, 10_000, tick) };
  const goodSnapshot = { ok: true, counts_at: now, order_epoch: cut, clear_hours: 1 };
  const runs = { path, id: 'refill', target: 2, rows: 1, maxOpen, waitMs: 1 };
  await assert.rejects(run({ ...runs, refill: { inputSha, cut: 'different' } }), /approved source/);
  await assert.rejects(run({ ...runs, refill: { inputSha: '0'.repeat(64), cut } }), /approved source/);
  assert.equal((await reader.unsafe("SELECT id FROM state_compute_order_imports WHERE id='refill'")).length, 0);
  let hasRoom = false, served = otherEngine, snapshot = { ...goodSnapshot, counts_at: at };
  const held = [];
  const refill = { ...orderRefill({ ...settings, capacity: () => hasRoom, release: () => ({ engine: served }),
    demand: () => snapshot, onHold: reason => held.push(reason),
    delay: async () => {
      if (held.at(-1) === 'host_capacity') hasRoom = true;
      else if (held.at(-1) === 'served_engine') served = engine;
      else if (held.at(-1) === 'clearance_budget') snapshot = goodSnapshot;
      else if (held.at(-1) === 'ready_stock') {
        assert.equal((await reader.unsafe("SELECT id FROM state_compute_order_imports WHERE id='refill'")).length, 0);
        await writer.unsafe('UPDATE state_compute_scheduling SET required_engine=$1 WHERE id=$2', [otherEngine, stock[0].id]);
      } else assert.fail('unexpected refill hold');
    } }), inputSha, cut, pollMs: 1 };
  const refillStop = new AbortController();
  await assert.rejects(run({ ...runs, refill, signal: refillStop.signal,
    onProgress: event => { if (event.result === 'queue_full') refillStop.abort(); } }), { name: 'AbortError' });
  assert.deepEqual(held, ['host_capacity', 'served_engine', 'clearance_budget', 'ready_stock']);
  const [saved] = await reader.unsafe("SELECT admitted,processed FROM state_compute_order_imports WHERE id='refill'");
  assert.deepEqual([Number(saved.admitted), Number(saved.processed)], [1, 1]);
  await writer.unsafe(`UPDATE state_compute_scheduling s SET state='withdrawn' FROM record_compute_order_import_batches r
    WHERE r.import_id='refill' AND s.task_id=ANY(r.task_ids)`);
  const restart = { ...orderRefill({ ...settings, capacity: () => true, release: () => ({ engine }),
    demand: () => goodSnapshot, delay: () => assert.fail('refill should resume below both limits') }), inputSha, cut };
  const resumed = await run({ ...runs, refill: restart });
  assert.deepEqual([resumed.result, resumed.processed, resumed.admitted], ['complete', 2, 2]);
  const [retired] = await writer.unsafe(`SELECT task_ids[1] id FROM record_compute_order_import_batches
    WHERE import_id='refill' ORDER BY first_byte DESC LIMIT 1`);
  await writer.begin(async tx => {
    await tx.unsafe("UPDATE state_compute_scheduling SET state='withdrawn' WHERE task_id=$1", [retired.id]);
    await tx.unsafe('DELETE FROM state_compute_scheduling WHERE task_id=$1', [retired.id]);
    await tx.unsafe('DELETE FROM entity_compute_tasks WHERE id=$1', [retired.id]);
  });
  const refillSeal = { id: 'refill', target: 2, inputSha, inputBytes: readFileSync(path).length, engine, cut };
  assert(await orderImportAtEnd(reader, refillSeal), 'completed stock handoff survives retired task detail');
  assert.equal((await run({ ...runs, refill: restart })).admitted, 2);
  const serviceRole = schema + '_service';
  await writer.unsafe('CREATE ROLE ' + serviceRole);
  const serviceUrl = new URL(url);
  serviceUrl.searchParams.set('role', serviceRole);
  const proofConnection = postgres(serviceUrl.href, { max: 1, fetch_types: false, connection: orderImportConnection(true, true) });
  const refillWriter = postgres(serviceUrl.href, { max: 1, fetch_types: false, connection: orderImportConnection(false, true) });
  try {
    await Promise.all([verifyRefillConnection(refillWriter, false), verifyRefillConnection(proofConnection, true)]);
    assert.equal((await proofConnection.unsafe('SELECT current_user AS role'))[0].role, serviceRole);
    await assert.rejects(verifyRefillConnection(refillWriter, true), /approved transaction/);
    await assert.rejects(proofConnection.unsafe('CREATE SCHEMA order_refill_proof_cannot_write'), { code: '25006' });
  } finally {
    await Promise.all([proofConnection.end(), refillWriter.end()]);
    await writer.unsafe('DROP ROLE ' + serviceRole);
  }
  const sealedSource = (name, first, orders) => {
    const sourcePath = file(name, orders), bytes = readFileSync(sourcePath);
    return { id: name, file: name + '.ndjson', first, last: first + orders.length - 1,
      input_bytes: bytes.length, input_sha256: createHash('sha256').update(bytes).digest('hex') };
  };
  const manifest = join(root, 'stock.json');
  writeFileSync(manifest, '{}');
  assert.throws(() => execFileSync(process.execPath, ['scripts/compute_order_import.mjs', 'run-stock', manifest,
    '--stock-anchor=stock-first', '--max-open=10000', '--refill-config='+manifest],
  { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 10_000, killSignal: 'SIGKILL' }),
  error => error.stderr.includes('invalid order stock manifest'));
  const publishStock = files => {
    const stage = manifest + '.writing';
    writeFileSync(stage, JSON.stringify({ v: 1, engine, order_epoch: cut, files }));
    renameSync(stage, manifest);
  };
  const first = sealedSource('stock-first', 200, [demand('stock-first', 601)]);
  const second = sealedSource('stock-second', 201, [demand('stock-second', 602), demand('stock-alias', 601)]);
  const third = sealedSource('stock-third', 203, [demand('stock-third', 603)]);
  const refillSource = source => ({ ...orderRefill({ ...settings, capacity: () => true, release: () => ({ engine }),
    demand: () => goodSnapshot, delay: () => assert.fail('stock should be admitted') }), source,
    inputSha: source.inputSha, cut: source.cut });
  const supply = { manifest, writer, reader, adapters, rows: 1, maxOpen: 10_000,
    anchor: first.id, openRefill: refillSource, importOrders: importComputeOrders, atEnd: orderImportAtEnd };
  publishStock([first, second]);
  const firstBatch = await runOrderStock({ ...supply, maxBatches: 1 });
  assert.deepEqual([firstBatch.result, firstBatch.id, firstBatch.admitted], ['batch_limit', first.id, 1]);
  assert.equal((await reader.unsafe('SELECT 1 FROM state_compute_order_imports WHERE id=$1', [second.id])).length, 0);
  rmSync(join(root, first.file));
  const partial = await runOrderStock({ ...supply, maxBatches: 1 });
  assert.deepEqual([partial.result, partial.id, partial.admitted, partial.processed], ['batch_limit', second.id, 1, 1]);
  const stockStop = new AbortController(), handoffs = [];
  await assert.rejects(runOrderStock({ ...supply, signal: stockStop.signal,
    onProgress: event => { if (['complete', 'input_exhausted'].includes(event.result)) handoffs.push(event.id); },
    delay: () => {
      if (handoffs.length === 1) publishStock([first, second, third]);
      else stockStop.abort();
    } }), { name: 'AbortError' });
  assert.deepEqual(handoffs, [second.id, third.id]);
  const counted = await reader.unsafe(`SELECT id,processed,admitted FROM state_compute_order_imports
    WHERE id IN ($1,$2,$3) ORDER BY id`, [first.id, second.id, third.id]);
  assert.deepEqual(counted.map(row => [row.id, Number(row.processed), Number(row.admitted)]),
    [[first.id, 1, 1], [second.id, 2, 1], [third.id, 1, 1]]);
  publishStock([first, { ...second, first: second.first + 1 }]);
  await assert.rejects(runOrderStock(supply), /consecutive sources/);
  publishStock([second, third]);
  await assert.rejects(runOrderStock(supply), /approved import/);
  const incorrect = sealedSource('stock-bad', 204, [demand('stock-bad', 604)]);
  publishStock([first, second, third, { ...incorrect, last: 205 }]);
  await assert.rejects(runOrderStock(supply), /order count or size/);
  assert.equal((await reader.unsafe('SELECT 1 FROM state_compute_order_imports WHERE id=$1', [incorrect.id])).length, 0);
  console.log('  ok  stock resumes existing imports and an appended source without a restart, gaps or duplicate orders');

  const fourth = sealedSource('accepted-fourth', 204, [demand('accepted-fourth', 701), demand('accepted-next', 702)]);
  const fifth = sealedSource('accepted-fifth', 206, [demand('accepted-fifth', 703)]);
  const sixth = sealedSource('accepted-sixth', 207, [demand('accepted-sixth', 704)]);
  const intake = { writer, reader, manifest, anchor: first.id, atEnd: orderImportAtEnd,
    maxPendingOrders: 2, maxPendingBytes: fourth.input_bytes + fifth.input_bytes + sixth.input_bytes };
  publishStock([first, second, third, fourth]);
  const active = await count();
  assert.deepEqual(await acceptOrderStock(intake), {
    result: 'accepted', stock: first.id, sources: 4, orders: 6, through: 205,
  });
  assert.equal(await count(), active, 'durable stock acceptance does not consume scheduling slots');
  const cliReceipt = JSON.parse(execFileSync(process.execPath, ['scripts/compute_order_import.mjs', 'accept-stock', manifest,
    '--stock-anchor='+first.id, '--max-pending-orders=2', '--max-pending-bytes='+intake.maxPendingBytes],
  { env, encoding: 'utf8', timeout: 10_000 }));
  assert.equal(cliReceipt.through, 205);
  const acceptedDispatch = { ...supply, manifest: undefined, directory: root, maxBatches: 1 };
  const heldFull = await runOrderStock({ ...acceptedDispatch, maxOpen: 1 });
  assert.equal(heldFull.result, 'queue_full');
  assert.equal(await count(), active);
  assert.equal((await readAcceptedStock(reader, first.id, root)).length, 4);

  publishStock([first, second, third, fourth, fifth]);
  await assert.rejects(acceptOrderStock(intake), /pending intake budget/);
  assert.equal((await reader.unsafe('SELECT 1 FROM state_compute_order_imports WHERE id=$1', [fifth.id])).length, 0);
  assert.equal((await runOrderStock(acceptedDispatch)).id, fourth.id);
  await assert.rejects(acceptOrderStock({ ...intake, maxPendingBytes: 1 }), /pending intake budget/);
  const lostReply = { begin: async operation => {
    await writer.begin(operation);
    throw Error('simulated socket loss after stock commit');
  } };
  const [recoveredStock, concurrentStock] = await Promise.all([
    acceptOrderStock({ ...intake, writer: lostReply }),
    acceptOrderStock({ ...intake, writer: peerWriter, reader: peerReader }),
  ]);
  assert.deepEqual(recoveredStock, concurrentStock);
  assert.equal(recoveredStock.through, 206);
  publishStock([first, second, third, fourth, { ...fifth, file: sixth.file }]);
  await assert.rejects(acceptOrderStock(intake), /replaces a sealed source/);
  publishStock([first, second, third, fourth, fifth, { ...sixth, input_sha256: '0'.repeat(64) }]);
  await assert.rejects(acceptOrderStock(intake), /content differs/);
  assert.equal((await readAcceptedStock(reader, first.id, root)).length, 5);

  rmSync(manifest);
  const dispatched = await runOrderStock({ ...acceptedDispatch, maxBatches: 2 });
  assert.deepEqual([dispatched.result, dispatched.id, dispatched.admitted], ['batch_limit', fifth.id, 1]);
  assert.equal(await count(), active + 3);
  assert.equal((await reader.unsafe('SELECT 1 FROM state_compute_order_imports WHERE id=$1', [sixth.id])).length, 0);
  const orders = [...new Map(discovered.slice(1).map(order => [order.target.computation, order])).values()].slice(0, 2);
  assert.equal(orders.length, 2);
  const windowSource = sealedSource('accepted-window', 207, orders);
  publishStock([first, second, third, fourth, fifth, windowSource]);
  await acceptOrderStock({ ...intake, maxPendingBytes: windowSource.input_bytes });
  const source = (await readAcceptedStock(reader, first.id, root)).at(-1);
  const split = Buffer.byteLength(JSON.stringify(orders[0])+'\n');
  const pages = await Promise.all(orders.map(async (order, i) => {
    const first = i === 0 ? 0 : split;
    return { first, last: i === 0 ? split : source.inputBytes, orders: 1,
      prepared: await boardCoverageOrderAdapter.prepare([order], { id: source.id, first, cut, engine }) };
  }));
  const [{ zones: beforeWindow }] = await reader.unsafe(queueZonesSql);
  const occupiedSlots = queueOrders(beforeWindow).total;
  const reservation = { id: source.id, cut, pages, maxOpen: MAX_ACTIVE_ORDERS };
  assert.equal((await reserveStockWindow(writer, { ...reservation, maxOpen: occupiedSlots + 1 })).result, 'queue_full');
  await assert.rejects(reserveStockWindow(writer, { ...reservation, maxOpen: MAX_ACTIVE_ORDERS + 1 }), /max-open/);
  const seedOccupied = n => writer.unsafe(`UPDATE state_queue_zone_seed
    SET zones=jsonb_build_array(jsonb_build_array(99,$2::bigint)) WHERE epoch=$1`, [cut, n]);
  await seedOccupied(MAX_ACTIVE_ORDERS - occupiedSlots - 1);
  const bounds = { id: source.id, rows: 1, maxOpen: MAX_ACTIVE_ORDERS, limit: 2 };
  assert.deepEqual(await stockWindowBounds(reader, bounds), { pages: [1] });
  assert.equal((await reserveStockWindow(writer, reservation)).result, 'queue_full');
  await seedOccupied(MAX_ACTIVE_ORDERS - occupiedSlots - 2);
  const { key } = await reserveStockWindow(writer, reservation);
  const late = await admitStockPage({ begin: async fn => {
    await writer.begin(fn);
    throw Error('page response lost after commit');
  } }, reader, { ...reservation, key, page: pages[1] });
  assert.equal(late.admitted, 1);
  const [hidden] = await reader.unsafe(`SELECT q.task_id,compute_order_stock_dispatchable(q.task_id,q.zone) eligible
    FROM state_compute_scheduling q JOIN state_compute_order_stock_holds h ON h.task_id=q.task_id
    WHERE h.window_key=$1`, [key]);
  assert.equal(hidden.eligible, false);
  assert.equal((await writer.unsafe(`UPDATE state_compute_scheduling SET lease='test-rejected',leased_to='test-rejected'
    WHERE task_id=$1 RETURNING id`, [hidden.task_id])).length, 0);
  await assert.rejects(publishStockWindow(writer, reader, { ...reservation, key }), /unresolved page/);
  const resumedWindow = await run({ ...source, maxOpen: reservation.maxOpen, rows: 1, writer: peerWriter, reader: peerReader,
    peerWriters: [writer], refill: { ...refillSource(source), wait: ({ reserved }) => assert.equal(reserved, true) } });
  assert.deepEqual([resumedWindow.result, resumedWindow.processed, resumedWindow.admitted], ['input_exhausted', 2, 2]);
  const visible = await reader.unsafe(`SELECT t.computation FROM record_compute_order_import_batches b
    CROSS JOIN LATERAL unnest(b.task_ids) a(id) JOIN entity_compute_tasks t USING (id)
    JOIN state_compute_scheduling q ON (q.id,q.task_id)=(t.computation,t.id)
    WHERE b.import_id=$1 AND compute_order_stock_dispatchable(q.task_id,q.zone) ORDER BY q.zone_position`, [source.id]);
  assert.deepEqual(visible.map(row=>row.computation), orders.map(order=>order.target.computation));
  assert.equal((await reader.unsafe('SELECT 1 FROM state_compute_order_stock_holds WHERE window_key=$1', [key])).length, 0);
  assert.equal((await stockWindowBounds(reader, bounds)).result, 'queue_full');
  await writer.unsafe("UPDATE state_compute_scheduling SET state='withdrawn' WHERE id=$1", [orders[0].target.computation]);
  assert.deepEqual(await stockWindowBounds(reader, bounds), { pages: [1] });
  console.log('  ok  parallel stock shares the cap and publishes a durable FIFO window after a lost reply');
  console.log('  ok  durable stock acceptance has its own budget and verified receipts; the dispatcher uses only accepted sources');
  console.log('  ok  low-ready refill, release and host holds, atomic high cap and resume from the same source');
  console.log('  ok  sealed input, lost reply, pause and resume, queue cap, coverage rollback, lock retry, duplicate replay, concurrent runners and Native gaps');
} finally {
  await Promise.all([reader.end(), peerReader.end(), peerWriter.end()]);
  await writer.unsafe('DROP SCHEMA '+schema+' CASCADE').catch(() => {});
  await writer.end();
  rmSync(root, { recursive: true, force: true });
}
