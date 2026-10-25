// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { assertGate, BROWSER_JOBS, output, policyDigest, PRODUCT_JOBS } from './ci_policy.mjs';

export function validProof(proof, sha, policy, full = false) {
  const complete = jobs => Array.isArray(proof?.jobs) && proof.jobs.length === jobs.length
    && jobs.every(j => proof.jobs.includes(j));
  return proof?.version === 1 && proof.sha === sha && proof.policy === policy
    && proof.reused === false && Array.isArray(proof.jobs)
    && proof.jobs.includes('site') && proof.jobs.every(j => PRODUCT_JOBS.includes(j))
    && new Set(proof.jobs).size === proof.jobs.length
    && (complete(PRODUCT_JOBS) || (!full && complete(BROWSER_JOBS)))
    && proof.jobs.every(j => proof.results?.[j] === 'success')
    && ['quick', 'commits', 'plan'].every(j => proof.results?.[j] === 'success');
}

export function trustedRun(run, { sha, repository, workflow, current }) {
  return run.id !== current && run.head_sha === sha && run.conclusion === 'success'
    && run.status === 'completed' && ['push', 'workflow_dispatch', 'schedule'].includes(run.event)
    && run.head_repository?.full_name === repository && run.workflow_id === workflow;
}

function gh(args) {
  return execFileSync('gh', args, { encoding: 'utf8', timeout: 60_000, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}
const api = path => JSON.parse(gh(['api', path]));

export function verifiedBase(base, repository, dependencies = {}) {
  const query = dependencies.api || api;
  const git = dependencies.git || (args => execFileSync('git', args, { encoding: 'utf8' }).trim());
  // Release and board snapshots add no source; verify their last source commit.
  const sha = git(['log', '-1', '--format=%H', base, '--', '.', ':!site',
    ':!desktop/payload', ':!mcp/engine.js', ':!data/board_state.yaml']);
  if (!sha) return false;
  const workflow = query(`repos/${repository}/actions/workflows/ci.yml`).id;
  const runs = query(`repos/${repository}/actions/workflows/${workflow}/runs?head_sha=${sha}&status=success&per_page=100`).workflow_runs;
  return runs.some(run => trustedRun(run, { sha, repository, workflow, current: 0 }));
}

export function findRun({ sha, repository, policy, full, current = 0 }) {
  const workflow = api(`repos/${repository}/actions/workflows/ci.yml`).id;
  const runs = api(`repos/${repository}/actions/workflows/${workflow}/runs?head_sha=${sha}&status=success&per_page=100`).workflow_runs;
  for (const run of runs) {
    if (!trustedRun(run, { sha, repository, workflow, current })) continue;
    const artifacts = api(`repos/${repository}/actions/runs/${run.id}/artifacts?per_page=100`).artifacts;
    if (!['site-built', 'ci-proof'].every(name => artifacts.some(a => a.name === name && !a.expired))) continue;
    const dir = mkdtempSync(join(tmpdir(), 'wfsim-ci-proof-'));
    try {
      gh(['run', 'download', String(run.id), '--repo', repository, '--name', 'ci-proof', '--dir', dir]);
      const proof = JSON.parse(readFileSync(join(dir, 'ci-proof.json'), 'utf8'));
      if (validProof(proof, sha, policy, full)) return String(run.id);
    } catch (e) { console.error(`Ignoring unusable CI proof from ${run.id}: ${e.message}`); }
    finally { rmSync(dir, { recursive: true, force: true }); }
  }
  return '';
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const mode = process.argv[2];
  if (mode === 'reuse') {
    let run = '';
    if (process.env.GITHUB_EVENT_NAME !== 'pull_request' && process.env.FORCE_FULL !== 'true'
      && process.env.GITHUB_EVENT_NAME !== 'schedule') {
      try {
        run = findRun({ sha: process.env.GITHUB_SHA, repository: process.env.GITHUB_REPOSITORY,
          policy: policyDigest(), full: true, current: Number(process.env.GITHUB_RUN_ID) });
      } catch (e) { console.log(`No reusable proof; running selected CI: ${e.message}`); }
    }
    output({ run });
    if (run && process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\nReusing full validation and site-built from run ${run}, on this exact SHA.\n`);
  } else if (mode === 'base') {
    console.log(verifiedBase(process.argv[3], process.env.GITHUB_REPOSITORY));
  } else if (mode === 'gate') {
    const plan = JSON.parse(process.env.CI_PLAN);
    const needs = JSON.parse(process.env.CI_NEEDS);
    const reused = Boolean(process.env.REUSE_RUN);
    assertGate(plan, needs, reused);
    writeFileSync('ci-proof.json', JSON.stringify({ version: 1, sha: process.env.GITHUB_SHA,
      policy: policyDigest(), jobs: plan.jobs, reused, source: process.env.REUSE_RUN || null,
      results: Object.fromEntries(Object.entries(needs).map(([j, n]) => [j, n.result])) }, null, 2));
  } else if (mode === 'ship') {
    // A successful docs-only or PR run is never a source of release artifacts.
    const sha = process.argv[3];
    const run = findRun({ sha, repository: process.env.GITHUB_REPOSITORY, policy: policyDigest(), full: false });
    console.log(run);
  } else throw new Error('Use reuse, base <sha>, gate or ship <sha>');
}
