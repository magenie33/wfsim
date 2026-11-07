// SPDX-License-Identifier: AGPL-3.0-or-later
// The emitted library's complete responses must equal the checked wasm's.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { loadEngine } from './wasm_engine.mjs';

const args = Object.fromEntries(process.argv.slice(2).map(a => {
  const i = a.indexOf('='); return [a.slice(0, i), a.slice(i + 1)];
}));
assert(args.pkg && args.library, 'give pkg=<checked wasm directory> library=<native library>');
const base = args.library.slice(0, -path.extname(args.library).length);
const report = JSON.parse(readFileSync(base + '.pgo.json', 'utf8'));
const raw = readFileSync(base + '.verify.json');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
assert.equal(sha(readFileSync(args.library)), report.selected_sha256, 'the library must be the measured bytes');
assert.equal(sha(raw), report.verification_sha256, 'the verification must be the measured responses');
const data = readdirSync(args.pkg).filter(n => /^pack\.data(?:\.[a-f0-9]+)?\.json$/.test(n));
assert.equal(data.length, 1, 'one checked data pack');
assert.equal(sha(readFileSync(path.join(args.pkg, data[0]))), report.data_sha256, 'native and wasm use the same data pack');
const expected = JSON.parse(raw), engine = loadEngine({ pkg: args.pkg });
const calls = JSON.parse(readFileSync('tests/fixtures/simulation-cost.json', 'utf8'));
assert.equal(Object.keys(expected.api).length, calls.length);
for (const c of calls) {
  assert.deepEqual(expected.api[c.id], engine.api(c.path, c.body), c.id);
}
const rows = JSON.parse(readFileSync('tests/gas/rows.json', 'utf8'));
assert.equal(Object.keys(expected.rows).length, rows.length);
for (const [i, row] of rows.entries()) {
  const order = engine.api('/api/board/order', row);
  assert.equal(order.ok, true);
  const request = { ...order.request, runs: 40 };
  const fold = engine.api('/api/board/fold', { request, from: 0, count: 40 });
  assert.equal(fold.ok, true);
  const score = engine.api('/api/board/score', { request, ruler: row.ruler, acc: fold.acc });
  assert.deepEqual(expected.rows[`${i}-${row.record.weapon}`], score, `${row.record.weapon} score/counts/gas/work`);
}
console.log(`Native PGO: emitted bytes and data verified; ${calls.length} API responses and ${rows.length} row scores/counts/gas/work equal the checked wasm.`);
