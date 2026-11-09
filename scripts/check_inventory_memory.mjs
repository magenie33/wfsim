// SPDX-License-Identifier: AGPL-3.0-or-later
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE = resolve(process.env.WFSIM_INVENTORY_MEMORY_SOURCE || ROOT);
const phase = spawnSync('python3', [join(ROOT, 'scripts/check_inventory_phase.py')], {
  encoding: 'utf8', timeout: 10000,
  env: { ...process.env, WFSIM_INVENTORY_PHASE_SOURCE: join(SOURCE, 'scripts/inventory_phase.py') },
});
if (phase.status !== 0) throw new Error(phase.stdout + phase.stderr);
const WORKFLOWS = resolve(process.env.WFSIM_WORKFLOW_DIR || join(SOURCE, '.github/workflows'));
const dir = mkdtempSync(join(tmpdir(), 'wfsim-inventory-memory-'));
const bin = join(dir, 'bin');
mkdirSync(bin);
let bad = 0, count = 0;
const check = (ok, name, detail = '') => {
  count += 1;
  console.log(`  ${ok ? 'ok  ' : 'FAIL'}  ${name}${ok || !detail ? '' : ` — ${detail}`}`);
  if (!ok) bad += 1;
};

// Privileged and allocating commands only change tiny fixture files.
const stub = `#!${process.execPath}
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const name = path.basename(process.argv[1]), args = process.argv.slice(2), e = process.env;
fs.appendFileSync(e.MEMORY_LOG, JSON.stringify([name, ...args]) + '\\n');
const state = JSON.parse(fs.readFileSync(e.MEMORY_STATE, 'utf8'));
const save = () => fs.writeFileSync(e.MEMORY_STATE, JSON.stringify(state));
if (name === 'sudo') {
  if (!['fallocate', 'mkswap', 'swapon', 'chmod', 'rm'].includes(args[0])) process.exit(97);
  const r = cp.spawnSync(path.join(e.MEMORY_BIN, args[0]), args.slice(1), { stdio: 'inherit' });
  process.exit(r.status ?? 98);
}
if (name === 'uname') console.log(e.MEMORY_KERNEL || 'Linux');
else if (name === 'free') console.log('Mem: 17000000000 0 17000000000');
else if (name === 'df') {
  const lowPath = e.MEMORY_LOW_PATH && fs.realpathSync(e.MEMORY_LOW_PATH) === fs.realpathSync(args.at(-1));
  const low = lowPath && !(e.MEMORY_RECOVER === 'yes' && state.cleaned) || (e.MEMORY_LOW_AFTER === 'yes' && state.allocated);
  console.log('Filesystem 1024-blocks Used Available Capacity Mounted on\\nfake 500000000 1 ' + (low ? 1 : 400000000) + ' 1% /');
} else if (name === 'fallocate') {
  state.allocated = args.at(-1); state.bytes = args[args.indexOf('-l') + 1]; save();
  fs.writeFileSync(state.allocated, 'tiny fake swap');
} else if (name === 'stat') console.log(e.MEMORY_WRONG_SIZE === 'yes' ? 4096 : state.bytes);
else if (name === 'chmod') fs.chmodSync(args.at(-1), Number.parseInt(args[0], 8));
else if (name === 'rm') { state.cleaned = args; save(); }
else if (name === 'mkswap') { if (e.MEMORY_FAIL === 'mkswap') process.exit(42); }
else if (name === 'swapon') {
  if (args.some(arg => arg.startsWith('--show'))) {
    if (!args.includes('--show=NAME')) console.log('NAME TYPE SIZE USED PRIO');
    for (const file of state.active) console.log(args.includes('--show=NAME') ? file : file + ' file 2147483648 0 -2');
  } else {
    if (e.MEMORY_FAIL === 'swapon') process.exit(43);
    if (e.MEMORY_FAIL !== 'inactive') { state.active.push(args.at(-1)); save(); }
  }
} else process.exit(99);
`;
for (const name of ['sudo', 'df', 'fallocate', 'mkswap', 'swapon', 'stat', 'uname', 'free', 'chmod', 'rm', 'swapoff', 'sysctl', 'dd']) {
  writeFileSync(join(bin, name), stub, { mode: 0o755 });
}

