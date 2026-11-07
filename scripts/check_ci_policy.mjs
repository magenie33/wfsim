// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import './check_ci_fast.mjs';
import { FAST_JOBS, fastJobs } from './ci_fast.mjs';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ALL_CI_JOBS, assertGate, BROWSER_JOBS, browserCheckPaths, changedPaths, checkJobPaths,
  classify, CONSUMER_JOBS, OPTIONAL_NATIVE_JOBS, PRODUCT_JOBS } from './ci_policy.mjs';
import { trustedRun, validProof, verifiedBase } from './ci_runs.mjs';

for (const path of ['engine/src/fight.rs', 'optimizer/src/search.rs', 'webapi/src/optimize.rs',
  'data/weapons/braton.yaml', 'Cargo.lock', '.github/workflows/ci.yml', 'scripts/ci_policy.mjs',
  'new-package/anything', 'desktop/src/main.rs', 'AGENTS.md', 'docs/CLIENTS.md', 'docs/measurements/M001.md']) {
  assert.equal(classify([path]).scope, 'full', path);
}
assert.deepEqual(classify([]).jobs, PRODUCT_JOBS);
assert.deepEqual(classify(['docs/UI.md'], true).jobs, PRODUCT_JOBS);
assert.equal(classify(['docs/UI.md', 'README.md']).jobs.length, 0);
for (const paths of [['docs/UI.md'], ['worker/index.js'], ['web/src/static/style.css'], ['site/release.json']]) {
  assert.deepEqual(classify(paths, false, false).jobs, PRODUCT_JOBS, 'an unverified base cannot justify skipping tests');
}
for (const path of ['web/src/static/style.css', 'data/i18n/zh/ui.yaml', 'worker/pg/schema.sql']) {
  const p = classify([path]);
  assert(p.jobs.includes('site') && p.jobs.includes('pipeline') && p.jobs.includes('parity'), path);
  assert(!p.jobs.includes('test') && !p.jobs.includes('gas'), path);
}
for (const [kind, paths] of Object.entries({ mcp: ['mcp/index.js', 'mcp/account.js', 'scripts/check_mcp_auth.mjs',
  'scripts/check_mcp_entry.mjs', 'scripts/mcp_test_loader.mjs'], bot: ['bot/engine.mjs', 'scripts/check_bot_auth.mjs'] })) {
  for (const path of paths) {
    assert.deepEqual(classify([path]).jobs, CONSUMER_JOBS[kind], path);
    assert.deepEqual(classify([path, 'web/src/static/style.css']).jobs, BROWSER_JOBS);
    assert.deepEqual(classify([path, 'engine/src/lib.rs']).jobs, PRODUCT_JOBS);
    for (const pair of [[path, 'site/release.json'], ['site/release.json', path]]) {
      assert.deepEqual(classify(pair).jobs, PRODUCT_JOBS, 'snapshot mixing cannot narrow validation');
    }
  }
}
assert.deepEqual(new Set(classify(['mcp/index.js', 'bot/engine.mjs']).jobs), new Set(['site', 'checks', 'mcp', 'bot']));
assert.deepEqual(new Set(classify(['mcp/headless.js']).jobs), new Set(['site', 'checks', 'mcp', 'bot']));
assert.deepEqual(classify(['mcp/new_shared_input.js']).jobs, PRODUCT_JOBS, 'unknown MCP dependencies fail closed');
assert.equal(classify(['engine/src/lib.rs']).deep, false);
const ciWiring = ['.github/workflows/ci.yml', '.github/workflows/ship.yml',
  '.github/actions/site-tools/action.yml', '.github/actions/site-tools/install.sh',
  '.github/actions/built-site/action.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
  'scripts/check_ci_policy.mjs', 'scripts/ci_hotfix.mjs', 'scripts/check_ci_hotfix.mjs',
  'scripts/check_ci_partitions.mjs', 'scripts/check_site_setup.mjs'];
