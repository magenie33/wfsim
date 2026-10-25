// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export const PRODUCT_JOBS = ['test', 'gas', 'engine-apart', 'published-board', 'site', 'checks', 'pipeline', 'page', 'parity', 'page-rest'];
export const BROWSER_JOBS = ['site', 'checks', 'pipeline', 'page', 'parity', 'page-rest'];

export function classify(paths, force = false) {
  const reasons = [];
  let scope = 'docs';
  for (const path of paths) {
    let kind;
    if (/^(site\/|desktop\/payload\/|mcp\/engine\.js$|data\/board_state\.yaml$)/.test(path)) kind = 'snapshot';
    else if (/^docs\/(CLIENTS\.md$|measurements\/)/.test(path)) kind = 'full';
    else if (/^(docs\/.*\.md$|README\.md$|LICENSE$)/.test(path)) kind = 'docs';
    else if (/^web\/src\/static\//.test(path) || /^data\/i18n\//.test(path)) kind = 'frontend';
    else if (/^(worker\/|bot\/|mcp\/)/.test(path)) kind = 'services';
    else kind = 'full';
    reasons.push({ path, kind });
    if (kind === 'full') scope = 'full';
    else if (scope !== 'full' && ['frontend', 'services'].includes(kind)) scope = 'product';
    else if (scope === 'docs' && kind === 'snapshot') scope = 'snapshot';
  }
  if (force || !paths.length) scope = 'full';
  const jobs = scope === 'full' ? PRODUCT_JOBS : scope === 'product' ? BROWSER_JOBS
    : scope === 'snapshot' ? ['published-board'] : [];
  return { scope, jobs, reasons, forced: force || !paths.length };
}

export function policyDigest() {
  const h = createHash('sha256');
  for (const path of ['.github/workflows/ci.yml', '.github/actions/site-tools/action.yml',
    '.github/actions/built-site/action.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
    'scripts/check_ci_policy.mjs', 'Cargo.toml', 'Cargo.lock']) {
    h.update(path).update('\0').update(readFileSync(path)).update('\0');
  }
  return h.digest('hex');
}

export function changedPaths(event, env, git = args => execFileSync('git', args, { encoding: 'utf8' }).trim()) {
  let base;
  if (env.GITHUB_EVENT_NAME === 'pull_request') {
    base = git(['merge-base', event.pull_request.base.sha, 'HEAD']);
  } else if (env.GITHUB_EVENT_NAME === 'push' && /^[a-f0-9]{40}$/.test(event.before || '') && !/^0+$/.test(event.before)) {
    base = event.before;
  } else if (env.GITHUB_REF !== 'refs/heads/main') {
    base = git(['merge-base', 'origin/main', 'HEAD']);
  } else return [];
  // Without rename detection, both the old and new paths take part in selection.
  return git(['diff', '--name-only', '--no-renames', '-z', base, 'HEAD']).split('\0').filter(Boolean);
}

export function assertGate(plan, needs, reused = false) {
  for (const name of ['plan', 'quick', 'commits']) {
    if (needs[name]?.result !== 'success') throw new Error(`${name}: ${needs[name]?.result || 'missing'}`);
  }
  for (const name of PRODUCT_JOBS) {
    const expected = !reused && plan.jobs.includes(name) ? 'success' : 'skipped';
    if (needs[name]?.result !== expected) throw new Error(`${name}: expected ${expected}, got ${needs[name]?.result || 'missing'}`);
  }
}

export function output(values) {
  for (const [key, value] of Object.entries(values)) {
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${typeof value === 'object' ? JSON.stringify(value) : value}\n`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  let paths = [];
  try { paths = changedPaths(event, process.env); }
  catch (e) { console.log(`Cannot establish changed paths; running full CI: ${e.message}`); }
  const plan = classify(paths, process.env.GITHUB_EVENT_NAME === 'schedule' || event.inputs?.full === true || event.inputs?.full === 'true');
  output({ plan, policy: policyDigest(), ...Object.fromEntries(PRODUCT_JOBS.map(j => [j.replaceAll('-', '_'), plan.jobs.includes(j)])) });
  const summary = `## CI scope: ${plan.scope}\n\nSelected: ${plan.jobs.join(', ') || 'quick checks only'}.\n\n${plan.forced ? 'Full validation requested or no reliable diff available.\n' : ''}\n${plan.reasons.map(r => `- ${JSON.stringify(r.path)}: ${r.kind}`).join('\n')}\n`;
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
}
