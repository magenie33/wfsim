// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
  let calls = 0;
  await waitForBoot(async () => ({ ready: ++calls === 3 }), { timeout: 100, interval: 1 });
  assert.equal(calls, 3, 'readiness ends the wait immediately');
  await assert.rejects(waitForBoot(async () => ({ ready: true, error: 'broken boot' })), /broken boot/);
  await assert.rejects(waitForBoot(async () => ({ ready: false }), { timeout: 5, interval: 1 }), /did not become ready/);
  console.log('Chrome startup retries once with diagnostics; failed boots never pass, and readiness waits are bounded.');
} finally { rmSync(dir, { recursive: true, force: true }); }
