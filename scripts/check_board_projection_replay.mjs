// SPDX-License-Identifier: AGPL-3.0-or-later
// A sealed-source offline replay has no database epoch and resumes committed pages.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, copyFileSync, readdirSync, rmSync } from 'node:fs';
import { join, resolve, basename } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { replayBoardProjection } from './replay_board_projection.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { fileURLToPath } from 'node:url';

if (process.argv[2] === 'kill-replay') {
  const [, , , capture, bin, work, manifestSha] = process.argv;
  await replayBoardProjection({ capture, bin, work, manifestSha, fault: async stage => {
    if (stage === 'after-input-commit-before-reply') process.kill(process.pid, 'SIGKILL');
  } });
  throw Error('process-death fixture never reached the committed page');
}

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const root = mkdtempSync(join(tmpdir(), 'board-replay-')), capture = join(root, 'capture'), bin = join(root, 'bin');
for (const dir of [capture, bin]) mkdirSync(dir);
copyFileSync(binary, join(bin, 'wfsim-board')); const engine = '0ae1bf8edb0f'; writeFileSync(join(bin, 'ENGINE'), engine);
const hash = data => createHash('sha256').update(data).digest('hex');
const native = await openInventoryOperator(binary);
let success = false;
try {
  const records = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(row => row.record);
  const descriptions = await native.call('describe', records.map((record, i) => ({ id: String(i), record })));
  const facts = descriptions.flatMap((row, i) => native.contract.rulers.flatMap(ruler => row.modes.map(mode => ({
    identity: row.candidate, ruler: ruler.id, mode, score: 123.45678901234 + i,
    measured_by: 'verified:' + engine, started_at: '2026-11-09T00:00:00.000Z', finished_at: '2026-11-09T00:00:01.123Z', cost_seconds: 1.25,
  }))));
  const raw = Object.fromEntries(['library','builds','facts','queue','tasks','meta'].map(key => [key, join(capture, key)]));
  writeFileSync(raw.library, JSON.stringify(records));
  writeFileSync(raw.builds, records.map((record, i) => JSON.stringify({ id: descriptions[i].candidate, at: 'day', record }) + '\n').join(''));
  writeFileSync(raw.facts, facts.map(row => JSON.stringify(row) + '\n').join(''));
  writeFileSync(raw.queue, ''); writeFileSync(raw.tasks, '');
  writeFileSync(raw.meta, JSON.stringify({ engine, snapshot: 'captured-source-only', started_at: '2026-11-09T00:00:00.000Z',
    counts: { library: records.length, facts: facts.length, queue: 0, tasks: 0 } }));
  const manifest = { bootstrap: true, raw, files: Object.values(raw).map(path => ({ path: '/protected/source/' + basename(path), sha256: hash(readFileSync(path)) })) };
  writeFileSync(join(capture, 'capture.json'), JSON.stringify(manifest));
  const manifestSha = hash(readFileSync(join(capture, 'capture.json'))), work = join(root, 'work');
  let lost = true;
  await assert.rejects(replayBoardProjection({ capture, bin, work, manifestSha, fault: async stage => {
    if (stage === 'after-input-commit-before-reply' && lost) { lost = false; throw Error('lost committed response'); }
  } }), /lost committed response/);
  const report = await replayBoardProjection({ capture, bin, work, manifestSha });
  assert.equal(report.database_epoch, null); assert.equal(report.publication_remote, false); assert.equal(report.published, false);
  assert.equal(report.phase, 'complete'); assert.equal(report.delivery_eligible, true);
  const oracle = join(root, 'oracle'); mkdirSync(oracle); const state = join(root, 'state.yaml'); writeFileSync(state, 'boards: {}\n');
  execFileSync(binary, ['--project', oracle, '--facts-in', raw.facts, '--state', state], { input: readFileSync(raw.library), timeout: 60000 });
  const files = (directory, prefix = '') => readdirSync(directory, { withFileTypes: true }).flatMap(entry => entry.isDirectory()
    ? files(join(directory, entry.name), prefix + entry.name + '/') : [prefix + entry.name]);
  for (const name of files(oracle)) assert.equal(readFileSync(join(work, 'verified', name), 'utf8'), readFileSync(join(oracle, name), 'utf8'), name);
  const clock = text => text.replace(/scored_at_epoch_seconds: \d+/g, 'scored_at_epoch_seconds: CLOCK');
  assert.equal(clock(readFileSync(join(work, 'pub/data/board_state.yaml'), 'utf8')), clock(readFileSync(state, 'utf8')));
  const stamp = readFileSync(join(work, 'verified/meta.json'), 'utf8');
  const again = await replayBoardProjection({ capture, bin, work, manifestSha }); assert.equal(again.question_digest, report.question_digest);
  assert.equal(readFileSync(join(work, 'verified/meta.json'), 'utf8'), stamp, 'the stamp must never digest/count itself');
  const killedWork = join(root, 'killed');
  const killed = spawnSync(process.execPath, [fileURLToPath(import.meta.url), 'kill-replay', capture, bin, killedWork, manifestSha], { timeout: 60000 });
  assert.equal(killed.signal, 'SIGKILL', killed.stderr?.toString());
  const recovered = await replayBoardProjection({ capture, bin, work: killedWork, manifestSha });
  assert.equal(recovered.question_digest, report.question_digest);
  for (const name of files(oracle)) assert.equal(readFileSync(join(killedWork, 'verified', name), 'utf8'), readFileSync(join(oracle, name), 'utf8'));
  await assert.rejects(replayBoardProjection({ capture, bin, work: join(root, 'too-small'), manifestSha, diskBudgetBytes: 8 * 1024 ** 3 }), /cannot hold/);
  const budgeted = await replayBoardProjection({ capture, bin, work: join(root, 'budgeted'), manifestSha, diskBudgetBytes: 9 * 1024 ** 3 });
  assert.equal(budgeted.disk.explicit, true); assert(budgeted.disk.database_limit_bytes < 1024 ** 3);
  assert.equal(budgeted.question_digest, report.question_digest);
  await assert.rejects(replayBoardProjection({ capture, bin, work: join(root, 'wrong-sha'), manifestSha: '0'.repeat(64) }), /manifest SHA256 mismatch/);
  writeFileSync(raw.tasks, '{}\n');
  await assert.rejects(replayBoardProjection({ capture, bin, work, manifestSha }), /source input SHA256 mismatch/);
  console.log('ok: protected full-source replay resumes a committed response loss; native board/index/state bytes match; source tampering fails closed; no database epoch or remote publication');
  success = true;
} catch (error) { console.error('replay evidence: ' + root); throw error; }
finally { await native.close(); if (success && !process.env.WFSIM_INVENTORY_REPLAY_EVIDENCE) rmSync(root, { recursive: true, force: true }); }
