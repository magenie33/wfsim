// Run the real shell loop with local transports; incomplete inventory cannot publish.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scripts = dirname(fileURLToPath(import.meta.url));
const requested = process.argv.indexOf('--script');
const source = requested < 0 ? join(scripts, 'live_board.sh') : resolve(process.argv[requested + 1]);
const bash = process.env.WFSIM_TEST_BASH || (existsSync('/opt/homebrew/bin/bash') ? '/opt/homebrew/bin/bash' : '/bin/bash');
const version = spawnSync(bash, ['-c', 'printf "%s" "${BASH_VERSINFO[0]}"'], { encoding: 'utf8' });
assert(version.status === 0 && Number(version.stdout) >= 4, 'live inventory check requires Bash4 or newer');
const root = mkdtempSync(join(tmpdir(), 'live-inventory-'));
const bin = join(root, 'bin'), fixtureScripts = join(root, 'scripts');
mkdirSync(bin); mkdirSync(fixtureScripts);
copyFileSync(source, join(fixtureScripts, 'live_board.sh'));
copyFileSync(join(scripts, 'db.sh'), join(fixtureScripts, 'db.sh'));
const executable = (name, body) => writeFileSync(join(bin, name), '#!' + process.execPath + '\n' + body, { mode: 0o755 });

const stub = String.raw`
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const home = process.env.CHECK_LIVE_DIR;
const event = (kind, extra = {}) => fs.appendFileSync(path.join(home, 'events.ndjson'), JSON.stringify({ kind, pid: process.pid, ...extra }) + '\n');
const control = () => JSON.parse(fs.readFileSync(path.join(home, 'control.json'), 'utf8'));
const tool = path.basename(process.argv[1]);
const stopping = kind => {
  for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => { event(kind); process.exit(0); });
};
if (tool === 'sleep') { setTimeout(() => {}, 40); }
else if (tool === 'curl') { event('unexpected-network'); process.exit(99); }
else if (tool === 'flock') {
  const args = process.argv.slice(2);
  if (args[0] === '-n') {
    const result = cp.spawnSync(args[2], args.slice(3), { stdio: 'inherit', env: process.env });
    process.exit(result.status ?? 99);
  }
} else if (tool === 'wfsim-board') {
  stopping('ranker-stopped'); event('ranker-start');
  const out = process.argv[3]; let epoch = null, step = 0;
  process.stdout.write('ok ready 0\n');
  const lines = require('node:readline').createInterface({ input: process.stdin });
  lines.on('line', command => {
    const c = control();
    if (command === 'publish' || command.startsWith('publish_step')) {
      if (epoch !== c.epoch) { epoch = c.epoch; step = 0; }
      step++;
      const partial = command !== 'publish' && step <= (c.native_pending_steps || 0);
      event(command === 'publish' ? 'native-publish' : 'native-step', { epoch, step, partial, mode: c.mode });
      fs.mkdirSync(out, { recursive: true });
      fs.writeFileSync(path.join(out, 'native.json'), JSON.stringify({ epoch, step, partial }));
      if (command === 'publish') process.stdout.write('ok published 1\n');
      else process.stdout.write('ok publish_step 1 0 ' + (partial ? '1' : '0') + '\n');
    } else { event('unexpected-ranker', { command }); process.stdout.write('error unexpected command\n'); }
  });
} else if (tool === 'python3') {
  const args = process.argv.slice(2), c = control();
  if (path.basename(args[0]) !== 'board_meta.py') { event('unexpected-python'); process.exit(99); }
  event(c.stamp_fail ? 'stamp-failed' : 'stamp', { epoch: c.epoch });
  if (c.stamp_fail) process.exit(1);
  fs.writeFileSync(args[3], JSON.stringify({ epoch: c.epoch, fresh: true }));
} else if (tool === 'node') {
  const args = process.argv.slice(2);
  while (args[0]?.startsWith('--')) args.shift();
  const script = path.basename(args.shift() || ''), c = control();
  if (script === 'inventory_pipeline.mjs') {
    const work = args[0]; fs.mkdirSync(path.join(work, 'inventory'), { recursive: true });
    event('inventory', { mode: c.mode, epoch: c.epoch, pending: c.mode === 'pending' });
    if (c.mode === 'fail') { event('inventory-failed'); process.exit(1); }
    fs.writeFileSync(path.join(work, 'inventory/status.json'), JSON.stringify({ pending: c.mode === 'pending', engine_epoch: c.epoch }));
  } else if (script === 'board_heads.mjs') event('heads', { epoch: c.epoch });
  else if (script === 'live_publish.mjs' && args[0] === 'push') {
    const meta = JSON.parse(fs.readFileSync(path.join(args[1], args[2], 'meta.json'), 'utf8'));
    event('upload-start', { meta }); stopping('upload-stopped');
    const release = c.upload_release_seq || 0;
    setInterval(() => { if ((control().upload_release_seq || 0) > release) { event('upload-finished'); process.exit(0); } }, 20);
  } else { event('unexpected-node', { script }); process.exit(99); }
} else { event('unexpected-tool', { tool }); process.exit(99); }
`;
for (const name of ['node', 'python3', 'flock', 'sleep', 'curl', 'wfsim-board']) executable(name, stub);
const delay = ms => new Promise(resolveDelay => setTimeout(resolveDelay, ms));
const alive = pid => { try { process.kill(pid, 0); return true; } catch (error) { if (error.code === 'ESRCH') return false; throw error; } };
function events(folder) {
  const path = join(folder, 'events.ndjson');
  if (!existsSync(path)) return [];
  const text = readFileSync(path, 'utf8');
  return text.slice(0, text.lastIndexOf('\n') + 1).split('\n').filter(Boolean).map(line => JSON.parse(line));
}
async function until(predicate, message, milliseconds = 12000) {
  const end = Date.now() + milliseconds;
  while (Date.now() < end) { if (predicate()) return; await delay(25); }
  assert.fail(message);
}
const count = (folder, kind) => events(folder).filter(row => row.kind === kind).length;
function control(folder, value) {
  const partial = join(folder, 'control.partial'); writeFileSync(partial, JSON.stringify(value)); renameSync(partial, join(folder, 'control.json'));
}
async function exited(child, milliseconds) {
  if (child.exitCode !== null || child.signalCode !== null) return true;
  return Promise.race([new Promise(resolveExit => child.once('exit', () => resolveExit(true))), delay(milliseconds).then(() => false)]);
}

