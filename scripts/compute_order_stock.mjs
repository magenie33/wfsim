// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, open, readFile } from 'node:fs/promises';
import { basename, dirname, join } from 'node:path';
import { setTimeout } from 'node:timers/promises';

const positive = value => Number.isSafeInteger(value) && value > 0;

export function orderStockSources(plan, directory) {
  if (plan?.v !== 1 || !/^[0-9a-f]{12,64}$/.test(plan.engine) || typeof plan.order_epoch !== 'string'
      || !Array.isArray(plan.files) || !plan.files.length) throw Error('invalid order stock manifest');
  const ids = new Set(), paths = new Set();
  let previous;
  return plan.files.map(file => {
    if (typeof file?.id !== 'string' || !file.id.trim() || ids.has(file.id)
        || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(file.file || '') || paths.has(file.file)
        || !positive(file.first) || !positive(file.last) || file.last < file.first
        || (previous !== undefined && file.first !== previous + 1)
        || !positive(file.input_bytes) || !/^[0-9a-f]{64}$/.test(file.input_sha256 || ''))
      throw Error('order stock must list unique, sealed, consecutive sources');
    ids.add(file.id);
    paths.add(file.file);
    previous = file.last;
    return {
      id: file.id, path: join(directory, file.file), first: file.first, last: file.last,
      inputSha: file.input_sha256, inputBytes: file.input_bytes, target: file.last - file.first + 1,
      engine: plan.engine, cut: plan.order_epoch,
    };
  });
}

async function stockRows(sql, anchor) {
  return sql.unsafe(`SELECT s.file,s.first_ordinal AS first,i.*
    FROM record_compute_order_stock_sources s JOIN state_compute_order_imports i ON i.id=s.import_id
    WHERE s.stock=$1 ORDER BY s.first_ordinal`, [anchor]);
}

function stockDescriptor(row) {
  return { id: row.id, file: row.file, first: Number(row.first),
    last: Number(row.first) + Number(row.target) - 1, input_bytes: Number(row.input_bytes), input_sha256: row.input_sha };
}

export async function readAcceptedStock(reader, anchor, directory) {
  const rows = await stockRows(reader, anchor);
  if (!rows.length) return [];
  const { engine, cut } = rows[0];
  if (rows.some(row => row.engine !== engine || row.cut !== cut))
    throw Error('accepted stock mixes engines or order cuts');
  return orderStockSources({ v: 1, engine, order_epoch: cut, files: rows.map(stockDescriptor) }, directory);
}

function checkPrefix(accepted, proposed) {
  if (accepted.some((source, at) => JSON.stringify(source) !== JSON.stringify(proposed[at])))
    throw Error('order stock manifest replaces a sealed source');
}

function checkImport(source, row) {
  if (!row || row.input_sha !== source.inputSha || Number(row.input_bytes) !== source.inputBytes ||
      row.engine !== source.engine || row.cut !== source.cut || Number(row.target) !== source.target)
    throw Error('stock source disagrees with the existing import');
}

async function confirmSourceFile(source, signal) {
  const file = await open(source.path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const before = await file.stat({ bigint: true });
    if (!before.isFile() || before.size !== BigInt(source.inputBytes))
      throw Error('stock source file size differs from the sealed manifest');
    const hash = createHash('sha256');
    let orders = 0, tail;
    for await (const bytes of file.createReadStream({ autoClose: false })) {
      signal?.throwIfAborted();
      hash.update(bytes);
      tail = bytes.at(-1);
      for (let offset = bytes.indexOf(10); offset >= 0; offset = bytes.indexOf(10, offset + 1)) orders++;
    }
    if (tail !== 10 || orders !== source.target || hash.digest('hex') !== source.inputSha)
      throw Error('stock source content differs from the sealed manifest');
    await file.sync();
    const after = await lstat(source.path, { bigint: true });
    if (after.dev !== before.dev || after.ino !== before.ino || after.size !== before.size ||
        after.mtimeNs !== before.mtimeNs || after.ctimeNs !== before.ctimeNs)
      throw Error('stock source changed during acceptance');
  } finally { await file.close(); }
}

