// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function clientReleaseFiles(directory) {
  const root = path.resolve(directory);
  assert(!/[\r\n]/.test(root), 'release directory must fit on one line');
  const manifest = JSON.parse(readFileSync(path.join(root, 'release.json'), 'utf8'));
  const pack = JSON.parse(readFileSync(path.join(root, 'npm-pack.json'), 'utf8'));
  assert(manifest.files && typeof manifest.files === 'object' && !Array.isArray(manifest.files), 'missing native files');
  const native = Object.values(manifest.files);
  assert(native.length, 'missing native files');
  assert(Array.isArray(pack) && pack.length === 1, 'one npm pack result is required');
  assert.equal(pack[0].version, manifest.version, 'npm pack version must match the release');
  assert(typeof pack[0].filename === 'string' && pack[0].filename.endsWith('.tgz'), 'missing npm tarball');
  const names = [...native.map(file => file.name), 'release.json', 'release.json.sig', 'SHA256SUMS', 'source.zip', pack[0].filename];
  const seen = new Set();
  for (const name of names) {
    assert(typeof name === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name), 'unsafe release asset name');
    assert(!seen.has(name), `duplicate GitHub release asset: ${name}`);
    seen.add(name);
  }
  return names.map(name => {
    const filename = path.join(root, name);
    assert(statSync(filename).isFile(), `release asset is not a file: ${name}`);
    return filename;
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    console.log(clientReleaseFiles(process.argv[2] || 'target/client-release').join('\n'));
  } catch (error) {
    console.error(`CLI release: ${error.message}`);
    process.exitCode = 1;
  }
}
