// SPDX-License-Identifier: AGPL-3.0-or-later
// A successful grader exit does not establish an exhaustive, settled reference.
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const pool = ['serration', 'split_chamber', 'point_strike', 'vital_sense',
  'rime_rounds', 'thermite_rounds', 'high_voltage', 'malignant_force'];

function verdict(output, status) {
  assert.equal(status, 0, 'wfsim-truth failed');
  assert(!output.includes('!!'), 'unsettled or unmatched truth report');
  const scope = output.match(/^\[scope\] (\d+) builds x arcanes = (\d+) jobs \((\d+) distinct to the fight\), exhaustive$/m);
  const reference = output.match(/^\[reference\] (\d+) runs each = (\d+) sims \| answer set (\d+) builds \| settled across seeds: (true|false) \| top-(\d+) overlap ([\d.]+)$/m);
  const search = output.match(/^\[search\] rank (\d+) \| regret ([\d.]+)% \| within noise: (true|false) \| top-(\d+) recall ([\d.]+)% \|/m);
  assert(scope && reference && search, 'incomplete truth report');
  const jobs = Number(scope[2]), distinct = Number(scope[3]), answers = Number(reference[3]);
  assert(jobs > 10 && jobs <= 1000 && distinct > 10 && distinct <= jobs, 'invalid exhaustive scope');
  assert(answers > 0 && answers * 3 < distinct, 'reference cannot distinguish its winner');
  assert.equal(Number(reference[1]), 200, 'reference precision changed');
  assert.equal(reference[4], 'true', 'reference seeds did not settle');
  assert.equal(Number(search[1]), 1, 'search missed the reference winner');
  assert.equal(Number(search[2]), 0, 'reference winner has nonzero regret');
  assert.equal(search[3], 'true', 'winner lies outside the reference answer set');
  return { jobs, distinct, answers, settled: true, rank: 1, within_noise: true,
    reference_top: Number(reference[5]), reference_overlap: Number(reference[6]),
    search_top: Number(search[4]), top_recall_percent: Number(search[5]) };
}

const good = '[scope] 209 builds x arcanes = 209 jobs (129 distinct to the fight), exhaustive\n'
  + '[reference] 200 runs each = 51600 sims | answer set 1 builds | settled across seeds: true | top-5 overlap 1.00\n'
  + '[search] rank 1 | regret 0.000% | within noise: true | top-5 recall 100% | 100 sims (1.0% of the reference)\n';
verdict(good, 0);
for (const bad of [good.replace('seeds: true', 'seeds: false'), good.replace('rank 1', 'rank 2'),
  good.replace('noise: true', 'noise: false'), good.replace('answer set 1', 'answer set 129'),
  good.replace('200 runs', '20 runs'), good.replace('= 209 jobs', '= 2000 jobs'),
  good.replace('regret 0.000', 'regret 1.000'), good + '!! 1 finalists were not in the exhaustive list', '']) {
  assert.throws(() => verdict(bad, 0));
}
assert.throws(() => verdict(good, 1));
console.log('PASS element truth verdict rejects unsettled, unmatched, degenerate and inaccurate reports');

if (!process.argv.includes('--self-test')) {
  const folder = join(root, 'target/element-truth');
  mkdirSync(folder, { recursive: true });
  const receipts = [];
  for (const weapon of ['verglas_prime', 'braton_prime']) {
    const args = ['run', '-p', 'wfsim-webapi', '--bin', 'wfsim-truth', '--',
      `weapon=${weapon}`, `pool=${pool.join(',')}`, 'min=4', 'size=4', 'strategy=quick',
      'candidate_runs=10', 'runs=50', 'truth_runs=200', 'level=1000', 'duration=60',
      'finalists=5', 'max_jobs=1000', 'threads=2'];
    const result = spawnSync('cargo', args, { cwd: root, encoding: 'utf8', maxBuffer: 4 << 20 });
    const stdout = result.stdout ?? '', stderr = result.stderr ?? '';
    writeFileSync(join(folder, `${weapon}.stdout.log`), stdout);
    writeFileSync(join(folder, `${weapon}.stderr.log`), stderr);
    process.stdout.write(stdout);
    process.stderr.write(stderr);
    if (result.error) throw result.error;
    receipts.push({ weapon, source: process.env.GITHUB_SHA ?? null, command: ['cargo', ...args],
      ...verdict(stdout, result.status) });
    writeFileSync(join(folder, 'verdict.json'), JSON.stringify(receipts, null, 2) + '\n');
  }
  console.log('PASS four-element truth quality', JSON.stringify(receipts));
}
