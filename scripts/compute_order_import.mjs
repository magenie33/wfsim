// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash } from 'node:crypto';
import { open } from 'node:fs/promises';
import { setTimeout } from 'node:timers/promises';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { pgDb, postgresClient } from '../worker/db.js';
import { facts } from '../worker/factory.js';
import { queueOrders, queueZonesSql } from '../worker/queue_counts.js';
import { COMPUTE_ORDER_IMPORT_ROWS } from './compute_order_import_schema.mjs';

const PAGE_BYTES = 8 * 1024 * 1024;
const REFILL_TRANSACTION_MS = 8000;
const ROLLED_BACK = new Set(['55P03', '40P01', '40001', '25P02']);
const integer = n => Number.isSafeInteger(n) && n > 0;
const readState = (sql, id) => sql.unsafe('SELECT * FROM state_compute_order_imports WHERE id=$1', [id]);

async function* orderLines(file, offset) {
  let prefix = Buffer.alloc(0);
  for await (const bytes of file.createReadStream({ start: offset, autoClose: false })) {
    const data = prefix.length ? Buffer.concat([prefix, bytes]) : bytes;
    let start = 0;
    for (let end; (end = data.indexOf(10, start)) >= 0; start = end + 1) {
      const line = data.subarray(start, end);
      offset += end - start + 1;
      yield { order: JSON.parse(line.toString('utf8')), end: offset };
    }
    prefix = data.subarray(start);
    if (prefix.length >= PAGE_BYTES) throw Error('one order exceeds the batch byte limit');
  }
  if (prefix.length) throw Error('order files require newline-terminated JSON records');
}

function prepareDemands(orders, { engine }) {
  const asks = orders.map(({ ask }) => {
    if (!ask || typeof ask.trigger_id !== 'string' || !ask.trigger_id.trim() || !Number.isFinite(ask.due_in_ms))
      throw Error('a bulk demand needs its original trigger and deadline');
    return ask;
  });
  return { asks, engine };
}

async function admitDemands(tx, { asks, engine }) {
  const client = postgresClient(tx);
  const library = pgDb({ ...client, transaction: fn => fn(client) });
  const ids = await facts().demandAll({ LIBRARY: library }, asks, engine);
  const tasks = await tx.unsafe(`SELECT t.id::text AS id
    FROM jsonb_array_elements_text(($1::text)::jsonb) x(id)
    CROSS JOIN LATERAL (SELECT q.task_id FROM state_compute_scheduling q
      WHERE q.id=x.id AND q.state='open' OFFSET 0) q
    JOIN entity_compute_tasks t ON t.id=q.task_id AND t.xmin=pg_current_xact_id()::xid
    ORDER BY t.id`, [JSON.stringify([...new Set(ids)])]);
  return tasks.map(({ id }) => id);
}

async function commitPage(writer, { id, first, last, orderCount, cut, maxOpen, adapter, prepared, automatic }) {
  return writer.begin(async tx => {
    const timeout = automatic ? REFILL_TRANSACTION_MS : 15_000;
    await tx.unsafe(`SET LOCAL lock_timeout='500ms'; SET LOCAL statement_timeout='${timeout}ms'; SET LOCAL jit=off`);
    await tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('compute-order-import',0))");
    const [run] = await tx.unsafe('SELECT * FROM state_compute_order_imports WHERE id=$1 FOR UPDATE', [id]);
    if (run.paused) return 'paused';
    if (Number(run.next_byte) !== first) throw Error('another runner advanced this order import');
    const [epoch] = await tx.unsafe("SELECT compute_order_cut_lock(), coalesce(compute_order_cut_epoch(),'') cut");
    if (epoch.cut !== cut) throw Error('the compute order cut changed');
    const [queue] = await tx.unsafe(queueZonesSql);
    if (!queue) return 'queue_unavailable';
    if (queueOrders(queue.zones).total + orderCount > maxOpen) return 'queue_full';
    const ids = await adapter.admit(tx, prepared);
    if (ids.length > orderCount || new Set(ids).size !== ids.length || ids.some(task => !/^[1-9][0-9]*$/.test(task)))
      throw Error('invalid physical task receipt');
    await tx.unsafe(`INSERT INTO record_compute_order_import_batches VALUES($1,$2,$3,$4,
      ARRAY(SELECT value::bigint FROM jsonb_array_elements_text(($5::text)::jsonb)))`,
      [id, first, last, orderCount, JSON.stringify(ids)]);
    await tx.unsafe(`UPDATE state_compute_order_imports
      SET next_byte=$2,processed=processed+$3,admitted=admitted+$4 WHERE id=$1`, [id, last, orderCount, ids.length]);
    return 'committed';
  });
}

