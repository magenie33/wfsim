// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { ALL_CI_JOBS, assertGate, checkJobPaths, OPTIONAL_NATIVE_JOBS, PRODUCT_JOBS } from './ci_policy.mjs';
import { assertHotfixProof, BOARD_ENTRY_CHANGE, HOTFIX_CHECKS, HOTFIX_PROFILES, HOTFIX_FACTORY_GROUPS, hotfixCommands, hotfixPlan } from './ci_hotfix.mjs';
import { trustedRun, validProof } from './ci_runs.mjs';
import { COMPANION_JOBS } from './full_work.mjs';
import { factoryCases, factoryMatrix, factoryCommand, factorySelection, assertFactoryCoverage } from './factory_shards.mjs';

const reason = 'Devices finish their packs and time out while asking for the next';
const paths = ['worker/pg/sync.sql', 'scripts/check_tasks.mjs', 'docs/CHECKS.md'];
const plan = hotfixPlan(paths, { reason });
assert.deepEqual(plan.profiles, ['factory-sql']);
assert.deepEqual(plan.matrix.group, ['async', 'factory', 'policy', ...HOTFIX_FACTORY_GROUPS].sort());
assert.equal(HOTFIX_FACTORY_GROUPS.length, 18);
assert.equal(plan.matrix.group.length, 21);
assert.deepEqual(hotfixPlan(['scripts/ci_runs.mjs'], { reason }).matrix.group, ['policy']);
assert.deepEqual(hotfixPlan([...paths, 'scripts/ci_runs.mjs'], { reason }).profiles, ['ci', 'factory-sql']);
for (const path of ['engine/src/fight.rs', 'optimizer/src/quick.rs', 'webapi/src/fight.rs',
  'data/weapons/torid.yaml', 'web/src/static/app/69-board-work.js', 'worker/index.js',
  'worker/pg/schema.sql', 'scripts/pg/test_db.mjs', 'scripts/pg/package-lock.json',
  '.github/workflows/ship.yml', 'site/release.json', 'docs/CLIENTS.md', 'docs/measurements/M001.md',
  'scripts/new-check.mjs', 'desktop/src/main.rs']) {
  assert.throws(() => hotfixPlan([...paths, path], { reason }), /Outside hotfix coverage/, path);
}
for (const options of [{ reason: '' }, { reason: ' '.repeat(3) }, { reason: 'x'.repeat(501) },
  { reason, verifiedBase: false }, { reason, force: true }, { reason, event: 'push' }, { reason, event: 'pull_request' }]) {
  assert.throws(() => hotfixPlan(paths, options), undefined, JSON.stringify(options));
}
for (const diff of [[], ['docs/CI.md']]) assert.throws(() => hotfixPlan(diff, { reason }));
for (const profile of Object.values(HOTFIX_PROFILES)) {
  for (const group of profile.groups) assert(hotfixCommands(group).length, group);
  for (const path of profile.paths) assert(existsSync(path), path);
}
for (const checks of Object.values(HOTFIX_CHECKS)) {
  assert.equal(new Set(checks).size, checks.length);
  for (const check of checks) assert(existsSync(`scripts/${check}.mjs`), check);
}

const extras = ['check_board_verify', 'check_board_sync', 'check_sync_proc', 'check_fair',
  'check_settler', 'check_settler_async', 'check_initiator', 'check_moot_checks', 'check_forget_past',
  'check_idle_devices', 'check_contribution', 'check_ledger', 'check_dials', 'check_clients_doc', 'check_appraise'];