const nonSearchTools = ['scripts/check_weapon_search.mjs', 'scripts/fetch_inbox.sh',
  'scripts/live_board.sh', 'scripts/ship_builds.sh', 'scripts/retire_scores.mjs', 'scripts/ship_queue.sh',
  'scripts/fetch_facts.sh', 'scripts/ship_facts.sh', 'scripts/live_publish.mjs', '.github/workflows/queue.yml',
  'deploy/factoryd_grants.sql', 'scripts/check_rescore_paths.mjs', 'scripts/capture_compute_inventory.mjs',
  'scripts/build_batches.mjs', 'scripts/ship_family.mjs', 'scripts/ship_bot.py', '.github/workflows/scores.yml',
  'scripts/factoryd.mjs', 'scripts/pg_task_archive.py', 'scripts/ship_task_archive.py', 'scripts/check_task_archive.py',
  'deploy/wfsim-task-archive.service', 'deploy/wfsim-task-archive.timer'];
for (const path of [...ciWiring, ...nonSearchTools]) {
  assert.equal(classify([path]).scope, 'full', path);
  assert.equal(classify([path]).deep, false, `${path} does not change search behavior`);
  assert(classify([path, 'optimizer/src/search.rs']).deep, 'CI wiring cannot hide search changes');
  assert(classify([path], true).deep, 'explicit full CI still runs every fixture');
  assert(classify([path], false, false).deep, 'unverified baselines retain deep coverage');
}
assert.equal(classify(['engine/src/board/equipment.rs', 'webapi/src/lib.rs',
  'scripts/check_equipment_library.mjs', '.github/workflows/ci.yml',
  'scripts/check_weapon_search.mjs', 'scripts/fetch_inbox.sh', 'scripts/live_board.sh', 'scripts/ship_builds.sh']).deep, false,
  'equipment-library checks and workflow wiring do not force full-catalog search');
for (const path of ['Cargo.toml', 'Cargo.lock', 'scripts/ci_rust.mjs',
  'scripts/check_future.mjs', '.github/workflows/unknown.yml']) assert(classify([path]).deep, path);
for (const paths of [[], ['optimizer/src/quick.rs'], ['webapi/src/optimize.rs'], ['unknown/file']]) assert(classify(paths).deep);
assert(classify(['mcp/index.js'], true).deep);
for (const path of ['scripts/check_kitgun_slot.mjs', 'scripts/check_riven_request.mjs']) {
  assert.deepEqual(classify([path]).jobs, BROWSER_JOBS, path);
  assert.deepEqual(classify([path], false, false).jobs, PRODUCT_JOBS);
  assert.deepEqual(classify([path], true).jobs, PRODUCT_JOBS);
  assert.equal(classify([path, 'engine/src/lib.rs']).scope, 'full');
}
for (const path of ['scripts/check_future.mjs', 'scripts/check_gas.mjs', 'scripts/check_lane_memory.mjs',
  'scripts/check_ci_partitions.mjs', 'scripts/check_assembly.mjs', 'scripts/cdp.mjs', 'scripts/build_site_app.py']) {
  assert.equal(classify([path]).scope, 'full', path);
}
assert.equal(classify(['web/src/static/style.css', 'engine/src/fight.rs']).scope, 'full');
assert.equal(classify(['data/board_state.yaml']).scope, 'snapshot');
assert(!classify(['site/asset/app.123.js']).jobs.includes('site'));
assert(classify(['site/asset/app.123.js', 'worker/index.js']).jobs.includes('site'));
assert.equal(classify(['data/board_state.yaml', 'worker/index.js']).scope, 'full');
assert.equal(classify(['docs/UI.md', 'data/board_state.yaml']).scope, 'snapshot');

