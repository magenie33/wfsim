// SPDX-License-Identifier: AGPL-3.0-or-later
// Run the official cycle and CLI with local transports; no production work is claimed.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'wfsim-settle-'));
let failures = 0, passed = 0;
const check = (name, fn) => {
  try { fn(); passed++; console.log('  ok  ' + name); }
  catch (e) { failures++; console.log('  FAIL ' + name + ': ' + e.message); }
};
const put = (name, body, mode = 0o644) => writeFileSync(join(dir, name), body, { mode });
try {
  mkdirSync(join(dir, 'bin'));
  put('bin/date', '#!/usr/bin/env bash\nf="$FIXTURE_CLOCK"; n=1000; [ ! -f "$f" ] || n=$(cat "$f"); n=$((n+1)); echo "$n" > "$f"; echo "$n"\n', 0o755);
  put('bin/sleep', '#!/usr/bin/env bash\nexit 0\n', 0o755);
  put('bin/nproc', '#!/usr/bin/env bash\necho 2\n', 0o755);
  put('bin/node', ['#!/usr/bin/env bash',
    'n=0; [ ! -f "$FIXTURE_COUNT" ] || n=$(cat "$FIXTURE_COUNT"); n=$((n+1)); echo "$n" > "$FIXTURE_COUNT"',
    'read -ra steps <<< "$FIXTURE_CODES"; i=$((n-1)); [ "$i" -lt "${#steps[@]}" ] || i=$((${#steps[@]}-1))',
    'exit "${steps[$i]}"', ''].join('\n'), 0o755);
  const cycle = codes => {
    for (const n of ['clock', 'count']) rmSync(join(dir, n), { force: true });
    return spawnSync('bash', [new URL('./settle_cycles.sh', import.meta.url).pathname, 'work', '.'], {
      cwd: dir, encoding: 'utf8', timeout: 10000,
      env: { ...process.env, PATH: join(dir, 'bin') + ':' + process.env.PATH, SETTLE_MINUTES: '1',
        FIXTURE_CLOCK: join(dir, 'clock'), FIXTURE_COUNT: join(dir, 'count'), FIXTURE_CODES: codes },
    });
  };
  check('an empty official queue finishes successfully', () => assert.equal(cycle('3').status, 0));
  check('a recovered transient cycle does not fail the run', () => assert.equal(cycle('1 1 0 3').status, 0));
  check('persistent failed cycles fail the run and stop retrying', () => {
    const r = cycle('1');
    assert.notEqual(r.status, 0);
    assert.ok(Number(readFileSync(join(dir, 'count'), 'utf8')) <= 3);
  });
  mkdirSync(join(dir, 'scripts')); mkdirSync(join(dir, 'worker'));
  put('package.json', '{"type":"module"}');
  put('scripts/live_orders.mjs', readFileSync(new URL('./live_orders.mjs', import.meta.url), 'utf8'));
  put('worker/board_kinds.js', readFileSync(new URL('../worker/board_kinds.js', import.meta.url), 'utf8'));
  put('scripts/wasm_witness.mjs', `export async function witness() { return { engine: 'fixture-engine', row: async () => ({score:1,metric:'kpm',work:1}), close: async () => {} }; }`);
  put('scripts/order_credit.mjs', `import {writeFileSync} from 'node:fs';
export async function official(what, body) {
  if (what === 'release') { writeFileSync('released','yes'); return {}; }
  return {tasks: body.states.includes('spot') ? [{question:'fixture-question',state:'spot',request:{record:{},ruler:'fixture-ruler',mode:'base'},demands:[{producer:'sweep',payload:{identity:'fixture-build'}}]}] : []};
}
export async function tell() { writeFileSync('acknowledged','yes'); }`);
  put('scripts/ship_facts.sh', '#!/usr/bin/env bash\necho "fixture: database refused score_witnesses write"\nexit 1\n');
  const r = spawnSync(process.execPath, [join(dir, 'scripts/live_orders.mjs'), 'settle', 'work', '.'], {
    cwd: dir, encoding: 'utf8', timeout: 10000, env: { ...process.env, WITNESS: 'official:fixture' },
  });
  check('shipping failure is nonzero and keeps its database explanation visible', () => {
    assert.notEqual(r.status, 0);
    assert.match(r.stdout + r.stderr, /fixture: database refused score_witnesses write/);
    assert.equal(existsSync(join(dir, 'acknowledged')), false);
  });
  check('shipping failure releases official claims', () => assert.equal(existsSync(join(dir, 'released')), true));
} finally { rmSync(dir, { recursive: true, force: true }); }
console.log(`\n${passed} passed, ${failures} failed`);
if (failures) process.exitCode = 1;
