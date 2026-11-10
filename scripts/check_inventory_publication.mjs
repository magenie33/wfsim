// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare actual disk-produced files with the unchanged full native renderer.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync, existsSync } from 'node:fs';
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
let native = await openInventoryOperator(binary);
const engine = '0ae1bf8edb0f', catalogPath = join(root, 'catalog.sqlite');
let catalog = openCatalog(catalogPath, { engine, contract: { v: 1 }, generation: 'fixture', create: true });
let index = new InventoryRelations(catalog, native.contract), context = {}, succeeded = false;
const records = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(r => r.record);
const rank = (id, n, max) => n === max ? id : id + '@' + n;
for (let i = 0; i < 300; i++) records.push({ ...records[1], mods: [
  rank('serration', i % 11, 10), rank('vital_sense', Math.floor(i / 11) % 6, 5), rank('hellfire', Math.floor(i / 66), 5),
  'speed_trigger', 'galvanized_aptitude', 'hammer_shot', 'vigilante_fervor', 'magnetic_capacity',
] });
const riven = { weapon: 'torid', arcanes: ['primary_deadhead'], exilus: 'vigilante_supplies',
  evolutions: ['torid_evo1_incarnon_form','torid_final_fusillade','torid_swift_deliverance','torid_survivors_edge'],
  mods: ['primed_cryo_rounds','malignant_force','hellfire','primary_acuity','galvanized_scope','vital_sense','hunter_track','riven'],
  riven_neg: 'status_duration', riven_pos: ['critical_chance','critical_damage','damage'] };
records.push({ weapon: 'tombfinger_secondary', grip: 'haymaker', loader: 'splat', arcanes: ['pax_charge','secondary_fortifier'],
  mods: ['jolt','pistol_pestilence','primed_heated_charge','pistol_acuity','hornet_strike','primed_target_cracker','galvanized_shot','anemic_agility'] },
{ weapon: 'sepfahn_one_handed', grip: 'peye', link: 'vargeet_jai_ii', arcanes: ['exodia_triumph','melee_exposure'], exilus: 'dreamers_wrath',
  mods: ['primed_fever_strike','vicious_frost','condition_overload','blood_rush','berserker_fury','weeping_wounds','magnetic_rush','riven'],
  riven_pos: ['critical_chance','critical_damage'], riven_neg: 'impact', riven_rolls: [1.1,1.1,0.9] });
records.push({ ...riven, riven_rolls: [0.9,1.1,1.1,0.9] }, { ...riven, riven_rolls: [1.1,1.1,1.1,0.9] });
const described = [];
for (let i = 0; i < records.length; i += 256)
  described.push(...await native.call('describe', records.slice(i, i + 256).map((record, n) => ({ id: 'seed-' + (i + n), record }))));
assert(described.every(row => row.candidate));
assert.equal(new Set(described.map(row => row.candidate)).size, records.length);
const sources = described.map((d, i) => ({ id: d.candidate, record: records[i] }));
sources.push({ id: 'alias', record: { ...records[0], note: 'source order alias' } }, { id: 'refused', record: { weapon: 'missing-weapon' } });
const facts = described.flatMap((d, i) => native.contract.rulers.flatMap((r, n) => d.modes.map(mode => ({
  identity: d.candidate, ruler: r.id, mode, score: (i >= records.length - 2 ? 123 : i === 3 ? 500 : i === 4 ? 100 : i === 5 ? 99.999999 : 400 + i % 100) + n * 0.000123456789,
  measured_by: i === 0 ? 'client:' + engine : 'verified:' + engine,
  cost_seconds: 1.25, started_at: '2026-11-09T00:00:00.000Z', finished_at: '2026-11-09T00:00:01.123Z',
}))));
const factsPath = join(root, 'facts.ndjson');
writeFileSync(factsPath, facts.map(row => JSON.stringify(row) + '\n').join(''));
const scene = native.contract.rulers.find(row => row.scene).id;
const legacy = '[{ "opaque": {"provenance":"raw bytes stay"} },' + JSON.stringify({ benchmark: 'removed_ruler_v1', score: 99 })
  + ',' + JSON.stringify({ benchmark: scene, mode: 'base', score: 42, mods: [] }) + ']';
