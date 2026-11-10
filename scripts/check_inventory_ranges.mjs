// SPDX-License-Identifier: AGPL-3.0-or-later
// Ten times more unrelated history must not multiply a local update's work.
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openCatalog } from './incremental_inventory.mjs';
const { InventoryRelations } = await import(process.argv[2] || './inventory_relations.mjs');

const root = mkdtempSync(join(tmpdir(), 'inventory-ranges-'));
const contract = { v: 1, entry_share: 0.2, rulers: [{ id: 'r', scene: '' }, { id: 's', scene: '' }] };
const engine = '0123456789ab', bucket = JSON.stringify(['fixture', false]);
const operate = async (op, items) => {
  if (op === 'facts') return items.map(item => ({ ...item, gate_ruler: item.ruler, scene: '' }));
  if (op !== 'gate') throw Error('this fixture must not normalize history');
  return items.map(g => ({ eligible: (g.share == null || g.share >= contract.entry_share)
    && (!g.default_present || g.default_share != null && g.default_share >= contract.entry_share)
    && (g.share != null || !g.owed) }));
};
const delta = (after, before) => Object.fromEntries(Object.keys(after).map(k => [k, after[k] - before[k]]));
async function run(size) {
  const path = join(root, `${size}.sqlite`);
  let catalog = openCatalog(path, { engine, contract: { v: 1 }, generation: 'test', create: true });
  let index = new InventoryRelations(catalog, contract);
  const descriptor = id => ({ candidate: id, bucket: ['fixture', false], admit: '0000000000000003',
    modes: ['base'], default: null, record: { input: id } });
  index.transaction(() => {
    for (let i = 0; i < size; i++) {
      const id = String(i).padStart(8, '0'), body = descriptor(id), score = i === 0 ? 100 : i === 1 ? 21 : 10;
      index.q("INSERT INTO entity_catalog_sources(id,candidate,body) VALUES(?,?,?)").run(id, id, JSON.stringify({ ...body, id }));
      index.q("INSERT INTO entity_catalog_candidates VALUES(?,?,?,?,?)").run(id, bucket, 3, null, JSON.stringify(body));
      index.q("INSERT INTO entity_inventory_builds(build_id,stored) VALUES(?,?)").run(id, JSON.stringify(body.record));
      index.q("INSERT INTO state_catalog_measurements VALUES(?,?,?,?,?,?,?,?,?)").run(id, 'r', 'base', score, '', bucket, 'a', 0, 'r');
    }
    for (const target of [0, 1]) index.q("INSERT INTO state_catalog_leaders VALUES(?,?,?,?,?)").run(bucket, 'r', 'base', target, 100);
    index.dirty('00000000'); index.dirty('00000001');
  });
  const drain = async () => { while (index.pending()) await index.step(operate, { limit: 7, budgetMs: 10 }); };
  await drain();
  assert.equal(catalog.summary().rows, 2);
  let before = { ...index.metrics };
  await index.facts([{ identity: '00000002', ruler: 'r', mode: 'base', score: 11, finished_at: 'b' }], operate);
  await drain();
  const local = delta(index.metrics, before);
  assert.equal(local.candidates, 1); assert.equal(local.range_rows, 0); assert.equal(local.normalized, 0);
  assert.equal(catalog.summary().rows, 2);
  before = { ...index.metrics };
  await index.facts([{ identity: '00000000', ruler: 'r', mode: 'base', score: 110, finished_at: 'c' }], operate);
  await drain();
  const crossing = delta(index.metrics, before);
  assert.equal(crossing.candidates, 2); assert.equal(crossing.range_rows, 2); assert.equal(catalog.summary().rows, 1);
  const plan = index.q(`EXPLAIN QUERY PLAN SELECT identity,score FROM state_catalog_measurements
    WHERE bucket=? AND gate_ruler=? AND mode=? AND retired=0 AND score>=? AND score<=?
    AND (score,identity)>(?,?) ORDER BY score,identity LIMIT ?`).all(bucket, 'r', 'base', 20, 22, 20, '', 7);
  assert(plan.some(p => /SEARCH .*rel_facts_score/.test(p.detail)), JSON.stringify(plan));
  assert(!plan.some(p => /TEMP B-TREE|SCAN state_catalog_measurements/.test(p.detail)), JSON.stringify(plan));
  catalog.close(); catalog = openCatalog(path); index = new InventoryRelations(catalog, contract);
  assert(!index.pending()); assert.equal(catalog.summary().rows, 1);
  await assert.rejects(index.step(operate, { limit: 257 }), /budget/);
  catalog.close();
  return { size, local, crossing };
}
try {
  const small = await run(1000), large = await run(10000);
  assert.deepEqual(large.local, small.local); assert.deepEqual(large.crossing, small.crossing);
  console.log(JSON.stringify({ small, large }));
  console.log('ok: 10x history keeps identical local work; indexed threshold crossing touches only changed memberships; restart retains state');
} finally { rmSync(root, { recursive: true, force: true }); }
