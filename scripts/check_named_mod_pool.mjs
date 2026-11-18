// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_named_mod_pool.mjs BASE_NODE NEW_NODE OUTPUT [ABBA_ROUNDS]
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';

const [before, after, output, rounds = '6'] = process.argv.slice(2);
assert.ok(before && after && output && Number.isSafeInteger(+rounds) && +rounds > 0);
const require = createRequire(import.meta.url);
const paths = [before, after].map(p => resolve(p));
const apis = paths.map(p => require(join(p, 'wfsim_wasm.js')).api);
const sha = data => createHash('sha256').update(data).digest('hex');
const read = p => JSON.parse(readFileSync(new URL(`../${p}`, import.meta.url), 'utf8'));
const own = (id, roll) => ({ id, spec: { bonuses: [{ id: 'heat', roll }, { id: 'critical_damage', roll }] } });
const short = { duration: 1, runs: 1, seed: 0x5eed };
const tests = [];
const add = (id, path, body, ok = true) => tests.push({ id, path, body, ok });
const boards = { plain: [], riven: [] };
for (const weapon of ['braton_prime', 'torid', 'ocucor', 'magistar', 'kuva_nukor']) {
  const rows = read(`site/board/${weapon}.json`);
  for (const riven of [false, true]) {
    const group = riven ? 'riven' : 'plain';
    const row = rows.find(r => r.benchmark === 'standard_single_target' && Boolean(r.riven) === riven)
      ?? rows.find(r => Boolean(r.riven) === riven);
    assert.ok(row, `${weapon} ${group} board row`);
    const record = Object.fromEntries(['mods', 'arcanes', 'evolutions', 'valence', 'exilus']
      .filter(k => k in row).map(k => [k, row[k]]));
    Object.assign(record, { weapon, riven_pos: row.riven?.bonuses, riven_neg: row.riven?.malus, riven_rolls: row.riven?.rolls });
    const order = { record, ruler: row.benchmark, mode: row.mode || 'base' };
    const id = `${weapon}-${group}`;
    add(`${id}-order`, '/api/board/order', order);
    const open = JSON.parse(apis[0]('/api/board/order', JSON.stringify(order)));
    assert.ok(open.ok, `${id}: ${JSON.stringify(open)}`);
    const req = { ...open.request, ...short };
    assert.equal(req.mods.some(m => m.startsWith('riven:')), riven);
    boards[group].push(req);
    add(id, '/api/simulate', req);
    add(`${id}-panel`, '/api/panel', req);
    add(`${id}-shard`, '/api/board/runs', { request: req, from: 0, count: 1 });
    add(`${id}-log`, '/api/log', req);
  }
}

const sameId = { ...short, weapon: 'braton_prime', mods: ['riven:mine', 'hellfire@4', 'cryo_rounds'] };
const requestA = { ...sameId, rivens: [own('mine', 0.94), own('mine', 1.06)] };
const requestB = { ...sameId, rivens: requestA.rivens.slice().reverse() };
const evo = { ...short, weapon: 'dual_toxocyst', mode: 'base', evolutions_as_given: true, mods: ['semi_pistol_cannonade@3'] };
for (const path of ['/api/simulate', '/api/panel']) {
  const tag = path.split('/').at(-1);
  add(`${tag}-first`, path, requestA);
  add(`${tag}-swapped`, path, requestB);
  add(`${tag}-first-again`, path, requestA);
  add(`${tag}-bare-evo`, path, { ...evo, evolutions: [] });
  add(`${tag}-incarnon-evo`, path, { ...evo, evolutions: ['dual_toxocyst_evo1_incarnon_form'] }, false);
  add(`${tag}-unknown`, path, { ...sameId, mods: ['riven:missing'] }, false);
  add(`${tag}-unknown-stat`, path, { ...requestA, rivens: [...requestA.rivens, { id: 'mine', spec: { positives: [{ id: 'typo' }] } }] }, false);
}
add('rank-conflict', '/api/simulate', { ...sameId, mods: ['hellfire@4', 'hellfire'] }, false);
add('riven-conflict', '/api/simulate', { ...sameId, mods: ['riven:one', 'riven:two'], rivens: [own('one', .94), own('two', 1.06)] }, false);
const { request: scope } = read('tests/client_search.json');
scope.mods['riven:mine'] = 'fixed';
Object.assign(scope, { rivens: [own('mine', 0.94)], search_strategy: 'four_elements',
  starts: [{ slots: ['riven:mine'], fixed: [{ kind: 'mods', idx: 0, placement: 'slot' }] }] });
add('riven-optimizer', '/api/compute/optimize', scope);

mkdirSync(output);
const write = (name, value) => writeFileSync(join(output, name), JSON.stringify(value, null, 2) + '\n');
write('calls.json', tests);
const responses = [];
const byId = new Map();
for (const { id, path, body, ok } of tests) {
  const wire = JSON.stringify(body);
  const [a, b] = apis.map(api => api(path, wire));
  if (a !== b) {
    writeFileSync(join(output, 'before-mismatch.json'), a);
    writeFileSync(join(output, 'after-mismatch.json'), b);
  }
  assert.ok(a === b, `${id}: response differs`);
  const parsed = JSON.parse(a);
  assert.equal(parsed.ok, ok, `${id}: ${a}`);
  if (path === '/api/simulate' && ok) assert.ok(parsed.shots > 0, `${id}: no shots`);
  byId.set(id, a);
  responses.push({ id, sha256: sha(a), raw: a });
}
for (const tag of ['simulate', 'panel']) {
  assert.notEqual(byId.get(`${tag}-first`), byId.get(`${tag}-swapped`));
  assert.equal(byId.get(`${tag}-first`), byId.get(`${tag}-first-again`));
}
assert.ok(JSON.parse(byId.get('riven-optimizer')).results.some(r => r.mods.includes('riven:mine')));
write('responses.json', responses);
console.log(`${responses.length} complete responses match`);

const report = { node: process.version, rounds: +rounds, exact: true, responses: responses.length,
  artifacts: paths.map(path => ({ path, sha256: sha(readFileSync(join(path, 'wfsim_wasm_bg.wasm'))) })), timings: [] };
const med = xs => {
  const sorted = xs.slice().sort((a, b) => a - b);
  return (sorted[xs.length / 2 - 1] + sorted[xs.length / 2]) / 2;
};
for (const [path, batch] of [['/api/simulate', 80], ['/api/panel', 40]]) {
  for (const [group, cases] of Object.entries(boards)) {
    const wires = Array.from({ length: batch }, (_, i) => JSON.stringify({ ...cases[i % cases.length], seed: short.seed + i }));
    const run = (side, expected = null) => {
      const start = performance.now();
      const actual = wires.map(wire => apis[side](path, wire));
      const ms = performance.now() - start;
      if (expected) assert.deepEqual(actual, expected, `${path} ${group} side ${side}`);
      return { actual, ms };
    };
    const { actual: expected } = run(0);
    run(1, expected);
    const samples = [];
    for (let i = 0; i < +rounds; i++) for (const side of [0, 1, 1, 0]) {
      const { ms } = run(side, expected);
      samples.push({ side, ms });
    }
    const medianMs = [0, 1].map(side => med(samples.filter(s => s.side === side).map(s => s.ms)));
    report.timings.push({ path, group, batch, wires, samples, medianMs, sha256: sha(JSON.stringify(expected)) });
    write('report.json', report);
    console.log(`${path} ${group} x${batch}: ${medianMs.map(ms => ms.toFixed(2)).join(' -> ')} ms`);
  }
}
