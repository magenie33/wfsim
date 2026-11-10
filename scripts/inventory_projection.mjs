// SPDX-License-Identifier: AGPL-3.0-or-later
// Bounded immutable input pages and the shared disk-classifier audit.
import { createWriteStream, statSync, openSync, readSync, closeSync } from 'node:fs';
import { once } from 'node:events';
import { join } from 'node:path';
import { auditInventoryInputs } from './inventory_inputs.mjs';

export async function readInventoryPage(path, offset, limit, { maxBytes = 4 * 1024 ** 2, maxRowBytes = 8 * 1024 ** 2 } = {}) {
  if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(limit) || limit < 1 || limit > 256
      || !Number.isSafeInteger(maxBytes) || maxBytes < 1 || !Number.isSafeInteger(maxRowBytes) || maxRowBytes < maxBytes)
    throw Error('invalid inventory page bound');
  const size = statSync(path).size;
  if (offset > size) throw Error('inventory checkpoint exceeds immutable input');
  const fd = openSync(path, 'r'), buffer = Buffer.allocUnsafe(65536);
  const rows = []; let next = offset, position = offset, available = 0, cursor = 0, readAt = offset, total = 0;
  const line = () => {
    const parts = []; let bytes = 0;
    for (;;) {
      if (cursor === available) {
        available = readSync(fd, buffer, 0, buffer.length, readAt); cursor = 0; readAt += available;
        if (!available) return bytes ? Buffer.concat(parts, bytes) : null;
      }
      let end = buffer.indexOf(10, cursor); if (end < 0 || end >= available) end = available;
      const part = buffer.subarray(cursor, end); bytes += part.length;
      if (bytes > maxRowBytes) throw Error('inventory input row exceeds byte bound');
      parts.push(Buffer.from(part)); position += end - cursor;
      cursor = end;
      if (end < available) { cursor++; position++; return Buffer.concat(parts, bytes); }
    }
  };
  try {
    for (;;) {
      const value = line(); if (value === null) break;
      if (rows.length && total + value.length > maxBytes) break;
      next = position;
      if (value.toString().trim()) { rows.push(JSON.parse(value.toString())); total += value.length; }
      if (rows.length === limit) break;
    }
  } finally { closeSync(fd); }
  return { rows, offset: next, done: next === size };
}

async function applyRows(index, operate, kind, rows, bootstrap) {
  if (kind === 'builds') await index.sources(rows, operate);
  else if (kind === 'facts') await index.facts(rows, operate);
  else if (kind === 'queue') index.queue(rows);
  else if (kind === 'tasks') index.catalog.applyLifecycle(rows, { bootstrap });
  else if (kind === 'reset') {
    const replace = rows.map(row => row.id);
    await index.facts([], operate, { replace }); index.queue([], { replace });
  } else throw Error('unknown relational input');
}

export async function applyInventoryPage(index, operate, page, kind, bootstrap) {
  if (kind !== 'delta') return applyRows(index, operate, kind, page.rows, bootstrap);
  let current = null, rows = [];
  for (const item of page.rows) {
    if (current !== item.kind && rows.length) { await applyRows(index, operate, current, rows, false); rows = []; }
    current = item.kind; rows.push(item.row);
  }
  if (rows.length) await applyRows(index, operate, current, rows, false);
}

export async function auditInventoryProjection(index, cut, directory) {
  const path = join(directory, 'relational-members.ndjson'), out = createWriteStream(path);
  let error = null, owners = 0, rows = 0;
  out.on('error', e => { error = e; });
  const write = async row => {
    if (error) throw error;
    if (!out.write(JSON.stringify(row) + '\n')) await once(out, 'drain');
  };
  try {
    for (const source of index.q('SELECT id,body FROM rel_candidates ORDER BY id').iterate()) {
      const candidate = JSON.parse(source.body);
      const binding = index.q('SELECT id FROM rel_sources WHERE candidate=? ORDER BY seq LIMIT 1').get(source.id);
      await write({ type: 'owner', owner: source.id, build_id: source.id, record: candidate.record, binding_owner: binding.id }); owners++;
      for (const member of index.q(`SELECT r.build_id,ru.name ruler,mo.name mode,m.queued FROM members m
        JOIN owners o ON o.id=m.owner JOIN records r ON r.id=m.candidate JOIN symbols ru ON ru.id=m.ruler
        JOIN symbols mo ON mo.id=m.mode WHERE o.owner=? ORDER BY ru.name,mo.name`).iterate(source.id)) {
        await write({ type: 'row', owner: source.id, ...member }); rows++;
      }
    }
    await write({ type: 'end', v: 1, ...cut.receipt, owners, rows, complete: true });
    out.end(); await once(out, 'finish'); if (error) throw error;
  } finally { out.destroy(); }
  const result = await auditInventoryInputs({ ...cut.raw, meta: cut.meta, rulers: index.contract.rulers.map(r => r.id),
    engineOutputs: [path], contract: cut.receipt.contract, generation: cut.receipt.generation,
    epoch: cut.receipt.epoch, out: join(directory, 'audit') });
  const sources = index.q('SELECT count(*) n FROM rel_sources').get().n;
  if (sources !== cut.meta.counts.library) throw Error('incomplete relational source baseline');
  index.catalog.seal({ snapshot: cut.receipt.snapshot, started_at: cut.receipt.started_at, expected: {
    owners, records: cut.meta.counts.library, tasks: cut.meta.counts.tasks,
    rows: result.plan.rows, queued: result.plan.queued, digest: result.plan.question_digest } });
}