const sentinel = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { detached: true, stdio: 'ignore' });
async function scenario(name, initial, check) {
  const folder = join(root, name), work = join(folder, 'work'); mkdirSync(join(work, 'verified'), { recursive: true });
  writeFileSync(join(work, 'library.json'), '[]'); writeFileSync(join(work, 'facts-known.ndjson'), '');
  writeFileSync(join(work, 'verified/meta.json'), JSON.stringify({ cached: true }));
  control(folder, initial);
  const child = spawn(bash, [join(fixtureScripts, 'live_board.sh'), work, bin], {
    cwd: folder, detached: true, stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, PATH: bin + ':' + dirname(process.execPath) + ':/opt/homebrew/bin:/usr/bin:/bin',
      CHECK_LIVE_DIR: folder, PG_URL: '', FACTORY_TOKEN: '', BOARD_PUSH_TOKEN: '',
      INCREMENTAL_INVENTORY: '1', POLL_SECONDS: '0.05', REFRESH_SECONDS: '3600', TMPDIR: folder },
  });
  let errors = ''; child.stderr.on('data', bytes => { errors = (errors + bytes).slice(-12000); }); child.stdout.resume();
  try {
    await check(folder, work);
    assert(!events(folder).some(row => row.kind.startsWith('unexpected-')), 'the shell used an unstubbed transport or command');
  } catch (error) { console.error('live inventory fixture:', folder, '\n' + errors.slice(-2000)); throw error; }
  finally {
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
    const normalExit = await exited(child, 2000);
    const owned = events(folder).filter(row => ['ranker-start', 'upload-start'].includes(row.kind)).map(row => row.pid);
    try {
      await until(() => owned.every(pid => !alive(pid)), 'EXIT left an owned ranker or uploader alive', 1500);
      assert(normalExit, 'the shell did not finish its EXIT cleanup');
      assert(alive(sentinel.pid), 'cleanup stopped an unrelated child');
    } finally {
      try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
  }
}

