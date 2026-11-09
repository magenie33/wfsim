#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash, randomBytes } from 'node:crypto';
import { mkdir, readFile, rename, unlink, writeFile, chmod } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

try {
  const target = `${process.arch === 'arm64' ? 'aarch64' : process.arch === 'x64' ? 'x86_64' : process.arch}-${{ win32: 'windows', darwin: 'macos', linux: 'linux' }[process.platform] || process.platform}`;
  const manifest = JSON.parse(await readFile(new URL('../release.json', import.meta.url), 'utf8'));
  const file = manifest.files[target];
  if (!file) throw new Error(`No official CLI build for ${target}. See https://wfsim.app/compute`);
  if (!/^[0-9a-f]{64}$/.test(file.sha256) || !Number.isSafeInteger(file.size) || file.size <= 0 || file.size > 64 * 1024 * 1024
      || !/^[0-9A-Za-z._-]+$/.test(manifest.version)) throw new Error('Invalid CLI release manifest');
  const base = process.env.WFSIM_CLI_CACHE || (process.platform === 'win32' ? path.join(process.env.LOCALAPPDATA || homedir(), 'WFSim', 'cli-cache')
    : path.join(process.env.XDG_CACHE_HOME || path.join(homedir(), '.cache'), 'wfsim-cli'));
  const dir = path.join(base, manifest.version, target);
  const executable = path.join(dir, process.platform === 'win32' ? 'wfsim.exe' : 'wfsim');
  const valid = bytes => bytes.length === file.size && createHash('sha256').update(bytes).digest('hex') === file.sha256;
  let cached = await readFile(executable).then(valid, () => false);
  if (!cached) {
    await mkdir(dir, { recursive: true, mode: 0o700 });
    let last;
    for (const url of file.urls) {
      let temporary;
      try {
        if (new URL(url).protocol !== 'https:') throw new Error('CLI download must use HTTPS');
        const response = await fetch(url, { signal: AbortSignal.timeout(60_000) });
        if (!response.ok) throw new Error(`Download returned HTTP ${response.status}`);
        const chunks = []; let size = 0;
        for await (const chunk of response.body) {
          size += chunk.length;
          if (size > file.size) throw new Error('CLI download is larger than its manifest');
          chunks.push(chunk);
        }
        const bytes = Buffer.concat(chunks);
        if (!valid(bytes)) throw new Error('CLI hash/size mismatch; refusing to run');
        temporary = `${executable}.${process.pid}.${randomBytes(6).toString('hex')}.tmp`;
        await writeFile(temporary, bytes, { mode: 0o700, flag: 'wx' });
        await chmod(temporary, 0o700);
        if (await readFile(executable).then(valid, () => false)) await unlink(temporary);
        else await rename(temporary, executable);
        cached = true;
        break;
      } catch (error) {
        if (temporary) await unlink(temporary).catch(() => {});
        last = error;
      }
    }
    if (!cached) throw last || new Error('No CLI download source is available');
  }
  const child = spawn(executable, process.argv.slice(2), { stdio: 'inherit', windowsHide: false });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
  child.on('error', error => { console.error(`WFSim: ${error.message}`); process.exitCode = 1; });
  child.on('exit', (code, signal) => { process.exitCode = code ?? (signal ? 1 : 0); });
} catch (error) {
  console.error(`WFSim: ${error.message}`);
  process.exitCode = 1;
}
