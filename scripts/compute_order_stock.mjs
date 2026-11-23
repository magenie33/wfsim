// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
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

export async function runOrderStock({ manifest, openRefill, importOrders, atEnd, anchor, reader, signal, onProgress = () => {},
  delay = () => setTimeout(10_000, undefined, { signal }), maxBatches = Infinity, ...options }) {
  if (maxBatches !== Infinity && !positive(maxBatches)) throw Error('invalid stock batch limit');
  let index = 0, previous = [], lastHold;
  for (;;) {
    signal?.throwIfAborted();
    const plan = JSON.parse(await readFile(manifest, 'utf8'));
    const sources = orderStockSources(plan, dirname(manifest));
    if (sources[0].id !== anchor) throw Error('order stock manifest no longer starts at the approved import');
    const sealed = sources.map(source => JSON.stringify(source));
    if (previous.some((source, at) => source !== sealed[at]))
      throw Error('order stock manifest replaces a sealed source');
    previous = sealed;
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
