// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, utimesSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { launchChrome, stopChrome, waitForBoot } from './cdp_boot.mjs';

const dir = mkdtempSync(join(tmpdir(), 'wfsim-cdp-test-'));
const fixture = join(dir, 'chrome.mjs');
writeFileSync(fixture, `
import {createServer} from 'node:http';
import {mkdirSync,writeFileSync} from 'node:fs';
const profile=process.argv.find(s=>s.startsWith('--user-data-dir=')).slice(16);
mkdirSync(profile,{recursive:true});
const mode=process.argv[2];
if(mode==='exit'||mode==='retry'&&profile.endsWith('-0')){console.error('fixture crash diagnostic');process.exit(17);}
if(mode==='hang'){setInterval(()=>{},1000);}
else {
 const server=createServer((q,r)=>{r.setHeader('content-type','application/json');r.end(JSON.stringify([{type:'page',webSocketDebuggerUrl:'ws://fixture'}]));});
 server.listen(0,'127.0.0.1',()=>writeFileSync(profile+'/DevToolsActivePort',String(server.address().port)+'\\n'));
}
`);
try {
  for (const [mode, attempts] of [['ok', 1], ['retry', 2]]) {
    const base = join(dir, mode);
    const started = await launchChrome(process.execPath, [fixture, mode], base, { timeout: 2000, interval: 10 });
    assert.equal(started.attempts, attempts);
    assert.equal(started.page.type, 'page');
    if (attempts === 2) assert(!existsSync(base + '-0'), 'a failed profile must be removed');
    await stopChrome(started.proc, started.profile);
    assert(!existsSync(started.profile));
  }
  for (const mode of ['exit', 'hang']) {
    const base = join(dir, mode);
    await assert.rejects(launchChrome(process.execPath, [fixture, mode], base, { timeout: 150, interval: 10 }),
      mode === 'exit' ? /fixture crash diagnostic/ : /debugging target/);
    assert(!existsSync(base + '-0') && !existsSync(base + '-1'), 'both failed attempts are cleaned');
  }
  const shared = join(dir, 'isolated-temp');
  mkdirSync(shared);
  const captureFile = join(shared, 'wfsim-capture.json'), captureDir = join(shared, 'wfsim-capture');
  const recentFile = join(shared, 'wfsim-recent.json'), otherFile = join(shared, 'unrelated-capture.json');
  mkdirSync(captureDir);
  for (const path of [captureFile, join(captureDir, 'receipt.json'), recentFile, otherFile]) writeFileSync(path, 'preserve');
  const active = await launchChrome(process.execPath, [fixture, 'ok'], join(shared, 'wfsim-active'), { timeout: 2000, interval: 10 });
  try {
    const marker = join(active.profile, 'owner.json');writeFileSync(marker, 'preserve');
    const old = new Date(Date.now() - 2 * 60 * 60 * 1000);
    for (const path of [captureFile, captureDir, otherFile, active.profile]) utimesSync(path, old, old);
    const probe = join(dir, 'open-app.mjs');
    writeFileSync(probe, `import assert from 'node:assert/strict';
import cp from 'node:child_process';import {syncBuiltinESMExports} from 'node:module';
assert.equal(process.env.TEMP,${JSON.stringify(shared)},'the fixture must not use shared TEMP');
const spawn=cp.spawn;
cp.spawn=(chrome,args,options)=>spawn(process.execPath,[${JSON.stringify(fixture)},'exit',...args],options);
syncBuiltinESMExports();
const {openApp}=await import(${JSON.stringify(new URL('./cdp.mjs', import.meta.url).href)});
await assert.rejects(openApp({base:'http://127.0.0.1:1',profile:'wfsim-owned'}),/fixture crash diagnostic/);
`);
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/^(TEMP|TMP|TMPDIR|CHROME)$/i.test(key)));
    const result = spawnSync(process.execPath, [probe], { encoding: 'utf8',
      env: { ...env, TEMP: shared, TMP: shared, TMPDIR: shared, CHROME: process.execPath } });
    assert.equal(result.status, 0, result.stderr);
    const preserved = [captureFile, join(captureDir, 'receipt.json'), marker, recentFile, otherFile]
      .map(path => ({ name: path.slice(shared.length + 1), retained: existsSync(path) && readFileSync(path, 'utf8') === 'preserve' }));
    assert.equal(active.proc.exitCode, null, 'the other profile still belongs to a running browser');
    assert(preserved.every(row => row.retained), 'starting a check must preserve unrelated TEMP entries and active profiles: ' + JSON.stringify(preserved));
    assert.deepEqual(readdirSync(shared).sort(), [captureFile, captureDir, recentFile, otherFile, active.profile].map(path => basename(path)).sort(),
      'failed openApp attempts remove only their own profiles');
    console.log('Isolated TEMP captures and an aged running profile survive another check; only owned profiles are removed.');
  } finally { await stopChrome(active.proc, active.profile); }
  assert(!existsSync(active.profile), 'the owner still removes its own profile when it stops');
  let calls = 0;
  await waitForBoot(async () => ({ ready: ++calls === 3 }), { timeout: 100, interval: 1 });
  assert.equal(calls, 3, 'readiness ends the wait immediately');
  await assert.rejects(waitForBoot(async () => ({ ready: true, error: 'broken boot' })), /broken boot/);
  await assert.rejects(waitForBoot(async () => ({ ready: false }), { timeout: 5, interval: 1 }), /did not become ready/);
  console.log('Chrome startup retries once with diagnostics; failed boots never pass, and readiness waits are bounded.');
} finally { rmSync(dir, { recursive: true, force: true }); }
