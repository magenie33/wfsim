// Confirmed uploads survive bounded cycles and interrupted ledger replacement.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, renameSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { pushBoard, savePushed } from './live_publish.mjs';

const root = mkdtempSync(join(tmpdir(), 'live-publish-'));
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const ledger = work => existsSync(join(work, 'pushed.json')) ? JSON.parse(readFileSync(join(work, 'pushed.json'), 'utf8')) : {};
const names = ['a.json', 'b.json', 'scene/c.json', 'meta.json'];
function fixture(name) {
  const workDir = join(root, name), boardDir = join(workDir, 'board');
  mkdirSync(join(boardDir, 'scene'), { recursive: true });
  for (const name of names) writeFileSync(join(boardDir, name), JSON.stringify({ name }));
  return { workDir, boardDir };
}
const filename = url => new URL(url).pathname.split('/api/board/live/')[1];
const ok = () => Response.json({ ok: true });
const aborting = async (_url, { signal }) => new Promise((_, reject) => {
  assert(signal instanceof AbortSignal);
  const guard = setTimeout(() => reject(new Error('request was not aborted')), 1000);
  signal.addEventListener('abort', () => { clearTimeout(guard); reject(signal.reason); }, { once: true });
});

try {
  {
    const input = fixture('file-bound'), calls = [];
    const send = async url => {
      const name = filename(url);
      if (calls.length) assert(ledger(input.workDir)[calls.at(-1)], 'the next upload preceded its confirmed checkpoint');
      calls.push(name); return ok();
    };
    assert.deepEqual(await pushBoard({ ...input, maxFiles: 2, send }), { sent: 2, pending: true });
    assert.deepEqual(calls, names.slice(0, 2)); assert(!ledger(input.workDir)['meta.json']);
    assert.deepEqual(await pushBoard({ ...input, maxFiles: 2, send }), { sent: 2, pending: false });
    assert.deepEqual(calls, names);
    for (const name of names) assert.equal(ledger(input.workDir)[name], sha(readFileSync(join(input.boardDir, name))));
    assert.deepEqual(await pushBoard({ ...input, send: () => assert.fail('unchanged files were sent again') }), { sent: 0, pending: false });
    console.log('ok: file-bounded cycles checkpoint each confirmed file, resume and publish meta last');
  }
  {
    const input = fixture('clock-bound'), calls = []; let clock = 0;
    const stopped = await pushBoard({ ...input, budgetMs: 10, now: () => clock,
      send: async url => { calls.push(filename(url)); clock = 10; return ok(); } });
    assert.deepEqual(stopped, { sent: 1, pending: true }); assert.deepEqual(calls, ['a.json']);
    assert(!ledger(input.workDir)['meta.json']);
    console.log('ok: the cycle deadline withholds the stamp and leaves confirmed progress resumable');
  }
  {
    const input = fixture('failure'), calls = [];
    await assert.rejects(pushBoard({ ...input, send: async url => {
      const name = filename(url); calls.push(name);
      if (name === 'b.json') throw new Error('connection cut');
      return ok();
    } }), /connection cut/);
    assert.deepEqual(calls, ['a.json', 'b.json']); assert.deepEqual(Object.keys(ledger(input.workDir)), ['a.json']);
    const resumed = [];
    await pushBoard({ ...input, send: async url => { resumed.push(filename(url)); return ok(); } });
    assert.deepEqual(resumed, names.slice(1));
    console.log('ok: a failed upload is retried only next cycle without losing earlier confirmed checkpoints');
  }
  {
    const input = fixture('lost-reply'), remote = new Map(), calls = [];
    await assert.rejects(pushBoard({ ...input, send: async (url, init) => {
      const name = filename(url); remote.set(name, sha(init.body)); calls.push(name); throw new Error('reply lost after PUT');
    } }), /reply lost/);
    assert.deepEqual(ledger(input.workDir), {}); assert.deepEqual(calls, ['a.json']);
    await pushBoard({ ...input, send: async (url, init) => { const name = filename(url); remote.set(name, sha(init.body)); calls.push(name); return ok(); } });
    assert.equal(remote.size, names.length); assert.deepEqual(calls, ['a.json', ...names]);
    console.log('ok: a committed PUT with a lost reply resumes idempotently');
  }
  {
    const input = fixture('timeout');
    await assert.rejects(pushBoard({ ...input, requestMs: 10, send: aborting }), /timeout/i);
    assert.deepEqual(ledger(input.workDir), {});
    assert.deepEqual(await pushBoard({ ...input, budgetMs: 10, requestMs: 1000, send: aborting }), { sent: 0, pending: true });
    assert.deepEqual(ledger(input.workDir), {});
    await pushBoard({ ...input, send: async () => ok() });
    console.log('ok: per-request timeout is finite; a cycle-deadline abort yields normally without advancing meta');
  }
  {
    const input = fixture('atomic'), path = join(input.workDir, 'pushed.json');
    savePushed(path, { 'a.json': 'confirmed' }); const prior = readFileSync(path, 'utf8');
    assert.throws(() => savePushed(path, { 'b.json': 'next' }, partial => {
      assert.deepEqual(JSON.parse(readFileSync(partial, 'utf8')), { 'b.json': 'next' });
      assert.equal(readFileSync(path, 'utf8'), prior); throw new Error('cut before rename');
    }), /cut before rename/);
    assert.equal(readFileSync(path, 'utf8'), prior);
    savePushed(path, { 'b.json': 'next' }, (partial, target) => { assert.equal(readFileSync(target, 'utf8'), prior); renameSync(partial, target); });
    assert.deepEqual(ledger(input.workDir), { 'b.json': 'next' });
    console.log('ok: a cut after temporary-file fsync preserves the complete prior ledger until atomic rename');
  }
  {
    const input = fixture('corrupt');
    for (const broken of ['{"a.json":', 'null', '[]']) {
      writeFileSync(join(input.workDir, 'pushed.json'), broken);
      await assert.rejects(pushBoard({ ...input, send: () => assert.fail('a corrupt ledger allowed an upload') }));
      assert.equal(readFileSync(join(input.workDir, 'pushed.json'), 'utf8'), broken);
    }
    console.log('ok: corrupt existing ledgers fail closed without an empty reset or network traffic');
  }
  console.log('\n7 live publisher checks passed');
} finally { rmSync(root, { recursive: true, force: true }); }