async function verifyPage(reader, id, first, last, orders) {
  const [proof] = await reader.begin(async tx => {
    await tx.unsafe("SET TRANSACTION READ ONLY; SET LOCAL statement_timeout='2s'; SET LOCAL jit=off");
    return tx.unsafe(`SELECT r.orders,r.next_byte,cardinality(r.task_ids) AS admitted,
      (SELECT count(*)::integer FROM unnest(r.task_ids) x(id) CROSS JOIN LATERAL (
        SELECT 1 FROM entity_compute_tasks t WHERE t.id=x.id OFFSET 0) task) AS supported
      FROM record_compute_order_import_batches r WHERE (r.import_id,r.first_byte)=($1,$2)`, [id, first]);
  });
  if (!proof) return null;
  if (Number(proof.next_byte) !== last || proof.orders !== orders || proof.supported !== proof.admitted)
    throw Error('committed order batch failed independent verification');
  return proof;
}

async function verifyImportPosition(reader, id, next) {
  if (next === 0) return;
  const [prior] = await reader.unsafe(`SELECT first_byte,orders FROM record_compute_order_import_batches
    WHERE import_id=$1 AND first_byte<$2 ORDER BY first_byte DESC LIMIT 1`, [id, next]);
  if (!prior || !await verifyPage(reader, id, Number(prior.first_byte), next, prior.orders))
    throw Error('cannot verify the saved import position');
}

export async function orderImportAtEnd(reader, { id, inputSha, inputBytes, engine, cut, target }) {
  const [run] = await readState(reader, id);
  if (!run) return false;
  if (run.input_sha !== inputSha || Number(run.input_bytes) !== inputBytes || run.engine !== engine ||
      run.cut !== cut || Number(run.target) !== target) throw Error('stock source disagrees with the existing import');
  if (Number(run.next_byte) !== inputBytes) return false;
  if (Number(run.processed) !== target) throw Error('stock source did not account for every order line');
  await verifyImportPosition(reader, id, inputBytes);
  return true;
}

async function admitPage(writer, reader, page, signal, retryRollbacks) {
  for (let attempt = 0; ; attempt++) {
    signal?.throwIfAborted();
    let result, error;
    try { result = await commitPage(writer, page); } catch (cause) { error = cause; }
    if (result && result !== 'committed') return { result };
    const proof = await verifyPage(reader, page.id, page.first, page.last, page.orderCount);
    if (proof) return { result: 'committed', proof };
    if (!ROLLED_BACK.has(error?.code))
      throw error || Error('order admission requires independent readback');
    if (!retryRollbacks || attempt >= 2) return { result: 'rolled_back' };
    await setTimeout(150 * (attempt + 1), undefined, { signal });
  }
}

