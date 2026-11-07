// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const FAST_JOBS = ['site', 'checks', 'services', 'pipeline', 'page', 'published-board'];

const PATHS = new Set([
  '.github/workflows/ci.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
  'scripts/ci_fast.mjs', 'scripts/check_ci_fast.mjs', 'scripts/check_ci_policy.mjs', 'scripts/check_ci_partitions.mjs',
  'cli/src/board/run.rs', 'cli/src/board/assemble/inventory.rs',
  'web/src/static/app/73-compute.js', 'data/i18n/zh/ui.yaml',
  'worker/clearance.js', 'worker/contribution.js', 'worker/usage_days.js',
  'worker/deadlines.js', 'worker/factory.js', 'worker/board_tasks.js', 'worker/task_observation.js',
  'worker/board_reviews.js', 'scripts/check_tasks.mjs',
  'scripts/ship_queue.sh', 'scripts/check_queue_once.mjs', 'scripts/check_pg_scripts.mjs', 'scripts/check_board_verify.mjs',
  'scripts/ship_factoryd.py', 'scripts/check_factory_deploy.py',
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
    const withoutQueueFence = text => text
      .replace(/CREATE OR REPLACE FUNCTION queue_once\(\) RETURNS trigger LANGUAGE plpgsql AS \$\$[\s\S]*?\$\$;/g, '')
      .replace(/DROP TRIGGER IF EXISTS queue_once ON queue;\s*CREATE TRIGGER queue_once BEFORE INSERT ON queue FOR EACH ROW EXECUTE FUNCTION queue_once\(\);/g, '')
      .replace(/CREATE UNIQUE INDEX IF NOT EXISTS queue_calculation ON queue \(build_id, ruler, mode\);/g, '')
      .split('\n').filter(line => line.trim() && !line.trimStart().startsWith('--')).join('\n');
    if (withoutQueueFence(before('worker/pg/library.sql')) !== withoutQueueFence(after('worker/pg/library.sql')))
      throw new Error('Fast CI cannot change table schemas outside the queue uniqueness fence');
  }
  for (const path of ['worker/board_reviews.js', 'scripts/check_tasks.mjs']) {
    if (scope.paths.includes(path) && before(path).replace(/\browRef\b/g, 'calculationId') !== after(path))
      throw new Error('Fast CI permits only the calculation ID name in reviews and generic task checks');
  }
  if (scope.paths.includes('worker/deadlines.js')) {
    const withoutRefresh = text => text.replace(/^export const REFRESH_WITHIN_MS = .*;\n/m, '')
      .replace(/^  refresh:.*,\n/m, '');
    if (withoutRefresh(before('worker/deadlines.js')) !== withoutRefresh(after('worker/deadlines.js')))
      throw new Error('Fast CI cannot change other task deadlines');
  }
  if (scope.paths.includes('scripts/ship_factoryd.py')) {
    const withoutQueueIndex = text => text.replaceAll(', "queue_calculation"', '').replaceAll(", 'queue_calculation'", '')
      .replace('SELECT count(*) = 4 AND', 'SELECT count(*) = 3 AND')
      .replace('CREATE (?:UNIQUE )?INDEX IF', 'CREATE INDEX IF')
      .replace('.replace("INDEX ", "INDEX CONCURRENTLY ", 1)', '.replace("CREATE INDEX ", "CREATE INDEX CONCURRENTLY ", 1)');
    if (withoutQueueIndex(before('scripts/ship_factoryd.py')) !== withoutQueueIndex(after('scripts/ship_factoryd.py')))
      throw new Error('Fast CI cannot change deployment beyond the queue index');
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
  if (JSON.stringify(proof.jobs) !== JSON.stringify(FAST_JOBS)) throw new Error('Missing fast CI coverage');
  const scope = fastScope(paths, { ...proof.fast, verifiedBase: true });
  if (JSON.stringify(scope.paths) !== JSON.stringify(proof.fast.paths)) throw new Error('Fast CI paths changed');
  return scope;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv[2] !== 'page') throw new Error('Use page for focused compute checks');
  for (const name of ['check_compute_orders', 'check_compute_ui']) {
    const result = spawnSync(process.execPath, [`scripts/${name}.mjs`], { stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status || 1);
  }
}