let serial = 0;
function run(extra = {}, setup = () => {}) {
  const fixture = join(dir, String(++serial));
  const work = join(fixture, 'work'), temp = join(fixture, 'temp'), runner = join(fixture, 'runner');
  for (const path of [work, temp, runner]) mkdirSync(path, { recursive: true });
  const log = join(fixture, 'calls'), statePath = join(fixture, 'state.json');
  const old = join(fixture, 'existing.swap'), swap = join(runner, 'wfsim-inventory.swap');
  writeFileSync(old, 'existing swap');
  writeFileSync(log, '');
  writeFileSync(statePath, JSON.stringify({ active: [old], allocated: null }));
  const paths = { fixture, work, temp, runner, swap, old };
  setup(paths);
  const values = typeof extra === 'function' ? extra(paths) : extra;
  const environment = { ...process.env, PATH: `${bin}:${process.env.PATH}`, GITHUB_ACTIONS: 'true',
      INVENTORY_RUNNER_ENVIRONMENT: 'github-hosted', INVENTORY_RUNNER_OS: 'Linux',
      RUNNER_TEMP: runner, TMPDIR: temp, MEMORY_LOG: log, MEMORY_STATE: statePath, MEMORY_BIN: bin,
      MEMORY_FAIL: '', MEMORY_LOW_PATH: '', MEMORY_LOW_AFTER: '', MEMORY_WRONG_SIZE: '', MEMORY_KERNEL: '', MEMORY_RECOVER: '', ...values };
  const result = spawnSync('bash', [join(SOURCE, 'scripts/prepare_inventory_memory.sh')], {
    cwd: work, encoding: 'utf8', timeout: 10000, maxBuffer: 1024 * 1024, env: environment,
  });
  return { ...result, ...paths, log, statePath, environment, calls: readFileSync(log, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse),
    state: JSON.parse(readFileSync(statePath, 'utf8')), oldContent: readFileSync(old, 'utf8') };
}
function recheck(prepared, extra = {}) {
  const result = spawnSync('bash', [join(SOURCE, 'scripts/prepare_inventory_memory.sh'), '--check'], {
    cwd: prepared.work, encoding: 'utf8', timeout: 10000, env: { ...prepared.environment, ...extra },
  });
  return { ...result, calls: readFileSync(prepared.log, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse),
    state: JSON.parse(readFileSync(prepared.statePath, 'utf8')) };
}
const called = (result, name) => result.calls.some(call => call[0] === name);
const allocated = result => called(result, 'fallocate');
const activated = result => result.calls.some(call => call[0] === 'swapon' && !call[1]?.startsWith('--show'));
const refusedBeforeAllocation = result => result.status !== 0 && !allocated(result) && !activated(result);

