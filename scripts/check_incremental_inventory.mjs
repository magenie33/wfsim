// The disk union must match the full classifier before and after replacement.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { questionId } from '../worker/task_contract.js';
import { shardInventory, auditInventoryInputs } from './inventory_inputs.mjs';
const { openCatalog } = await import(process.argv[2] || './incremental_inventory.mjs');

const dir = mkdtempSync(join(tmpdir(), 'inventory-incremental-'));
const engine = '4393e1730621', contract = { v: 1, covers: ['new_build', 'sweep'], algorithm: 'board-inventory-v1' };
const snapshot = '100:100:', started_at = '2026-11-06T00:00:00.000Z';
const opened = [];
let serial = 0, checks = 0;
const check = (name, fn) => { fn(); checks++; console.log(`ok: ${name}`); };
const file = (name, rows) => {
  const path = join(dir, name); writeFileSync(path, rows.map(row => JSON.stringify(row)).join('\n') + '\n'); return path;
};
const catalog = name => {
  const db = openCatalog(join(dir, `${name}.sqlite`), { engine, contract, generation: name, create: true }); opened.push(db); return db;
};
const record = input => ({ input });
const records = [
  { id: 'a', record: record('a') }, { id: 'alias', record: record('a') },
  { id: 'cached', record: record('cached') }, { id: 'spot', record: record('spot') },
  { id: 'unadmitted', record: record('unadmitted') }, { id: 'stored', record: record('original') },
  { id: 'orphan', record: record('orphan') },
];
const qid = build => questionId('simulate', { mode: 'base', record: records.find(row => row.id === build).record, ruler: 'r' });
const future = await questionId('future_verb', { input: 'future' });
const states = [
  { id: await qid('cached'), verb: 'simulate', state: 'fact', fact_engine: engine },
  { id: await qid('spot'), verb: 'simulate', state: 'spot', fact_engine: engine },
  { id: await qid('a'), verb: 'simulate', state: 'open', kind: 'future_owner' },
  { id: future, verb: 'future_verb', state: 'open', kind: 'future_owner' },
];
const row = (build_id, queued = true) => ({ build_id, ruler: 'r', mode: 'base', queued });
// Frozen engine membership includes aliases, refused rows and unadmitted work.
const owners = Object.freeze(records.map((entry, source) => ({ owner: entry.id, source,
  build_id: entry.id === 'orphan' ? null : entry.id === 'alias' ? 'a' : entry.id,
  record: entry.id === 'stored' ? record('normalized-fallback') : entry.record,
  weapon: entry.id === 'orphan' ? null : 'torid', riven: false,
  rows: entry.id === 'orphan' ? [] : entry.id === 'a' ? [row('a'), row('a')]
    : [row(entry.id === 'alias' ? 'a' : entry.id, !['stored', 'unadmitted'].includes(entry.id))],
})).sort((a, b) => a.owner < b.owner ? -1 : 1));
const scope = { weapon: 'torid', riven: false };

function output(db, selected, { complete = true, next_owner = null, extra = {} } = {}) {
  const rows = [];
  for (const owner of selected) {
    const { rows: members, ...header } = owner;
    rows.push({ type: 'owner', ...header });
    for (const member of members) rows.push({ type: 'row', owner: owner.owner, ...member });
  }
  rows.push({ type: 'end', v: 1, engine, contract, generation: db.meta.generation,
    snapshot: db.meta.epoch?.snapshot || snapshot, ...(db.meta.epoch ? { epoch: db.meta.epoch.epoch } : {}), scope,
    owners: selected.length, rows: selected.reduce((n, owner) => n + owner.rows.length, 0), complete, next_owner, ...extra });
  return file(`engine-${serial++}.ndjson`, rows);
}
async function apply(db, selected, changes = [], key = 'cohort') {
  db.queueJob(key, scope, changes);
  const job = db.jobs().find(row => row.key === key);
  return db.replaceOwners(output(db, selected), { job, snapshot });
}
async function seed(db) {
  await db.importRecords(records);
  await db.importLifecycle(states, { bootstrap: true });
  await apply(db, owners);
}

