// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { FULL_WORK, COMPANION_JOBS, companionJobs } from './full_work.mjs';
import { assertGate, classify, ALL_CI_JOBS, OPTIONAL_NATIVE_JOBS } from './ci_policy.mjs';
import { timingReport } from './ci_metrics.mjs';

assert.deepEqual(FULL_WORK.map(row => row.id), ['validation', 'site', 'desktop', 'live-runtime', 'intake', 'scoring', 'board', 'clients']);
for (const row of FULL_WORK) {
  assert(row.inputs.length && new Set(row.inputs).size === row.inputs.length);
  for (const field of ['invalidate', 'checkpoint', 'budget', 'evidence', 'rollback']) assert(row[field]?.trim(), `${row.id}: ${field}`);
}
assert.deepEqual(companionJobs([]), []);
assert.deepEqual(companionJobs(['services']), ['runtime', 'factory-cases', 'factory']);
assert.deepEqual(companionJobs(['pipeline'], true), ['runtime', 'protocol']);
assert.deepEqual(companionJobs(['site']), ['runtime', 'prepare-desktop', 'prepare-live']);
const plan = classify(['new-core/input']);
const results = Object.fromEntries(['plan', 'quick', 'commits', ...ALL_CI_JOBS, ...COMPANION_JOBS, ...OPTIONAL_NATIVE_JOBS]
  .map(name => [name, { result: ['plan', 'quick', 'commits', ...plan.jobs, ...companionJobs(plan.jobs)].includes(name) ? 'success' : 'skipped' }]));
assertGate(plan, results);
for (const name of COMPANION_JOBS) {
  const missing = structuredClone(results); delete missing[name];
  assert.throws(() => assertGate(plan, missing), undefined, `${name}: missing companion blocks`);
  for (const state of ['failure', 'skipped', 'cancelled']) {
    const failed = structuredClone(results); failed[name].result = state;
    assert.throws(() => assertGate(plan, failed), undefined, `${name}: ${state} blocks`);
  }
}
const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
for (const name of COMPANION_JOBS) assert(workflow.includes(`\n  ${name}:\n`));
for (const job of ['site', 'services', 'pipeline']) {
  const section = workflow.split(`\n  ${job}:\n`)[1].split(/\n  [\w-]+:\n/)[0];
  assert.match(section, /needs: \[plan, quick, runtime\]/, 'production runtime precedes expensive work');
}
const fixture = timingReport({ id: 1, head_sha: 'source', status: 'completed', created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:01:00Z' },
  [{ name: 'test', started_at: '2026-01-01T00:00:10Z', completed_at: '2026-01-01T00:00:50Z',
    steps: [{ name: 'fixtures', started_at: '2026-01-01T00:00:15Z', completed_at: '2026-01-01T00:00:45Z' }] }]);
assert.equal(fixture.elapsed_seconds, 60);
assert.equal(fixture.jobs[0].wait_before_start_seconds, 10);
assert.equal(fixture.jobs[0].run_seconds, 40);
assert.equal(fixture.slowest_steps[0].seconds, 30);
assert.equal(fixture.cache_hits, null);
console.log('Full-work registry, required companions, failed/missing producer gates and timing semantics PASS');
