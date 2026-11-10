// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash, createPrivateKey, createPublicKey } from 'node:crypto';
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { signRelease, verifyRelease } from './sign_client_release.mjs';

const root = process.cwd();
const binary = path.resolve('client/target/debug', process.platform === 'win32' ? 'wfsim.exe' : 'wfsim');
const temporary = mkdtempSync(path.join(os.tmpdir(), 'wfsim-host-update-'));
const release = path.join(temporary, 'release');
let checks = 0;
const ok = name => { checks++; console.log(`  ok ${name}`); };
const until = async fn => { const end = Date.now() + 25_000; while (!fn()) { assert(Date.now() < end, 'background host condition timed out'); await new Promise(r => setTimeout(r, 50)); } };
let server, device;
const command = (...args) => spawnSync(binary, ['--data-dir', device, ...args], { encoding: 'utf8', timeout: 15_000 });
try {
  mkdirSync(release);
  const seed = '07'.repeat(32);
  const privateKey = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.from(seed, 'hex')]), type: 'pkcs8', format: 'der' });
  const publicKey = createPublicKey(privateKey).export({ type: 'spki', format: 'der' }).subarray(-32).toString('hex');
  const manifest = { v: 1, launch: 1, version: '0.2.0', npm: true, files: {} };
  const manifestFile = path.join(release, 'release.json');
  const raw = JSON.stringify(manifest, null, 2) + '\n';
  writeFileSync(manifestFile, raw);
  assert.equal(signRelease(release, seed, [publicKey]).version, '0.2.0');
  assert.deepEqual(verifyRelease(release, [publicKey]), manifest);
  ok('host manifest signing covers the exact original bytes');
  writeFileSync(manifestFile, JSON.stringify(manifest));
  assert.throws(() => verifyRelease(release, [publicKey]), /signature mismatch/);
  ok('equivalent JSON with different bytes fails verification');
  writeFileSync(manifestFile, raw);
  assert.throws(() => verifyRelease(release), /signature mismatch/);
  assert.throws(() => signRelease(release, seed), /does not match/);
  ok('a fixture key cannot sign or verify production releases');
  unlinkSync(path.join(release, 'release.json.sig'));
  const refused = spawnSync(process.env.PYTHON || 'python3', ['scripts/release_client.py', release, '--dry-run'], { encoding: 'utf8' });
  assert.notEqual(refused.status, 0);
  assert.match(refused.stderr, /signature|release.json.sig/);
  ok('an unsigned host release fails before any upload');

  const versionText = spawnSync(binary, ['--version'], { encoding: 'utf8', env: { ...process.env, WFSIM_CLI_NO_DELEGATE: '1' } }).stdout.trim().split(' ')[1];
  const [major, minor, patch] = versionText.split(/[.-]/).map(Number);
  const nextVersion = `${major}.${minor}.${patch + 1}`;
  const next = path.join(temporary, process.platform === 'win32' ? 'next.exe' : 'next');
  const nextTarget = path.resolve('client/target/host-update-test');
  const build = spawnSync('cargo', ['build', '--locked', '--manifest-path', 'client/Cargo.toml', '--target-dir', nextTarget], { encoding: 'utf8', timeout: 300_000, env: { ...process.env, WFSIM_CLI_BUILD: nextVersion } });
  assert.equal(build.status, 0, build.stderr);
  writeFileSync(next, readFileSync(path.join(nextTarget, 'debug', process.platform === 'win32' ? 'wfsim.exe' : 'wfsim'))); chmodSync(next, 0o700);
  const badSource = path.join(temporary, 'bad.rs');
  const bad = path.join(temporary, process.platform === 'win32' ? 'bad.exe' : 'bad');
  writeFileSync(badSource, `fn main() { if std::env::args().any(|a| a == "--version") { println!("wfsim ${nextVersion}"); } }\n`);
  const badBuild = spawnSync('rustc', [badSource, '-o', bad], { encoding: 'utf8', timeout: 60_000 });
  assert.equal(badBuild.status, 0, badBuild.stderr);
  const lifecycle = spawnSync('cargo', ['test', '--locked', '--manifest-path', 'client/Cargo.toml', 'host_update_', '--', '--ignored', '--nocapture'], {
    encoding: 'utf8', timeout: 90_000, env: { ...process.env, WFSIM_CLI_TEST_BINARY: next, WFSIM_CLI_TEST_BASE_BINARY: binary, WFSIM_CLI_TEST_BAD_BINARY: bad },
  });
  assert.equal(lifecycle.status, 0, lifecycle.stdout + lifecycle.stderr);
  assert.match(lifecycle.stdout, /2 passed/);
  ok('signed native activation, same-run restart, identity/config/outbox preservation and crash rollback use a real CLI worker');

  const packageDir = path.join(temporary, 'npm');
  cpSync('client/npm', packageDir, { recursive: true });
  const bytes = readFileSync(binary);
  const version = versionText;
  const target = `${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}-${{ darwin: 'macos', win32: 'windows', linux: 'linux' }[process.platform]}`;
  const npmCache = path.join(temporary, 'npm-cache');
  const cached = path.join(npmCache, version, target, process.platform === 'win32' ? 'wfsim.exe' : 'wfsim');
  mkdirSync(path.dirname(cached), { recursive: true });
  writeFileSync(cached, bytes); chmodSync(cached, 0o700);
  writeFileSync(path.join(packageDir, 'release.json'), JSON.stringify({ v: 1, launch: 1, version, files: {
    [target]: { size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex'), urls: [] },
  } }));
  server = createServer((request, response) => { assert.equal(request.url, '/cli/latest.json'); response.writeHead(404).end(); });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  device = path.join(temporary, 'device'); mkdirSync(device);
  writeFileSync(path.join(device, 'config.json'), JSON.stringify({ server: `http://127.0.0.1:${server.address().port}`, range: { min: 0, max: 0 }, name: 'Cache fixture', paused: false, consent: { v: 1, at: new Date().toISOString() } }));
  const start = spawnSync(process.execPath, [path.join(packageDir, 'bin/wfsim.mjs'), '--data-dir', device, 'start'], {
    encoding: 'utf8', timeout: 15_000, env: { ...process.env, WFSIM_CLI_CACHE: npmCache },
  });
  assert.equal(start.status, 0, start.stderr);
  const status = () => { try { return JSON.parse(readFileSync(path.join(device, 'status.json'), 'utf8')); } catch { return {}; } };
  await until(() => status().state === 'CPU budget is zero');
  const launcher = path.join(device, 'launcher', process.platform === 'win32' ? 'wfsim.exe' : 'wfsim');
  assert(existsSync(launcher));
  unlinkSync(cached);
  assert.equal(command('status').status, 0);
  assert.notEqual(status().state, 'stopped');
  ok('npm background computing keeps its own persistent launcher after the npm cache is removed');
  assert.equal(command('stop').status, 0);
  await until(() => status().state === 'stopped');
  ok('the original command still stops the same device after launcher relocation');
  if (process.platform === 'linux') {
    const shim = path.join(temporary, 'bin'); mkdirSync(shim);
    writeFileSync(path.join(shim, 'systemctl'), '#!/bin/sh\nexit 0\n'); chmodSync(path.join(shim, 'systemctl'), 0o700);
    const config = path.join(temporary, 'systemd');
    const install = spawnSync(binary, ['--data-dir', device, 'service', 'install'], { encoding: 'utf8', timeout: 15_000,
      env: { ...process.env, XDG_CONFIG_HOME: config, PATH: `${shim}${path.delimiter}${process.env.PATH}` } });
    assert.equal(install.status, 0, install.stderr);
    const unit = readFileSync(path.join(config, 'systemd/user/wfsim-compute.service'), 'utf8');
    assert(unit.includes(launcher) && unit.includes('Restart=on-failure'));
    assert(!unit.includes(npmCache) && !unit.includes(root));
    ok('Linux startup service points to the persistent launcher without changing the real user service manager');
  }
  console.log(`CLI host update: ${checks} checks passed; no publication or live work.`);
} finally {
  if (device) { command('stop'); await new Promise(r => setTimeout(r, 1000)); }
  if (server) await new Promise(resolve => server.close(resolve));
  rmSync(temporary, { recursive: true, force: true });
}
