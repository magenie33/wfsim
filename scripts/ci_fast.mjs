// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';

const PATHS = new Set([
  '.github/workflows/ci.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
  'scripts/ci_fast.mjs', 'scripts/check_ci_fast.mjs', 'scripts/check_ci_policy.mjs',
  'cli/src/board/run.rs', 'cli/src/board/assemble/inventory.rs',
  'web/src/static/app/73-compute.js', 'data/i18n/zh/ui.yaml',
  'worker/clearance.js', 'worker/contribution.js', 'worker/usage_days.js',
  'worker/pg/sync.sql', 'worker/pg/library.sql', 'scripts/factoryd.mjs',
  'scripts/inventory_phase.py', 'scripts/usage.py', 'scripts/check_clearance.mjs',
  'scripts/check_compute_orders.mjs', 'scripts/check_contribution.mjs',
  'scripts/check_factory_capacity.mjs', 'scripts/check_forget_past.mjs',
  'scripts/check_initiator.mjs', 'scripts/check_inventory_memory.mjs',
  'scripts/check_inventory_phase.py', 'scripts/check_usage_events.mjs', 'scripts/check_usage_report.py',
]);
const git = args => execFileSync('git', args, { encoding: 'utf8' });

export function fastScope(paths, { base, verifiedBase, force = false, hotfix = false, reason = '' }) {
  if (!verifiedBase || !/^[a-f0-9]{40}$/.test(base || '') || force || hotfix || !paths.length || !reason.trim())
    throw new Error('Fast CI requires a verified comparison base, reason and ordinary nonempty diff');
  for (const path of paths) {
    if (!PATHS.has(path) && !/^docs\/(?!CLIENTS\.md$|measurements\/).*\.md$/.test(path))
      throw new Error(`Outside fast maintenance coverage: ${path}`);
  }
  return { base, paths: [...paths].sort(), reason: reason.trim() };
}

export function checkFastChanges(scope, head = 'HEAD', run = git) {
  const before = path => run(['show', `${scope.base}:${path}`]);
  const after = path => run(['show', `${head}:${path}`]);
  if (scope.paths.includes('worker/pg/sync.sql')) {
    const withoutMaintenance = text => text.replace(/CREATE OR REPLACE FUNCTION factory_forget_(?:codes|past)\([\s\S]*?\$\$;/g, '');
    if (withoutMaintenance(before('worker/pg/sync.sql')) !== withoutMaintenance(after('worker/pg/sync.sql')))
      throw new Error('Fast CI cannot change other factory SQL');
  }
  if (scope.paths.includes('worker/pg/library.sql')) {
    const diff = run(['diff', '--unified=0', scope.base, head, '--', 'worker/pg/library.sql']);
    if (diff.split('\n').filter(line => /^[+-]/.test(line) && !/^(---|\+\+\+)/.test(line))
      .some(line => !/^[+-]\s*(?:--|$)/.test(line))) throw new Error('Fast CI cannot change table schemas');
  }
  const native = 'cli/src/board/assemble/inventory.rs';
  if (scope.paths.includes(native) && before(native).split('#[cfg(test)]')[0] !== after(native).split('#[cfg(test)]')[0])
    throw new Error('Fast CI cannot change the incremental inventory protocol');
  if (scope.paths.includes('scripts/factoryd.mjs')) {
    const withoutBudget = text => text.replace(/\bPAST_BATCH = [\d_]+/, '');
    if (withoutBudget(before('scripts/factoryd.mjs')) !== withoutBudget(after('scripts/factoryd.mjs')))
      throw new Error('Fast CI cannot change daemon scheduling or settlement');
  }
}

export function checkFastProof(proof, paths) {
  if (proof.scope !== 'fast' || !proof.fast) throw new Error('Missing fast CI scope');
  const scope = fastScope(paths, { ...proof.fast, verifiedBase: true });
  if (JSON.stringify(scope.paths) !== JSON.stringify(proof.fast.paths)) throw new Error('Fast CI paths changed');
  return scope;
}