assert.deepEqual(HOTFIX_CHECKS.factory, extras);
assert.deepEqual(HOTFIX_CHECKS.async, ['check_board_verify', 'check_board_sync', 'check_fair']);
const commands = mkdtempSync(join(tmpdir(), 'wfsim-hotfix-commands-'));
try {
  const preload = join(commands, 'capture.mjs'), output = join(commands, 'calls.jsonl');
  writeFileSync(preload, `import {appendFileSync} from 'node:fs';
import cp from 'node:child_process';import {syncBuiltinESMExports} from 'node:module';
cp.spawnSync=(exe,args,options)=>{appendFileSync(process.env.HOTFIX_CAPTURE,JSON.stringify({args,env:{
WFSIM_SETTLE_ASYNC:options.env.WFSIM_SETTLE_ASYNC,WFSIM_SYNC_PROC:options.env.WFSIM_SYNC_PROC}})+'\\n');
return {status:Number(process.env.HOTFIX_CAPTURE_EXIT||0)};};syncBuiltinESMExports();`);
  const invoke = (group, extra = {}) => spawnSync(process.execPath,
    ['--import', pathToFileURL(preload).href, 'scripts/ci_hotfix.mjs', group], {
      encoding: 'utf8', env: { ...process.env, WFSIM_SETTLE_ASYNC: '0', WFSIM_SYNC_PROC: '0', HOTFIX_CAPTURE: output, ...extra },
    });
  const receipts = [], called = [];
  for (const group of plan.matrix.group) {
    writeFileSync(output, '');
    const result = invoke(group);
    assert.equal(result.status, 0, result.stderr);
    const rows = readFileSync(output, 'utf8').trim().split('\n').map(JSON.parse);
    assert.deepEqual(rows, hotfixCommands(group).map(({command,env}) => ({args:command.slice(1),env:{
      WFSIM_SETTLE_ASYNC:'0',WFSIM_SYNC_PROC:'0',...env}})), group);
    called.push(...rows);
    for (const row of rows.filter(row => row.args[0] === 'scripts/check_tasks.mjs')) {
      const selected = factorySelection(row.args.slice(1), row.env), mode = selected.mode === 'async-proc' ? 'async' : selected.mode;
      assert.equal(row.env.WFSIM_SYNC_PROC, mode === 'async' ? '1' : '0');
      receipts.push({ ...selected, mode, success: true, failures: 0,
        completed: selected.cases.map(item => ({id:item.id,ok:true,checks:0,ms:1})) });
    }
  }
  assert.equal(called.filter(row => row.args[0] === 'scripts/check_tasks.mjs').length, 18);
  assert.deepEqual(assertFactoryCoverage(receipts), {jobs:18,modes:['sync','async'],cases:1076});
  for (const mode of ['sync','async']) {
    const all = receipts.filter(row => row.mode === mode).flatMap(row => row.cases.map(item => item.id));
    assert.equal(new Set(all).size, all.length, mode + ' executes each case once');
    assert.deepEqual([...all].sort(), factoryCases().map(item => item.id).sort());
  }
  for (const row of factoryMatrix()) {
    const expected = factoryCommand(row), group = `factory-${row.mode}-${row.group}-${row.shard.replace('/', '-of-')}`;
    assert.deepEqual(hotfixCommands(group)[0].command, expected.command);
    assert.equal(hotfixCommands(group)[0].env.WFSIM_SETTLE_ASYNC, row.mode === 'async' ? '1' : '0');
  }
  for (const group of [HOTFIX_FACTORY_GROUPS[0], 'async', 'factory'])
    assert.equal(invoke(group, {HOTFIX_CAPTURE_EXIT:'7'}).status, 7, group + ' preserves a failed child');
  for (const group of ['factory-wrong-random-0-of-8', 'factory-sync-random-8-of-8', 'factory-sync-regressions-1-of-1']) {
    const result = invoke(group);
    assert.notEqual(result.status, 0);assert.match(result.stderr, /Unknown hotfix check group/);
  }
  const mutations = {
    missing_group: rows => rows.pop(),
    duplicate_group: rows => rows.push(rows[0]),
    wrong_mode: rows => { rows[0].mode = 'wrong'; },
    wrong_case: rows => { rows[0].completed[0].id = 'fixed:wrong-case'; },
  };
  for (const [name, mutate] of Object.entries(mutations)) {
    const rows = structuredClone(receipts);mutate(rows);
    const files = rows.map((row,i) => {const path=join(commands, name+'-'+i+'.json');writeFileSync(path,JSON.stringify(row));return path;});
    const result = spawnSync(process.execPath, ['scripts/check_factory_shards.mjs', ...files], {encoding:'utf8'});
    assert.notEqual(result.status, 0, name + ' must fail actual factory coverage');
    assert.match(result.stderr, /factory/);
    console.log('Hotfix actual factory coverage rejects ' + name);
  }
  console.log('Hotfix CLI retains all 400+100+38 cases once per mode in 18 factory shards, both async flags and every extra check.');
} finally {rmSync(commands,{recursive:true,force:true});}

const needs = Object.fromEntries(['plan', 'quick', 'commits', 'coordination', ...ALL_CI_JOBS]
  .map(job => [job, { result: PRODUCT_JOBS.includes(job) ? 'skipped' : 'success' }])
  .concat([...OPTIONAL_NATIVE_JOBS, ...COMPANION_JOBS].map(job => [job, { result: 'skipped' }])));
assertGate(plan, needs);
for (const job of ['plan', 'quick', 'commits', 'coordination', 'hotfix']) {
  for (const result of ['failure', 'cancelled', 'skipped', undefined]) {
    assert.throws(() => assertGate(plan, { ...needs, [job]: { result } }), undefined, `${job}: ${result}`);
  }
}
assert.throws(() => assertGate(plan, { ...needs, test: { result: 'success' } }));
const sha = 'a'.repeat(40), base = 'b'.repeat(40);
const proof = { version: 1, scope: 'hotfix', sha, policy: 'p', reused: false, jobs: plan.jobs,
  hotfix: { base, reason, paths: plan.paths, profiles: plan.profiles, matrix: plan.matrix },
  results: Object.fromEntries(Object.entries(needs).map(([job, value]) => [job, value.result])) };