export async function importComputeOrders({ writer, reader, path, id, target, engine, maxOpen,
  rows = 256, signal, waitMs = 0, maxBatches = Infinity, retryRollbacks = true,
  adapters = new Map(), refill, onProgress = () => {} }) {
  if (writer === reader || typeof id !== 'string' || !id.trim() || !/^[0-9a-f]{12,64}$/.test(engine || '') ||
      ![target, maxOpen, rows].every(integer) || rows > COMPUTE_ORDER_IMPORT_ROWS ||
      (maxBatches !== Infinity && !integer(maxBatches)))
    throw Error('invalid compute order import parameters');
  const routes = new Map([['demand', { prepare: prepareDemands, admit: admitDemands }], ...adapters]);
  const file = await open(path, 'r');
  try {
    const original = await file.stat({ bigint: true });
    if (!original.isFile() || original.size > BigInt(Number.MAX_SAFE_INTEGER)) throw Error('use a regular order file');
    const checkFile = async () => {
      const current = await file.stat({ bigint: true });
      if (current.ctimeNs !== original.ctimeNs || current.mtimeNs !== original.mtimeNs || current.size !== original.size)
        throw Error('order file changed during the import');
    };
    const hash = createHash('sha256');
    let lines = 0;
    for await (const bytes of file.createReadStream({ start: 0, autoClose: false })) {
      signal?.throwIfAborted();
      hash.update(bytes);
      if (refill?.source) for (let newline = bytes.indexOf(10); newline >= 0; newline = bytes.indexOf(10, newline + 1)) lines++;
    }
    await checkFile();
    const inputSha = hash.digest('hex'), inputBytes = Number(original.size);
    const [{ cut }] = await reader.unsafe("SELECT coalesce(compute_order_cut_epoch(),'') cut");
    if (refill && (refill.inputSha !== inputSha || refill.cut !== cut))
      throw Error('order refill input or cut differs from the approved source');
    if (refill?.source && (inputBytes !== refill.source.inputBytes || lines !== target))
      throw Error('stock source order count or size differs from its approved file');
    if (refill && !(await readState(reader, id)).length) {
      await refill.wait({ cut, engine, signal });
      await checkFile();
    }
    await writer.unsafe(`INSERT INTO state_compute_order_imports(id,input_sha,input_bytes,engine,cut,target)
      VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING`, [id, inputSha, inputBytes, engine, cut, target]);
    const [run] = await readState(reader, id);
    if (run.input_sha !== inputSha || Number(run.input_bytes) !== inputBytes || run.engine !== engine ||
        run.cut !== cut || Number(run.target) !== target) throw Error('order import has different immutable inputs');
    let next = Number(run.next_byte), admitted = Number(run.admitted), processed = Number(run.processed);
    const status = result => ({ result, admitted, processed, without_new_task: processed - admitted,
      remaining: target - admitted, next_byte: next });
    await verifyImportPosition(reader, id, next);
    if (run.paused) return status('paused');
    const stream = orderLines(file, next);
    try {
      let pending = await stream.next(), batches = 0;
      while (!pending.done && admitted < target) {
        signal?.throwIfAborted();
        const orders = [], kind = pending.value.order?.kind;
        const adapter = routes.get(kind);
        if (!adapter) throw Error('unknown order kind');
        let last = next;
        const width = Math.min(rows, target - admitted, maxOpen);
        while (!pending.done && pending.value.order?.kind === kind) {
          if (pending.value.end - next > PAGE_BYTES) {
            if (!orders.length) throw Error('one order exceeds the batch byte limit');
            break;
          }
          orders.push(pending.value.order);
          last = pending.value.end;
          if (orders.length === width) { pending = null; break; }
          pending = await stream.next();
        }
        await checkFile();
        const prepared = await adapter.prepare(orders, { id, first: next, cut, engine });
        const page = { id, first: next, last, orderCount: orders.length, cut, maxOpen, adapter, prepared, automatic: !!refill };
        let attempt;
        do {
          if (refill) {
            await refill.wait({ cut, engine, signal });
            await checkFile();
          }
          attempt = await admitPage(writer, reader, page, signal, retryRollbacks);
          if (!waitMs || (attempt.result !== 'queue_full' && !(refill && attempt.result === 'rolled_back'))) break;
          refill?.observe(attempt);
          onProgress(status(attempt.result));
          await setTimeout(refill && attempt.result === 'queue_full' ? refill.pollMs : waitMs, undefined, { signal });
          await checkFile();
        } while (true);
        if (attempt.result !== 'committed') return status(attempt.result);
        next = last;
        admitted += attempt.proof.admitted;
        processed += orders.length;
        onProgress(status('running'));
        if (admitted === target) return status('complete');
        if (next === inputBytes) return status('input_exhausted');
        if (++batches === maxBatches) return status('batch_limit');
        pending ??= await stream.next();
      }
      return status(admitted === target ? 'complete' : 'input_exhausted');
    } finally { await stream.return(); }
  } finally { await file.close(); }
}

export function orderImportConnection(proof, automatic) {
  const base = { application_name: proof ? 'wfsim-order-import-proof' : 'wfsim-order-import',
    statement_timeout: 15_000, lock_timeout: 500, jit: 'off' };
  if (!automatic) return base;
  return { ...base, statement_timeout: REFILL_TRANSACTION_MS, transaction_timeout: REFILL_TRANSACTION_MS,
    temp_file_limit: proof ? '0' : '16MB', default_transaction_read_only: proof ? 'on' : 'off' };
}

