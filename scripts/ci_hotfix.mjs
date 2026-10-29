// SPDX-License-Identifier: AGPL-3.0-or-later
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const HOTFIX_PROFILES = {
  'factory-sql': {
    paths: ['worker/pg/sync.sql', 'scripts/check_tasks.mjs', 'scripts/check_board_sync.mjs',
      'scripts/check_board_verify.mjs', 'scripts/check_sync_proc.mjs', 'scripts/check_fair.mjs',
      'scripts/check_settler.mjs', 'scripts/check_settler_async.mjs', 'scripts/check_initiator.mjs',
      'scripts/check_moot_checks.mjs', 'scripts/check_forget_past.mjs', 'scripts/check_idle_devices.mjs'],
    groups: ['factory', 'async'],
  },
  ci: {
    paths: ['.github/workflows/ci.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
      'scripts/ci_hotfix.mjs', 'scripts/check_ci_policy.mjs', 'scripts/check_ci_partitions.mjs',
      'scripts/check_ci_hotfix.mjs'],
    groups: ['policy'],
  },
};

export const HOTFIX_CHECKS = {
  policy: ['check_ci_policy', 'check_ci_partitions', 'check_ci_hotfix'],
  factory: ['check_tasks', 'check_board_verify', 'check_board_sync', 'check_sync_proc',
    'check_fair', 'check_settler', 'check_settler_async', 'check_initiator',
    'check_moot_checks', 'check_forget_past', 'check_idle_devices', 'check_contribution',
    'check_ledger', 'check_dials', 'check_clients_doc', 'check_appraise'],
  async: ['check_tasks', 'check_board_verify', 'check_board_sync', 'check_fair'],
};

export function hotfixPlan(paths, { verifiedBase = true, reason = '', event = 'workflow_dispatch', force = false } = {}) {
  if (event !== 'workflow_dispatch' || force) throw new Error('Hotfix CI requires a manual dispatch without full=true');
  if (!verifiedBase || !paths.length) throw new Error('Hotfix CI requires a verified baseline and a nonempty diff');
  if (!reason.trim() || reason.length > 500) throw new Error('Hotfix CI requires a reason of 1–500 characters');
  const profiles = new Set(), reasons = [];
  for (const path of paths) {
    if (/^docs\/.*\.md$/.test(path) && !/^docs\/(CLIENTS\.md$|measurements\/)/.test(path)) {
      reasons.push({ path, kind: 'docs' });
      continue;
    }
    const profile = Object.entries(HOTFIX_PROFILES).find(([, p]) => p.paths.includes(path))?.[0];
    if (!profile) throw new Error(`Outside hotfix coverage: ${path}`);
    profiles.add(profile);
    reasons.push({ path, kind: profile });
  }
  if (!profiles.size) throw new Error('Hotfix CI requires a supported code or check change');
  const groups = new Set(['policy']);
  for (const p of profiles) for (const group of HOTFIX_PROFILES[p].groups) groups.add(group);
  return { scope: 'hotfix', jobs: ['hotfix'], profiles: [...profiles].sort(),
    matrix: { group: [...groups].sort() }, paths: [...paths].sort(), reasons, reason: reason.trim(), verifiedBase: true };
}

export function assertHotfixProof(proof, sha, policy, paths) {
  if (proof?.version !== 1 || proof.sha !== sha || proof.policy !== policy || proof.reused !== false
      || proof.scope !== 'hotfix' || !/^[a-f0-9]{40}$/.test(proof.hotfix?.base || '')) throw new Error('Invalid hotfix proof identity');
  const plan = hotfixPlan(paths, { reason: proof.hotfix.reason });
  for (const key of ['paths', 'profiles', 'matrix']) {
    if (JSON.stringify(proof.hotfix[key]) !== JSON.stringify(plan[key])) throw new Error(`Hotfix proof ${key} does not cover this diff`);
  }
  if (JSON.stringify(proof.jobs) !== JSON.stringify(plan.jobs)) throw new Error('Hotfix proof jobs do not match');
  for (const job of ['plan', 'quick', 'commits', 'coordination', 'hotfix']) {
    if (proof.results?.[job] !== 'success') throw new Error(`Hotfix proof ${job} did not succeed`);
  }
  return plan;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const group = process.argv[2], checks = HOTFIX_CHECKS[group];
  if (!checks) throw new Error(`Unknown hotfix check group: ${group}`);
  const env = { ...process.env, ...(group === 'async' ? { WFSIM_SETTLE_ASYNC: '1', WFSIM_SYNC_PROC: '1' } : {}) };
  for (const name of checks) {
    console.log(`\n${group}: ${name}`);
    const result = spawnSync(process.execPath, [`scripts/${name}.mjs`], { env, stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status || 1);
  }
}
