// Complete demand counts canonical questions from one snapshot.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { publishInventory } from './compute_inventory.mjs';
import { inventoryWrite, readInventory, INVENTORY_MAX_MS } from '../worker/task_inventory.js';
import { testDb } from './pg/test_db.mjs';
import worker from '../worker/index.js';
import { questionId } from '../worker/task_contract.js';
const dir = mkdtempSync(join(tmpdir(), 'compute-inventory-test-')), now = Date.now(), engine = '4393e1730621';
const file = (name, rows) => { const p = join(dir, name); writeFileSync(p, rows.map(r => JSON.stringify(r)).join('\n') + '\n'); return p; };
const row = (id, ruler = 'r') => ({ build_id: id, ruler, mode: 'base' });
try {
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
  const admitted = await classifyInventory({ ...input, queue: file('admitted', [row('a'), row('alias'), row('cached'), row('spot'), row('unadmitted')]), missing: file('empty', []), meta: { ...meta, counts: { ...meta.counts, queue: 5 } } });
  assert.equal(admitted.rows, plan.rows);
  const settled = await classifyInventory({ ...input, tasks: file('settled', [...states, { id: await id('a'), verb: 'simulate', state: 'fact', fact_engine: engine }]), meta: { ...meta, counts: { ...meta.counts, tasks: 4 } } });
  assert.equal(settled.rows, 3);
  await assert.rejects(classifyInventory({ ...input, meta: { ...meta, counts: { ...meta.counts, tasks: 100 } } }), /incomplete/);
  await assert.rejects(classifyInventory({ ...input, missing: file('bad-build', [row('absent')]) }), /unavailable/);
  await assert.rejects(classifyInventory({ ...input, missing: file('bad-ruler', [row('a', 'old')]) }), /unknown/);
  console.log('ok: canonical aliases and batches share one task; cached delivery is zero work; spot checks count once; admission preserves demand; future task kinds count');
  const db = await testDb('library');
  const save = async (p, at = now) => { const w = inventoryWrite(p, at); await db.run(w.sql, ...w.params); };
  await save(plan); assert.equal((await readInventory(db, now)).total, 4);
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
  await db.run("INSERT INTO factory_snapshot(id,at,engine,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit) VALUES (1,'x','ffffffffffff',0,0,0,0,0,0,'x','x')");
  assert.equal(await readInventory(db, now), null, 'an engine change kept a stale inventory goal');
  await db.close();
  console.log('ok: authenticated publication preserves one snapshot, rejects partial/old scans and expires; lifecycle export follows live independent consumers');
} finally { rmSync(dir, { recursive: true, force: true }); }
