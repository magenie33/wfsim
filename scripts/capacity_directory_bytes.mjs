// SPDX-License-Identifier: AGPL-3.0-or-later
// A disappearing child invalidates the whole count, including files already measured.
import { lstatSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

export function assertCapacityDirectory(path, expected) {
  const stat = lstatSync(path);
  if (stat.isSymbolicLink() || !stat.isDirectory()) throw Error('offline capacity directory cannot contain links or change type');
  if (stat.dev !== expected.dev || stat.ino !== expected.ino) throw Error('offline capacity directory identity or filesystem changed');
}

export function capacityDirectoryBytes(root, identity) {
  function walk(path) {
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) throw Error('offline capacity output cannot contain links');
    if (stat.dev !== identity.dev) throw Error('offline capacity filesystem changed');
    if (stat.isDirectory()) return readdirSync(path).reduce((bytes, name) => bytes + walk(join(path, name)), 0);
    if (!stat.isFile()) throw Error('offline capacity output has an unsupported file type');
    return stat.size;
  }
  for (let attempt = 0; attempt < 3; attempt++) {
    assertCapacityDirectory(root, identity);
    try {
      const bytes = walk(root);
      assertCapacityDirectory(root, identity);
      return bytes;
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      assertCapacityDirectory(root, identity);
      if (attempt === 2) throw error;
    }
  }
}