const needs = plan => Object.fromEntries(['plan', 'quick', 'commits', ...ALL_CI_JOBS].map(j =>
  [j, { result: ['plan', 'quick', 'commits', ...plan.jobs].includes(j) ? 'success' : 'skipped' }])
  .concat(OPTIONAL_NATIVE_JOBS.map(j => [j, { result: 'skipped' }])));
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
for (const name of OPTIONAL_NATIVE_JOBS) {
  for (const result of ['success', 'failure', 'skipped', 'cancelled']) assertGate(full, { ...needs(full), [name]: { result } });
  const missing = needs(full);
  delete missing[name];
  assert.throws(() => assertGate(full, missing));
}
const proof = { version: 1, sha: 'abc', policy: 'p', reused: false, deep: true, jobs: PRODUCT_JOBS,
  results: Object.fromEntries(['quick', 'commits', 'plan', ...PRODUCT_JOBS].map(j => [j, 'success'])) };
assert(validProof(proof, 'abc', 'p', true));
const fastProof = { ...proof, scope: 'fast', jobs: FAST_JOBS, fast: { base: 'a'.repeat(40), paths: [], reason: 'maintenance' } };
assert(validProof(fastProof, 'abc', 'p'));
assert(!validProof(fastProof, 'abc', 'p', true));
assert(!validProof({ ...fastProof, fast: null }, 'abc', 'p'));
assert(!validProof({ ...proof, fast: fastProof.fast }, 'abc', 'p'));
assert(validProof(fastProof, 'abc', 'p', false, PRODUCT_JOBS, true));
assert(!validProof({ ...fastProof, jobs: FAST_JOBS.slice(1) }, 'abc', 'p'));
const parentFast = { ...fastProof, fast: { ...fastProof.fast, paths: ['engine/src/board/builds/identity.rs'] } };
assert(!validProof(parentFast, 'abc', 'p'), 'parent repair proof cannot omit gas/native prerequisites');
assert(validProof({ ...parentFast, jobs: fastJobs(parentFast.fast.paths) }, 'abc', 'p'));
assert(!validProof({ ...parentFast, jobs: fastJobs(parentFast.fast.paths),
  results: { ...proof.results, gas: 'failure' } }, 'abc', 'p'));
for (const bad of [{ ...proof, sha: 'other' }, { ...proof, policy: 'old' }, { ...proof, reused: true },
  { ...proof, jobs: ['site'] }, { ...proof, results: {} }, { ...proof, version: 0 }]) {
  assert(!validProof(bad, 'abc', 'p', true));
}
assert(!validProof({ ...proof, jobs: [] }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: ['site'] }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: PRODUCT_JOBS.filter(j => j !== 'test') }, 'abc', 'p'));
assert(validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', true));
assert(validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', false, BROWSER_JOBS));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', false, ['test']));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', false, ['published-board']));
assert(validProof(proof, 'abc', 'p', false, PRODUCT_JOBS));
assert(!validProof({ ...proof, deep: false }, 'abc', 'p', true));
assert(!validProof({ ...proof, deep: false }, 'abc', 'p', false, PRODUCT_JOBS, true));
for (const jobs of Object.values(CONSUMER_JOBS)) {
  assert(validProof({ ...proof, jobs }, 'abc', 'p'));
  assert(!validProof({ ...proof, jobs }, 'abc', 'p', false, ['page']));
}
assert(!validProof({ ...proof, jobs: CONSUMER_JOBS.mcp }, 'abc', 'p', false, CONSUMER_JOBS.bot));
assert(!validProof({ ...proof, jobs: ['site', 'checks'] }, 'abc', 'p'));
const run = { id: 1, head_sha: 'abc', conclusion: 'success', status: 'completed', event: 'workflow_dispatch',
  head_repository: { full_name: 'owner/repo' }, workflow_id: 9 };