export async function acceptOrderStock({ writer, reader, manifest, anchor, maxPendingOrders, maxPendingBytes, atEnd, signal }) {
  if (writer === reader || !positive(maxPendingOrders) || !positive(maxPendingBytes))
    throw Error('stock acceptance requires independent connections and explicit order and byte limits');
  const directory = dirname(manifest);
  const proposed = orderStockSources(JSON.parse(await readFile(manifest, 'utf8')), directory);
  if (proposed[0].id !== anchor) throw Error('order stock manifest no longer starts at the approved import');
  const accepted = await readAcceptedStock(reader, anchor, directory);
  checkPrefix(accepted, proposed);
  for (const source of proposed.slice(accepted.length)) {
    signal?.throwIfAborted();
    if (!await atEnd(reader, source)) await confirmSourceFile(source, signal);
  }
  const dir = await open(directory, constants.O_RDONLY | constants.O_DIRECTORY);
  try { await dir.sync(); } finally { await dir.close(); }
  signal?.throwIfAborted();

  let error;
  try {
    await writer.begin(async tx => {
      await tx.unsafe("SELECT pg_advisory_xact_lock(hashtextextended('compute-order-stock:'||$1,0))", [anchor]);
      const [{ cut }] = await tx.unsafe("SELECT compute_order_cut_lock(),coalesce(compute_order_cut_epoch(),'') cut");
      if (cut !== proposed[0].cut) throw Error('order stock cut changed before acceptance');
      const prefix = await readAcceptedStock(tx, anchor, directory);
      checkPrefix(prefix, proposed);
      const additions = proposed.slice(prefix.length);
      if (!additions.length) return;
      const batch = JSON.stringify(additions.map(({ id, first, path, inputSha, inputBytes, engine, cut, target }) =>
        ({ id, first, file: basename(path), input_sha: inputSha, input_bytes: inputBytes, engine, cut, target })));
      await tx.unsafe(`INSERT INTO state_compute_order_imports(id,input_sha,input_bytes,engine,cut,target)
        SELECT id,input_sha,input_bytes,engine,cut,target FROM jsonb_to_recordset($1::text::jsonb)
        x(id text,input_sha text,input_bytes bigint,engine text,cut text,target bigint) ORDER BY id ON CONFLICT DO NOTHING`, [batch]);
      const runs = await tx.unsafe(`SELECT i.* FROM jsonb_array_elements_text($1::text::jsonb) x(id)
        JOIN state_compute_order_imports i ON i.id=x.id`, [JSON.stringify(additions.map(source => source.id))]);
      const byId = new Map(runs.map(row => [row.id, row]));
      for (const source of additions) checkImport(source, byId.get(source.id));
      const all = [...await stockRows(tx, anchor), ...runs];
      const pendingOrders = all.reduce((n, row) => n + BigInt(row.target) - BigInt(row.processed), 0n);
      const pendingBytes = all.reduce((n, row) => n + BigInt(row.input_bytes) - BigInt(row.next_byte), 0n);
      if (pendingOrders > BigInt(maxPendingOrders) || pendingBytes > BigInt(maxPendingBytes))
        throw Error('order stock exceeds its pending intake budget');
      await tx.unsafe(`INSERT INTO record_compute_order_stock_sources(stock,first_ordinal,file,import_id)
        SELECT $1,x.first,x.file,x.id FROM jsonb_to_recordset($2::text::jsonb) x(id text,first bigint,file text)`,
      [anchor, batch]);
    });
  } catch (cause) { error = cause; }

  const checked = await readAcceptedStock(reader, anchor, directory);
  if (checked.length < proposed.length) throw error || Error('order stock acceptance requires independent readback');
  checkPrefix(proposed, checked);
  return { result: 'accepted', stock: anchor, sources: proposed.length,
    orders: proposed.at(-1).last - proposed[0].first + 1, through: proposed.at(-1).last };
}

export async function runOrderStock({ manifest, directory, openRefill, importOrders, atEnd, anchor, reader, signal, onProgress = () => {},
  delay = () => setTimeout(10_000, undefined, { signal }), maxBatches = Infinity, ...options }) {
  if (maxBatches !== Infinity && !positive(maxBatches)) throw Error('invalid stock batch limit');
  let index = 0, previous = [], lastHold;
  for (;;) {
    signal?.throwIfAborted();
    const sources = directory === undefined
      ? orderStockSources(JSON.parse(await readFile(manifest, 'utf8')), dirname(manifest))
      : await readAcceptedStock(reader, anchor, directory);
    if (sources.length && sources[0].id !== anchor) throw Error('order stock manifest no longer starts at the approved import');
    checkPrefix(previous, sources);
    previous = sources;
    const source = sources[index];
    if (!source) {
      const hold = { result: 'stock_wait', sources: sources.length };
      if (lastHold !== hold.result) onProgress(hold);
      lastHold = hold.result;
      await delay();
      continue;
    }
    if (await atEnd(reader, source)) {
      index++;
      continue;
    }
    const refill = await openRefill(source);
    const progress = event => {
      if (event.result === 'running') maxBatches--;
      onProgress({ ...event, id: source.id });
    };
    const result = await importOrders({ ...options, ...source, reader, signal, refill, maxBatches, onProgress: progress });
    if (result.result === 'complete' || result.result === 'input_exhausted') {
      if (!await atEnd(reader, source)) throw Error('order stock cannot advance without verified EOF');
      progress(result);
      if (!maxBatches) return { ...result, result: 'batch_limit', id: source.id };
      index++;
      lastHold = undefined;
      continue;
    }
    if (!['paused', 'queue_unavailable'].includes(result.result)) return { ...result, id: source.id };
    if (lastHold !== result.result) progress(result);
    lastHold = result.result;
    await delay();
  }
}
