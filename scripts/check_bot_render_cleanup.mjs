// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { EventEmitter } from 'node:events';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import fs from 'node:fs/promises';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

const scratch = mkdtempSync(join(tmpdir(), 'wfsim-bot-cleanup-check-'));
const prior = { spawn: childProcess.spawn, rm: fs.rm, fetch: globalThis.fetch,
  chrome: process.env.CHROME, temporary: process.env.TMPDIR };
const children = [];
try {
  process.env.CHROME = process.execPath;
  process.env.TMPDIR = scratch;
  childProcess.spawn = (_command, args) => {
    const proc = new EventEmitter();
    proc.exitCode = null; proc.signalCode = null;
    proc.stderr = Object.assign(new EventEmitter(), { unref() {} });
    proc.unref = () => {};
    proc.kill = () => {};
    const dir = args.find(arg => arg.startsWith('--user-data-dir=')).split('=')[1];
    mkdirSync(join(dir, 'Default'));
    writeFileSync(join(dir, 'DevToolsActivePort'), '12345\n');
    children.push(proc);
    return proc;
  };
  globalThis.fetch = async () => ({ ok: true });
  syncBuiltinESMExports();
  const source = readFileSync(new URL(process.env.WFSIM_RENDER_SOURCE || '../bot/render.mjs', import.meta.url), 'utf8');
  const { launch } = await import('data:text/javascript,' + encodeURIComponent(source + '\nexport { launch };'));
  const first = await launch();
  const unrelated = join(scratch, 'unrelated');
  writeFileSync(unrelated, 'preserve');
  first.proc.exitCode = 0;
  first.proc.emit('exit', 0);
  assert(existsSync(first.dir), 'a parent exit cannot remove a profile still held by its children');
  writeFileSync(join(first.dir, 'Default', 'late-child-write'), 'saved after parent exit');
  const second = await launch();
  assert.notEqual(second.slot, first.slot, 'the closing browser still owns its cache slot');
  first.proc.emit('close', 0);
  for (let attempt = 0; existsSync(first.dir) && attempt < 100; attempt++) await sleep(10);
  assert(!existsSync(first.dir), 'closing stdio releases and cleans the complete owned profile');
  assert.equal(readFileSync(unrelated, 'utf8'), 'preserve');

  const denied = Object.assign(new Error('fixture cleanup denied'), { code: 'EACCES' });
  fs.rm = async path => { assert.equal(path, second.dir); throw denied; };
  syncBuiltinESMExports();
  const cleanup = second.proc.listeners('close')[0];
  assert(cleanup, 'cleanup follows the close event');
  second.proc.exitCode = 0;
  second.proc.emit('exit', 0);
  await assert.rejects(cleanup(), error => error === denied, 'permanent cleanup errors must propagate');
  console.log('bot Chrome cleanup: exit/close ordering, late child writes, cache ownership and permanent errors pass');
} finally {
  childProcess.spawn = prior.spawn; fs.rm = prior.rm; globalThis.fetch = prior.fetch;
  syncBuiltinESMExports();
  for (const [key, value] of [['CHROME', prior.chrome], ['TMPDIR', prior.temporary]]) {
    if (value === undefined) delete process.env[key]; else process.env[key] = value;
  }
  for (const proc of children) proc.exitCode = 0;
  rmSync(scratch, { recursive: true, force: true });
}