const context = { sha: 'abc', repository: 'owner/repo', workflow: 9, current: 2 };
assert(trustedRun(run, context));
for (const patch of [{ id: 2 }, { head_sha: 'other' }, { conclusion: 'failure' }, { status: 'in_progress' },
  { event: 'pull_request' }, { display_title: 'hotfix: task' }, { workflow_id: 8 }, { head_repository: { full_name: 'fork/repo' } }]) {
  assert(!trustedRun({ ...run, ...patch }, context), JSON.stringify(patch));
}
const baseline = candidate => ({
  git: args => {
    assert(args.includes(':!site') && args.includes(':!data/board_state.yaml'));
    return 'abc';
  },
  api: path => path.endsWith('/ci.yml') ? { id: 9 } : { workflow_runs: candidate ? [candidate] : [] },
});
assert(verifiedBase('snapshot', 'owner/repo', baseline(run)));
for (const bad of [null, { ...run, conclusion: 'failure' }, { ...run, event: 'pull_request' },
  { ...run, display_title: 'hotfix: task' }, { ...run, head_sha: 'old' }]) {
  assert(!verifiedBase('snapshot', 'owner/repo', baseline(bad)));
}
assert.throws(() => verifiedBase('snapshot', 'owner/repo', {
  ...baseline(run), api: () => { throw new Error('API unavailable'); },
}));

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
const browserChecks = browserCheckPaths(workflow);
assert(browserChecks.size > 0);
for (const path of browserChecks) {
  const jobs = [...checkJobPaths(workflow).get(path)];
  const expected = jobs.every(j => j === 'mcp') ? CONSUMER_JOBS.mcp
    : jobs.every(j => j === 'bot') ? CONSUMER_JOBS.bot : BROWSER_JOBS;
  assert.deepEqual(classify([path]).jobs, expected, path);
}
assert.deepEqual([...browserCheckPaths('jobs:\n  gas:\n    run: node scripts/check_gas.mjs\n  page:\n    run: node scripts/check_example.mjs\n    run: node scripts/check_gas.mjs\n  quick:\n    run: node scripts/check_quick.mjs\n')],
  ['scripts/check_example.mjs']);
assert.deepEqual([...browserCheckPaths('jobs:\n  page:\n    # node scripts/check_comment.mjs\n    - name: node scripts/check_title.mjs\n      run: |\n        # node scripts/check_disabled.mjs\n        node scripts/check_example.mjs\n  gas:\n    - run: |\n        MODE=build node scripts/check_example.mjs\n')], []);
const gate = workflow.slice(workflow.indexOf('\n  ci-required:'));
assert.match(gate, /if:.*always\(\)/);
const jobIds = [...workflow.slice(workflow.indexOf('\njobs:\n')).matchAll(/^  ([\w-]+):$/gm)]
  .map(m => m[1]).filter(j => j !== 'ci-required');
const gateJobs = gate.match(/needs: \[([^\]]+)\]/)[1].split(',').map(j => j.trim());
assert.deepEqual(new Set(gateJobs), new Set(jobIds), 'every workflow job must reach the required gate');
assert.match(workflow, /cancel-in-progress:.*refs\/heads\/main/);
assert(!workflow.split('\n  site:')[1].split('\n  native-mac:')[0].includes("github.event_name != 'pull_request'"), 'PRs build the current site');
assert.match(workflow, /node scripts\/check_ci_policy\.mjs/);
const pipeline = workflow.split('\n  pipeline:')[1].split('\n  page:')[0];
assert.match(pipeline, /needs: \[plan, quick\]/);
assert(!pipeline.includes('built-site'), 'SQL and protocol checks must not wait for artwork');
const services = workflow.split('\n  services:')[1].split('\n  mcp:')[0];
assert.match(services, /needs: \[plan, quick\]/);
assert(!services.includes('built-site'));
assert.match(workflow, /- id: scope\s+env:\s+GH_TOKEN:.*\n\s+run: node scripts\/ci_policy\.mjs/);
const ship = readFileSync('.github/workflows/ship.yml', 'utf8');
assert.match(ship.split('\njobs:')[0], /actions: read/, 'release evidence needs permission to download CI proofs');
console.log('CI selection, failed/cancelled gates, exact-SHA trust and rename coverage hold.');
