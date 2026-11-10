// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { hotfixPlan } from './ci_hotfix.mjs';
import { checkFastChanges, fastJobs, fastScope } from './ci_fast.mjs';
import { DEEP_SCOPE_GROUPS, RUST_SUITES } from './ci_rust.mjs';
import { COMPANION_JOBS, companionJobs } from './full_work.mjs';
import { FACTORY_CHECKS, factoryMatrix } from './factory_shards.mjs';

export const BROWSER_JOBS = ['site', 'checks', 'services', 'pipeline', 'page', 'parity', 'page-rest', 'mcp', 'bot'];
export const PRODUCT_JOBS = ['test', 'gas', 'engine-apart', 'published-board', ...BROWSER_JOBS];
export const ALL_CI_JOBS = [...PRODUCT_JOBS, 'hotfix'];
export const CONSUMER_JOBS = { mcp: ['site', 'checks', 'mcp'], bot: ['site', 'checks', 'bot'] };
export const OPTIONAL_NATIVE_JOBS = ['native-mac', 'native-linux'];
const SERVICE_CHECK_JOBS = [...BROWSER_JOBS, 'runtime', 'factory-cases', 'factory'];
const CI_WIRING = ['.github/workflows/ci.yml', '.github/workflows/ship.yml',
  '.github/actions/site-tools/action.yml', '.github/actions/built-site/action.yml',
  '.github/actions/site-tools/install.sh', 'scripts/check_site_setup.mjs',
  'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs', 'scripts/check_ci_policy.mjs',
  'scripts/ci_hotfix.mjs', 'scripts/check_ci_hotfix.mjs', 'scripts/check_ci_partitions.mjs',
  'scripts/ci_fast.mjs', 'scripts/check_ci_fast.mjs', 'scripts/full_work.mjs',
  'scripts/check_full_work.mjs', 'scripts/check_edge_runtime.mjs', 'scripts/ci_metrics.mjs',
  'scripts/factory_shards.mjs', 'scripts/check_factory_shards.mjs',
  'scripts/release_readiness.py', 'scripts/check_release_readiness.py'];
const NON_SEARCH_TOOLS = ['scripts/check_weapon_search.mjs', 'scripts/fetch_inbox.sh',
  'scripts/live_board.sh', 'scripts/ship_builds.sh', 'scripts/retire_scores.mjs', 'scripts/ship_queue.sh',
  'scripts/fetch_facts.sh', 'scripts/ship_facts.sh', 'scripts/live_publish.mjs', '.github/workflows/queue.yml',
  'deploy/factoryd_grants.sql', 'scripts/check_rescore_paths.mjs', 'scripts/capture_compute_inventory.mjs',
  'scripts/build_batches.mjs', 'scripts/ship_family.mjs', 'scripts/ship_bot.py', '.github/workflows/scores.yml',
  'scripts/factoryd.mjs', 'scripts/pg_task_archive.py', 'scripts/ship_task_archive.py', 'scripts/check_task_archive.py',
  'scripts/ship.py', 'scripts/release_stage.py', 'scripts/check_release_stage.py', 'scripts/check_ship_bot_assets.py',
  'scripts/payload_manifest.py', 'scripts/release_desktop.py', 'scripts/pg/test_db.mjs', 'scripts/intake_batches.mjs',
  'deploy/wfsim-task-archive.service', 'deploy/wfsim-task-archive.timer'];

export function checkJobPaths(workflow) {
  const sections = workflow.split(/^  ([\w-]+):[ \t]*$/m);
  const paths = new Map();
  for (let i = 1; i < sections.length; i += 2) {
    const lines = sections[i + 1].split('\n');
    for (let j = 0; j < lines.length; j++) {
      const run = lines[j].match(/^[ \t]*(?:- )?run:[ \t]*(.*)$/);
      if (!run) continue;
      const commands = [];
      if (/^[|>][+-]?$/.test(run[1])) {
        const indent = lines[j].indexOf('run:');
        while (j + 1 < lines.length && (!lines[j + 1].trim() || lines[j + 1].search(/\S/) > indent)) commands.push(lines[++j]);
      } else commands.push(run[1]);
      for (const line of commands.filter(line => !line.trimStart().startsWith('#'))) {
        const invoked = [...line.matchAll(/\bnode (scripts\/check_[\w-]+\.mjs)\b/g)].map(m => m[1]);
        if (/\bnode scripts\/factory_shards\.mjs run\b/.test(line)) invoked.push(FACTORY_CHECKS.command[1]);
        for (const path of invoked) {
          if (!paths.has(path)) paths.set(path, new Set());
          paths.get(path).add(sections[i]);
        }
      }
    }
  }
  return paths;
}
export function browserCheckPaths(workflow) {
  return new Set([...checkJobPaths(workflow)].filter(([, jobs]) => [...jobs].every(j => SERVICE_CHECK_JOBS.includes(j))).map(([p]) => p));
}
const owners = checkJobPaths(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));

