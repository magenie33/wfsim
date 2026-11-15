// SPDX-License-Identifier: AGPL-3.0-or-later
// Real filesystem mutations invalidate a sample after its first file was counted.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const { offlineCapacityGuard } = await import(pathToFileURL(resolve(process.env.WFSIM_CAPACITY_GUARD_MODULE || 'scripts/offline_capacity_guard.mjs')));
const { inventoryDiskBudget } = await import(pathToFileURL(resolve(process.env.WFSIM_DISK_BUDGET_MODULE || 'scripts/inventory_disk_budget.mjs')));
const native = { lstatSync: fs.lstatSync, readdirSync: fs.readdirSync, statfsSync: fs.statfsSync }, reserve = 8 * 1024 ** 3;
const total = reserve + 4 * 1024 ** 2, limit = 1024 ** 2;
let failures = 0, passed = 0;
function patched(run, hooks = {}) {
  const now = Date.now;
  fs.lstatSync = (path, ...args) => { hooks.stat?.(String(path)); const stat = native.lstatSync(path, ...args); return hooks.result?.(String(path), stat) || stat; };
  fs.readdirSync = (path, ...args) => {
    const entries = native.readdirSync(path, ...args).sort((a, b) => String(a.name || a).localeCompare(String(b.name || b)));
    hooks.list?.(String(path)); return entries;
  };
  fs.statfsSync = path => ({ ...native.statfsSync(path), bavail: hooks.lowFree ? 0 : 1024 ** 3, bsize: 4096 });
  syncBuiltinESMExports();
  try { return run(); } finally { Object.assign(fs, native); Date.now = now; syncBuiltinESMExports(); }
}
function fixture(kind, test) {
  const base = fs.mkdtempSync(join(tmpdir(), 'capacity-walk-')), tree = join(base, 'tree');
  fs.mkdirSync(tree); const a = join(tree, 'a'), b = join(tree, 'b'), outside = join(base, 'outside');
  fs.writeFileSync(a, 'a'.repeat(8)); fs.writeFileSync(b, 'b'.repeat(16)); fs.writeFileSync(outside, 'outside');
  const allocation = join(base, 'allocation.json');
  fs.writeFileSync(allocation, JSON.stringify({ directory: tree, max_new_bytes: limit, minimum_host_free_bytes: reserve }));
  const make = () => kind === 'offline' ? offlineCapacityGuard(allocation, tree) : inventoryDiskBudget({ work: tree, totalBytes: total });
  const measured = guard => kind === 'offline' ? guard.report().measured_growth_bytes : guard.report().measured_source_and_work_bytes;
  const initial = guard => kind === 'offline' ? measured(guard) : guard.report().existing_source_and_work_bytes;
  const grow = bytes => { const fd = fs.openSync(a, 'r+'); try { fs.ftruncateSync(fd, bytes); } finally { fs.closeSync(fd); } };
  try { test({ base, tree, a, b, outside, make, measured, initial, grow }); }
  finally { fs.rmSync(base, { recursive: true, force: true }); }
}
function check(kind, name, test) {
  try { fixture(kind, test); passed++; console.log(`ok: ${kind}: ${name}`); }
  catch (error) { failures++; console.error(`FAIL: ${kind}: ${name}: ${error.stack}`); }
}
const enoent = () => Object.assign(Error('concurrent temporary file removal'), { code: 'ENOENT' });
for (const kind of ['offline', 'inventory'].filter(kind => !process.env.WFSIM_CAPACITY_WALK_KIND || kind === process.env.WFSIM_CAPACITY_WALK_KIND)) {
  for (const overflow of [false, true]) check(kind, `deletion restarts the whole sample${overflow ? ' and still rejects overflow' : ''}`, f => {
    let counted = 0, fired = false, active = !overflow;
    patched(() => {
      if (overflow) { const guard = f.make(); counted = 0; active = true; assert.throws(() => guard.check(true), /exceeded/); }
      else { const guard = f.make(); assert.equal(f.initial(guard), 64); assert.equal(f.measured(guard), 64); }
      assert(fired); assert(counted >= 2, 'the already measured file must be measured again');
    }, { stat(path) {
      if (path === f.a) counted++;
      if (path === f.b && active && !fired) {
        assert(counted > 0); fired = true; f.grow(overflow ? (kind === 'offline' ? limit : total) + 1 : 64); fs.unlinkSync(f.b);
      }
    } });
  });
  check(kind, 'continuous ENOENT refuses after three whole attempts', f => {
    let attempts = 0, counted = 0;
    patched(() => {
      assert.throws(f.make, { code: 'ENOENT' }); assert.equal(attempts, 3); assert.equal(counted, 3);
    }, { stat(path) { if (path === f.a) counted++; if (path === f.b) { attempts++; throw enoent(); } } });
  });
  check(kind, 'a failed sample retains the last complete count and does not advance its cooldown', f => {
    let blocked = false;
    patched(() => {
      const guard = f.make(), now = Date.now() + 2000; Date.now = () => now; blocked = true;
      assert.throws(() => guard.check(false), { code: 'ENOENT' }); assert.equal(f.measured(guard), 24);
      blocked = false; f.grow(64); guard.check(false); assert.equal(f.measured(guard), 80);
    }, { stat(path) { if (blocked && path === f.b) throw enoent(); } });
  });
  for (const code of ['EACCES', 'EIO', 'ENOTDIR']) check(kind, `${code} fails without retry`, f => {
    let attempts = 0;
    patched(() => { assert.throws(f.make, { code }); assert.equal(attempts, 1); }, {
      stat(path) { if (path === f.b) { attempts++; throw Object.assign(Error('fixture failure'), { code }); } },
    });
  });
  check(kind, 'deleting the root refuses immediately', f => {
    let attempts = 0;
    patched(() => { assert.throws(f.make, { code: 'ENOENT' }); assert.equal(attempts, 1); }, {
      stat(path) { if (path === f.b) { attempts++; fs.rmSync(f.tree, { recursive: true }); } },
    });
  });
  check(kind, 'replacing the root during a sample refuses even without ENOENT', f => {
    let replaced = false;
    patched(() => assert.throws(f.make, /identity|filesystem changed/), { stat(path) {
      if (path === f.b && !replaced) {
        replaced = true; fs.renameSync(f.tree, join(f.base, 'retained')); fs.mkdirSync(f.tree);
        fs.writeFileSync(f.a, 'new-a'); fs.writeFileSync(f.b, 'new-b');
      }
    } });
  });
  for (const link of [false, true]) check(kind, `a ${link ? 'linked' : 'replaced'} root cannot pass a throttled check`, f => {
    patched(() => {
      const guard = f.make(), retained = join(f.base, 'retained'); fs.renameSync(f.tree, retained);
      if (link) fs.symlinkSync(retained, f.tree, 'dir'); else fs.mkdirSync(f.tree);
      assert.throws(() => guard.check(false), /identity|filesystem changed|cannot contain links/);
    });
  });
  for (const link of [true, false]) check(kind, `fresh lstat handles a stale ${link ? 'file-to-link' : 'file-to-directory'} entry`, f => {
    let replaced = false;
    patched(() => {
      if (link) assert.throws(f.make, /cannot contain links/);
      else { const guard = f.make(); assert.equal(f.initial(guard), 137); assert.equal(f.measured(guard), 137); }
    }, { list(path) {
      if (path === f.tree && !replaced) {
        replaced = true; fs.unlinkSync(f.b);
        if (link) fs.symlinkSync(f.outside, f.b); else { fs.mkdirSync(f.b); fs.writeFileSync(join(f.b, 'child'), 'x'.repeat(129)); }
      }
    } });
  });
  check(kind, 'a child on another filesystem is refused', f => {
    patched(() => assert.throws(f.make, /filesystem/), {
      result(path, stat) { if (path === f.b) return new Proxy(stat, { get(target, key) { return key === 'dev' ? target.dev + 1 : Reflect.get(target, key); } }); },
    });
  });
  check(kind, 'the host free-space reserve is still enforced', f => {
    patched(() => assert.throws(f.make, /reserve/), { lowFree: true });
  });
}
assert(passed > 0, 'no capacity walk cases selected');
assert.equal(failures, 0, `${failures} capacity walk regressions failed`);
console.log(`ok: ${passed} capacity walk regressions passed`);
