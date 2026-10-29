// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { assertGate, BROWSER_JOBS, output, policyDigest, PRODUCT_JOBS } from './ci_policy.mjs';
import { assertHotfixProof } from './ci_hotfix.mjs';

export function validProof(proof, sha, policy, full = false, required = []) {
  const complete = jobs => Array.isArray(proof?.jobs) && proof.jobs.length === jobs.length
    && jobs.every(j => proof.jobs.includes(j));
  return proof?.version === 1 && proof.scope !== 'hotfix' && proof.sha === sha && proof.policy === policy
    && proof.reused === false && Array.isArray(proof.jobs)
    && proof.jobs.includes('site') && proof.jobs.every(j => PRODUCT_JOBS.includes(j))
    && new Set(proof.jobs).size === proof.jobs.length
    && (complete(PRODUCT_JOBS) || (!full && complete(BROWSER_JOBS)))
    && required.every(j => proof.jobs.includes(j))
    && proof.jobs.every(j => proof.results?.[j] === 'success')
    && ['quick', 'commits', 'plan'].every(j => proof.results?.[j] === 'success');
}

export function trustedRun(run, { sha, repository, workflow, current, hotfix = false }) {
  return run.id !== current && run.head_sha === sha && run.conclusion === 'success'
    && run.status === 'completed' && ['push', 'workflow_dispatch', 'schedule'].includes(run.event)
    && (hotfix || !run.display_title?.startsWith('hotfix:'))
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

export function findRun({ sha, repository, policy, full, required = [], current = 0 }) {
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
      if (validProof(proof, sha, policy, full, required)) return String(run.id);
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
        const plan = JSON.parse(process.env.CI_PLAN);
        run = findRun({ sha: process.env.GITHUB_SHA, repository: process.env.GITHUB_REPOSITORY,
          policy: policyDigest(), full: false, required: plan.jobs, current: Number(process.env.GITHUB_RUN_ID) });
      } catch (e) { console.log(`No reusable proof; running selected CI: ${e.message}`); }
    }
    output({ run });
    if (run && process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\nReusing validated coverage and site-built from run ${run}, on this exact SHA.\n`);
  } else if (mode === 'base') {
    console.log(verifiedBase(process.argv[3], process.env.GITHUB_REPOSITORY));
  } else if (mode === 'gate') {
    const plan = JSON.parse(process.env.CI_PLAN);
    const needs = JSON.parse(process.env.CI_NEEDS);
    const reused = Boolean(process.env.REUSE_RUN);
    assertGate(plan, needs, reused);
    writeFileSync('ci-proof.json', JSON.stringify({ version: 1, sha: process.env.GITHUB_SHA,
      policy: policyDigest(), jobs: plan.jobs, reused, source: process.env.REUSE_RUN || null,
      ...(plan.scope === 'hotfix' ? { scope: 'hotfix', hotfix: { base: plan.base, reason: plan.reason,
        paths: plan.paths, profiles: plan.profiles, matrix: plan.matrix } } : {}),
      results: Object.fromEntries(Object.entries(needs).map(([j, n]) => [j, n.result])) }, null, 2));
  } else if (mode === 'hotfix') {
    const [sha, id] = process.argv.slice(3), repository = process.env.GITHUB_REPOSITORY;
    if (!/^[a-f0-9]{40}$/.test(sha || '') || !/^\d+$/.test(id || '') || !repository) throw new Error('Use hotfix <full sha> <run id> with GITHUB_REPOSITORY');
    if (execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() !== sha) throw new Error('Check out the validated hotfix commit first');
    execFileSync('git', ['diff', '--exit-code', '--quiet', 'HEAD']);
    const run = api(`repos/${repository}/actions/runs/${id}`);
    const workflow = api(`repos/${repository}/actions/workflows/ci.yml`).id;
    if (!trustedRun(run, { sha, repository, workflow, current: 0, hotfix: true })
        || !run.display_title?.startsWith('hotfix:')) throw new Error('Not a successful hotfix CI run on this exact commit');
    const artifacts = api(`repos/${repository}/actions/runs/${id}/artifacts?per_page=100`).artifacts;
    if (!artifacts.some(a => a.name === 'ci-proof' && !a.expired)) throw new Error('Hotfix proof is missing or expired');
    const dir = mkdtempSync(join(tmpdir(), 'wfsim-hotfix-proof-'));
    try {
      gh(['run', 'download', id, '--repo', repository, '--name', 'ci-proof', '--dir', dir]);
      const proof = JSON.parse(readFileSync(join(dir, 'ci-proof.json'), 'utf8'));
      if (!/^[a-f0-9]{40}$/.test(proof.hotfix?.base || '') || !verifiedBase(proof.hotfix.base, repository)) throw new Error('Hotfix baseline is not verified');
      const paths = execFileSync('git', ['diff', '--name-only', '--no-renames', '-z', proof.hotfix?.base || '', sha], { encoding: 'utf8' }).split('\0').filter(Boolean);
      const plan = assertHotfixProof(proof, sha, policyDigest(), paths);
      console.log(JSON.stringify({ run: id, sha, base: proof.hotfix.base, profiles: plan.profiles, scope: plan.scope }));
    } finally { rmSync(dir, { recursive: true, force: true }); }
  } else if (mode === 'ship') {
    // A successful docs-only or PR run is never a source of release artifacts.
    const sha = process.argv[3];
    const run = findRun({ sha, repository: process.env.GITHUB_REPOSITORY, policy: policyDigest(), full: false });
    console.log(run);
  } else throw new Error('Use reuse, base <sha>, gate, hotfix <sha> <run id> or ship <sha>');
}
