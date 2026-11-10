// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const args = Object.fromEntries(process.argv.slice(2).map(a => { const i = a.indexOf('='); return [a.slice(0, i), a.slice(i + 1)]; }));
const helper = path.resolve('scripts/client_release_files.mjs');
const temporary = mkdtempSync(path.join(os.tmpdir(), 'wfsim-release-'));
const directory = path.join(temporary, 'release assets');
const natives = ['wfsim-x86_64-linux', 'wfsim-x86_64-windows.exe', 'wfsim-aarch64-macos', 'wfsim-x86_64-macos'];
const manifest = { version: '0.1.0', files: Object.fromEntries(natives.map(name => [name, { name }])) };
const save = (name, value) => writeFileSync(path.join(directory, name), JSON.stringify(value));
const run = () => spawnSync(process.execPath, [helper, directory], { encoding: 'utf8' });
let checks = 0;
const ok = label => { checks++; console.log(`  ok ${label}`); };
const refuse = (label, pattern) => {
  const result = run();
  assert.notEqual(result.status, 0, label);
  assert.equal(result.stdout, '', 'a refused list must not emit partial upload paths');
  assert.match(result.stderr, pattern); ok(label);
};

try {
  mkdirSync(directory);
  for (const name of [...natives, 'release.json', 'release.json.sig', 'SHA256SUMS', 'source.zip', 'wfsim-old.tgz', 'wfsim-unlisted', 'scratch.txt']) {
    writeFileSync(path.join(directory, name), name);
  }
  save('release.json', manifest);
  for (const filename of ['wfsim-cli-0.1.0.tgz', 'wfsim-0.1.0.tgz']) {
    writeFileSync(path.join(directory, filename), 'npm fixture');
    save('npm-pack.json', [{ version: '0.1.0', filename }]);
    const result = run();
    assert.equal(result.status, 0, result.stderr);
    const files = result.stdout.trim().split(/\r?\n/);
    assert.deepEqual(files, [...natives, 'release.json', 'release.json.sig', 'SHA256SUMS', 'source.zip', filename].map(name => path.join(directory, name)));
    assert.equal(new Set(files.map(file => path.basename(file))).size, files.length);
    ok(`${filename} is included once beside exactly the manifest's native files`);
    const oldList = [...readdirSync(directory).filter(name => name.startsWith('wfsim-')),
      'release.json', 'SHA256SUMS', 'source.zip', ...readdirSync(directory).filter(name => name.endsWith('.tgz'))];
    assert.equal(oldList.filter(name => name === filename).length, 2);
    ok(`the old overlapping globs reproduce the duplicate ${filename}`);
  }
  const pack = [{ version: '0.1.0', filename: 'wfsim-0.1.0.tgz' }];
  save('release.json', { ...manifest, files: { ...manifest.files, duplicate: { name: natives[0] } } });
  refuse('duplicate native asset names fail before upload', /duplicate GitHub release asset/);
  save('release.json', { ...manifest, files: { ...manifest.files, collision: { name: pack[0].filename } } });
  refuse('a native/tarball name collision fails before upload', /duplicate GitHub release asset/);
  save('release.json', { ...manifest, files: { outside: { name: '../outside' } } });
  refuse('asset paths cannot escape the release directory', /unsafe release asset name/);
  save('release.json', manifest);
  save('npm-pack.json', []); refuse('missing npm pack output is refused', /one npm pack result/);
  save('npm-pack.json', [...pack, ...pack]); refuse('multiple npm pack results are refused', /one npm pack result/);
  save('npm-pack.json', [{ ...pack[0], version: '0.0.9' }]); refuse('a stale package version is refused', /must match the release/);
  save('npm-pack.json', pack); unlinkSync(path.join(directory, pack[0].filename));
  refuse('a missing packed tarball fails before upload', /ENOENT/);
  const workflow = readFileSync(args.workflow || '.github/workflows/client-release.yml', 'utf8');
  assert(/npm pack[^\n]*--json\s*>\s*target\/client-release\/npm-pack\.json/.test(workflow), 'record the exact npm pack result before selecting assets');
  assert(/node scripts\/client_release_files\.mjs target\/client-release > target\/client-release-assets\.txt/.test(workflow), 'select assets through the checked manifest helper');
  assert(/mapfile -t cli_assets < target\/client-release-assets\.txt/.test(workflow), 'read the validated asset list into an argument array');
  assert(/"\$\{cli_assets\[@\]\}"/.test(workflow), 'pass every asset as one quoted argument');
  assert(!workflow.includes('target/client-release/wfsim-*') && !workflow.includes('target/client-release/*.tgz'));
  assert(workflow.includes('node scripts/check_client_release.mjs'));
  assert(readFileSync('.github/workflows/client.yml', 'utf8').includes('node scripts/check_client_release.mjs'));
  ok('the release workflow uses the checked exact list instead of overlapping globs');
  const publish = workflow.slice(workflow.indexOf('\n  publish:'));
  const npm = publish.match(/npm install --global npm@(\d+)\.(\d+)\.(\d+)/);
  assert(npm && (+npm[1] > 11 || +npm[1] === 11 && +npm[2] >= 15), 'pin npm with trusted-publisher management support');
  const node = publish.match(/node-version:\s*['"]?(\d+)\.(\d+)\.(\d+)/);
  assert(node && (+node[1] > 22 || +node[1] === 22 && +node[2] >= 14), 'trusted publishing needs Node 22.14 or later');
  assert(publish.includes('id-token: write') && publish.includes('environment: release'));
  assert(publish.includes("!inputs.cors_only && github.ref == 'refs/heads/main'"));
  assert(!publish.includes('NPM_TOKEN') && !publish.includes('NODE_AUTH_TOKEN'), 'npm publication uses the workflow identity');
  ok('trusted publishing has a supported npm and preserves main/environment protection without a token secret');
  const pkg = JSON.parse(readFileSync('client/npm/package.json', 'utf8'));
  assert.equal(pkg.repository?.type, 'git');
  assert.equal(pkg.repository?.url, 'git+https://github.com/magenie33/wfsim.git');
  ok('the npm repository URL matches the authorized GitHub publisher');
  assert(publish.includes('node scripts/sign_client_release.mjs target/client-release'));
  assert(publish.includes('UPDATE_KEY: ${{ secrets.UPDATE_KEY }}'));
  assert(publish.indexOf('node scripts/sign_client_release.mjs') < publish.indexOf('gh release create'));
  ok('CLI signing reuses the existing protected key before publication');
  console.log(`CLI release assets: ${checks} checks passed; no upload or native rebuild.`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
