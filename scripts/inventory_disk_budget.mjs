// SPDX-License-Identifier: AGPL-3.0-or-later
// An explicit offline budget includes source files, database and output scratch.
import { statSync, lstatSync, realpathSync, statfsSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { assertCapacityDirectory, capacityDirectoryBytes } from './capacity_directory_bytes.mjs';

const RESERVE = 8 * 1024 ** 3;
export function inventoryDiskBudget({ work, sources = [], totalBytes, databasePath, reserveBytes = RESERVE, scopeRoot }) {
  if(!Number.isSafeInteger(reserveBytes)||reserveBytes<RESERVE)throw Error('invalid inventory filesystem reserve');
  if (totalBytes === undefined) return { databaseBytes: 8 * 1024 ** 3, check() {}, report: () => ({ explicit: false }) };
  work = resolve(work);
  const scope = resolve(scopeRoot || work), root = lstatSync(scope), directory = lstatSync(work);
  const realScope = realpathSync(scope), realWork = realpathSync(work);
  if (!root.isDirectory() || root.isSymbolicLink() || !directory.isDirectory() || directory.isSymbolicLink()
      || root.dev !== directory.dev || work !== scope && !work.startsWith(scope + '/')
      || realWork !== realScope && !realWork.startsWith(realScope + '/'))
    throw Error('offline budget scope must contain the unlinked work directory on its filesystem');
  const sourcePaths = [...new Set(sources.map(path => {
    const named = resolve(path);if (lstatSync(named).isSymbolicLink()) throw Error('offline protected source must be an unlinked file');return realpathSync(named);
  }))];
  const sourceBytes = sourcePaths.reduce((n, path) => {
    const item = lstatSync(path);if (!item.isFile() || item.isSymbolicLink()) throw Error('offline protected source must be an unlinked file');return n + item.size;
  }, 0);
  const outsideBytes = sourcePaths.filter(path => path !== realScope && !path.startsWith(realScope + '/'))
    .reduce((n, path) => n + statSync(path).size, 0);
  const initialWork = capacityDirectoryBytes(scope, root), existingDatabase = databasePath && existsSync(databasePath) ? statSync(databasePath).size : 0;
  const alreadyUsed = outsideBytes + initialWork, otherBytes = alreadyUsed - existingDatabase;
  if (!Number.isSafeInteger(totalBytes) || totalBytes <= otherBytes + reserveBytes + 1024 ** 2)
    throw Error('offline disk budget cannot hold its protected inputs and scratch reserve');
  const databaseBytes = totalBytes - otherBytes - reserveBytes;
  let last = 0, peak = alreadyUsed, used = alreadyUsed;
  const free = () => { assertCapacityDirectory(work, directory); const fs = statfsSync(work); return fs.bavail * fs.bsize; };
  const initialFree = free();
  const additional = Math.max(0, totalBytes - alreadyUsed);
  if (initialFree < additional + reserveBytes) throw Error('offline disk budget would consume the filesystem reserve');
  const budget = { databaseBytes,
    check(force = false) {
      assertCapacityDirectory(scope, root);
      assertCapacityDirectory(work, directory);
      if (!force && Date.now() - last < 1000) return;
      used = outsideBytes + capacityDirectoryBytes(scope, root); peak = Math.max(peak, used);
      if (used > totalBytes) throw Error('offline source and scratch exceeded the explicit disk budget');
      if (free() < reserveBytes) throw Error('offline replay reached the filesystem free-space reserve');
      last = Date.now();
    },
    report() { return { explicit: true, budget_scope: scope, work_directory: work, total_bytes: totalBytes, protected_source_bytes: sourceBytes,
      database_limit_bytes: databaseBytes, scratch_reserve_bytes: reserveBytes, filesystem_reserve_bytes: reserveBytes,
      existing_source_and_work_bytes: alreadyUsed, additional_budget_bytes: additional,
      measured_source_and_work_bytes: used, peak_measured_source_and_work_bytes: peak,
      transient_sort_hard_limit: false }; },
  };
  budget.check(true); return budget;
}
