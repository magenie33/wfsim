// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { ALL_CI_JOBS, assertGate, PRODUCT_JOBS } from './ci_policy.mjs';
import { assertHotfixProof, HOTFIX_CHECKS, HOTFIX_PROFILES, hotfixPlan } from './ci_hotfix.mjs';
import { trustedRun, validProof } from './ci_runs.mjs';

const reason = 'Devices finish their packs and time out while asking for the next';
const paths = ['worker/pg/sync.sql', 'scripts/check_tasks.mjs', 'docs/CHECKS.md'];
const plan = hotfixPlan(paths, { reason });
assert.deepEqual(plan.profiles, ['factory-sql']);
assert.deepEqual(plan.matrix.group, ['async', 'factory', 'policy']);
assert.deepEqual(hotfixPlan(['scripts/ci_runs.mjs'], { reason }).matrix.group, ['policy']);
assert.deepEqual(hotfixPlan([...paths, 'scripts/ci_runs.mjs'], { reason }).profiles, ['ci', 'factory-sql']);
for (const path of ['engine/src/fight.rs', 'optimizer/src/quick.rs', 'webapi/src/fight.rs',
  'data/weapons/torid.yaml', 'web/src/static/app/69-board-work.js', 'worker/index.js',
  'worker/pg/schema.sql', 'scripts/pg/test_db.mjs', 'scripts/pg/package-lock.json',
  '.github/workflows/ship.yml', 'site/release.json', 'docs/CLIENTS.md', 'docs/measurements/M001.md',
  'scripts/new-check.mjs', 'desktop/src/main.rs']) {
  assert.throws(() => hotfixPlan([...paths, path], { reason }), /Outside hotfix coverage/, path);
}
for (const options of [{ reason: '' }, { reason: ' '.repeat(3) }, { reason: 'x'.repeat(501) },
  { reason, verifiedBase: false }, { reason, force: true }, { reason, event: 'push' }, { reason, event: 'pull_request' }]) {
  assert.throws(() => hotfixPlan(paths, options), undefined, JSON.stringify(options));
}
for (const diff of [[], ['docs/CI.md']]) assert.throws(() => hotfixPlan(diff, { reason }));
for (const profile of Object.values(HOTFIX_PROFILES)) {
  for (const group of profile.groups) assert(HOTFIX_CHECKS[group]?.length, group);
  for (const path of profile.paths) assert(existsSync(path), path);
}
for (const checks of Object.values(HOTFIX_CHECKS)) {
  assert.equal(new Set(checks).size, checks.length);
  for (const check of checks) assert(existsSync(`scripts/${check}.mjs`), check);
}

const needs = Object.fromEntries(['plan', 'quick', 'commits', 'coordination', ...ALL_CI_JOBS]
  .map(job => [job, { result: PRODUCT_JOBS.includes(job) ? 'skipped' : 'success' }]));
assertGate(plan, needs);
for (const job of ['plan', 'quick', 'commits', 'coordination', 'hotfix']) {
  for (const result of ['failure', 'cancelled', 'skipped', undefined]) {
    assert.throws(() => assertGate(plan, { ...needs, [job]: { result } }), undefined, `${job}: ${result}`);
  }
}
assert.throws(() => assertGate(plan, { ...needs, test: { result: 'success' } }));
const sha = 'a'.repeat(40), base = 'b'.repeat(40);
const proof = { version: 1, scope: 'hotfix', sha, policy: 'p', reused: false, jobs: plan.jobs,
  hotfix: { base, reason, paths: plan.paths, profiles: plan.profiles, matrix: plan.matrix },
  results: Object.fromEntries(Object.entries(needs).map(([job, value]) => [job, value.result])) };
assertHotfixProof(proof, sha, 'p', paths);
for (const patch of [{ sha: 'wrong' }, { policy: 'old' }, { scope: 'full' }, { reused: true },
  { jobs: PRODUCT_JOBS }, { results: {} }, { hotfix: { ...proof.hotfix, base: '' } },
  { hotfix: { ...proof.hotfix, profiles: ['ci'] } }, { hotfix: { ...proof.hotfix, matrix: { group: ['policy'] } } }]) {
  assert.throws(() => assertHotfixProof({ ...proof, ...patch }, sha, 'p', paths));
}
assert.throws(() => assertHotfixProof(proof, sha, 'p', [...paths, 'scripts/ci_policy.mjs']));
assert(!validProof(proof, sha, 'p'), 'a hotfix never supplies a site release');
assert(!validProof({ ...proof, jobs: PRODUCT_JOBS }, sha, 'p'), 'a hotfix cannot relabel itself as full coverage');
const run = { id: 1, head_sha: sha, conclusion: 'success', status: 'completed', event: 'workflow_dispatch',
  display_title: 'hotfix: branch', head_repository: { full_name: 'owner/repo' }, workflow_id: 9 };
const context = { sha, repository: 'owner/repo', workflow: 9, current: 0 };
assert(!trustedRun(run, context), 'a hotfix never establishes a regular CI baseline');
assert(trustedRun(run, { ...context, hotfix: true }));
assert(!trustedRun({ ...run, event: 'pull_request' }, { ...context, hotfix: true }));

const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
assert.match(workflow, /run-name:.*inputs\.hotfix.*hotfix:/);
assert.match(workflow, /hotfix:\s+description:.*\n\s+type: boolean\n\s+default: false/);
assert.match(workflow, /FORCE_FULL:.*inputs\.full \|\| inputs\.hotfix/);
assert.match(workflow, /hotfix:\n\s+needs: \[plan, quick\]\n\s+if: needs\.plan\.outputs\.hotfix == 'true'/);
assert.match(workflow, /matrix:.*fromJSON\(needs\.plan\.outputs\.hotfix_matrix\)/);
assert.match(workflow, /run: node scripts\/ci_hotfix\.mjs "\$GROUP"/);
assert.match(workflow, /- run: node scripts\/check_ci_hotfix\.mjs/);
console.log('Hotfix scopes, required checks, failed gates and separate exact-commit evidence hold.');