assertHotfixProof(proof, sha, 'p', paths);
for (const patch of [{ sha: 'wrong' }, { policy: 'old' }, { scope: 'full' }, { reused: true },
  { jobs: PRODUCT_JOBS }, { results: {} }, { hotfix: { ...proof.hotfix, base: '' } },
  { hotfix: { ...proof.hotfix, profiles: ['ci'] } }, { hotfix: { ...proof.hotfix, matrix: { group: ['policy'] } } }]) {
  assert.throws(() => assertHotfixProof({ ...proof, ...patch }, sha, 'p', paths));
}
assert.throws(() => assertHotfixProof(proof, sha, 'p', [...paths, 'scripts/ci_policy.mjs']));
for (const [name, mutate] of Object.entries({
  missing_group: rows => rows.pop(), duplicate_group: rows => rows.push(rows[0]),
  wrong_mode: rows => { rows[0] = 'factory-wrong-random-0-of-8'; },
  wrong_case_group: rows => { rows[0] = 'factory-sync-regressions-1-of-1'; },
})) {
  const group = [...plan.matrix.group];mutate(group);
  assert.throws(() => assertHotfixProof({ ...proof, hotfix: { ...proof.hotfix, matrix: {group} } }, sha, 'p', paths), /matrix/);
  console.log('Hotfix proof rejects ' + name);
}

assert(!validProof(proof, sha, 'p'), 'a hotfix never supplies a site release');
assert(!validProof({ ...proof, jobs: PRODUCT_JOBS }, sha, 'p'), 'a hotfix cannot relabel itself as full coverage');
const run = { id: 1, head_sha: sha, conclusion: 'success', status: 'completed', event: 'workflow_dispatch',
  display_title: 'hotfix: branch', head_repository: { full_name: 'owner/repo' }, workflow_id: 9 };
const context = { sha, repository: 'owner/repo', workflow: 9, current: 0 };
assert(!trustedRun(run, context), 'a hotfix never establishes a regular CI baseline');
assert(trustedRun(run, { ...context, hotfix: true }));
assert(!trustedRun({ ...run, event: 'pull_request' }, { ...context, hotfix: true }));

