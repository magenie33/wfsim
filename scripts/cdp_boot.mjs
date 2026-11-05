// SPDX-License-Identifier: AGPL-3.0-or-later
// Only process startup is retried; a connected page's failed assertion is final.
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const sleep = ms => new Promise(r => setTimeout(r, ms));

export async function stopChrome(proc, profile) {
  const exited = proc.exitCode !== null || proc.signalCode !== null;
  if (!exited) {
    const exit = new Promise(r => proc.once('exit', r));
    if (process.platform === 'win32') spawnSync('taskkill', ['/pid', String(proc.pid), '/T', '/F'], { stdio: 'ignore' });
    else proc.kill();
    await Promise.race([exit, sleep(1000)]);
  }
  for (let i = 0; i < 5; i++) {
    try { rmSync(profile, { recursive: true, force: true }); return; }
    catch { await sleep(100); }
  }
}

export async function launchChrome(chrome, args, profileBase, { timeout = 45000, interval = 100 } = {}) {
  const failures = [];
  for (let attempt = 0; attempt < 2; attempt++) {
    const profile = `${profileBase}-${attempt}`;
    const proc = spawn(chrome, [...args, `--user-data-dir=${profile}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '', spawnError;
    proc.stderr?.on('data', b => { stderr = (stderr + b).slice(-4096); });
    proc.on('error', e => { spawnError = e; });
    const deadline = Date.now() + timeout;
    try {
      while (Date.now() < deadline) {
        if (spawnError) throw spawnError;
        if (proc.exitCode !== null || proc.signalCode !== null) throw new Error(`exited ${proc.exitCode ?? proc.signalCode}`);
        let port;
        try { port = Number(readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]); }
        catch { /* Chrome has not written its port yet. */ }
        if (port > 0 && port < 65536) {
          try {
            const r = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(Math.max(1, Math.min(2000, deadline - Date.now()))) });
            if (r.ok) {
              const page = (await r.json()).find(t => t.type === 'page' && t.webSocketDebuggerUrl);
              if (page) return { proc, profile, page, attempts: attempt + 1 };
            }
          } catch { /* The debugging listener is still starting. */ }
        }
        await sleep(interval);
      }
      throw new Error('debugging target did not become ready');
    } catch (e) {
      failures.push(`${e.message}; stderr: ${stderr.trim() || '(empty)'}`);
      await stopChrome(proc, profile);
      console.error(`Chrome launch ${attempt + 1}/2: ${failures.at(-1)}`);
    }
  }
  throw new Error(`Chrome did not start (${chrome}): ${failures.join(' | ')}`);
}

export async function waitForBoot(read, { timeout = 13000, interval = 50 } = {}) {
  const deadline = Date.now() + timeout;
  do {
    const state = await read();
    if (state?.error) throw new Error(`Page boot failed: ${state.error}`);
    if (state?.ready) return;
    await sleep(interval);
  } while (Date.now() < deadline);
  throw new Error(`Page did not become ready within ${timeout} ms`);
}
