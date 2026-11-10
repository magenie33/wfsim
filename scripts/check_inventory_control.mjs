// SPDX-License-Identifier: AGPL-3.0-or-later
// Real SQLite locks and immutable tasks fence generation starts and interrupted full captures.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { InventoryControl, InventoryActionRequired, inspectInventoryTask } from './inventory_control.mjs';

const work = mkdtempSync(join(tmpdir(), 'inventory-control-')), root = join(work, 'inventory'), context = { mode: 'relational', engine: 'a'.repeat(12), contract: { v: 1, covers: ['board'] }, status: null };
let checks = 0, control, child, complete = false;
const check = (name, value) => { assert.ok(value, name); checks++; console.log('ok ' + name); };
try {
  control = new InventoryControl(root);
  let begins = 0;
  const sql = { unsafe: async () => { begins++; return [{ generation: { generation: 'one' } }]; } };
  await assert.rejects(control.begin(context, sql), error => error instanceof InventoryActionRequired && error.stage === 'needs_bootstrap');
  check('background startup cannot create a generation without an explicit task', begins === 0);
  assert.throws(() => control.request({ ...context, reason: 'too large', maxNewBytes: Number.MAX_SAFE_INTEGER }), error => error.stage === 'needs_space');
  check('a request exceeding free space is refused before task admission', control.current() === null);
  const task = control.request({ ...context, reason: 'explicit fixture', maxNewBytes: 1024 ** 3 });
  assert.throws(() => control.db.prepare('UPDATE entity_inventory_rebuild_tasks SET body=? WHERE id=?').run('{}', task.id), /immutable rebuild request/);
  assert.throws(() => control.db.prepare('DELETE FROM entity_inventory_rebuild_tasks WHERE id=?').run(task.id), /rebuild requests are retained/);
  check('immutable request identity and inputs are separate from mutable execution state', control.current().phase === 'requested');
  check('status stays readable while the controller holds its writer lock', inspectInventoryTask(root).id === task.id);
  assert.throws(() => new InventoryControl(root), error => error.stage === 'controller_busy');
  check('a second controller cannot enter the same working directory', true);
  await assert.rejects(control.begin({ ...context, contract: { ...context.contract, changed: true } }, sql), error => error.stage === 'request_changed');
  check('a stale semantic request cannot authorize generation creation', begins === 0);
  const begun = await control.begin(context, sql);
  check('the matching explicit task starts exactly one generation', begins === 1 && begun.generation === 'one');
  check('a committed task binding can restore its local pointer without starting again', control.bound({ ...context, status: { generation: 'one' } }).generation === 'one' && begins === 1);
  await assert.rejects(control.begin(context, sql), error => error.stage === 'needs_bootstrap');
  check('the same authorization cannot be consumed to start another generation', begins === 1);
  control.phase(task.id, 'active', 'one');
  const state = { generation: 'one', rebuild_task: task.id };
  let captures = 0;
  await assert.rejects(control.capture(state, async () => { captures++; writeFileSync(join(root, 'partial-capture'), 'retained'); throw Error('lost MVCC connection'); }, () => {}), /lost MVCC connection/);
  await assert.rejects(control.capture(state, async () => { captures++; }, () => {}), error => error.stage === 'needs_resume');
  check('failed full capture never restarts implicitly or discards its partial evidence', captures === 1 && readFileSync(join(root, 'partial-capture'), 'utf8') === 'retained');
  control.cancel(task.id, 'operator cancelled failed task');
  const next = control.request({ ...context, expectedGeneration: 'one', reason: 'new explicit attempt', maxNewBytes: 1024 ** 3 });
  check('a retry is a new explicit task and preserves the previous request', next.id !== task.id && control.db.prepare('SELECT count(*) n FROM entity_inventory_rebuild_tasks').get().n === 2);
  await assert.rejects(control.begin({ ...context, status: { generation: 'other' } }, sql), error => error.stage === 'request_changed');
  check('the expected database generation fences a stale request', begins === 1);
  control.close(); control = null;

  const code = `import {InventoryControl} from ${JSON.stringify(new URL('./inventory_control.mjs', import.meta.url).href)};const c=new InventoryControl(process.argv[1]);process.stdout.write('locked\\n');setInterval(()=>{},1000);await new Promise(()=>{});`;
  child = spawn(process.execPath, ['--input-type=module', '-e', code, root], { stdio: ['ignore', 'pipe', 'pipe'] });
  await Promise.race([once(child.stdout, 'data'), new Promise((_, reject) => setTimeout(() => reject(Error('lock child startup timed out')), 5000))]);
  assert.throws(() => new InventoryControl(root), error => error.stage === 'controller_busy');
  check('the writer exclusion applies across actual processes', true);
  const stopped = once(child, 'exit'); child.kill('SIGKILL'); await stopped; child = null;
  control = new InventoryControl(root);
  check('process death releases the kernel lock without erasing task history', control.current().id === next.id);
  control.close(); control = null;
  const cli = (...args) => JSON.parse(execFileSync(process.execPath, [fileURLToPath(new URL('./inventory_rebuild.mjs', import.meta.url)), ...args, '--work', dirname(root)], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 10000 }));
  check('the deployed status command reads the durable task without starting it', cli('status').task.phase === 'requested');
  const cancelled = cli('cancel', '--task', next.id, '--reason', 'explicit CLI cancellation').task;
  check('the deployed cancel command retains the task identity and records cancellation', cancelled.id === next.id && cancelled.phase === 'cancelled');
  complete = true;
  console.log(JSON.stringify({ ok: true, checks, production_changes: false }));
} finally {
  if (child) child.kill('SIGKILL'); control?.close();
  if (complete) rmSync(work, { recursive: true, force: true }); else console.error('retained controller fixture: ' + work);
}
