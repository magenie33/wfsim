// SPDX-License-Identifier: AGPL-3.0-or-later
// An explicit offline budget includes source files, database and output scratch.
import { readdirSync, statSync, statfsSync } from 'node:fs';
import { join } from 'node:path';

const RESERVE = 8 * 1024 ** 3;
function bytesUnder(path) {
  let bytes = 0;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) throw Error('offline scratch cannot contain links');
    bytes += entry.isDirectory() ? bytesUnder(child) : statSync(child).size;
  }
  return bytes;
}
export function inventoryDiskBudget({ work, sources, totalBytes }) {
  if (totalBytes === undefined) return { databaseBytes: 8 * 1024 ** 3, check() {}, report: () => ({ explicit: false }) };
  const sourceBytes = [...new Set(sources)].reduce((n, path) => n + statSync(path).size, 0);
  if (!Number.isSafeInteger(totalBytes) || totalBytes <= sourceBytes + RESERVE + 1024 ** 2)
    throw Error('offline disk budget cannot hold its protected inputs and scratch reserve');
  const databaseBytes = totalBytes - sourceBytes - RESERVE;
  let last = 0, peak = sourceBytes, used = sourceBytes;
  const free = () => { const fs = statfsSync(work); return fs.bavail * fs.bsize; };
  const initialFree = free();
  if (initialFree < totalBytes - sourceBytes + RESERVE) throw Error('offline disk budget would consume the filesystem reserve');
  const budget = { databaseBytes,
    check(force = false) {
      if (!force && Date.now() - last < 1000) return;
      last = Date.now(); used = sourceBytes + bytesUnder(work); peak = Math.max(peak, used);
      if (used > totalBytes) throw Error('offline source and scratch exceeded the explicit disk budget');
      if (free() < RESERVE) throw Error('offline replay reached the filesystem free-space reserve');
    },
    report() { return { explicit: true, total_bytes: totalBytes, protected_source_bytes: sourceBytes,
      database_limit_bytes: databaseBytes, scratch_reserve_bytes: RESERVE, filesystem_reserve_bytes: RESERVE,
      measured_source_and_work_bytes: used, peak_measured_source_and_work_bytes: peak,
      transient_sort_hard_limit: false }; },
  };
  budget.check(true); return budget;
}
