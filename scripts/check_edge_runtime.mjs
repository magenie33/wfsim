// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import { delimiter, dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const ship = readFileSync('scripts/ship.py', 'utf8');
const pin = ship.match(/^WRANGLER = "(wrangler@[\d.]+)"$/m)?.[1];
assert(pin, 'the release must pin Wrangler');
if (!process.argv.includes('--installed')) {
  const result = spawnSync(process.platform === 'win32' ? 'npm.cmd' : 'npm',
    ['exec', '--yes', '--package=' + pin, '--', process.execPath, resolve(process.argv[1]), '--installed'],
    { stdio: 'inherit', timeout: 180000 });
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}
const executable = (process.env.PATH || '').split(delimiter).map(p => join(p, 'wrangler')).find(existsSync);
assert(executable, 'run through the release-pinned npm package');
const packageRoot = dirname(dirname(realpathSync(executable)));
assert.equal(JSON.parse(readFileSync(join(packageRoot, 'package.json'))).version, pin.split('@')[1]);
const require = createRequire(join(packageRoot, 'package.json'));
const { Miniflare, convertV4MiniflareOptions } = await import(pathToFileURL(require.resolve('miniflare')).href);
const config = readFileSync('wrangler.jsonc', 'utf8');
const compatibilityDate = config.match(/"compatibility_date":\s*"([\d-]+)"/)[1];
const compatibilityFlags = JSON.parse(config.match(/"compatibility_flags":\s*(\[[^\]]*\])/)[1]);
const helper = readFileSync(process.env.WFSIM_CLIENT_RELEASE_SOURCE || 'worker/client_release.js', 'utf8');
const fixture = JSON.stringify({ v: 1, version: 'fixture', npm: false, files: { 'linux-x64': { url: 'fixture' } } });
const latest = 'https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com/cli/latest.json';
const entry = `import {clientRelease} from './client_release.js';
export default {async fetch(request) {
  const mode=new URL(request.url).searchParams.get('mode');
  return mode==='arrow'?clientRelease(request,input=>fetch(input)):clientRelease(request);
}};`;

async function exercise(source, broken = false) {
  let status = 200, body = fixture, headers = {}, calls = 0;
  const root = resolve('target/edge-runtime');
  const mf = new Miniflare(convertV4MiniflareOptions({ host: '127.0.0.1', port: 0,
    compatibilityDate, compatibilityFlags, cf: false, modules: [
      { type: 'ESModule', path: join(root, 'index.js'), contents: entry },
      { type: 'ESModule', path: join(root, 'client_release.js'), contents: source },
    ], outboundService: request => {
      calls++;
      assert.equal(request.url, latest);
      assert.equal(request.method, 'GET');
      assert.equal(request.headers.get('accept'), 'application/json');
      assert.equal(request.headers.get('authorization'), null);
      assert.equal(request.headers.get('cookie'), null);
      return new Response(body, { status, headers });
    } }));
  try {
    for (const mode of ['default', 'arrow']) {
      const response = await mf.dispatchFetch('https://wfsim.app/api/cli/latest?mode=' + mode,
        { headers: { authorization: 'reader-secret', cookie: 'session=reader-secret' } });
      assert.equal(response.status, broken ? 502 : 200, 'actual Workers runtime manifest response');
      if (!broken) assert.equal(await response.text(), fixture);
      assert.equal(response.headers.get('cache-control'), 'no-store');
    }
    if (broken) { assert.equal(calls, 0, 'invalid Request fails before the network'); return; }
    const before = calls;
    assert.equal((await mf.dispatchFetch('https://wfsim.app/api/cli/latest', { method: 'POST' })).status, 405);
    assert.equal(calls, before, 'unsupported methods make no upstream request');
    for (const bad of [
      { status: 302, body: '', headers: { location: 'https://elsewhere.invalid/' } },
      { status: 503, body: 'unavailable' }, { status: 200, body: '<html>' },
      { status: 200, body: '{"v":1,"files":[]}' },
      { status: 200, body: fixture, headers: { 'content-length': '65537' } },
      { status: 200, body: 'x'.repeat(65537) },
    ]) {
      ({ status, body, headers = {} } = bad);
      assert.equal((await mf.dispatchFetch('https://wfsim.app/api/cli/latest')).status, 502);
    }
  } finally { await mf.dispose(); }
}
await exercise(helper);
assert(helper.includes('redirect: "manual"'), 'the negative control must change the current redirect mode');
await exercise(helper.replace('redirect: "manual"', 'redirect: "error"'), true);
console.log(`Workers runtime PASS: ${pin}, ${compatibilityDate}; default/arrow fetch, credential isolation, redirects, bounds and real invalid-Request negative control`);
