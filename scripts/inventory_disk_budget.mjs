// SPDX-License-Identifier: AGPL-3.0-or-later
// An explicit offline budget includes source files, database and output scratch.
import { readdirSync, statSync, statfsSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

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
export function inventoryDiskBudget({ work, sources = [], totalBytes, databasePath, reserveBytes = RESERVE }) {
  if(!Number.isSafeInteger(reserveBytes)||reserveBytes<RESERVE)throw Error('invalid inventory filesystem reserve');
  if (totalBytes === undefined) return { databaseBytes: 8 * 1024 ** 3, check() {}, report: () => ({ explicit: false }) };
  work = resolve(work);
  const sourceBytes = [...new Set(sources)].reduce((n, path) => n + statSync(path).size, 0);
  const outsideBytes = [...new Set(sources.map(path => resolve(path)))].filter(path => !path.startsWith(work + '/'))
    .reduce((n, path) => n + statSync(path).size, 0);
  const initialWork = bytesUnder(work), existingDatabase = databasePath && existsSync(databasePath) ? statSync(databasePath).size : 0;
  const alreadyUsed = outsideBytes + initialWork, otherBytes = alreadyUsed - existingDatabase;
  if (!Number.isSafeInteger(totalBytes) || totalBytes <= otherBytes + reserveBytes + 1024 ** 2)
    throw Error('offline disk budget cannot hold its protected inputs and scratch reserve');
  const databaseBytes = totalBytes - otherBytes - reserveBytes;
  let last = 0, peak = alreadyUsed, used = alreadyUsed;
  const free = () => { const fs = statfsSync(work); return fs.bavail * fs.bsize; };
  const initialFree = free();
  const additional = Math.max(0, totalBytes - alreadyUsed);
  if (initialFree < additional + reserveBytes) throw Error('offline disk budget would consume the filesystem reserve');
  const budget = { databaseBytes,
    check(force = false) {
      if (!force && Date.now() - last < 1000) return;
      last = Date.now(); used = outsideBytes + bytesUnder(work); peak = Math.max(peak, used);
      if (used > totalBytes) throw Error('offline source and scratch exceeded the explicit disk budget');
      if (free() < reserveBytes) throw Error('offline replay reached the filesystem free-space reserve');
    },
    report() { return { explicit: true, total_bytes: totalBytes, protected_source_bytes: sourceBytes,
      database_limit_bytes: databaseBytes, scratch_reserve_bytes: reserveBytes, filesystem_reserve_bytes: reserveBytes,
      existing_source_and_work_bytes: alreadyUsed, additional_budget_bytes: additional,
      measured_source_and_work_bytes: used, peak_measured_source_and_work_bytes: peak,
      transient_sort_hard_limit: false }; },
  };
  budget.check(true); return budget;
}