const sourceRoot = process.cwd(), temporary = mkdtempSync(join(tmpdir(), 'wfsim-board-entry-scope-'));
const [live, split, entryCheck] = HOTFIX_PROFILES['board-entry'].paths;
const originalLive = readFileSync(live, 'utf8').replace(BOARD_ENTRY_CHANGE.liveAfter, BOARD_ENTRY_CHANGE.liveBefore);
const originalSplit = readFileSync(split, 'utf8').split(BOARD_ENTRY_CHANGE.marker)[0] + BOARD_ENTRY_CHANGE.marker + BOARD_ENTRY_CHANGE.footerBefore;
try {
  process.chdir(temporary);
  const git = args => execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const put = (path, body) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, body); };
  const commit = () => {
    git(['add', '-A']);
    git(['-c', 'core.hooksPath=/dev/null', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'scope fixture']);
    return git(['rev-parse', 'HEAD']);
  };
  const diff = (base, head) => git(['diff', '--name-only', '--no-renames', base, head]).split('\n').filter(Boolean);
  git(['init', '-q']);git(['config', 'user.name', 'Scope Check']);git(['config', 'user.email', 'scope@example.test']);
  put(live, originalLive);put(split, originalSplit);put('.github/workflows/ci.yml', 'jobs: {}\n');put('docs/CHECKS.md', 'Registered checks.\n');
  put('scripts/other_runtime.mjs', 'export const limit = 1;\n');put('worker/pg/sync.sql', '-- unchanged\n');
  const base = commit();
  put(live, originalLive.replace(BOARD_ENTRY_CHANGE.liveBefore, BOARD_ENTRY_CHANGE.liveAfter));
  put(split, originalSplit.replace(BOARD_ENTRY_CHANGE.footerBefore, BOARD_ENTRY_CHANGE.footerAfter));
  put(entryCheck, '// isolated CLI regression fixture\n');
  const entry = commit();
  assert.deepEqual(hotfixPlan(diff(base, entry), { base, head: entry, reason }).profiles, ['board-entry']);
  put('.github/workflows/ci.yml', 'jobs: {}\n# policy wiring\n');put('docs/CHECKS.md', 'Registered checks with the CLI regression.\n');
  const head = commit(), paths = diff(base, head), options = { base, head, reason };
  const selected = hotfixPlan(paths, options);
  assert.deepEqual(selected.profiles, ['board-entry', 'ci']);
  assert.deepEqual(selected.matrix.group, ['board-entry', 'policy']);
  const receipt = { ...proof, sha: head, jobs: selected.jobs,
    hotfix: { base, reason, paths: selected.paths, profiles: selected.profiles, matrix: selected.matrix } };
  assertHotfixProof(receipt, head, 'p', paths);
  for (const patch of [{ base: undefined }, { head: undefined }, { base: 'HEAD' }, { head: 'HEAD' }, { base: 'f'.repeat(40) }])
    assert.throws(() => hotfixPlan(paths, { ...options, ...patch }));
  const controls = [
    ['split runtime body', () => put(split, readFileSync(split, 'utf8').replace('if(!process.env.PG_URL)', 'if(!process.env.PG_BAD)'))],
    ['split footer body', () => put(split, readFileSync(split, 'utf8').replace('else await serveBoardStage', "else throw Error('changed');await serveBoardStage"))],
    ['old split footer', () => put(split, originalSplit)],
    ['live runtime body', () => put(live, readFileSync(live, 'utf8') + '\nexit 0\n')],
    ['live command option', () => put(live, readFileSync(live, 'utf8').replace('--max-old-space-size=192', '--max-old-space-size=193'))],
    ['runtime mode', () => chmodSync(split, 0o755)],
    ['extra runtime', () => put('scripts/other_runtime.mjs', 'export const limit = 2;\n')],
    ['another registered runtime profile', () => put('worker/pg/sync.sql', '-- changed contract\n')],
    ['unrelated ordinary docs', () => put('docs/MECHANICS.md', 'Changed mechanics.\n')],
    ['client contract', () => put('docs/CLIENTS.md', 'Changed contract.\n')],
    ['value data', () => put('data/weapons/entry.yaml', 'damage: 1\n')],
    ['missing CLI regression', () => rmSync(entryCheck)],
  ];
  for (const [name, mutate] of controls) {
    git(['reset', '--hard', head]);mutate();const changed = commit(), paths = diff(base, changed);
    assert.throws(() => hotfixPlan(paths, { base, head: changed, reason }), undefined, name);
    assert.throws(() => assertHotfixProof({ ...receipt, sha: changed }, changed, 'p', paths), undefined, name + ' proof');
    if (name === 'extra runtime') assert.throws(() => hotfixPlan(selected.paths, { base, head: changed, reason }), /actual Git diff/);
  }
  git(['reset', '--hard', head]);git(['rm', '-q', entryCheck]);commit();put(entryCheck, '// existing check\n');commit();
  put(live, originalLive);put(split, originalSplit);const existingBase = commit();
  put(live, originalLive.replace(BOARD_ENTRY_CHANGE.liveBefore, BOARD_ENTRY_CHANGE.liveAfter));
  put(split, originalSplit.replace(BOARD_ENTRY_CHANGE.footerBefore, BOARD_ENTRY_CHANGE.footerAfter));
  put(entryCheck, '// changed existing check\n');const existingHead = commit();
  assert.throws(() => hotfixPlan(diff(existingBase, existingHead), { base: existingBase, head: existingHead, reason }), /new regular CLI check/);
  console.log('Board-entry actual Git scopes accept only the approved entry delta; body, footer, mode, extra-runtime and forged-path controls reject.');
} finally { process.chdir(sourceRoot);rmSync(temporary, { recursive: true, force: true }); }

const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
assert.match(workflow, /run-name:.*inputs\.hotfix.*hotfix:/);
assert.match(workflow, /hotfix:\s+description:.*\n\s+type: boolean\n\s+default: false/);
assert.match(workflow, /FORCE_FULL:.*inputs\.full \|\| inputs\.hotfix/);
assert.match(workflow.split('\n  hotfix:\n')[1].split(/\n  [\w-]+:\n/)[0], /timeout-minutes: 20\b/);
assert.match(workflow, /hotfix:\n\s+needs: \[plan, quick\]\n\s+if: needs\.plan\.outputs\.hotfix == 'true'/);
assert.match(workflow, /matrix:.*fromJSON\(needs\.plan\.outputs\.hotfix_matrix\)/);
assert.match(workflow, /run: node scripts\/ci_hotfix\.mjs "\$GROUP"/);
assert.match(workflow, /- run: node scripts\/check_ci_hotfix\.mjs/);
assert.match(workflow, /node scripts\/check_board_cli_entry\.mjs/);
assert.deepEqual([...checkJobPaths(workflow).get('scripts/check_board_cli_entry.mjs')], ['pipeline']);
assert.equal(workflow.split('node scripts/check_board_cli_entry.mjs').length - 1, 1);
assert.match(readFileSync('scripts/ci_policy.mjs', 'utf8'), /head: process\.env\.GITHUB_SHA/);
console.log('Hotfix scopes, required checks, failed gates and separate exact-commit evidence hold.');
