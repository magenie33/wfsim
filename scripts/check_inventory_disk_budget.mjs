// SPDX-License-Identifier: AGPL-3.0-or-later
// Disk exhaustion must preserve the last committed source page and checkpoint.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, openSync, closeSync, ftruncateSync, symlinkSync, statfsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { openCatalog } from './incremental_inventory.mjs';
import { inventoryDiskBudget } from './inventory_disk_budget.mjs';
import { offlineCapacityGuard } from './offline_capacity_guard.mjs';

const root = mkdtempSync(join(tmpdir(), 'inventory-disk-')), work = join(root, 'work'), source = join(root, 'source');
mkdirSync(work); writeFileSync(source, 'protected-input');
let catalog;
try {
  const baseline = inventoryDiskBudget({ work, sources: [source] });
  assert.equal(baseline.databaseBytes, 8 * 1024 ** 3, 'the live catalog default must retain its disk ceiling');
  assert.throws(() => inventoryDiskBudget({ work, sources: [source], totalBytes: 8 * 1024 ** 3 }), /cannot hold/);
  const totalBytes = 9 * 1024 ** 3, budget = inventoryDiskBudget({ work, sources: [source], totalBytes });
  assert.equal(budget.databaseBytes, totalBytes - 8 * 1024 ** 3 - 15);
  assert.equal(budget.report().transient_sort_hard_limit, false);
  const sparse = join(work, 'oversized'); const fd = openSync(sparse, 'w'); ftruncateSync(fd, totalBytes); closeSync(fd);
  assert.throws(() => budget.check(true), /exceeded the explicit disk budget/); rmSync(sparse);
  const path = join(work, 'catalog.sqlite'), options = { engine: '0123456789ab', contract: { v: 1 }, generation: 'fixture', create: true, databaseBytes: 512 * 1024 };
  catalog = openCatalog(path, options); catalog.putRecords([{ id: 'one', record: { weapon: 'braton' } }]);
  catalog.setMeta({ ...catalog.meta, input: { file: 0, offset: 1 } }); const before = JSON.stringify(catalog.meta);
  assert.throws(() => catalog.transaction(() => {
    catalog.putRecords([{ id: 'one', record: { weapon: 'changed' } }, { id: 'large', record: { provenance: 'x'.repeat(1024 ** 2) } }]);
    catalog.setMeta({ ...catalog.meta, input: { file: 0, offset: 2 } });
  }), /database or disk is full/);
  catalog.close(); catalog = openCatalog(path, options);
  assert.equal(JSON.stringify(catalog.meta), before);
  assert.deepEqual(JSON.parse(catalog.record('one').record), { weapon: 'braton' });
  assert.throws(() => catalog.record('large'), /unavailable/);
  assert.equal(catalog.db.prepare('PRAGMA quick_check').get().quick_check, 'ok');
  const resumedBudget = inventoryDiskBudget({ work, databasePath: path, sources: [source], totalBytes });
  assert(resumedBudget.report().existing_source_and_work_bytes > 15);
  assert.equal(resumedBudget.report().additional_budget_bytes, totalBytes - resumedBudget.report().existing_source_and_work_bytes,
    'restart reserves only capacity not already occupied by its durable database');
  const inside = join(work, 'captured'); writeFileSync(inside, 'inside-source');
  const insideBudget = inventoryDiskBudget({ work, databasePath: path, sources: [source, inside], totalBytes });
  assert.equal(insideBudget.report().existing_source_and_work_bytes - resumedBudget.report().existing_source_and_work_bytes, 13,
    'sources inside the live work directory must not be counted twice');
  const allocation = join(root, 'allocation.json'), reserved = join(root, 'allocated'); mkdirSync(reserved);
  const limit = { directory: reserved, max_new_bytes: 1024, minimum_host_free_bytes: 8 * 1024 ** 3 };
  writeFileSync(allocation, JSON.stringify(limit));
  assert.throws(() => offlineCapacityGuard(allocation, work), /outside its explicit capacity/);
  const guard = offlineCapacityGuard(allocation, reserved); writeFileSync(join(reserved, 'over'), 'x'.repeat(1025));
  assert.throws(() => guard.check(true), /allocation exceeded/); rmSync(join(reserved, 'over'));
  symlinkSync(source, join(reserved, 'link')); assert.throws(() => guard.check(true), /cannot contain links/); rmSync(join(reserved, 'link'));
  const fs = statfsSync(root); writeFileSync(allocation, JSON.stringify({ ...limit, minimum_host_free_bytes: fs.bavail * fs.bsize + 1024 ** 3 }));
  assert.throws(() => offlineCapacityGuard(allocation, reserved), /host free-space reserve/);
  assert.throws(() => inventoryDiskBudget({ work, reserveBytes: 1 }), /filesystem reserve/);
  console.log('ok: unchanged live disk default; explicit total budget includes protected source and scratch; SQLITE_FULL rolls back the whole page and restart preserves its prior checkpoint/record');
  console.log('ok: explicit output allocation rejects overflow, linked output, wrong directory and depleted host reserve');
} finally { catalog?.close(); rmSync(root, { recursive: true, force: true }); }
