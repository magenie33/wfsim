// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_form_lookup.mjs OLD_NODE_PKG NEW_NODE_PKG [report.json]
// Both wasm-bindgen packages embed the same data. No live board calls.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

const [oldDir, newDir, out] = process.argv.slice(2);
assert(oldDir && newDir, 'expected baseline and candidate directories');
const load = createRequire(import.meta.url);
const pkgs = [oldDir, newDir].map(dir => load(resolve(dir, 'wfsim_wasm.js')));
const fixture = name => JSON.parse(readFileSync(new URL(`../tests/fixtures/engine-compare/${name}.json`, import.meta.url)));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const digest = createHash('sha256');
const call = (i, path, body) => pkgs[i].api(path, JSON.stringify(body));
function equal(path, body) {
  const before = call(0, path, body);
  assert(call(1, path, body) === before, `${path}: ${JSON.stringify(body).slice(0, 200)}`);
  digest.update(before);
  return JSON.parse(before);
}

const meta = equal('/api/meta', {});
assert(meta.weapons.length > 100);
for (const {id} of meta.weapons) {
  const reply = equal('/api/panel', { weapon: id });
  assert(reply.ok, `${id}: ${reply.error}`);
}
assert.equal(equal('/api/panel', {weapon:'not-in-the-catalog'}).ok, false);
assert.equal(equal('/api/simulate', {weapon:'bo_prime',mode:'base'}).ok, false);
const controls = fixture('native-calls');
for (const c of controls) assert(equal(c.path, c.body).ok, c.id);

// Time complete APIs, not a private hash-table lookup. Include catalog misses
// for heavy forms on guns and multi-form, melee and modular resolutions.
const weapons = ['torid','synoid_gammacor','dual_toxocyst','bo_prime','magistar','phantasma_prime',
  'sporelacer_secondary','tombfinger_primary','boar_prime','kuva_grattler','verglas_prime','coda_bubonico'];
const batches = {
  panels: weapons.map(weapon => ['/api/panel', {weapon}]),
  simulations: weapons.map(weapon => ['/api/simulate', {weapon, mode:weapon==='bo_prime'?'heavy':'base', enemy:'corrupted_heavy_gunner',
    level:100, runs:1, duration:2, seed:24301, headshot_pct:50}]),
};
const med = rows => {const a = rows.toSorted((x,y)=>x-y); return (a[(a.length-1)>>1]+a[a.length>>1])/2;};
const result = { node:process.version, wasm:[oldDir,newDir].map(p=>hash(readFileSync(resolve(p,'wfsim_wasm_bg.wasm')))),
  roster:meta.weapons.length, controls:controls.length, passes:12, batches:{} };
for (const [name,batch] of Object.entries(batches)) {
  const cases = batch.map(([path,body]) => {
    assert.equal(equal(path,body).ok, true, `${name}: ${body.weapon}`);
    return [path,JSON.stringify(body),call(0,path,body)];
  });
  const ms = [[],[]];
  for (const side of 'ABBA'.repeat(3)) {
    const i = side === 'A' ? 0 : 1, start = performance.now();
    for (let pass=0;pass<result.passes;pass++) for (const [path,wire,expected] of cases) {
      assert(pkgs[i].api(path,wire) === expected, `${side}: ${path}`);
    }
    ms[i].push(performance.now()-start);
  }
  result.batches[name] = {requests_per_sample:result.passes*cases.length, medians_ms:ms.map(med), ms};
}
result.answers = digest.digest('hex');
console.log(JSON.stringify(result,null,2));
if(out) writeFileSync(out, JSON.stringify(result,null,2));
