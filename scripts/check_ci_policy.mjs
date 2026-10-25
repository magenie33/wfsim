// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assertGate, BROWSER_JOBS, changedPaths, classify, PRODUCT_JOBS } from './ci_policy.mjs';
import { trustedRun, validProof } from './ci_runs.mjs';

for (const path of ['engine/src/fight.rs', 'optimizer/src/search.rs', 'webapi/src/optimize.rs',
  'data/weapons/braton.yaml', 'Cargo.lock', '.github/workflows/ci.yml', 'scripts/ci_policy.mjs',
  'new-package/anything', 'desktop/src/main.rs', 'AGENTS.md', 'docs/CLIENTS.md', 'docs/measurements/M001.md']) {
  assert.equal(classify([path]).scope, 'full', path);
}
assert.deepEqual(classify([]).jobs, PRODUCT_JOBS);
assert.deepEqual(classify(['docs/UI.md'], true).jobs, PRODUCT_JOBS);
assert.equal(classify(['docs/UI.md', 'README.md']).jobs.length, 0);
for (const path of ['web/src/static/style.css', 'data/i18n/zh/ui.yaml', 'worker/pg/schema.sql', 'bot/engine.mjs', 'mcp/index.js']) {
  const p = classify([path]);
  assert(p.jobs.includes('site') && p.jobs.includes('pipeline') && p.jobs.includes('parity'), path);
  assert(!p.jobs.includes('test') && !p.jobs.includes('gas'), path);
}
assert.equal(classify(['web/src/static/style.css', 'engine/src/fight.rs']).scope, 'full');
assert.equal(classify(['data/board_state.yaml']).scope, 'snapshot');
assert(!classify(['site/asset/app.123.js']).jobs.includes('site'));
assert(classify(['site/asset/app.123.js', 'worker/index.js']).jobs.includes('site'));
assert.equal(classify(['data/board_state.yaml', 'worker/index.js']).scope, 'full');
assert.equal(classify(['docs/UI.md', 'data/board_state.yaml']).scope, 'snapshot');

const needs = plan => Object.fromEntries(['plan', 'quick', 'commits', ...PRODUCT_JOBS].map(j =>
  [j, { result: ['plan', 'quick', 'commits', ...plan.jobs].includes(j) ? 'success' : 'skipped' }]));
for (const plan of [classify(['docs/UI.md']), classify(['worker/index.js']), classify(['engine/src/lib.rs'])]) {
  assertGate(plan, needs(plan));
  for (const job of ['plan', 'quick', 'commits', ...plan.jobs]) {
    for (const result of ['failure', 'cancelled', 'skipped', undefined]) {
      const n = needs(plan);
      n[job].result = result;
      assert.throws(() => assertGate(plan, n), undefined, `${job} ${result} must block`);
    }
  }
}
const full = classify([], true);
const reusedNeeds = needs(classify(['docs/UI.md']));
assertGate(full, reusedNeeds, true);
assert.throws(() => assertGate(full, reusedNeeds));
assert.throws(() => assertGate(full, { ...needs(full), coordination: { result: 'failure' } }));
const proof = { version: 1, sha: 'abc', policy: 'p', reused: false, jobs: PRODUCT_JOBS,
  results: Object.fromEntries(['quick', 'commits', 'plan', ...PRODUCT_JOBS].map(j => [j, 'success'])) };
assert(validProof(proof, 'abc', 'p', true));
for (const bad of [{ ...proof, sha: 'other' }, { ...proof, policy: 'old' }, { ...proof, reused: true },
  { ...proof, jobs: ['site'] }, { ...proof, results: {} }, { ...proof, version: 0 }]) {
  assert(!validProof(bad, 'abc', 'p', true));
}
assert(!validProof({ ...proof, jobs: [] }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: ['site'] }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: PRODUCT_JOBS.filter(j => j !== 'test') }, 'abc', 'p'));
assert(validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', true));
const run = { id: 1, head_sha: 'abc', conclusion: 'success', status: 'completed', event: 'workflow_dispatch',
  head_repository: { full_name: 'owner/repo' }, workflow_id: 9 };
const context = { sha: 'abc', repository: 'owner/repo', workflow: 9, current: 2 };
assert(trustedRun(run, context));
for (const patch of [{ id: 2 }, { head_sha: 'other' }, { conclusion: 'failure' }, { status: 'in_progress' },
  { event: 'pull_request' }, { workflow_id: 8 }, { head_repository: { full_name: 'fork/repo' } }]) {
  assert(!trustedRun({ ...run, ...patch }, context), JSON.stringify(patch));
}

const dir = mkdtempSync(join(tmpdir(), 'wfsim-ci-diff-'));
try {
  const git = args => execFileSync('git', args, { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git(['init', '-b', 'main']);
  git(['config', 'user.name', 'fixture']);
  git(['config', 'user.email', 'fixture@example.invalid']);
  writeFileSync(join(dir, 'mechanics.rs'), 'fn main() {}');
  git(['add', '.']); git(['commit', '-m', 'fixture']);
  const base = git(['rev-parse', 'HEAD']);
  git(['mv', 'mechanics.rs', 'README.md']); git(['commit', '-am', 'rename']);
  const paths = changedPaths({ before: base }, { GITHUB_EVENT_NAME: 'push' }, git);
  assert.deepEqual(new Set(paths), new Set(['mechanics.rs', 'README.md']));
  assert.equal(classify(paths).scope, 'full', 'a rename out of code still tests code');
  assert.deepEqual(changedPaths({ pull_request: { base: { sha: base } } }, { GITHUB_EVENT_NAME: 'pull_request' }, git), paths);
} finally { rmSync(dir, { recursive: true, force: true }); }

const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
const gate = workflow.slice(workflow.indexOf('\n  ci-required:'));
assert.match(gate, /if:.*always\(\)/);
const jobIds = [...workflow.slice(workflow.indexOf('\njobs:\n')).matchAll(/^  ([\w-]+):$/gm)]
  .map(m => m[1]).filter(j => j !== 'ci-required');
const gateJobs = gate.match(/needs: \[([^\]]+)\]/)[1].split(',').map(j => j.trim());
assert.deepEqual(new Set(gateJobs), new Set(jobIds), 'every workflow job must reach the required gate');
assert.match(workflow, /cancel-in-progress:.*refs\/heads\/main/);
assert(!workflow.includes("github.event_name != 'pull_request'"), 'PRs build the current site');
assert.match(workflow, /node scripts\/check_ci_policy\.mjs/);
console.log('CI selection, failed/cancelled gates, exact-SHA trust and rename coverage hold.');
