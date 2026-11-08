// Complete demand counts canonical questions from one snapshot.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { bindInventoryBuilds } from './bind_inventory_builds.mjs';
import { publishInventory, inventoryCacheKiB } from './compute_inventory.mjs';
import { inventoryWrite, readInventory, readInventoryState, INVENTORY_MAX_MS } from '../worker/task_inventory.js';
import { testDb } from './pg/test_db.mjs';
import worker from '../worker/index.js';
import { questionId } from '../worker/task_contract.js';
const dir = mkdtempSync(join(tmpdir(), 'compute-inventory-test-')), now = Date.now(), engine = '4393e1730621';
const file = (name, rows) => { const p = join(dir, name); writeFileSync(p, rows.map(r => JSON.stringify(r)).join('\n') + '\n'); return p; };
const row = (id, ruler = 'r') => ({ build_id: id, ruler, mode: 'base' });
try {
  for (const [available, expected] of [[0, 16384], [-1, 16384], [NaN, 16384],
    [1024 ** 3, 256 * 1024], [8 * 1024 ** 3, 2 * 1024 ** 2], [64 * 1024 ** 3, 4 * 1024 ** 2]])
    assert.equal(inventoryCacheKiB(available), expected);
  const records = ['a', 'cached', 'spot', 'orphan', 'unadmitted'].map(id => ({ id, record: { input: id } }));
  records.push({ id: 'alias', record: records[0].record });
  const id = async build => questionId('simulate', { record: records.find(r => r.id === build).record, ruler: 'r', mode: 'base' });
  const cached = await id('cached'), spot = await id('spot'), future = await questionId('future_verb', { input: 'new' });
  const states = [
    { id: cached, verb: 'simulate', state: 'fact', fact_engine: engine },
    { id: spot, verb: 'simulate', state: 'spot', fact_engine: engine },
    { id: future, verb: 'future_verb', state: 'open', kind: 'future_owner' },
  ];
  const meta = { started_at: new Date(now - 60000).toISOString(), snapshot: '100:100:', engine, counts: { library: records.length, tasks: states.length, queue: 7 } };
  const input = { queue: file('queue', [row('a'), row('a'), row('alias'), row('cached'), row('spot'), row('orphan'), row('retired', 'old')]),
    missing: file('missing', [row('unadmitted'), row('cached'), row('spot')]), orphans: file('orphans', [row('orphan')]),
    builds: file('builds', records), tasks: file('tasks', states), meta, rulers: ['r'] };
  const plan = await classifyInventory(input);
  assert.equal(plan.rows, 4); assert.equal(plan.queued, 3);
  assert.deepEqual(plan.verbs, { future_verb: 1, simulate: 3 });
  assert.equal(plan.orders.find(o => o.kind === 'verification').total, 1);
  const questions = join(dir, 'questions.ndjson');
  const audited = await classifyInventory({ ...input, questions });
  const tuples = readFileSync(questions, 'utf8'), classified = tuples.trim().split('\n').map(JSON.parse);
  assert.equal(audited.question_digest, createHash('sha256').update(tuples).digest('hex'));
  assert.deepEqual(classified.map(q => q.id), [await id('a'), await id('unadmitted'), spot, future].sort());
  assert.deepEqual(classified.find(q => q.id === spot), { id: spot, kind: 'verification', verb: 'simulate', queued: 1 });
  await assert.rejects(classifyInventory({ ...input, questions, missing: file('audit-bad-build', [row('absent')]) }), /unavailable/);
  assert.equal(readFileSync(questions, 'utf8'), tuples, 'a failed audit replaced the completed task set');
  await assert.rejects(classifyInventory({ ...input, tasks: file('duplicate-states', [...states, states[0]]),
    meta: { ...meta, counts: { ...meta.counts, tasks: states.length + 1 } } }), /duplicate inventory task state/);
  for (const cacheKiB of [16384, 1024 ** 2, 4 * 1024 ** 2, 16 * 1024 ** 2]) {
    const path = join(dir, `cached-questions-${cacheKiB}`);
    assert.deepEqual(await classifyInventory({ ...input, questions: path, cacheKiB }), audited);
    assert.equal(readFileSync(path, "utf8"), tuples);
  }
  for (const cacheKiB of [0, -1, NaN, 16383, 16 * 1024 ** 2 + 1, "16384;DROP TABLE records"])
    await assert.rejects(classifyInventory({ ...input, cacheKiB }), /invalid inventory cache budget/);
  console.log("ok: bounded pager budgets preserve every question and category; invalid budgets fail before SQL");
  const stored = await classifyInventory({ ...input,
    canonical: file('conflicting-canonical', [{ id: 'a', record: { input: 'different' } }]) });
  assert.deepEqual(stored, plan, 'an engine fallback replaced an existing stored record');
  const admitted = await classifyInventory({ ...input, queue: file('admitted', [row('a'), row('alias'), row('cached'), row('spot'), row('unadmitted')]), missing: file('empty', []), meta: { ...meta, counts: { ...meta.counts, queue: 5 } } });
  assert.equal(admitted.rows, plan.rows);
  const settled = await classifyInventory({ ...input, tasks: file('settled', [...states, { id: await id('a'), verb: 'simulate', state: 'fact', fact_engine: engine }]), meta: { ...meta, counts: { ...meta.counts, tasks: 4 } } });
  assert.equal(settled.rows, 3);
  await assert.rejects(classifyInventory({ ...input, meta: { ...meta, counts: { ...meta.counts, tasks: 100 } } }), /incomplete/);
  await assert.rejects(classifyInventory({ ...input, missing: file('bad-build', [row('absent')]) }), /unavailable/);
  const rebound = await classifyInventory({ ...input, missing: file('normalized', [row('normalized')]),
    canonical: file('canonical', [{ id: 'normalized', source: 0, record: records[0].record }]) });
  assert.equal(rebound.rows, 3, 'a normalized identity created a second task for the same input');
  await assert.rejects(classifyInventory({ ...input, missing: file('bad-ruler', [row('a', 'old')]) }), /unknown/);
  console.log('ok: canonical aliases and batches share one task; cached delivery is zero work; spot checks count once; admission preserves demand; future task kinds count');
  const db = await testDb('library');
  await db.run("INSERT INTO builds(id,at,record) VALUES ('old','2026-01-01','{\"weapon\":\"torid\"}')");
  const binding = { builds: file('binding-builds', [{ id: 'old', at: '2026-01-01', record: { weapon: 'torid' } }]),
    canonical: file('binding-canonical', [{ id: 'new', source: 0, record: { weapon: 'torid', mods: [] } }]),
    missing: file('binding-missing', [row('new')]), queue: file('binding-queue', [row('retired', 'old')]), orphans: file('binding-orphans', []), rulers: ['r'] };
  const run = (sql, args) => db.all(sql, ...args);
  await bindInventoryBuilds(binding, run); await bindInventoryBuilds(binding, run);
  assert.equal((await db.get('SELECT count(*) n FROM builds')).n, 2);
  assert.equal((await db.get("SELECT at FROM builds WHERE id='new'")).at, '2026-01-01');
  assert.equal((await db.get('SELECT count(*) n FROM family_transfers')).n, 0);
  await assert.rejects(bindInventoryBuilds({ ...binding, canonical: file('binding-missing-map', []) }, run), /unbound/);
  const save = async (p, at = now) => { const w = inventoryWrite(p, at); await db.run(w.sql, ...w.params); };
  await save(plan); assert.equal((await readInventory(db, now)).total, 4);
  assert.equal((await readInventoryState(db, now)).lastComplete, null, 'an unknown factory engine qualified for display');
  await db.run("INSERT INTO factory_snapshot(id,at,engine,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit) VALUES (1,'x',?,0,0,0,0,0,0,'x','x')", engine);
  let reads = 0;
  const state = await readInventoryState({ prepare(sql) { reads++; return db.prepare(sql); } }, now);
  assert.equal(reads, 1, 'fresh and display inventory used separate database reads');
  assert.deepEqual(state.lastComplete, state.fresh, 'fresh inventory was not retained across a cached CDN expiry');
  const expired = await readInventoryState(db, now + INVENTORY_MAX_MS + 2);
  assert.equal(expired.fresh, null);
  assert.deepEqual(expired.lastComplete, state.fresh, 'display changed the original complete snapshot or its timestamps');
  const original = JSON.stringify(state.fresh);
  for (const body of [
    '{', JSON.stringify({ ...state.fresh, orders: [null] }),
    JSON.stringify({ ...state.fresh, total: 999 }), JSON.stringify({ ...state.fresh, verbs: { simulate: 999 } }),
    JSON.stringify({ ...state.fresh, started_at: new Date(now + 1).toISOString() }),
    JSON.stringify({ ...state.fresh, at: new Date(now + 60_000).toISOString() }),
    JSON.stringify({ ...state.fresh, expires_at: new Date(Date.parse(state.fresh.expires_at) + 60_000).toISOString() }),
    JSON.stringify({ ...state.fresh, at: state.fresh.expires_at }),
  ]) {
    await db.run("UPDATE factory_findings SET body=? WHERE kind='task_inventory'", body);
    assert.equal((await readInventoryState(db, now)).lastComplete, null, 'malformed or invalid-at-publication inventory qualified for display');
  }
  await db.run("UPDATE factory_findings SET body=? WHERE kind='task_inventory'", original);
  await db.run("UPDATE factory_snapshot SET engine='95d73da96acb' WHERE id=1");
  assert.deepEqual(await readInventoryState(db, now), { fresh: null, lastComplete: null }, 'a different engine qualified for display');
  await db.run("UPDATE factory_snapshot SET engine=? WHERE id=1", engine);
  console.log('ok: one persisted snapshot supplies strict freshness and original-time same-engine display; unknown, future, malformed and invalid publications are refused');
  await save({ ...plan, rows: 999, orders: [{ kind: 'board', total: 999 }], verbs: { simulate: 999 }, started_at: new Date(now - 120000).toISOString() }, now + 1);
  assert.equal((await readInventory(db, now + 1)).total, 4);
  assert.equal(await readInventory(db, now + INVENTORY_MAX_MS + 2), null);
  assert.throws(() => inventoryWrite({ ...plan, v: 1 }, now), /incomplete/);
  assert.throws(() => inventoryWrite({ ...plan, queued: 999 }, now), /incomplete/);
  const send = (url, init) => worker.fetch(new Request(url, init), { LIBRARY: db, FACTORY_TOKEN: 'test' });
  const publish = (p, token = 'test') => publishInventory(p, { origin: 'https://example.test', token, send });
  await assert.rejects(publish(plan, 'wrong'), /401/); await publish(plan);
  assert.equal((await readInventory(db, now)).total, 4, 'publication mixed a later ledger count into the snapshot');
  const request = '{}';
  await db.run("INSERT INTO questions(id,verb,request,state,due_at,opened_at,producer) VALUES (?,'future_verb',?,'open','x','x','sweep')", future, request);
  await db.run("INSERT INTO demands(producer,ref,question,state,asked_at,due_at) VALUES ('future_owner','new',?,'live','x','x')", future);
  const exported = await db.all('SELECT * FROM factory_inventory_rows(?,?::text[])', engine, ['sweep']);
  assert.equal(exported.find(q => q.id === future).kind, 'future_owner');
  await db.run("UPDATE demands SET state='withdrawn'");
  assert.equal((await db.all('SELECT * FROM factory_inventory_rows(?,?::text[])', engine, ['sweep'])).length, 0);
  await db.run("UPDATE factory_snapshot SET engine='ffffffffffff' WHERE id=1");
  assert.equal(await readInventory(db, now), null, 'an engine change kept a stale inventory goal');
  const current = { ...plan, engine: 'ffffffffffff' };
  await save(current);
  await save({ ...plan, started_at: new Date(now - 1000).toISOString() });
  assert.equal((await readInventory(db, now)).engine, current.engine, 'an obsolete full scanner overwrote the current-engine inventory');
  await db.close();
  console.log('ok: authenticated publication preserves one snapshot, rejects partial/old scans and expires; lifecycle export follows live independent consumers');
} finally { rmSync(dir, { recursive: true, force: true }); }