try {
  const wired = [['queue.yml', 'take-in']].map(([name, job]) => {
    const lines = readFileSync(join(WORKFLOWS, name), 'utf8').split('\n');
    const at = lines.indexOf(`  ${job}:`), next = lines.findIndex((line, i) => i > at && /^ {2}[A-Za-z0-9_-]+:/.test(line));
    const body = lines.slice(at, next < 0 ? lines.length : next).join('\n');
    const steps = body.split(/(?=^ {6}- )/m), memory = steps.filter(step => step.includes('- name: prepare complete inventory memory'));
    const index = steps.indexOf(memory[0]);
    return memory.length === 1 && /if: steps\.wired\.outputs\.yes == 'true'/.test(memory[0])
      && memory[0].includes('INVENTORY_RUNNER_ENVIRONMENT: ${{ runner.environment }}')
      && memory[0].includes('INVENTORY_RUNNER_OS: ${{ runner.os }}')
      && memory[0].includes('run: bash scripts/prepare_inventory_memory.sh')
      && steps.findIndex(step => step.includes('id: wired')) < index
      && steps.findIndex(step => step.includes('id: intake')) > index
      && steps.findIndex(step => step.includes('bash scripts/reconcile_queue.sh')) > index;
  });
  check(wired.every(Boolean), 'full queue maintenance prepares hosted memory before its complete capture');
  const afterSetup = ['queue.yml'].map(name => {
    const text = readFileSync(join(WORKFLOWS, name), 'utf8'), steps = text.split(/(?=^ {6}- )/m);
    const scan = steps.findIndex(step => step.includes('bash scripts/reconcile_queue.sh'));
    const guard = steps.findIndex(step => step.includes('- name: check complete inventory scratch'));
    return guard > 0 && guard === scan - 1 && steps[guard].includes('run: bash scripts/prepare_inventory_memory.sh --check')
      && steps.every((step, index) => !step.includes('cargo build') && !step.includes('actions/cache@') || index < guard || index > scan);
  });
  check(afterSetup.every(Boolean), 'full queue maintenance rechecks scratch immediately before capture');
  const success = run();
  const checked = recheck(success);
  check(checked.status === 0 && checked.calls.filter(call => call[0] === 'fallocate').length === 1
    && checked.state.active.includes(success.swap) && checked.state.active.includes(success.old),
  'a post-setup recheck preserves the prepared swap without allocating it again', checked.stderr);
  const cached = run(), low = recheck(cached, { MEMORY_LOW_PATH: cached.work });
  check(low.status !== 0 && low.stderr.includes('insufficient scratch after cached builds')
    && low.calls.filter(call => call[0] === 'fallocate').length === 1,
  'cache growth below the scratch floor stops before complete capture', low.stderr);
  const reclaimable = run(), reclaimed = recheck(reclaimable, { MEMORY_LOW_PATH: reclaimable.work, MEMORY_RECOVER: 'yes' });
  check(reclaimed.status === 0 && reclaimed.state.cleaned && reclaimed.calls.filter(call => call[0] === 'fallocate').length === 1,
    'a post-cache recheck can release only the existing unused SDK allowance', reclaimed.stderr);
  const missing = run(); rmSync(missing.swap);
  check(recheck(missing).status !== 0, 'a post-setup check refuses a missing prepared swap');
  const lostSwap = run(); writeFileSync(lostSwap.statePath, JSON.stringify({ ...lostSwap.state, active: [lostSwap.old] }));
  check(recheck(lostSwap).status !== 0, 'a post-setup check refuses an inactive prepared swap');
  check(success.status === 0 && success.state.bytes === '25769803776' && success.state.active.includes(success.swap)
    && success.state.active.includes(success.old) && success.oldContent === 'existing swap'
    && success.calls.filter(call => call[0] === 'fallocate').length === 1
    && !success.calls.some(call => ['swapoff', 'sysctl', 'dd'].includes(call[0]))
    && readFileSync(success.swap).length < 1024,
  'a new fixed 24GiB swap preserves existing swaps without large test allocation', success.stderr);
  const hosts = [{ GITHUB_ACTIONS: 'false' }, { INVENTORY_RUNNER_ENVIRONMENT: 'self-hosted' },
    { INVENTORY_RUNNER_OS: 'macOS' }, { MEMORY_KERNEL: 'Darwin' }].map(env => run(env));
  check(hosts.every(refusedBeforeAllocation), 'nonhosted and nonLinux environments refuse before allocation');
  const paths = [run({ RUNNER_TEMP: 'relative' }), run(p => ({ RUNNER_TEMP: join(p.fixture, 'absent') })),
    run(p => ({ RUNNER_TEMP: p.old }))];
  check(paths.every(refusedBeforeAllocation), 'runner temp must be an existing absolute directory');
  const disks = ['work', 'temp', 'runner'].map(key => run(p => ({ MEMORY_LOW_PATH: p[key] })));
  check(disks.every(refusedBeforeAllocation), 'low workspace, TMPDIR or runner disk refuses before allocation');
  const recovered = run(p => ({ MEMORY_LOW_PATH: p.work, MEMORY_RECOVER: 'yes' }));
  check(recovered.status === 0 && !called(success, 'rm')
    && JSON.stringify(recovered.state.cleaned) === JSON.stringify(['-rf', '--', '/usr/local/lib/android', '/usr/share/dotnet', '/opt/ghc'])
    && recovered.calls.findIndex(call => call[0] === 'rm') < recovered.calls.findIndex(call => call[0] === 'fallocate'),
  'low disk may reclaim only the three unused SDK directories before rechecking');
  const post = run({ MEMORY_LOW_AFTER: 'yes' });
  check(post.status !== 0 && allocated(post) && post.calls.filter(call => call[0] === 'df').length > 3,
    'postallocation scratch space is checked and insufficient space fails');
  const existing = run({}, p => writeFileSync(p.swap, 'do not overwrite'));
  const linked = run({}, p => symlinkSync(p.old, p.swap));
  check(refusedBeforeAllocation(existing) && readFileSync(existing.swap, 'utf8') === 'do not overwrite'
    && refusedBeforeAllocation(linked) && linked.oldContent === 'existing swap',
  'existing swap file and symlink targets are never overwritten');
  const wrong = run({ MEMORY_WRONG_SIZE: 'yes' });
  check(wrong.status !== 0 && allocated(wrong) && !called(wrong, 'mkswap') && !activated(wrong),
    'wrong allocated size cannot reach formatting or activation');
  const format = run({ MEMORY_FAIL: 'mkswap' });
  check(format.status !== 0 && called(format, 'mkswap') && !activated(format), 'formatting failure cannot activate swap');
  const activation = run({ MEMORY_FAIL: 'swapon' });
  check(activation.status !== 0 && activated(activation), 'activation command failure remains a failure');
  const inactive = run({ MEMORY_FAIL: 'inactive' });
  check(inactive.status !== 0 && activated(inactive) && !inactive.state.active.includes(inactive.swap),
    'successful activation command without the exact active path is refused');

  const loop = readFileSync(join(SOURCE, 'scripts/reconcile_queue.sh'), 'utf8')
    .match(/^while IFS= read -r id; do\n[\s\S]*?^done < "\$roster"/m)?.[0] || '';
  function merge(fail) {
    const fixture = join(dir, `merge-${fail}`), parts = join(fixture, 'parts'), commands = join(fixture, 'bin');
    mkdirSync(parts, { recursive: true });
    mkdirSync(commands);
    const missing = join(fixture, 'missing'), orphans = join(fixture, 'orphans'), roster = join(fixture, 'rulers');
    writeFileSync(missing, ''); writeFileSync(orphans, ''); writeFileSync(roster, 'b\na\n');
    for (const id of ['b', 'a']) for (const kind of ['missing', 'orphans']) writeFileSync(join(parts, `${id}.${kind}`), `${id}.${kind}\n`);
    writeFileSync(join(commands, 'cat'), '#!/usr/bin/env bash\nif [ "$MERGE_FAIL" = yes ] && [[ "$1" == */a.orphans ]]; then exit 44; fi\nexec /bin/cat "$@"\n', { mode: 0o755 });
    const result = spawnSync('bash', ['-c', `set -euo pipefail\n${loop}`], { cwd: fixture, encoding: 'utf8', timeout: 5000,
      env: { ...process.env, PATH: `${commands}:${process.env.PATH}`, parts, missing, orphans, roster, MERGE_FAIL: fail ? 'yes' : 'no' } });
    return { ...result, parts, missing: readFileSync(missing, 'utf8'), orphans: readFileSync(orphans, 'utf8') };
  }
  const merged = merge(false);
  check(loop && merged.status === 0 && merged.missing === 'b.missing\na.missing\n' && merged.orphans === 'b.orphans\na.orphans\n'
    && ['a', 'b'].every(id => ['missing', 'orphans'].every(kind => !existsSync(join(merged.parts, `${id}.${kind}`)))),
  'successful merge preserves roster byte order and releases each copied pair');
  const interrupted = merge(true);
  check(loop && interrupted.status !== 0 && ['missing', 'orphans'].every(kind => existsSync(join(interrupted.parts, `a.${kind}`))),
    'a failed second copy exits without deleting either failed ruler input');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`Inventory memory: ${count - bad}/${count} passed.`);
if (bad) throw new Error(`${bad} inventory memory checks failed`);