let passed = false;
try {
  await scenario('pending-and-steps', { mode: 'pending', epoch: 'cut-a', native_pending_steps: 2 }, async (folder, work) => {
    await until(() => events(folder).filter(row => row.kind === 'inventory' && row.epoch === 'cut-a').length >= 2, 'pending inventory did not repeat');
    control(folder, { mode: 'pending', epoch: 'cut-b', native_pending_steps: 2 });
    await until(() => events(folder).filter(row => row.kind === 'inventory' && row.epoch === 'cut-b').length >= 2, 'pending engine epoch did not advance');
    for (const kind of ['native-publish', 'native-step', 'heads', 'stamp', 'upload-start']) assert.equal(count(folder, kind), 0, 'pending inventory reached ' + kind);
    assert.deepEqual(JSON.parse(readFileSync(join(work, 'verified/meta.json'), 'utf8')), { cached: true });
    control(folder, { mode: 'ready', epoch: 'cut-b', native_pending_steps: 2 });
    await until(() => count(folder, 'upload-start') === 1, 'completed inventory/native steps did not upload');
    const rows = events(folder), steps = rows.filter(row => row.kind === 'native-step');
    assert.deepEqual(steps.map(row => row.partial), [true, true, false]);
    for (const [index, step] of steps.entries()) {
      assert.equal(step.mode, 'ready');
      if (index) assert(rows.slice(rows.indexOf(steps[index - 1]) + 1, rows.indexOf(step)).some(row => row.kind === 'inventory'), 'native steps blocked the inventory loop');
    }
    assert(rows.indexOf(rows.find(row => row.kind === 'stamp')) > rows.indexOf(steps[2]), 'metadata preceded native completion');
    assert.equal(count(folder, 'heads'), 1); assert.equal(count(folder, 'stamp'), 1); assert.equal(count(folder, 'native-publish'), 0);
    const cycles = count(folder, 'inventory'), uploader = rows.find(row => row.kind === 'upload-start');
    control(folder, { mode: 'ready', epoch: 'cut-c', native_pending_steps: 2, upload_release_seq: 0 });
    await until(() => count(folder, 'inventory') >= cycles + 3, 'blocked uploader stopped inventory progress');
    assert(alive(uploader.pid)); assert.equal(count(folder, 'upload-start'), 1); assert.equal(count(folder, 'stamp'), 1);
    assert.equal(count(folder, 'native-step'), 3, 'a native writer ran while the owned uploader was reading');
    assert.deepEqual(uploader.meta, { epoch: 'cut-b', fresh: true });
    control(folder, { mode: 'ready', epoch: 'cut-c', native_pending_steps: 2, upload_release_seq: 1 });
    await until(() => count(folder, 'stamp') === 2 && count(folder, 'upload-start') === 2, 'native writer did not resume after uploader completion');
    const resumed = events(folder), nextSteps = resumed.filter(row => row.kind === 'native-step' && row.epoch === 'cut-c');
    assert.deepEqual(nextSteps.map(row => row.partial), [true, true, false]);
    assert(resumed.indexOf(resumed.find(row => row.kind === 'upload-finished')) < resumed.indexOf(nextSteps[0]), 'the next writer preceded uploader completion');
  });
  console.log('ok: pending inventory and native steps withhold publication; completed output uploads once while cycles continue');
  await scenario('inventory-failure', { mode: 'fail', epoch: 'failed-cut' }, async folder => {
    await until(() => count(folder, 'inventory-failed') >= 2, 'inventory failure did not finish another cycle');
    for (const kind of ['native-publish', 'native-step', 'heads', 'stamp', 'upload-start']) assert.equal(count(folder, kind), 0, 'failed inventory reached ' + kind);
  });
  console.log('ok: failed inventory never stamps or uploads cached metadata');
  await scenario('stamp-retry', { mode: 'ready', epoch: 'cut-stamp', stamp_fail: true }, async folder => {
    await until(() => count(folder, 'stamp-failed') >= 2, 'failed stamp was not retried');
    assert.equal(count(folder, 'upload-start'), 0, 'cached metadata uploaded after a failed stamp');
    const steps = count(folder, 'native-step');
    control(folder, { mode: 'ready', epoch: 'cut-stamp', stamp_fail: false });
    await until(() => count(folder, 'upload-start') === 1, 'successful stamp retry never uploaded');
    assert.equal(count(folder, 'native-step'), steps, 'stamp retry repeated native publication');
    assert.equal(count(folder, 'stamp'), 1);
    const prior = events(folder), uploader = prior.find(row => row.kind === 'upload-start');
    const ranks = count(folder, 'ranker-start');
    control(folder, { mode: 'fail', epoch: 'cut-stamp' });
    await until(() => count(folder, 'ranker-start') > ranks && count(folder, 'inventory-failed') >= 2, 'failed inventory did not restart its ranker');
    assert(!alive(uploader.pid), 'failed inventory restarted a writer while its uploader remained alive');
    const failed = events(folder), stopped = failed.find(row => row.kind === 'upload-stopped' && row.pid === uploader.pid);
    const restart = failed.filter(row => row.kind === 'ranker-start')[ranks];
    assert(stopped && failed.indexOf(stopped) < failed.indexOf(restart), 'uploader was not stopped before ranker restart');
  });
  console.log('ok: stamp failures keep cached metadata private and a successful retry uploads without another publish');
  passed = true;
} finally {
  sentinel.kill('SIGTERM'); if (!await exited(sentinel, 1000)) sentinel.kill('SIGKILL');
  if (passed) rmSync(root, { recursive: true, force: true });
}
