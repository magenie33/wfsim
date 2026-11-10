// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare actual disk-produced files with the unchanged full native renderer.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { InventoryPublication } from './inventory_publication.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
const root = mkdtempSync(join(tmpdir(), 'inventory-publication-')), work = join(root, 'work'), oracle = join(root, 'oracle');
mkdirSync(work); mkdirSync(oracle); mkdirSync(join(work, 'pub/data'), { recursive: true });
writeFileSync(join(work, 'pub/data/board_state.yaml'), 'boards: {}\n');
writeFileSync(join(root, 'oracle-state.yaml'), 'boards: {}\n');
const native = await openInventoryOperator(binary);
const engine = '0ae1bf8edb0f', catalogPath = join(root, 'catalog.sqlite');
let catalog = openCatalog(catalogPath, { engine, contract: { v: 1 }, generation: 'fixture', create: true });
let index = new InventoryRelations(catalog, native.contract), context = {}, succeeded = false;
const records = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(r => r.record);
const described = await native.call('describe', records.map((record, i) => ({ id: 'seed-' + i, record })));
const sources = described.map((d, i) => ({ id: d.candidate, record: records[i] }));
sources.push({ id: 'alias', record: { ...records[0], note: 'source order alias' } }, { id: 'refused', record: { weapon: 'missing-weapon' } });
const facts = described.flatMap((d, i) => native.contract.rulers.flatMap((r, n) => d.modes.map(mode => ({
  identity: d.candidate, ruler: r.id, mode, score: (i + 1) * 100 + n * 0.000123456789,
  measured_by: i === 0 ? 'client:' + engine : 'verified:' + engine,
  cost_seconds: 1.25, started_at: '2026-11-09T00:00:00.000Z', finished_at: '2026-11-09T00:00:01.123Z',
}))));
const factsPath = join(root, 'facts.ndjson');
writeFileSync(factsPath, facts.map(row => JSON.stringify(row) + '\n').join(''));
async function normalize() {
  await index.sources(sources, native.call);
  for (let offset = 0; offset < facts.length; offset += 256) await index.facts(facts.slice(offset, offset + 256), native.call);
  while (index.pending()) await index.step(native.call, { budgetMs: 100 });
}
async function publish(epoch, fault) {
  for (let n = 0; n < 10000; n++) {
    const publication = new InventoryPublication(index, work, context, fault);
    const result = await publication.step({ epoch }, native.call, { budgetMs: 50 });
    if (!result.pending) return;
  }
  throw Error('disk publication did not converge');
}
function files(directory, prefix = '') {
  const output = new Map();
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name), name = prefix + entry.name;
    if (entry.isDirectory()) for (const item of files(path, name + '/')) output.set(...item);
    else if (name.endsWith('.json')) output.set(name, readFileSync(path, 'utf8'));
  }
  return output;
}
function fullOracle() {
  execFileSync(binary, ['--project', oracle, '--facts-in', factsPath, '--state', join(root, 'oracle-state.yaml')],
    { input: JSON.stringify(sources.map(s => s.record)), stdio: ['pipe', 'ignore', 'pipe'], timeout: 60000 });
}
function equalFiles() {
  assert.deepEqual(files(join(work, 'verified')), files(oracle), 'disk publication differs from full native board bytes');
  const generatedClock = text => text.replace(/scored_at_epoch_seconds: \d+/g, 'scored_at_epoch_seconds: CLOCK');
  assert.equal(generatedClock(readFileSync(join(work, 'pub/data/board_state.yaml'), 'utf8')),
    generatedClock(readFileSync(join(root, 'oracle-state.yaml'), 'utf8')), 'board submissions/listed/state bytes differ');
}
try {
  await normalize(); await publish('one'); fullOracle(); equalFiles();
  console.log('ok: every real board file, group index and state count matches the full native renderer byte for byte');
  succeeded = true;
} catch (error) { console.error('publication evidence: ' + root); throw error; }
finally {
  context.writer?.close(); catalog.close(); await native.close();
  if (succeeded && !process.env.WFSIM_INVENTORY_PUBLICATION_EVIDENCE) rmSync(root, { recursive: true, force: true });
}
