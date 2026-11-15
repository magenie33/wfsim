// SPDX-License-Identifier: AGPL-3.0-or-later
// Bounded immutable input pages and the shared disk-classifier audit.
import { statSync, openSync, readSync, closeSync } from 'node:fs';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { readInventoryRows } from './incremental_inventory.mjs';

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

export async function auditInventoryProjection(index, cut, directory, { check = () => {} } = {}) {
  const canonical = function* () {
    for (const row of index.q('SELECT id,body FROM entity_catalog_candidates ORDER BY id').iterate()) {
      const d = JSON.parse(row.body);
      if (!index.q('SELECT 1 FROM entity_catalog_sources WHERE candidate=? LIMIT 1').get(row.id)) throw Error('unbound audit canonical identity');
      yield { id: row.id, record: d.record || JSON.parse(index.catalog.recordBody(d.record_ref)) };
    }
  };
  const members = function* () {
    for (const row of index.q(`SELECT r.build_id,ru.name ruler,mo.name mode FROM relation_inventory_owner_members m
      JOIN entity_inventory_builds r ON r.id=m.candidate JOIN entity_inventory_symbols ru ON ru.id=m.ruler
      JOIN entity_inventory_symbols mo ON mo.id=m.mode WHERE m.queued=0`).iterate()) yield row;
  };
  const queued = index.q(`SELECT 1 FROM relation_inventory_owners o JOIN relation_inventory_owner_members m ON m.owner=o.id
    WHERE o.owner=? AND m.ruler=(SELECT id FROM entity_inventory_symbols WHERE category='ruler' AND name=?)
    AND m.mode=(SELECT id FROM entity_inventory_symbols WHERE category='mode' AND name=?) AND m.queued=1 LIMIT 1`);
  const orphans = async function* () {
    for await (const row of readInventoryRows(cut.raw.queue)) if (!queued.get(row.build_id,row.ruler,row.mode)) yield row;
  };
  const plan = await classifyInventory({ ...cut.raw, meta: cut.meta, rulers: index.contract.rulers.map(r => r.id),
    canonical: canonical(), missing: members(), orphans: orphans(), questions: true, check, scratchRoot: directory });
  const sources = index.q('SELECT count(*) n FROM entity_catalog_sources').get().n;
  if (sources !== cut.meta.counts.library) throw Error('incomplete relational source baseline');
  const owners = index.q('SELECT count(*) n FROM entity_catalog_candidates').get().n;
  index.catalog.seal({ snapshot: cut.receipt.snapshot, started_at: cut.receipt.started_at, expected: {
    owners, records: cut.meta.counts.library, tasks: cut.meta.counts.tasks,
    rows: plan.rows, queued: plan.queued, digest: plan.question_digest } });
  return { plan };
}
