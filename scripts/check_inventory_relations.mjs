// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare indexed changes with the independent full engine and classifier.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, openSync, closeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { classifyInventory } from './classify_compute_inventory.mjs';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const root = mkdtempSync(join(tmpdir(), 'inventory-relations-')), engine = '0ae1bf8edb0f';
const err = openSync(join(root, 'engine.log'), 'w');
let operator, catalog, succeeded = false, serial = 0;
const file = (name, rows) => { const path = join(root, name); writeFileSync(path, rows.map(r => JSON.stringify(r)).join('\n') + '\n'); return path; };
const schema = { v: 1, algorithm: 'board-inventory-relations-v1', covers: ['new_build', 'sweep'] };
const sources = new Map(), facts = new Map(), queue = new Map();
const factKey = f => JSON.stringify([f.identity, f.ruler, f.mode]);
const queueKey = f => JSON.stringify([f.build_id, f.ruler, f.mode]);
let index;
const operate = (op, items) => operator.call(op, items);
async function source(rows) {
  await index.sources(rows, operate);
  for (const row of rows) { if (row.deleted) sources.delete(row.id); else sources.set(row.id, row); }
}
async function fact(rows) { await index.facts(rows, operate); for (const row of rows) facts.set(factKey(row), row); }
function queued(rows) { index.queue(rows); for (const row of rows) queue.set(queueKey(row), row); }
async function drain() {
  let steps = 0;
  do { await index.step(operate, { limit: 7, budgetMs: 0 }); assert(++steps < 10000); } while (index.pending());
}
async function oracle(label) {
  const dir = join(root, `oracle-${serial++}`); mkdirSync(dir);
  const raw = { builds: file(`builds-${serial}`, [...sources.values()]), facts: file(`facts-${serial}`, [...facts.values()]),
    queue: file(`queue-${serial}`, [...queue.values()]), tasks: file(`tasks-${serial}`, []) };
  const rulers = operator.contract.rulers.map(r => r.id);
  execFileSync(binary, ['--each-ruler', '--dry-run', '--gate', '--facts-in', raw.facts, '--queue-in', raw.queue,
    '--inventory-builds', join(dir, 'canonical'), '--queue-missing', join(dir, '{id}.missing'), '--queue-orphans', join(dir, '{id}.orphans')],
  { input: JSON.stringify([...sources.values()].map(r => r.record)), stdio: ['pipe', 'pipe', err], timeout: 60000 });
  for (const suffix of ['missing', 'orphans']) writeFileSync(join(dir, suffix), rulers.map(r => readFileSync(join(dir, `${r}.${suffix}`), 'utf8')).join(''));
  const expected = await classifyInventory({ ...raw, rulers, canonical: join(dir, 'canonical'), missing: join(dir, 'missing'),
    orphans: join(dir, 'orphans'), questions: join(dir, 'questions'), meta: { engine, snapshot: 'fixture',
      started_at: '2026-11-09T00:00:00.000Z', counts: { library: sources.size, queue: queue.size, tasks: 0 } } });
  assert.equal(catalog.digest(), expected.question_digest, `${label}: full/incremental tuples differ; fixture ${root}`);
  for (const key of ['rows', 'queued', 'orders', 'verbs']) assert.deepEqual(catalog.summary()[key], JSON.parse(JSON.stringify(expected[key])), `${label}: ${key}`);
  console.log(`ok: ${label} matches every independent full-engine question/category/queued tuple`);
}
try {
  operator = await openInventoryOperator(binary, { stderr: err });
  const path = join(root, 'catalog.sqlite');
  catalog = openCatalog(path, { engine, contract: schema, generation: 'fixture', create: true });
  index = new InventoryRelations(catalog, operator.contract);
  const raw = ['magnetic_capacity', 'primed_magazine_warp', 'fast_hands'].map(last => ({ weapon: 'braton_prime',
    mods: ['vital_sense', 'hellfire', 'speed_trigger', 'primary_acuity', 'galvanized_aptitude', 'hammer_shot', 'vigilante_fervor', last],
    evolutions: ['braton_prime_evo1_incarnon_form', 'braton_prime_daring_reverie', 'braton_prime_voids_guidance', 'braton_prime_prelude_of_might'],
    arcanes: ['primary_deadhead'] }));
  const described = await operate('describe', raw.map((record, i) => ({ id: `seed-${i}`, record })));
  assert(described.every(d => d.candidate)); assert.equal(new Set(described.map(d => d.candidate)).size, 3);
  const [a, b, c] = described.map(d => ({ id: d.candidate, record: d.record }));
  await source([a, b, c, { id: 'alias', record: { ...a.record, note: 'source alias' } },
    { id: 'refused', record: { weapon: 'not-a-weapon' } }]);
  const ruler = operator.contract.rulers.find((_, i) => described.every(d => (BigInt(`0x${d.admit}`) & 1n << BigInt(i)) !== 0n)).id;
  const scored = (id, score, extra = {}) => ({ identity: id, ruler, mode: 'base', score, measured_by: 'fixture', cost_seconds: 1,
    started_at: '2026-11-09T00:00:00.000Z', finished_at: '2026-11-09T00:00:01.000Z', ...extra });
  await fact([scored(a.id, 100), scored(b.id, 20), scored(c.id, 19.9)]);
  queued([{ batch: 'one', build_id: c.id, ruler, mode: 'base' }, { batch: 'two', build_id: 'orphan', ruler, mode: 'base' }]);
  await drain(); await oracle('bootstrap, aliases, inclusive gate, queued parked row and orphan');

  const before = { ...index.metrics };
  await fact([scored(c.id, 19.8, { finished_at: '2026-11-09T00:00:02.000Z' })]);
  await drain(); await oracle('nonleader update');
  assert.equal(index.metrics.range_rows, before.range_rows, 'unchanged aggregate propagated to other builds');
  assert.equal(index.metrics.candidates - before.candidates, 1);

  await fact([scored(a.id, 120, { finished_at: '2026-11-09T00:00:03.000Z' })]);
  await index.step(operate, { limit: 1, budgetMs: 0 });
  assert(index.pending());
  const contract = operator.contract;
  await operator.close(); catalog.close();
  operator = await openInventoryOperator(binary, { stderr: err });
  assert.deepEqual(operator.contract, contract);
  catalog = openCatalog(path); index = new InventoryRelations(catalog, operator.contract);
  await drain(); await oracle('restart in the middle of leader fanout');
  assert.equal(index.metrics.normalized, 0, 'restart normalized source history');

  await fact([scored(a.id, 120, { retired_at: '2026-11-09T00:00:04.000Z' })]);
  await drain(); await oracle('leader retirement and replacement');
  await source([{ id: a.id, deleted: true }]);
  await drain(); await oracle('stored canonical deletion retains normalized alias');
  await source([{ id: 'alias', deleted: true }]);
  await drain(); await oracle('last canonical source deletion');

  const saved = catalog.digest(), normalized = index.metrics.normalized;
  await source([b, c]); await drain();
  assert.equal(catalog.digest(), saved); assert.equal(index.metrics.normalized, normalized);
  console.log('ok: unchanged source replay performs no normalization or output changes');

  await fact([scored(b.id, 100, { ruler: ruler + '_v99', finished_at: '2026-11-09T00:00:05.000Z' })]);
  await drain(); await oracle('older ruler family affects admission without completing a current cell');
  const scene = operator.contract.rulers.find(r => r.scene && described.every(d =>
    (BigInt(`0x${d.admit}`) & 1n << BigInt(operator.contract.rulers.indexOf(r))) !== 0n));
  assert(scene, 'fixture needs a scene target');
  await fact([scored(b.id, 10, { ruler: scene.id }), scored(c.id, 100, { ruler: scene.id })]);
  await drain(); await oracle('scene-specific shares override base fallback');
  index.queue([], { replace: [c.id] });
  for (const [key, row] of queue) if (row.build_id === c.id) queue.delete(key);
  await drain(); await oracle('queue withdrawal removes already-owed protection');

  const child = { weapon: 'torid', arcanes: ['primary_deadhead'], exilus: 'vigilante_supplies',
    evolutions: ['torid_evo1_incarnon_form', 'torid_final_fusillade', 'torid_swift_deliverance', 'torid_survivors_edge'],
    mods: ['primed_cryo_rounds', 'malignant_force', 'hellfire', 'primary_acuity', 'galvanized_scope', 'vital_sense', 'hunter_track@0', 'riven'],
    riven_neg: 'status_duration', riven_pos: ['critical_chance', 'critical_damage', 'damage'], riven_rolls: [1.1, 1.1, 1.1, 0.9] };
  const parent = { ...child, mods: child.mods.map(m => m.split('@')[0]) };
  const family = await operate('describe', [child, parent, { ...parent, riven_rolls: [0.9, 1.1, 1.1, 0.9] }]
    .map((record, i) => ({ id: `family-${i}`, record })));
  const members = family.map(d => ({ id: d.candidate, record: d.record }));
  assert(family[0].default === family[1].candidate && family[2].default === family[1].candidate);
  await source([members[0], members[2]]); await drain(); await oracle('legacy rank and roll variants without their parent');
  await source([members[1]]); await drain(); await oracle('parent appearance parks both dependent variants');
  await fact([scored(members[1].id, 100)]); await drain(); await oracle('parent earning releases dependent rank and roll variants');
  await fact([scored(members[1].id, 100, { retired_at: '2026-11-09T00:00:06.000Z' })]);
  await drain(); await oracle('parent fact retirement updates every reverse dependency');
  succeeded = true;
} catch (error) { console.error(`relational inventory evidence: ${root}`); throw error; }
finally {
  await operator?.close(); catalog?.close(); closeSync(err);
  if (succeeded && !process.env.WFSIM_INVENTORY_RELATIONS_EVIDENCE) rmSync(root, { recursive: true, force: true });
}