mkdirSync(join(work, 'verified')); writeFileSync(join(work, 'verified/acceltra.json'), legacy);
async function normalize() {
  for (let offset = 0; offset < sources.length; offset += 256) await index.sources(sources.slice(offset, offset + 256), native.call);
  for (let offset = 0; offset < facts.length; offset += 256) await index.facts(facts.slice(offset, offset + 256), native.call);
  while (index.pending()) await index.step(native.call, { budgetMs: 100 });
}
async function publish(epoch, fault, operate = native.call) {
  for (let n = 0; n < 10000; n++) {
    const publication = new InventoryPublication(index, work, context, fault);
    const result = await publication.step({ epoch }, operate, { budgetMs: 50 });
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
  rmSync(oracle, { recursive: true, force: true }); mkdirSync(oracle);
  writeFileSync(join(oracle, 'acceltra.json'), legacy);
  execFileSync(binary, ['--project', oracle, '--facts-in', factsPath, '--state', join(root, 'oracle-state.yaml')],
    { input: JSON.stringify(sources.map(s => s.record)), stdio: ['pipe', 'ignore', 'pipe'], timeout: 60000 });
}
async function reopen() {
  context.writer?.close(); catalog.close(); await native.close(); native = await openInventoryOperator(binary);
  catalog = openCatalog(catalogPath); index = new InventoryRelations(catalog, native.contract); context = {};
}
function equalFiles() {
  assert.deepEqual(files(join(work, 'verified')), files(oracle), 'disk publication differs from full native board bytes');
  const generatedClock = text => text.replace(/scored_at_epoch_seconds: \d+/g, 'scored_at_epoch_seconds: CLOCK');
  assert.equal(generatedClock(readFileSync(join(work, 'pub/data/board_state.yaml'), 'utf8')),
    generatedClock(readFileSync(join(root, 'oracle-state.yaml'), 'utf8')), 'board submissions/listed/state bytes differ');
}
try {
  await normalize();
  let lost = true;
  await assert.rejects(publish('one', async stage => { if (stage === 'after-output-before-checkpoint' && lost) {
    lost = false; throw Error('uncheckpointed output tail');
  } }), /uncheckpointed output tail/);
  await reopen(); lost = true;
  await assert.rejects(publish('one', async stage => { if (stage === 'after-file-before-checkpoint' && lost) {
    lost = false; throw Error('renamed file reply loss');
  } }), /renamed file reply loss/);
  new InventoryPublication(index, work, context).compatibility('changed-publisher-after-rename');
  await reopen(); lost = true;
  await assert.rejects(publish('one', async stage => { if (stage === 'after-index-before-checkpoint' && lost) {
    lost = false; throw Error('renamed index reply loss');
  } }), /renamed index reply loss/);
  await reopen(); lost = true;
  await assert.rejects(publish('one', async stage => { if (stage === 'after-state-before-checkpoint' && lost) {
    lost = false; throw Error('state checkpoint reply loss');
  } }), /state checkpoint reply loss/);
  await reopen(); await publish('one'); fullOracle(); equalFiles();
  const pages = [...files(join(work, 'verified')).entries()].filter(([name]) => !name.endsWith('index.json')).flatMap(([, body]) => JSON.parse(body));
  assert(pages.length > 256); assert(pages.some(row => row.riven?.rolls[0] === 1.1));
  assert(!pages.some(row => row.riven?.rolls[0] === 0.9));
  const kitgun = JSON.parse(readFileSync(join(work, 'verified/tombfinger_secondary.json')));
  const zaw = JSON.parse(readFileSync(join(work, 'verified/sepfahn_one_handed.json')));
  assert(kitgun.length && kitgun.every(row => row.grip === 'haymaker' && row.loader === 'splat'));
  assert(zaw.length && zaw.every(row => row.grip === 'peye' && row.link === 'vargeet_jai_ii'));
  console.log('ok: cross-batch same-weapon order, global shape/default tie, inclusive floor and output/rename crash recovery match every native board/index/state byte');
  const before = files(join(work, 'verified'));
  const change = { ...facts.find(row => row.identity === described[1].candidate), measured_by: 'verified:ffffffffffff',
    finished_at: '2026-11-09T00:01:02.987Z', provenance: { original: 'author-clock-work', work: 123456789 } };
  await index.facts([change], native.call);
  const target = facts.findIndex(row => row.identity === change.identity && row.ruler === change.ruler && row.mode === change.mode);
  facts[target] = change; writeFileSync(factsPath, facts.map(row => JSON.stringify(row) + '\n').join(''));
  while (index.pending()) await index.step(native.call, { budgetMs: 100 });
  await publish('two'); fullOracle(); equalFiles();
  assert.notDeepEqual(files(join(work, 'verified')), before, 'metadata-only fact updates must reach publication');
  assert([...files(join(work, 'verified')).values()].some(body => body.includes('ffffffffffff')));
  assert.deepEqual(JSON.parse(index.q('SELECT body FROM rel_publication_facts WHERE identity=? AND ruler=? AND mode=?')
    .get(change.identity, change.ruler, change.mode).body).provenance, change.provenance);
  assert(existsSync(join(work, 'verified/braton_prime.json')));
  console.log('ok: warm metadata-only fact replacement retains score, original author/clock and exact native bytes');
  const dirty = () => { for (const row of index.q('SELECT DISTINCT weapon FROM rel_publication_sources').iterate()) index.publicationDirty(row.weapon); };
  dirty(); await publish('broken-floor', undefined, async (op, rows) => op === 'floor' ? rows.map(() => ({ keep: true })) : native.call(op, rows));
  assert.throws(equalFiles, /disk publication differs from full native board bytes/);
  dirty(); await publish('repaired-floor'); fullOracle(); equalFiles();
  console.log('ok: a real keep-all-floor mutant fails the native byte oracle; restored native decisions pass');
  succeeded = true;
} catch (error) { console.error('publication evidence: ' + root); throw error; }
finally {
  context.writer?.close(); catalog.close(); await native.close();
  if (succeeded && !process.env.WFSIM_INVENTORY_PUBLICATION_EVIDENCE) rmSync(root, { recursive: true, force: true });
}
