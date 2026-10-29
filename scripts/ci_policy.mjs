// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { hotfixPlan } from './ci_hotfix.mjs';

export const PRODUCT_JOBS = ['test', 'gas', 'engine-apart', 'published-board', 'site', 'checks', 'pipeline', 'page', 'parity', 'page-rest'];
export const BROWSER_JOBS = ['site', 'checks', 'pipeline', 'page', 'parity', 'page-rest'];
export const ALL_CI_JOBS = [...PRODUCT_JOBS, 'hotfix'];

export function browserCheckPaths(workflow) {
  const sections = workflow.split(/^  ([\w-]+):[ \t]*$/m);
  const paths = new Set(), other = new Set();
  for (let i = 1; i < sections.length; i += 2) {
    const owners = BROWSER_JOBS.includes(sections[i]) ? paths : other;
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
        for (const command of line.matchAll(/\bnode (scripts\/check_[\w-]+\.mjs)\b/g)) owners.add(command[1]);
      }
    }
  }
  return new Set([...paths].filter(path => !other.has(path)));
}
const browserChecks = browserCheckPaths(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));

export function classify(paths, force = false, verifiedBase = true) {
  const reasons = [];
  let scope = 'docs';
  let snapshot = false;
  for (const path of paths) {
    let kind;
    if (/^(site\/|desktop\/payload\/|mcp\/engine\.js$|data\/board_state\.yaml$)/.test(path)) kind = 'snapshot';
    else if (/^docs\/(CLIENTS\.md$|measurements\/)/.test(path)) kind = 'full';
    else if (/^(docs\/.*\.md$|README\.md$|LICENSE$)/.test(path)) kind = 'docs';
    else if (/^web\/src\/static\//.test(path) || /^data\/i18n\//.test(path)) kind = 'frontend';
    else if (/^(worker\/|bot\/|mcp\/)/.test(path)) kind = 'services';
    else if (browserChecks.has(path)) kind = 'browser-check';
    else kind = 'full';
    reasons.push({ path, kind });
    snapshot ||= kind === 'snapshot';
    if (kind === 'full') scope = 'full';
    else if (scope !== 'full' && ['frontend', 'services', 'browser-check'].includes(kind)) scope = 'product';
    else if (scope === 'docs' && kind === 'snapshot') scope = 'snapshot';
  }
  if (snapshot && scope === 'product') scope = 'full';
  if (force || !paths.length || !verifiedBase) scope = 'full';
  const jobs = scope === 'full' ? PRODUCT_JOBS : scope === 'product' ? BROWSER_JOBS
    : scope === 'snapshot' ? ['published-board'] : [];
  return { scope, jobs, reasons, forced: force || !paths.length || !verifiedBase, verifiedBase };
}

export function policyDigest() {
  const h = createHash('sha256');
  for (const path of ['.github/workflows/ci.yml', '.github/actions/site-tools/action.yml',
    '.github/actions/built-site/action.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
    'scripts/check_ci_policy.mjs', 'scripts/ci_hotfix.mjs', 'scripts/check_ci_hotfix.mjs',
    'scripts/ci_rust.mjs', 'scripts/check_ci_partitions.mjs', 'scripts/riven_request_cases.mjs',
    '.github/actions/site-tools/install.sh', 'scripts/check_site_setup.mjs', 'Cargo.toml', 'Cargo.lock']) {
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
  for (const [name, job] of Object.entries(needs)) {
    if (!ALL_CI_JOBS.includes(name) && job.result !== 'success') throw new Error(`${name}: ${job.result}`);
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
  const force = process.env.GITHUB_EVENT_NAME === 'schedule' || event.inputs?.full === true || event.inputs?.full === 'true';
  const hotfix = event.inputs?.hotfix === true || event.inputs?.hotfix === 'true';
  let verified = false;
  if (!force && (hotfix || classify(paths).scope !== 'full')) {
    try {
      const base = changedBase(event, process.env);
      verified = execFileSync(process.execPath, ['scripts/ci_runs.mjs', 'base', base], { encoding: 'utf8' }).trim() === 'true';
    } catch (e) { console.log(`Cannot verify the comparison base; running full CI: ${e.message}`); }
  }
  const plan = hotfix ? { ...hotfixPlan(paths, { verifiedBase: verified, reason: event.inputs?.reason || '',
    event: process.env.GITHUB_EVENT_NAME, force }), base: changedBase(event, process.env) } : classify(paths, force, verified);
  output({ plan, policy: policyDigest(), hotfix_matrix: plan.matrix || { group: [] },
    ...Object.fromEntries(ALL_CI_JOBS.map(j => [j.replaceAll('-', '_'), plan.jobs.includes(j)])) });
  const summary = `## CI scope: ${plan.scope}\n\nSelected: ${plan.jobs.join(', ') || 'quick checks only'}.\n\n${plan.forced ? 'Full validation requested, baseline unverified, or no reliable diff available.\n' : ''}\n${plan.reasons.map(r => `- ${JSON.stringify(r.path)}: ${r.kind}`).join('\n')}\n`;
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
}