export function releaseCoverage(jobs) {
  const same = wanted => jobs.length === wanted.length && wanted.every(j => jobs.includes(j));
  return same(PRODUCT_JOBS) || same(BROWSER_JOBS) || same(CONSUMER_JOBS.mcp) || same(CONSUMER_JOBS.bot)
    || same([...new Set([...CONSUMER_JOBS.mcp, ...CONSUMER_JOBS.bot])]);
}

export function classify(paths, force = false, verifiedBase = true) {
  const reasons = [];
  let scope = 'docs';
  let snapshot = false;
  const consumers = new Set();
  for (const path of paths) {
    let kind;
    if (/^(site\/|desktop\/payload\/|mcp\/engine\.js$|data\/board_state\.yaml$)/.test(path)) kind = 'snapshot';
    else if (/^docs\/(CLIENTS\.md$|measurements\/)/.test(path)) kind = 'full';
    else if (/^(docs\/.*\.md$|README\.md$|LICENSE$)/.test(path)) kind = 'docs';
    else if (/^web\/src\/static\//.test(path) || /^data\/i18n\//.test(path)) kind = 'frontend';
    else if (path === 'mcp/headless.js') kind = 'headless';
    else if (/^mcp\/(index\.js|account\.js|a2a\.js|wrangler\.jsonc)$/.test(path)
      || path === 'scripts/mcp_test_loader.mjs') kind = 'mcp';
    else if (/^bot\//.test(path)) kind = 'bot';
    else if (/^worker\//.test(path)) kind = 'services';
    else if (owners.has(path) && [...owners.get(path)].every(j => SERVICE_CHECK_JOBS.includes(j))) {
      const jobs = [...owners.get(path)];
      kind = jobs.every(j => j === 'mcp') ? 'mcp' : jobs.every(j => j === 'bot') ? 'bot' : 'browser-check';
    }
    else kind = 'full';
    reasons.push({ path, kind });
    snapshot ||= kind === 'snapshot';
    if (kind === 'mcp' || kind === 'bot') consumers.add(kind);
    if (kind === 'headless') { consumers.add('mcp'); consumers.add('bot'); }
    if (kind === 'full') scope = 'full';
    else if (scope !== 'full' && ['frontend', 'services', 'browser-check'].includes(kind)) scope = 'product';
    else if (scope === 'docs' && consumers.size) scope = 'consumers';
    else if (scope === 'docs' && kind === 'snapshot') scope = 'snapshot';
  }
  if (snapshot && (scope === 'product' || consumers.size)) scope = 'full';
  if (force || !paths.length || !verifiedBase) scope = 'full';
  const jobs = scope === 'full' ? PRODUCT_JOBS : scope === 'product' ? BROWSER_JOBS
    : scope === 'consumers' ? [...new Set([...consumers].sort().flatMap(c => CONSUMER_JOBS[c]))]
    : scope === 'snapshot' ? ['published-board'] : [];
  return { scope, jobs, reasons, forced: force || !paths.length || !verifiedBase, verifiedBase,
    deep: force || !paths.length || !verifiedBase
      || paths.some(p => /^(optimizer\/|webapi\/src\/optimize(?:\.rs|\/))/.test(p))
      || reasons.some(r => r.kind === 'full' && !CI_WIRING.includes(r.path) && !NON_SEARCH_TOOLS.includes(r.path)
        && !/^(engine\/|data\/|webapi\/src\/|native\/|wasm\/|cli\/|web\/|desktop\/)/.test(r.path)) };
}

export function policyDigest() {
  const h = createHash('sha256');
  for (const path of [...CI_WIRING, 'scripts/ci_rust.mjs', 'scripts/riven_request_cases.mjs', 'scripts/native_artifact.py',
    'scripts/check_native_artifact.py', 'scripts/native_pgo.py', 'scripts/check_native_pgo.py', 'scripts/check_native_pgo.mjs',
    'scripts/ci_native_linux.py', 'scripts/release_engine_lib.py', 'Cargo.toml', 'Cargo.lock']) {
    h.update(path).update('\0').update(readFileSync(path)).update('\0');
  }
  return h.digest('hex');
}

const gitOutput = args => execFileSync('git', args, { encoding: 'utf8' });

export function changedBase(event, env, git = gitOutput) {
  let base;
  if (env.GITHUB_EVENT_NAME === 'pull_request') {
    base = git(['merge-base', event.pull_request.base.sha, 'HEAD']).trim();
  } else if (env.GITHUB_EVENT_NAME === 'push' && /^[a-f0-9]{40}$/.test(event.before || '') && !/^0+$/.test(event.before)) {
    base = event.before;
  } else if (env.GITHUB_REF !== 'refs/heads/main') {
    base = git(['merge-base', 'origin/main', 'HEAD']).trim();
  } else return null;
  return base;
}

export function changedPaths(event, env, git = gitOutput) {
  const base = changedBase(event, env, git);
  if (!base) return [];
  // Without rename detection, both the old and new paths take part in selection.
  return git(['diff', '--name-only', '--no-renames', '-z', base, 'HEAD']).split('\0').filter(Boolean);
}

export function assertGate(plan, needs, reused = false) {
  for (const name of ['plan', 'quick', 'commits']) {
    if (needs[name]?.result !== 'success') throw new Error(`${name}: ${needs[name]?.result || 'missing'}`);
  }
  for (const name of ALL_CI_JOBS) {
    const expected = !reused && plan.jobs.includes(name) ? 'success' : 'skipped';
    if (needs[name]?.result !== expected) throw new Error(`${name}: expected ${expected}, got ${needs[name]?.result || 'missing'}`);
  }
  for (const name of COMPANION_JOBS) {
    const expected = !reused && companionJobs(plan.jobs, !!plan.fast).includes(name) ? 'success' : 'skipped';
    if (needs[name]?.result !== expected) throw new Error(`${name}: expected ${expected}, got ${needs[name]?.result || 'missing'}`);
  }
  for (const [name, job] of Object.entries(needs)) {
    if (OPTIONAL_NATIVE_JOBS.includes(name)) {
      if (!['success', 'failure', 'skipped', 'cancelled'].includes(job.result)) throw new Error(`${name}: missing result`);
      continue;
    }
    if (![...ALL_CI_JOBS, ...COMPANION_JOBS].includes(name) && job.result !== 'success') throw new Error(`${name}: ${job.result}`);
  }
  for (const name of OPTIONAL_NATIVE_JOBS) {
    if (!needs[name]) throw new Error(`${name}: missing`);
  }
}

export function output(values) {
  for (const [key, value] of Object.entries(values)) {
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${typeof value === 'object' ? JSON.stringify(value) : value}\n`);
  }
}

export function jobPlan(plan) {
  const { reasons, ...execution } = plan;
  return execution;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  let paths = [];
  try { paths = changedPaths(event, process.env); }
  catch (e) { console.log(`Cannot establish changed paths; running full CI: ${e.message}`); }
  const force = process.env.GITHUB_EVENT_NAME === 'schedule' || event.inputs?.full === true || event.inputs?.full === 'true';
  const hotfix = event.inputs?.hotfix === true || event.inputs?.hotfix === 'true';
  const fast = event.inputs?.fast === true || event.inputs?.fast === 'true';
  let verified = false;
  if (!force && paths.length) {
    try {
      const base = changedBase(event, process.env);
      verified = execFileSync(process.execPath, ['scripts/ci_runs.mjs', 'base', base], { encoding: 'utf8' }).trim() === 'true';
    } catch (e) { console.log(`Cannot verify the comparison base; running full CI: ${e.message}`); }
  }
  const plan = hotfix ? { ...hotfixPlan(paths, { verifiedBase: verified, reason: event.inputs?.reason || '',
    event: process.env.GITHUB_EVENT_NAME, force }), base: changedBase(event, process.env) } : classify(paths, force, verified);
  if (fast) {
    plan.fast = fastScope(paths, { base: changedBase(event, process.env), verifiedBase: verified,
      force, hotfix, reason: event.inputs?.reason || '' });
    checkFastChanges(plan.fast);
    plan.scope = 'fast';
    plan.jobs = fastJobs(plan.fast.paths);
    plan.deep = false;
  }
  output({ plan: jobPlan(plan), policy: policyDigest(), native: plan.jobs.includes('gas'), hotfix_matrix: plan.matrix || { group: [] },
    factory_matrix: { include: factoryMatrix() },
    ...Object.fromEntries(COMPANION_JOBS.map(j => [j.replaceAll('-', '_'), companionJobs(plan.jobs, !!plan.fast).includes(j)])),
    test_matrix: { suite: [...RUST_SUITES, ...(plan.deep ? Object.keys(DEEP_SCOPE_GROUPS) : [])] },
    ...Object.fromEntries(ALL_CI_JOBS.map(j => [j.replaceAll('-', '_'), plan.jobs.includes(j)])) });
  const summary = `## CI scope: ${plan.scope}\n\nSelected: ${plan.jobs.join(', ') || 'quick checks only'}.\n\n${plan.forced ? 'Full validation requested, baseline unverified, or no reliable diff available.\n' : ''}\n${plan.reasons.map(r => `- ${JSON.stringify(r.path)}: ${r.kind}`).join('\n')}\n`;
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
}
