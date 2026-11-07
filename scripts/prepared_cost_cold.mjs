// SPDX-License-Identifier: AGPL-3.0-or-later
// Separate first-use catalog loading from warmed request preparation.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { performance } from 'node:perf_hooks';

const code = fs.readFileSync('target/bake/wasm/prepared_cost.js', 'utf8') + '\nthis.bench = wasm_bindgen;';
const bytes = fs.readFileSync('target/bake/wasm/prepared_cost_bg.wasm');
const before = performance.now();
const module = new WebAssembly.Module(bytes);
const compilation_ms = performance.now() - before;
const release = JSON.parse(fs.readFileSync('site/release.json', 'utf8'));
const pack = fs.readFileSync('site/pkg/' + release.packs.find(n => n.startsWith('pack.data.')), 'utf8');
const cases = JSON.parse(fs.readFileSync('target/bake/cases.json', 'utf8'))
  .filter(c => ['api/torid-fold', 'api/rhino-roar-fold', 'api/two-combatants-fold', 'api/torid-361-fold'].includes(c.id));
const result = [];
for (const c of cases) {
  const samples = [];
  for (let rep = 0; rep < 3; rep++) {
    const context = vm.createContext({ console, TextEncoder, TextDecoder, WebAssembly });
    vm.runInContext(code, context);
    context.module = module;
    vm.runInContext('bench.initSync({ module })', context);
    const B = context.bench, body = JSON.stringify(c.request);
    let start = performance.now();
    assert.equal(B.install_data(pack), '');
    const pack_install_ms = performance.now() - start;
    start = performance.now();
    B.prepare(body, 1);
    const first_prepare_ms = performance.now() - start;
    start = performance.now();
    B.prepare(body, 200);
    const warm_prepare_ms = (performance.now() - start) / 200;
    samples.push({ pack_install_ms, first_prepare_ms, warm_prepare_ms });
  }
  result.push({ id: c.id, samples });
}
fs.writeFileSync('target/bake/cold.json', JSON.stringify({ compilation_ms, result }, null, 2));
console.log(JSON.stringify({ compilation_ms, result }, null, 2));