export async function verifyRefillConnection(sql, proof) {
  const [{ ok }] = await sql.unsafe(`SELECT NOT pg_is_in_recovery()
    AND current_setting('transaction_timeout')::interval=interval '8 seconds'
    AND pg_size_bytes(current_setting('temp_file_limit')) BETWEEN 0 AND $1
    AND current_setting('default_transaction_read_only')=$2 AS ok`, [proof ? 0 : 16 * 1024 * 1024, proof ? 'on' : 'off']);
  if (!ok) throw Error('order refill connection lacks the approved transaction and temporary file limits');
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  const { positionals: [operation, id, path, extra], values } = parseArgs({ allowPositionals: true,
    options: { target: { type: 'string' }, engine: { type: 'string' }, 'max-open': { type: 'string' },
      rows: { type: 'string' }, batches: { type: 'string' }, once: { type: 'boolean' },
      'refill-config': { type: 'string' }, 'stock-anchor': { type: 'string' },
      'max-pending-orders': { type: 'string' }, 'max-pending-bytes': { type: 'string' } } });
  const url = process.env.WFSIM_COMPUTE_ORDER_DATABASE_URL;
  const stock = operation === 'run-stock' || operation === 'run-accepted-stock';
  const accept = operation === 'accept-stock';
  const intakeLimits = values['max-pending-orders'] !== undefined || values['max-pending-bytes'] !== undefined;
  if (!url || !id || extra || !(stock || accept || ['run', 'status', 'pause', 'resume'].includes(operation))
      || (operation === 'run') !== !!path
      || (values['refill-config'] && (!(stock || operation === 'run') || values.once))
      || (stock && (!values['refill-config'] || !values['stock-anchor'] || values.engine || values.target))
      || (!(stock || accept) && values['stock-anchor'])
      || (!accept && intakeLimits)
      || (accept && (!values['stock-anchor'] || values.engine || values.target || values['max-open'] ||
        values.rows || values.batches || values.once)))
    throw Error('usage: compute_order_import.mjs run ID FILE --target=N --engine=ID --max-open=N, run-stock MANIFEST or run-accepted-stock DIRECTORY with --stock-anchor=ID --max-open=N --refill-config=FILE, accept-stock MANIFEST --stock-anchor=ID --max-pending-orders=N --max-pending-bytes=N, or status|pause|resume ID');
  const { default: postgres } = await import('./pg/node_modules/postgres/src/index.js');
  const connect = proof => postgres(url, { max: 1, fetch_types: false,
    connection: orderImportConnection(proof, !!values['refill-config']) });
  const writer = connect(false), reader = connect(true);
  const controller = new AbortController();
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => controller.abort());
  try {
    if (accept) {
      const { acceptOrderStock } = await import('./compute_order_stock.mjs');
      const result = await acceptOrderStock({ writer, reader, manifest: id, anchor: values['stock-anchor'],
        maxPendingOrders: Number(values['max-pending-orders']), maxPendingBytes: Number(values['max-pending-bytes']),
        atEnd: orderImportAtEnd, signal: controller.signal });
      console.log(JSON.stringify(result));
    } else if (operation === 'run' || stock) {
      const { boardCoverageOrderAdapter } = await import('./board_coverage_orders.mjs');
      const config = values['refill-config'];
      const maxOpen = Number(values['max-open']);
      const options = { writer, reader, maxOpen, rows: values.rows === undefined ? undefined : Number(values.rows),
        maxBatches: values.batches === undefined ? undefined : Number(values.batches), signal: controller.signal,
        waitMs: config ? 500 : values.once ? 0 : 8000, retryRollbacks: !values.once && !config,
        adapters: new Map([['board_coverage', boardCoverageOrderAdapter]]),
        onProgress: event => console.error(JSON.stringify(event)) };
      if (config) await Promise.all([verifyRefillConnection(writer, false), verifyRefillConnection(reader, true)]);
      const openRefill = async source => (await import('./compute_order_refill.mjs')).openOrderRefill(reader, config, maxOpen, source);
      const result = stock
        ? await (await import('./compute_order_stock.mjs')).runOrderStock({ ...options,
          ...(operation === 'run-accepted-stock' ? { directory: id } : { manifest: id }),
          anchor: values['stock-anchor'], openRefill, importOrders: importComputeOrders, atEnd: orderImportAtEnd })
        : await importComputeOrders({ ...options, path, id, engine: values.engine, target: Number(values.target),
          refill: config ? await openRefill() : null });
      console.log(JSON.stringify(result));
    } else {
      const [state] = operation === 'status' ? await readState(reader, id) : await writer.unsafe(
        'UPDATE state_compute_order_imports SET paused=$2 WHERE id=$1 RETURNING *', [id, operation === 'pause']);
      if (!state) throw Error('unknown compute order import');
      console.log(JSON.stringify(state));
    }
  } catch (error) {
    if (!values['refill-config'] || !controller.signal.aborted || error.name !== 'AbortError') throw error;
  } finally { await Promise.all([writer.end(), reader.end()]); }
}
