// SPDX-License-Identifier: AGPL-3.0-or-later
// Semantic identity is compact; runtime bytes and unknown inputs stay strict.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { compatibilityCanonical, compatibilityKey, readInventoryCompatibility } from './inventory_compatibility.mjs';

const root = mkdtempSync(join(tmpdir(), 'inventory-compat-')), binary = 'a'.repeat(64);
const inputs = { mode: 'semantic', recipe: 'inventory-catalog-v1', target: 'x86_64-unknown-linux-gnu',
  build: { rust: 'fixture', container: 'fixture', command: 'fixture', rustflags: '', features: [], rules: 'b'.repeat(64), yaml: 'fixture',
    environment: { WFSIM_DATA_APART: null, WFSIM_GAS_OFF: null } }, crates: ['cli','engine','optimizer','webapi'],
  manifests: Object.fromEntries(['Cargo.toml','Cargo.lock','cli/Cargo.toml','engine/Cargo.toml','optimizer/Cargo.toml','webapi/Cargo.toml'].map(path => [path, binary])),
  rust: Object.fromEntries(['cli','engine','optimizer','webapi'].map(crate => [crate + '/fixture.rs', binary])),
  engine_data: { 'data/fixture.yaml': binary }, engine_code: binary, unknown: [] };
const put = (data = inputs, extra = {}) => writeFileSync(join(root, 'inventory-compat.json'), JSON.stringify({ format: 1,
  recipe: inputs.recipe, target: inputs.target, key: compatibilityKey(data), inputs: data, binary_sha256: binary, ...extra }));
try {
  assert.equal(readInventoryCompatibility(root, binary).mode, 'strict-binary'); put();
  const held = readInventoryCompatibility(root, binary); assert.equal(held.mode, 'semantic'); assert(JSON.stringify(held).length < 300);
  const large = structuredClone(inputs);
  for (let i = 0; i < 5000; i++) large.engine_data['data/fixture_' + i + '.yaml'] = binary;
  put(large); assert(JSON.stringify(readInventoryCompatibility(root, binary)).length < 300, '5k input hashes must not enter persisted identity');
  const fixture = { z: ['界😀\u007f', null], a: { y: true, x: 42 } };
  const canonical = execFileSync('python3', ['-c','import sys,json;print(json.dumps(json.load(sys.stdin),sort_keys=True,separators=(",",":"),allow_nan=False),end="")'],
    { input: JSON.stringify(fixture), encoding: 'utf8' });
  assert.equal(compatibilityCanonical(fixture), canonical, 'consumer canonical bytes must match the producer recursively');
  for (const change of [data => { data.rust['cli/fixture.rs'] = 'c'.repeat(64); }, data => { data.manifests['Cargo.lock'] = 'c'.repeat(64); },
    data => { data.build.rust = 'other-toolchain'; }, data => { data.engine_data['data/fixture.yaml'] = 'c'.repeat(64); }]) {
    const changed = structuredClone(inputs); change(changed); put(changed);
    assert.notDeepEqual(readInventoryCompatibility(root, binary).identity, held.identity);
  }
  for (const change of [data => { data.unknown = ['future-input']; }, data => { delete data.manifests['Cargo.lock']; },
    data => { data.mode = 'full-fallback'; data.fallback = { binary_sha256: binary, raw_inputs: {} }; }]) {
    const changed = structuredClone(inputs); change(changed); put(changed); assert.equal(readInventoryCompatibility(root, binary).mode, 'strict-binary');
  }
  put(inputs, { recipe: 'future-recipe' }); assert.equal(readInventoryCompatibility(root, binary).mode, 'strict-binary');
  put(inputs, { key: '0'.repeat(64) }); assert.throws(() => readInventoryCompatibility(root, binary), /input digest mismatch/);
  put(inputs, { binary_sha256: '0'.repeat(64) }); assert.throws(() => readInventoryCompatibility(root, binary), /binary binding mismatch/);
  put(inputs, { recipe: 'future', binary_sha256: '0'.repeat(64) }); assert.throws(() => readInventoryCompatibility(root, binary), /binary binding mismatch/);
  console.log('ok: compact semantic identity, Python canonical bytes, Rust/lock/toolchain/data changes, unknown/missing closure fallback and wrong-binding refusal');
  const result = spawnSync(process.execPath, ['scripts/check_inventory_board_pipeline.mjs'], {
    stdio: 'inherit', env: { ...process.env, WFSIM_CHECK_INVENTORY_COMPAT: '1' }, timeout: 180000,
  });
  assert.equal(result.status, 0, result.error?.message || 'actual controller compatibility checks failed');
} finally { rmSync(root, { recursive: true, force: true }); }
