// SPDX-License-Identifier: AGPL-3.0-or-later
// Actual heads, stamp and receiver consume a deliverable largest-weapon fixture.
import assert from 'node:assert/strict';
import { spawnSync, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, openSync, closeSync, writeSync, writeFileSync, readFileSync, statSync, copyFileSync, rmSync, ftruncateSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash } from 'node:crypto';
import { jsonArrayRows } from './json_array_stream.mjs';
import { readInventoryPage } from './inventory_projection.mjs';
import { outputWriter, fileHash } from './inventory_publication_io.mjs';
import { pushBoard } from './live_publish.mjs';
import { boardDeliveryPending } from './inventory_board_pipeline.mjs';
import { pushLive } from '../worker/live_board.js';
import { boardHeads } from '../mcp/headless.js';

const root = mkdtempSync(join(tmpdir(), 'projection-consumers-')), board = join(root, 'verified'); mkdirSync(board);
const run = (program, args, options = {}) => { const out = spawnSync(program, args, { encoding: 'utf8', timeout: 120000, ...options });
  assert.equal(out.status, 0, out.stderr || out.error?.message); return out; };
try {
  const array = join(root, 'array');
  const values = ['界'.repeat(65536), { nested: [{ escaped: '\\"],}{' }], value: -1.2e30 }, null, false, 12345];
  writeFileSync(array, JSON.stringify(values));
  const parsed = []; for await (const row of jsonArrayRows(array)) parsed.push(JSON.parse(row)); assert.deepEqual(parsed, values);
  const offsets = []; for await (const row of jsonArrayRows(array, { withOffsets: true })) offsets.push(row);
  const resumed = []; for await (const row of jsonArrayRows(array, { offset: offsets[1].offset })) resumed.push(JSON.parse(row));
  assert.deepEqual(resumed, values.slice(2));
  for (const broken of ['[1,]', '[{},{}]garbage', '[', '{}', '[1 2]', '[,1]']) {
    writeFileSync(array, broken); await assert.rejects(async () => { for await (const _ of jsonArrayRows(array)) { /* consume EOF */ } });
  }
  writeFileSync(array, '[{},{}]garbage');
  await assert.rejects(async () => { let count = 0; for await (const row of jsonArrayRows(array, { withOffsets: true })) {
    assert.equal(row.done, false); if (++count === 2) break;
  } }, /after array/);
  const lines = join(root, 'lines'); writeFileSync(lines, '{"x":"界"}\r\n\n{"x":2}');
  let page = await readInventoryPage(lines, 0, 1); assert.deepEqual(page.rows, [{ x: '界' }]);
  page = await readInventoryPage(lines, page.offset, 1); assert.deepEqual(page.rows, [{ x: 2 }]); assert(page.done);
  const output = join(root, 'output'), context = {}; let writer = await outputWriter(context, output); writer.append('durable');
  const receipt = writer.checkpoint(); writer.append('uncommitted tail'); writer.close();
  writer = await outputWriter({}, output, receipt); writer.append(' complete'); await writer.finish();
  assert.equal(readFileSync(output, 'utf8'), 'durable complete');
  console.log('ok: Unicode/chunk/offset/EOF checks, incomplete rows and uncheckpointed tail recovery');

  const rich = join(root, 'rich'); mkdirSync(join(rich, 'sp200'), { recursive: true }); const all = [];
  for (const scene of ['', 'sp200']) {
    const rows = [];
    for (const mode of ['base','automatic']) for (const riven of [false,true]) for (let i = 0; i < 150; i++) rows.push({
      benchmark: scene ? 'scene-ruler' : 'standard_single_target', mode, score: Math.floor(i / 5) * 0.123456789,
      mods: ['serration@' + i % 10], measured_at: i === 0 ? '2026-11-09T01:00Z' : '2026-11-09T00:00Z',
      ...(riven ? { riven: { bonuses: ['damage','critical_chance'], malus: null, rolls: [1.1,1.1] } } : {}),
    });
    writeFileSync(join(rich, scene, 'braton_prime.json'), JSON.stringify(rows)); all.push(...rows);
  }
  run(process.execPath, ['scripts/board_heads.mjs', rich]);
  assert.deepEqual(JSON.parse(readFileSync(join(rich, 'heads/braton_prime.json'))), boardHeads(all),
    'scene/mode/riven thresholds, all ties and newest below the cutoff must match the authoritative boardHeads');

  const path = join(board, 'braton_prime.json'), fd = openSync(path, 'w'); writeSync(fd, '[');
  const ids = ['serration','vital_sense','hellfire','speed_trigger','hammer_shot','vigilante_fervor','magazine_warp','fast_hands'];
  for (let i = 0; i < 60000; i++) {
    let ranks = i;
    const mods = ids.map((id, n) => { const max = n === 0 ? 10 : 5, rank = ranks % (max + 1); ranks = Math.floor(ranks / (max + 1)); return id + (rank === max ? '' : '@' + rank); });
    const row = { benchmark: 'standard_single_target', mode: 'base', score: 1000, shown: '1000', mods,
      evolutions: [], arcanes: ['primary_deadhead'], valence: '', measured_at: '2026-11-09T00:00Z', measured_by: '0123456789ab' };
    writeSync(fd, (i ? ',' : '') + JSON.stringify(row));
  }
  writeSync(fd, ']'); closeSync(fd); writeFileSync(join(board, 'index.json'), '{}');
  assert(statSync(path).size < 32 * 1024 ** 2, 'the positive file must be genuinely deliverable');
  run(process.execPath, ['--max-old-space-size=32', 'scripts/board_heads.mjs', board]);
  assert.equal(await fileHash(join(board, 'heads/braton_prime.json')), await fileHash(path), 'all 60k first-place ties and their source order survive');
  const broken = join(root, 'broken'); mkdirSync(broken); copyFileSync(path, join(broken, 'braton_prime.json'));
  const mutant = resolve('target/board_heads_full_memory_mutant.mjs');
  writeFileSync(mutant, execFileSync('git', ['show','61de56dcda71301bbaefc16b8fcc99baf4cc4fc3:scripts/board_heads.mjs']));
  const old = spawnSync(process.execPath, ['--max-old-space-size=32', mutant, broken], { encoding: 'utf8', timeout: 120000 });
  rmSync(mutant);
  assert.notEqual(old.status, 0, 'the old full-memory consumer must fail under the same heap bound');
  assert.match(old.stderr, /heap out of memory|Allocation failed/);

  const state = join(root, 'state.yaml'); writeFileSync(state, 'boards: {}\n');
  const oldStamp = join(root, 'old_stamp.py'), original = execFileSync('git', ['show','61de56dcda71301bbaefc16b8fcc99baf4cc4fc3:scripts/board_meta.py'], { encoding: 'utf8' });
  writeFileSync(oldStamp, original.replace('encoding="utf-8", newline="\\n")', 'encoding="utf-8")'));
  run('python3', [oldStamp, board, state, join(root, 'old-meta.json')]);
  run('python3', ['scripts/board_meta.py', board, state, join(board, 'meta.json')]);
  const stamp = readFileSync(join(board, 'meta.json'), 'utf8'); assert.equal(stamp, readFileSync(join(root, 'old-meta.json'), 'utf8'));
  run('python3', ['scripts/board_meta.py', board, state, join(board, 'meta.json')]); assert.equal(readFileSync(join(board, 'meta.json'), 'utf8'), stamp);
  assert.equal(JSON.parse(stamp).rows, 60000);
  const remote = new Map(), token = process.env.BOARD_PUSH_TOKEN; process.env.BOARD_PUSH_TOKEN = 'fixture-only';
  const env = { BOARD_PUSH_TOKEN: 'fixture-only', PUBLIC: { put: async (key, value) => remote.set(key, createHash('sha256').update(Buffer.from(value)).digest('hex')) } };
  try {
    assert.equal(await boardDeliveryPending(root), true);
    await assert.rejects(pushBoard({ workDir: root, boardDir: board, send: async () => { throw Error('publication interrupted'); } }), /interrupted/);
    assert.equal(await boardDeliveryPending(root), true);
    await pushBoard({ workDir: root, boardDir: board, send: (url, init) => pushLive(new Request(url, init), env, new URL(url).pathname.split('/api/board/live/')[1]) });
    assert.equal(await boardDeliveryPending(root), false);
    assert.equal(remote.get('board/braton_prime.json'), await fileHash(path));
    assert.equal(remote.get('board/heads/braton_prime.json'), await fileHash(path));
    assert.equal(remote.get('board.meta.json'), createHash('sha256').update(stamp).digest('hex'));
    const oversized = join(board, 'a.json'), fd = openSync(oversized, 'w'); ftruncateSync(fd, 32 * 1024 ** 2 + 1); closeSync(fd);
    await assert.rejects(pushBoard({ workDir: root, boardDir: board, send: () => assert.fail('an oversized file reached the receiver') }), /32MiB/);
  } finally { if (token === undefined) delete process.env.BOARD_PUSH_TOKEN; else process.env.BOARD_PUSH_TOKEN = token; }
  console.log('ok: 60k same-weapon tied rows yield exact heads under32MiB heap while old consumer fails; fresh native stamp bytes stay stable; actual unchanged receiver accepts every final file; interrupted delivery remains pending; oversized files fail before upload');
} catch (error) { console.error('consumer evidence: ' + root); throw error; }
finally { if (!process.env.WFSIM_INVENTORY_CONSUMER_EVIDENCE) rmSync(root, { recursive: true, force: true }); }
