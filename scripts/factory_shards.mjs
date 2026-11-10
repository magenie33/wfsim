// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash } from 'node:crypto';
import { readFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { spawnSync, execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const FACTORY_CHECKS = {
  command: ['node', 'scripts/check_tasks.mjs'],
  trials: 400,
  modes: {
    sync: { WFSIM_SETTLE_ASYNC: '0', WFSIM_SYNC_PROC: '0' },
    async: { WFSIM_SETTLE_ASYNC: '1', WFSIM_SYNC_PROC: '0' },
  },
  groups: { random: { shards: 8 }, regressions: { shards: 1 } },
};

const files = ['scripts/check_tasks.mjs', 'scripts/pg/test_db.mjs', 'scripts/factory_shards.mjs'];
const source = name => readFileSync(new URL('../' + name, import.meta.url), 'utf8');
const sha = process.env.GITHUB_SHA || execFileSync('git', ['rev-parse', 'HEAD'], {
  cwd: new URL('../', import.meta.url), encoding: 'utf8',
}).trim();
if (!/^[0-9a-f]{40}$/.test(sha)) throw Error('factory coverage needs an exact source SHA');
export const factorySource = () => Object.fromEntries(files.map(name => [name,
  createHash('sha256').update(source(name)).digest('hex')]));
export const factoryMode = (env = process.env) => (env.WFSIM_SETTLE_ASYNC === '1' ? 'async' : 'sync')
  + (env.WFSIM_SYNC_PROC === '1' ? '-proc' : '');

export function factoryCases(trials = FACTORY_CHECKS.trials) {
  if (!Number.isSafeInteger(trials) || trials < 0) throw Error('trials must be a nonnegative safe integer');
  const random = (family, count, first) => Array.from({ length: count }, (_, index) =>
    ({ id: `${family}:${index}`, family, index, seed: first + index }));
  const fixed = [...source(FACTORY_CHECKS.command[1]).matchAll(/await runCase\("(fixed:[a-z0-9-]+)", async \(\) => \{/g)]
    .map(([, id]) => ({ id, family: 'regressions' }));
  if (!fixed.length || new Set(fixed.map(row => row.id)).size !== fixed.length) throw Error('missing or duplicate fixed factory cases');
  return [...random('liveness', trials, 1), ...random('termination', Math.ceil(trials / 4), 10000), ...fixed];
}

export function factorySelection(args = [], env = process.env) {
  const values = {};
  for (const arg of args) {
    const match = /^(trials|group|shard|receipt)=(.+)$/.exec(arg);
    if (!match || Object.hasOwn(values, match[1])) throw Error(`invalid factory argument: ${arg}`);
    values[match[1]] = match[2];
  }
  const trials = values.trials === undefined ? FACTORY_CHECKS.trials : Number(values.trials);
  const group = values.group || 'all', shard = values.shard || '0/1';
  if (!['all', ...Object.keys(FACTORY_CHECKS.groups)].includes(group)) throw Error(`unknown factory group: ${group}`);
  const parts = /^(0|[1-9]\d*)\/([1-9]\d*)$/.exec(shard);
  if (!parts) throw Error(`invalid factory shard: ${shard}`);
  const index = Number(parts[1]), total = Number(parts[2]);
  if (!Number.isSafeInteger(index) || !Number.isSafeInteger(total) || index >= total
      || (group !== 'random' && shard !== '0/1')) throw Error(`invalid factory shard: ${group} ${shard}`);
  const cases = factoryCases(trials).filter(row => {
    if (row.family === 'regressions') return group !== 'random';
    return group !== 'regressions' && row.index % total === index;
  });
  return { version: 1, sha, mode: factoryMode(env), group, shard, trials, source: factorySource(), cases,
    ...(values.receipt ? { receipt: resolve(values.receipt) } : {}) };
}

export function factoryMatrix({ trials = FACTORY_CHECKS.trials, modes = Object.keys(FACTORY_CHECKS.modes),
  shards = FACTORY_CHECKS.groups.random.shards } = {}) {
  factoryCases(trials);
  if (!Number.isSafeInteger(shards) || shards < 1 || !modes.length || new Set(modes).size !== modes.length
      || modes.some(mode => !Object.hasOwn(FACTORY_CHECKS.modes, mode))) throw Error('invalid factory matrix');
  return modes.flatMap(mode => Object.entries(FACTORY_CHECKS.groups).flatMap(([group, spec]) =>
    Array.from({ length: group === 'random' ? shards : spec.shards }, (_, index) => ({ mode, group,
      shard: `${index}/${group === 'random' ? shards : spec.shards}`, trials }))));
}

export function factoryCommand(row, receipt) {
  const { mode, group, shard, trials = FACTORY_CHECKS.trials } = row;
  if (!Object.hasOwn(FACTORY_CHECKS.modes, mode)) throw Error(`unknown factory mode: ${mode}`);
  const args = [`trials=${trials}`, `group=${group}`, `shard=${shard}`, ...(receipt ? [`receipt=${receipt}`] : [])];
  factorySelection(args, FACTORY_CHECKS.modes[mode]);
  return { command: [...FACTORY_CHECKS.command, ...args], env: FACTORY_CHECKS.modes[mode] };
}

export function assertFactoryReceipt(receipt, plan) {
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  if (receipt?.version !== 1 || receipt.success !== true || receipt.failures !== 0
      || !same(receipt.source, plan.source)) throw Error('factory receipt did not finish on this source');
  for (const key of ['sha', 'mode', 'group', 'shard', 'trials', 'cases']) {
    if (!same(receipt[key], plan[key])) throw Error(`factory receipt has wrong ${key}`);
  }
  if (!Array.isArray(receipt.completed)) throw Error('factory receipt has no completed cases');
  const ids = receipt.completed.map(row => row.id);
  if (new Set(ids).size !== ids.length) throw Error('duplicate completed factory case');
  if (!same([...ids].sort(), plan.cases.map(row => row.id).sort())) throw Error('missing or unexpected completed factory case');
  if (receipt.completed.some(row => row.ok !== true || !Number.isFinite(row.ms) || row.ms < 0
      || !Number.isSafeInteger(row.checks) || row.checks < 0)) throw Error('factory case did not pass');
  return receipt;
}

export function assertFactoryCoverage(receipts, options = {}) {
  const expectedSha = options.sha === undefined ? sha : options.sha;
  if (!/^[0-9a-f]{40}$/.test(expectedSha)) throw Error('factory coverage needs an exact expected SHA');
  const matrix = factoryMatrix(options), plans = matrix.map(row => ({ ...factorySelection(
    factoryCommand(row).command.slice(2), FACTORY_CHECKS.modes[row.mode]), sha: expectedSha }));
  const slot = row => `${row.mode}/${row.group}/${row.shard}`;
  const expected = new Map(plans.map(plan => [slot(plan), plan])), seen = new Set();
  for (const receipt of receipts) {
    const key = slot(receipt);
    if (!expected.has(key) || seen.has(key)) throw Error(`missing, duplicate or unexpected factory mode/group/shard: ${key}`);
    assertFactoryReceipt(receipt, expected.get(key));
    seen.add(key);
  }
  if (seen.size !== expected.size) throw Error(`missing factory mode/group/shard: ${seen.size}/${expected.size}`);
  return { jobs: seen.size, modes: [...new Set(matrix.map(row => row.mode))],
    cases: receipts.reduce((n, receipt) => n + receipt.completed.length, 0) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [action, ...args] = process.argv.slice(2);
  if (action === 'matrix') {
    if (args.length) throw Error('matrix takes no arguments');
    console.log(JSON.stringify({ include: factoryMatrix() }));
  } else if (action === 'manifest') {
    console.log(JSON.stringify(factorySelection(args, process.env), null, 2));
  } else if (action === 'run') {
    if (args.filter(arg => arg.startsWith('mode=')).length !== 1) throw Error('run needs exactly one mode=');
    const mode = args.find(arg => arg.startsWith('mode='))?.slice(5);
    if (!Object.hasOwn(FACTORY_CHECKS.modes, mode)) throw Error('run needs a known mode=');
    const selected = args.filter(arg => !arg.startsWith('mode='));
    const plan = factorySelection(selected, FACTORY_CHECKS.modes[mode]);
    if (!plan.receipt) throw Error('run needs receipt= for completed-case coverage');
    mkdirSync(dirname(plan.receipt), { recursive: true });
    const result = spawnSync(process.execPath, [FACTORY_CHECKS.command[1], ...selected], {
      env: { ...process.env, ...FACTORY_CHECKS.modes[mode], WFSIM_TEST_DB_TIMING: '1' }, stdio: 'inherit',
    });
    if (result.error) throw result.error;
    process.exitCode = result.status === 0 ? 0 : result.status || 1;
  } else throw Error('expected matrix, manifest or run');
}