try {
  const reference = await classifyInventory({
    builds: file('builds', records), tasks: file('tasks', states),
    queue: file('queue', [row('a'), row('a'), row('alias'), row('cached'), row('spot'), row('orphan'), { ...row('retired'), ruler: 'old' }]),
    missing: file('missing', [row('unadmitted', false), row('stored', false), row('cached', false), row('spot', false)]),
    orphans: file('orphans', [row('orphan')]), rulers: ['r'], questions: join(dir, 'reference-questions'),
    meta: { engine, snapshot, started_at, counts: { library: records.length, tasks: states.length, queue: 7 } },
  });
  const db = catalog('active'); await seed(db);
  const referenceRows = readFileSync(join(dir, 'reference-questions'), 'utf8').trim().split('\n').map(JSON.parse);
  check('immutable engine output matches every full-scan QID/category/queued tuple', () => {
    assert.deepEqual([...db.tuples()], referenceRows);
    assert.equal(db.digest(), reference.question_digest);
    assert.deepEqual(db.summary(), { rows: reference.rows, queued: reference.queued, orders: reference.orders.map(row => ({ ...row })), verbs: reference.verbs });
    assert.equal(reference.rows, 5); assert.equal(reference.queued, 3);
  });
  check('stored records win over normalized fallbacks; aliases and repeated batches share one QID', () => {
    assert(db.record('stored').record.includes('original'));
    assert.equal(db.statement('SELECT board_refs FROM state_inventory_computations WHERE id=?').get(Buffer.from(referenceRows.find(r => r.id === states[2].id).id, 'hex')).board_refs, 2);
    assert.equal(db.owner('stored').weapon, 'torid');
    assert.deepEqual(db.ownerRows(null, 2).map(row => row.id), ['a', 'alias']);
  });
  await db.importLifecycle(states, { bootstrap: true });
  check('bootstrap can replay identical committed pages while rejecting changed snapshot rows', () => {
    assert.equal(db.digest(), reference.question_digest);
    assert.throws(() => db.applyLifecycle([{ ...states[0], state: 'open' }], { bootstrap: true }), /state changed/);
  });
  const expected = { owners: owners.length, records: records.length, tasks: states.length,
    rows: reference.rows, queued: reference.queued, digest: reference.question_digest };
  db.seal({ snapshot, started_at, expected });
  const published = { generation: 'active', engine, contract, snapshot, started_at, total: reference.rows, registered: reference.queued };
  db.activate(published);
  const before = db.digest(); await apply(db, owners);
  check('replaying a complete engine chunk is idempotent', () => { assert.equal(db.digest(), before); assert.deepEqual(db.meta.published, published); });

  db.queueJob('broken', scope, []);
  const broken = db.jobs().find(row => row.key === 'broken');
  const truncated = file('truncated', [{ type: 'owner', ...owners[0] },
    { type: 'row', owner: 'a', build_id: 'a', ruler: 'poison', mode: 'fresh', queued: true }]);
  await assert.rejects(db.replaceOwners(truncated, { job: broken, snapshot }), /incomplete/);
  check('an interrupted engine stream rolls back owner removals and cannot publish', () => {
    assert.equal(db.digest(), before); assert.throws(() => db.plan(), /not complete/);
    const symbol = db.symbol('ruler', 'poison');
    assert.equal(db.statement("SELECT id FROM entity_inventory_symbols WHERE category='ruler' AND name='poison'").get().id, symbol);
  });
  await db.replaceOwners(output(db, []), { job: broken, snapshot });

  const input = { scope: 'build', key: 'a', revision: '9007199254740993' };
  db.queueJob('racing', scope, [input]);
  let job = db.jobs().find(row => row.key === 'racing');
  await db.replaceOwners(output(db, [owners[0]], { complete: false, next_owner: owners[0].owner }), { job, snapshot });
  db.queueJob('racing', scope, [{ ...input, revision: '9007199254740994' }]);
  job = db.jobs().find(row => row.key === 'racing');
  check('a new invalidation retains the current bounded cursor until its pass ends', () => assert.equal(job.after_owner, 'a'));
  await db.replaceOwners(output(db, [owners[1]]), { job, snapshot });
  job = db.jobs().find(row => row.key === 'racing');
  check('invalidation during fanout starts a fresh pass and withholds the old acknowledgment', () => {
    assert.equal(job.after_owner, null); assert.equal(db.pendingAcks().length, 0);
  });
  await db.replaceOwners(output(db, [owners[0], owners[1]]), { job, snapshot });
  const receipt = db.pendingAcks()[0];
  check('revision tokens retain all 64-bit digits and a stale local acknowledgment cannot erase newer work', () => {
    assert.equal(receipt.revision, '9007199254740994');
    db.acknowledge([input]); assert.deepEqual(db.pendingAcks(), [receipt]);
  });
  const pending = new Map([[input.key, '9007199254740995']]);
  const matched = pending.get(receipt.key) === receipt.revision;
  check('a concurrent production CAS miss retains the newer dirty key', () => {
    assert.equal(matched, false); assert.equal(pending.get(input.key), '9007199254740995');
    assert.throws(() => db.plan(), /not complete/);
  });
  db.acknowledge([receipt]);

  const parked = owners.filter(row => ['stored', 'unadmitted'].includes(row.owner)).map(owner => ({ ...owner, rows: [] }));
  await apply(db, parked);
  check('an authoritative leader fanout replaces all affected owners, including more than one task', () => {
    assert.equal(db.summary().rows, reference.rows - 2); assert.equal(db.summary().queued, 3);
  });
  await apply(db, owners.filter(row => ['stored', 'unadmitted'].includes(row.owner)));
  db.applyLifecycle([{ id: states[0].id, deleted: true }]);
  check('retirement/removal restores the exact board task without a synthetic rescore', () => assert.equal(db.summary().rows, reference.rows + 1));
  db.applyLifecycle([states[0]]);
  db.applyLifecycle([{ id: await qid('unadmitted'), verb: 'simulate', state: 'open', kind: 'another_consumer' }]);
  check('an independent consumer registers an unadmitted board task without counting it twice', () => {
    assert.equal(db.summary().rows, reference.rows); assert.equal(db.summary().queued, reference.queued + 1);
  });
  db.applyLifecycle([{ id: await qid('unadmitted'), deleted: true }]);
  db.applyLifecycle([{ ...states[1], state: 'fact' }]);
  check('current-engine completion removes spot work exactly once', () => assert.equal(db.summary().rows, reference.rows - 1));
  db.applyLifecycle([{ ...states[1], state: 'disputed', served_answered: true }]);
  check('served disputed work is verification and overlaps its board QID', () => {
    assert.equal(db.summary().rows, reference.rows); assert.equal(db.summary().orders.find(row => row.kind === 'verification').total, 1);
  });
  db.applyLifecycle([states[1]]);
  await apply(db, [{ ...owners[0], build_id: null, rows: [] }]);
  check('deleting one alias does not delete another source contribution', () => assert.equal(db.summary().rows, reference.rows));
  await apply(db, [{ ...owners[1], build_id: null, rows: [] }]);
  check('the last board alias falls back to its live independent consumer', () => {
    assert.equal(db.summary().rows, reference.rows); assert.equal(db.summary().orders.find(row => row.kind === 'future_owner').total, 2);
  });
  db.applyLifecycle([{ id: states[2].id, deleted: true }]);
  check('withdrawing the last independent consumer removes only that QID', () => assert.equal(db.summary().rows, reference.rows - 1));

  const shadow = catalog('shadow'); await seed(shadow);
  const twin = catalog('twin'); await seed(twin);
  check('full audit compares sorted QIDs, category and registered flag', () => assert.equal(shadow.audit(twin).equal, true));
  shadow.putRecords([{ id: 'stored', record: record('different-question-same-count') }]);
  await apply(shadow, owners.filter(row => row.owner === 'stored'));
  check('equal totals with different task identities fail audit and bootstrap activation', () => {
    assert.equal(shadow.summary().rows, twin.summary().rows);
    assert.equal(shadow.audit(twin).equal, false);
    assert.throws(() => shadow.seal({ snapshot, started_at, expected }), /digest mismatch/);
    assert.throws(() => shadow.activate({ ...published, generation: 'shadow' }), /not complete/);
    assert.deepEqual(db.meta.published, published);
  });
  check('stale generation and engine cannot open or activate another catalog', () => {
    assert.throws(() => openCatalog(db.path, { engine: 'ffffffffffff', contract }), /generation mismatch/);
    assert.throws(() => db.activate({ ...published, generation: 'wrong' }), /receipt mismatch/);
    assert.throws(() => db.activate({ ...published, total: 999 }), /counts mismatch/);
  });
  db.close(); opened.splice(opened.indexOf(db), 1);
  const resumed = openCatalog(join(dir, 'active.sqlite'), { engine, contract, generation: 'active' }); opened.push(resumed);
  check('durable restart preserves the catalog and last publication receipt', () => assert.deepEqual(resumed.meta.published, published));
  check('cache, mmap and temp work stay bounded independently of question count', () => {
    assert.equal(resumed.statement('PRAGMA cache_size').get().cache_size, -16384);
    assert.equal(resumed.statement('PRAGMA mmap_size').get().mmap_size, 0);
    assert.equal(resumed.statement('PRAGMA temp_store').get().temp_store, 1);
    assert.throws(() => resumed.putRecords(Array(257).fill(records[0])), /exceeds bound/);
    assert.throws(() => resumed.jobs(257), /limit/);
  });

  const cut = catalog('continuous'); await seed(cut); cut.seal({ snapshot, started_at, expected });
  const cutReceipt = { epoch: 'cut-1', generation: 'continuous', engine, contract, status: 'sealed', snapshot: '200:200:',
    started_at: '2026-11-06T00:01:00.000Z', change_count: 2, manifest_sha: '1'.repeat(64) };
  cut.beginEpoch(cutReceipt);
  await cut.applyOwners(output(cut, owners.map(owner => ({ ...owner, rows: [] }))), { op: 'bootstrap' });
  check('warming a restarted engine preserves the complete catalog membership', () => assert.equal(cut.digest(), reference.question_digest));
  const captured = [{ scope: 'build', key: 'a', revision: '101' }, { scope: 'question', key: future, revision: '102' }];
  cut.putRecords([{ id: 'a', deleted: true }]);
  cut.applyLifecycle([{ id: future, deleted: true }]);
  await cut.applyOwners(output(cut, [{ ...owners[0], build_id: null, rows: [] }]), { snapshot: cutReceipt.snapshot, op: 'apply' });
  cut.queueJob('cut-cohort', scope, [captured[0]]);
  cut.freezeEpoch();
  const later = new Map(captured.map(row => [row.key, '1000']));
  let frozenJob = cut.jobs()[0];
  await cut.replaceOwners(output(cut, [{ ...owners[0], build_id: null, rows: [] }], { complete: false, next_owner: 'a' }),
    { job: frozenJob, snapshot: cutReceipt.snapshot });
  check('a frozen cut excludes later dirty revisions without restarting its hot cohort', () => {
    assert.throws(() => cut.queueJob('new-arrival', scope, [{ ...captured[0], revision: '1001' }]), /frozen/);
    assert.equal(cut.jobs()[0].after_owner, 'a'); assert.equal(cut.pendingAcks().length, 0);
  });
  later.set('a', '1002');
  frozenJob = cut.jobs()[0];
  await cut.replaceOwners(output(cut, owners.slice(1)), { job: frozenJob, snapshot: cutReceipt.snapshot });
  cut.recordReceipts([captured[1]]);
  for (const token of cut.pendingAcks()) if (later.get(token.key) === token.revision) later.delete(token.key);
  cut.acknowledge(cut.pendingAcks());
  cut.recordReceipts(captured);
  check('replaying the whole token spool after local ACK cannot process a token twice', () => {
    assert.equal(cut.pendingAcks().length, 0); assert.equal(cut.meta.epoch.tokens_processed, 2);
    assert.throws(() => cut.recordReceipts([{ ...captured[0], revision: 'different-cut' }]), /token changed/);
  });
  const completedCut = cut.finishEpoch();
  check('completion can replay after a crash before publication', () => assert.deepEqual(cut.finishEpoch(), completedCut));
  check('continuous later writes do not prevent publication of the completed honest snapshot', () => {
    assert.equal(completedCut.rows, reference.rows - 1); assert.equal(completedCut.snapshot, cutReceipt.snapshot);
    assert.equal(completedCut.started_at, cutReceipt.started_at); assert.equal(later.size, 2);
    assert.throws(() => cut.plan({ snapshot: 'newer', started_at: '2026-11-06T00:02:00.000Z' }), /cannot advance/);
  });
  cut.activate({ ...cutReceipt, total: completedCut.rows, registered: completedCut.queued });
  check('completed cut receipt stays tied to its captured epoch and future dirty remains pending', () => {
    assert.equal(cut.meta.published.epoch, 'cut-1'); assert.equal(later.get('a'), '1002');
    assert.throws(() => cut.activate({ ...cutReceipt, epoch: 'cut-2' }), /epoch mismatch/);
  });
  const engineFile = output(twin, owners);
  const factsFile = file('facts', [{ identity: 'a', ruler: 'r:v1', mode: 'base', score: 100, retired_at: '2026-11-06T00:00:00.000Z' },
    { identity: 'alias', ruler: 'retired-ruler', mode: 'base', score: 90 }, { identity: 'absent', ruler: 'r', mode: 'base', score: 3 }]);
  const queueFile = file('shard-queue', [...readFileSync(join(dir, 'queue'), 'utf8').trim().split('\n').map(JSON.parse), { ...row('a'), ruler: 'retired-ruler' }]);
  const shardOptions = { builds: join(dir, 'builds'), facts: factsFile, queue: queueFile, tasks: join(dir, 'tasks'),
    owners: output(twin, owners.map(owner => ({ ...owner, rows: [] }))), out: join(dir, 'shards'), engine, contract, snapshot, generation: 'twin' };
  const shards = await shardInventory(shardOptions), replayedShards = await shardInventory(shardOptions);
  check('disk-indexed input partition preserves aliases, old ruler facts and retired queued rows', () => {
    assert.equal(shards.scopes.length, 1); assert.equal(shards.counts.queue, 8); assert.equal(shards.counts.unmapped_queue, 2);
    const scope = shards.scopes[0], facts = readFileSync(scope.facts, 'utf8').trim().split('\n').map(JSON.parse);
    const queue = readFileSync(scope.queue, 'utf8').trim().split('\n').map(JSON.parse);
    assert.equal(facts.length, 2); assert.equal(facts[0].ruler, 'r:v1'); assert.equal(facts[1].ruler, 'retired-ruler');
    assert(queue.some(row => row.ruler === 'retired-ruler')); assert(queue.some(row => row.build_id === 'alias'));
    assert.deepEqual(scope.counts, { builds: 6, facts: 2, queue: 6 });
    assert.deepEqual(shards, replayedShards);
  });
  const auditOptions = { builds: join(dir, 'builds'), queue: join(dir, 'queue'), tasks: join(dir, 'tasks'),
    meta: { engine, snapshot, started_at, counts: { library: records.length, tasks: states.length, queue: 7 } }, rulers: ['r'],
    engineOutputs: [engineFile], out: join(dir, 'audit'), contract, generation: 'twin' };
  const audited = await auditInventoryInputs(auditOptions), replayedAudit = await auditInventoryInputs(auditOptions);
  check('bootstrap audit reconstructs full missing/orphan inputs without duplicating the gate', () => {
    assert.equal(audited.plan.question_digest, reference.question_digest);
    assert.deepEqual(readFileSync(audited.orphans, 'utf8').trim().split('\n').map(JSON.parse),
      [{ build_id: 'alias', ruler: 'r', mode: 'base' }, { build_id: 'orphan', ruler: 'r', mode: 'base' }]);
    assert.deepEqual(JSON.parse(JSON.stringify(audited)), replayedAudit);
  });
  console.log(`${checks} incremental inventory checks passed`);
} finally { for (const db of opened) db.close(); rmSync(dir, { recursive: true, force: true }); }
