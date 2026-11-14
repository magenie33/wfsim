// SPDX-License-Identifier: AGPL-3.0-or-later
// A completed intake batch survives a later failure; arrivals are spent last.
import { createReadStream, closeSync, openSync, readFileSync, writeFileSync, mkdtempSync, rmSync, existsSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { checkInboxReceipt } from './inbox_reader.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export const INTAKE_BATCH_ROWS = 100, INTAKE_BATCH_BYTES = 1024 * 1024;
const jsonLines = file => readFileSync(file, 'utf8').split('\n').filter(Boolean).map(JSON.parse);

export function runIntakeCommand(command, args, { input, output, signal } = {}) {
  const descriptors = [];
  const descriptor = (file, mode, fallback) => {
    if (!file) return fallback;
    const fd = openSync(file, mode); descriptors.push(fd); return fd;
  };
  return new Promise((accept, reject) => {
    let child;
    try {
      const argv = command === 'bash' && process.platform === 'win32' ? args.map(arg => arg.replaceAll('\\', '/')) : args;
      child = spawn(command, argv, { signal, stdio: [descriptor(input, 'r', 'ignore'), descriptor(output, 'w', 'inherit'), 'inherit'] });
    } catch (error) { descriptors.forEach(closeSync); reject(error); return; }
    descriptors.forEach(closeSync);
    child.once('error', reject);
    child.once('exit', (code, terminated) => code === 0 ? accept() : reject(new Error(`${command} failed (${terminated || code})`)));
  });
}

export async function intakeBatches(file, { intake = process.env.WFSIM_INTAKE_BIN || join(ROOT, 'target/release/wfsim-intake'), deadlineSeconds = 900,
  maxRows = INTAKE_BATCH_ROWS, maxBytes = INTAKE_BATCH_BYTES, run = runIntakeCommand, signal, onCommitted, doneFile } = {}) {
  if (![maxRows, maxBytes].every(n => Number.isSafeInteger(n) && n > 0)
      || !Number.isFinite(deadlineSeconds) || deadlineSeconds < 0
      || onCommitted !== undefined && typeof onCommitted !== 'function') throw new Error('invalid intake batch bounds');
  const scratch = mkdtempSync(join(tmpdir(), 'wfsim-intake-batches-'));
  const path = name => join(scratch, name), started = Date.now(), end = started + deadlineSeconds * 1000;
  const state = new DatabaseSync(path('first-builds.sqlite'));
  state.exec("PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=FILE; CREATE TABLE state_first_build_submission_times(id TEXT PRIMARY KEY, at TEXT) WITHOUT ROWID");
  const first = state.prepare("INSERT INTO state_first_build_submission_times(id,at) VALUES(?,?) ON CONFLICT DO NOTHING");
  const clock = state.prepare("SELECT at FROM state_first_build_submission_times WHERE id=?");
  const result = { batches: 0, arrivals: 0, builds: 0 };
  if (doneFile) writeFileSync(doneFile, '');
  let pending = [], bytes = 0;
  const execute = (command, args, io = {}) => run(command, args, { ...io, signal });
  const ship = async () => {
    if (!pending.length || Date.now() >= end) return false;
    writeFileSync(path('inbox.ndjson'), pending.join('\n') + '\n');
    await execute(intake, ['--deadline', String(Math.max(0, Math.ceil((end - Date.now()) / 1000))), '--done', path('done.txt')],
      { input: path('inbox.ndjson'), output: path('builds.ndjson') });
    const builds = jsonLines(path('builds.ndjson'));
    state.exec('BEGIN');
    for (const build of builds) {
      first.run(build.id, build.at);
      build.at = clock.get(build.id).at;
    }
    state.exec('COMMIT');
    writeFileSync(path('builds.ndjson'), builds.map(build => JSON.stringify(build) + '\n').join(''));
    // Captured workflows admit every sourced computation before inbox acknowledgement.
    await execute('bash', [join(ROOT, 'scripts/ship_builds.sh'), path('builds.ndjson'), path('empty')]);
    if(onCommitted)await onCommitted(path('builds.ndjson'));
    await execute('bash', [join(ROOT, 'scripts/ship_builds.sh'), path('empty'), path('done.txt')]);
    if (doneFile) writeFileSync(doneFile, readFileSync(path('done.txt')), { flag: 'a' });
    result.batches++; result.builds += builds.length;
    result.arrivals += readFileSync(path('done.txt'), 'utf8').split('\n').filter(Boolean).length;
    console.log(`intake: committed batch ${result.batches}, ${result.arrivals} arrival(s)`);
    pending = []; bytes = 0;
    return true;
  };
  const input = createReadStream(file), lines = createInterface({ input, crlfDelay: Infinity });
  try {
    writeFileSync(path('empty'), '');
    for await (const line of lines) {
      signal?.throwIfAborted();
      if (Date.now() >= end) break;
      if (!line.trim()) continue;
      const size = Buffer.byteLength(line) + 1;
      if (size > maxBytes) throw new Error('one intake arrival exceeds the batch byte limit');
      if (pending.length && (pending.length >= maxRows || bytes + size > maxBytes)) if (!await ship()) break;
      if (Date.now() >= end) break;
      pending.push(line); bytes += size;
    }
    await ship();
    return { ...result, elapsed_ms: Date.now() - started };
  } finally {
    lines.close(); input.destroy(); state.close(); rmSync(scratch, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const controller = new AbortController();
  const stop = () => controller.abort();
  process.once('SIGINT', stop); process.once('SIGTERM', stop);
  try {
    if (!process.argv[2] || process.argv.length !== 3) throw new Error('usage: intake_batches.mjs <inbox.ndjson>');
    const inputFile=process.argv[2],receiptFile=inputFile+'.receipt.json';
    if(existsSync(receiptFile))checkInboxReceipt({receiptFile,inputFile,allowReadError:true});
    await intakeBatches(process.argv[2], { signal: controller.signal });
    if(existsSync(receiptFile))checkInboxReceipt({receiptFile,inputFile});
  } catch (error) { console.error(`intake: ${error.message}`); process.exitCode = 1; }
}
