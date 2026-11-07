// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare the research-only retained plan with the actual shipping wasm answers.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { performance } from 'node:perf_hooks';
import { loadEngine } from './wasm_engine.mjs';

const dir = process.argv[2] || 'target/bake/wasm';
const cases = JSON.parse(fs.readFileSync('target/bake/cases.json', 'utf8'));
const release = JSON.parse(fs.readFileSync('site/release.json', 'utf8'));
const context = vm.createContext({ console, TextEncoder, TextDecoder, WebAssembly });
vm.runInContext(fs.readFileSync(`${dir}/prepared_cost.js`, 'utf8') + '\nthis.bench = wasm_bindgen;', context);
context.bytes = fs.readFileSync(`${dir}/prepared_cost_bg.wasm`);
vm.runInContext('bench.initSync({ module: bytes })', context);
const B = context.bench;
const pack = release.packs.find(n => n.startsWith('pack.data.'));
assert.equal(B.install_data(fs.readFileSync(`site/pkg/${pack}`, 'utf8')), '');
const official = loadEngine();
const result = [];
for (const c of cases) {
  const body = JSON.stringify(c.request);
  B.install(body);
  const expected = official.api('/api/board/fold', { request: c.request, from: 0, count: 40 });
  assert.equal(expected.ok, true, c.id);
  B.prepare(body, 10);
  const preparation = [];
  for (let i = 0; i < 3; i++) {
    const start = performance.now();
    B.prepare(body, 200);
    preparation.push((performance.now() - start) / 200);
  }
  preparation.sort((a, b) => a - b);
  const variants = [];
  for (const width of [1, 7, 40]) {
    for (const retain of [false, true]) assert.deepEqual(JSON.parse(B.run(width, retain)), expected, `${c.id}/${width}/${retain}`);
    const current_ms = [], retained_ms = [];
    for (let rep = 0; rep < 2; rep++) {
      for (const retain of rep % 2 ? [true, false, false, true] : [false, true, true, false]) {
        const start = performance.now();
        const out = B.run(width, retain);
        const elapsed = performance.now() - start;
        assert.deepEqual(JSON.parse(out), expected);
        (retain ? retained_ms : current_ms).push(elapsed);
      }
    }
    variants.push({ width, current_ms, retained_ms });
  }
  console.log(`${c.id}: WASM prepare ${preparation[1].toFixed(5)} ms; all complete accumulators equal the shipping engine`);
  result.push({ id: c.id, prepare_ms: preparation[1], preparation_samples_ms: preparation,
    request_bytes: Buffer.byteLength(body), variants, result: expected });
}
fs.writeFileSync('target/bake/wasm.json', JSON.stringify(result));
