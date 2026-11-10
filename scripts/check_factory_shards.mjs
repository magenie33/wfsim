// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { FACTORY_CHECKS, factoryCases, factoryCommand, factoryMatrix, factorySelection,
  assertFactoryReceipt, assertFactoryCoverage } from './factory_shards.mjs';

const args = process.argv.slice(2), paths = args.filter(arg => !arg.startsWith('trials=') && !arg.startsWith('shards='));
const options = Object.fromEntries(args.filter(arg => /^(trials|shards)=/.test(arg)).map(arg => {
  const [key, value] = arg.split('='); return [key, Number(value)];
}));
if (paths.length) {
  const files = paths.flatMap(path => statSync(path).isDirectory()
    ? readdirSync(path, { recursive: true }).filter(name => name.endsWith('.json')).map(name => join(path, name)) : [path]);
  const summary = assertFactoryCoverage(files.map(path => JSON.parse(readFileSync(path, 'utf8'))), options);
  console.log('factory coverage: ' + JSON.stringify(summary));
} else {
  assert.equal(args.length, 0, 'coverage options require actual receipts');
  const cases = factoryCases();
  const family = name => cases.filter(row => row.family === name);
  assert.deepEqual(family('liveness').map(row => row.seed), Array.from({ length: 400 }, (_, i) => i + 1));
  assert.deepEqual(family('termination').map(row => row.seed), Array.from({ length: 100 }, (_, i) => i + 10000));
  assert.equal(family('regressions').length, 35, 'fixed factory groups stay registered');
  const matrix = factoryMatrix();
  assert.equal(matrix.length, 18);
  const plans = matrix.map(row => factorySelection(factoryCommand(row).command.slice(2), FACTORY_CHECKS.modes[row.mode]));
  for (const mode of Object.keys(FACTORY_CHECKS.modes)) {
    const selected = plans.filter(row => row.mode === mode).flatMap(row => row.cases.map(item => item.id));
    assert.equal(new Set(selected).size, selected.length);
    assert.deepEqual([...selected].sort(), cases.map(row => row.id).sort());
  }
  for (const input of [['trials=NaN'], ['trials=-1'], ['group=missing'], ['group=random', 'shard=8/8'],
    ['group=random', 'shard=-1/8'], ['group=regressions', 'shard=0/8'], ['trials=4', 'trials=8']]) {
    assert.throws(() => factorySelection(input));
  }
  assert.throws(() => factoryMatrix({ modes: ['sync', 'sync'] }));
  assert.throws(() => factoryMatrix({ modes: ['missing'] }));
  const receipts = plans.map(plan => ({ ...plan, success: true, failures: 0,
    completed: plan.cases.map(row => ({ id: row.id, ok: true, checks: 0, ms: 1 })) }));
  assert.deepEqual(assertFactoryCoverage(receipts), { jobs: 18, modes: ['sync', 'async'], cases: 1070 });
  const badCase = structuredClone(receipts[0]);
  badCase.completed[0].ok = false;
  assert.throws(() => assertFactoryReceipt(badCase, plans[0]), /did not pass/);
  const stale = structuredClone(receipts[0]); stale.source[FACTORY_CHECKS.command[1]] = 'stale';
  assert.throws(() => assertFactoryReceipt(stale, plans[0]), /this source/);
  const oldRevision = structuredClone(receipts[0]); oldRevision.sha = '0'.repeat(40);
  assert.throws(() => assertFactoryReceipt(oldRevision, plans[0]), /wrong sha/);
  assert.throws(() => assertFactoryCoverage(receipts, { sha: '0'.repeat(40) }), /wrong sha/);
  const dir = mkdtempSync(join(tmpdir(), 'factory-coverage-'));
  try {
    const mutations = {
      missing_case: rows => rows[0].completed.pop(),
      duplicate_case: rows => rows[0].completed.push(rows[0].completed[0]),
      missing_shard: rows => rows.splice(0, 1),
      duplicate_shard: rows => rows.push(rows[0]),
      missing_mode: rows => rows.splice(0, 9),
      wrong_mode: rows => { rows[0].mode = 'async'; },
    };
    for (const [name, mutate] of Object.entries(mutations)) {
      const rows = structuredClone(receipts); mutate(rows);
      const path = join(dir, name + '.json');
      const files = rows.map((row, i) => {
        const file = path + '.' + i; writeFileSync(file, JSON.stringify(row)); return file;
      });
      const result = spawnSync(process.execPath, [resolve('scripts/check_factory_shards.mjs'), ...files], { encoding: 'utf8' });
      assert.notEqual(result.status, 0, `${name} must fail the actual coverage CLI`);
      assert.match(result.stderr, /factory/);
      console.log(`ok: actual coverage CLI rejects ${name}`);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
  console.log('factory partitions preserve 400 liveness seeds, 100 termination seeds and 35 fixed groups in each mode');
}
