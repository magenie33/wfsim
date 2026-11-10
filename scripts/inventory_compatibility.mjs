// SPDX-License-Identifier: AGPL-3.0-or-later
// Qualified semantic identity never substitutes for the actual executable binding.
import { existsSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { canon } from '../worker/task_contract.js';

const SHA = /^[a-f0-9]{64}$/;
const sha = value => typeof value === 'string' && SHA.test(value);
export const INVENTORY_COMPAT_RECIPE = 'inventory-catalog-v1';
export const compatibilityCanonical = value => canon(value).replace(/[^\x00-\x7e]/g, c => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'));
export const compatibilityKey = value => createHash('sha256').update(compatibilityCanonical(value)).digest('hex');
const map = value => value && !Array.isArray(value) && typeof value === 'object' && Object.keys(value).length
  && Object.entries(value).every(([path, hash]) => path && sha(hash));
const strings = value => Array.isArray(value) && value.every(item => typeof item === 'string');
const exact = (value, keys) => value && canon(Object.keys(value).sort()) === canon(keys.sort());
function semanticInputs(inputs, target) {
  return exact(inputs, ['mode','recipe','target','build','crates','manifests','rust','engine_data','engine_code','unknown'])
    && inputs.mode === 'semantic' && inputs.recipe === INVENTORY_COMPAT_RECIPE && inputs.target === target
    && exact(inputs.build, ['rust','container','command','rustflags','features','rules','environment','yaml'])
    && ['rust','container','command','rustflags','yaml'].every(key => typeof inputs.build[key] === 'string')
    && exact(inputs.build.environment, ['WFSIM_DATA_APART','WFSIM_GAS_OFF'])
    && Object.values(inputs.build.environment).every(value => value === null || typeof value === 'string')
    && inputs.build.rust && inputs.build.container && inputs.build.command && sha(inputs.build.rules)
    && strings(inputs.build.features) && strings(inputs.crates)
    && ['cli','engine','optimizer','webapi','tools/gas-macro'].every(crate => inputs.crates.includes(crate))
    && map(inputs.manifests) && ['Cargo.toml','Cargo.lock','cli/Cargo.toml','engine/Cargo.toml','optimizer/Cargo.toml','webapi/Cargo.toml']
      .every(path => sha(inputs.manifests[path]))
    && inputs.crates.every(crate => sha(inputs.manifests[crate + '/Cargo.toml']))
    && map(inputs.rust) && inputs.crates.every(crate => Object.keys(inputs.rust).some(path => path.startsWith(crate + '/')))
    && map(inputs.engine_data) && sha(inputs.engine_code) && Array.isArray(inputs.unknown) && inputs.unknown.length === 0;
}
export function readInventoryCompatibility(bin, adapter) {
  const path = join(bin, 'inventory-compat.json'), fallback = { mode: 'strict-binary', identity: { binary_sha256: adapter } };
  if (!existsSync(path)) return fallback;
  if (statSync(path).size > 8 * 1024 ** 2) throw Error('inventory compatibility sidecar exceeds byte bound');
  let sidecar; try { sidecar = JSON.parse(readFileSync(path, 'utf8')); } catch { return fallback; }
  if (sidecar?.binary_sha256 !== undefined && sidecar.binary_sha256 !== adapter) throw Error('inventory compatibility binary binding mismatch');
  if (!exact(sidecar, ['format','recipe','target','key','inputs','binary_sha256']) || sidecar.format !== 1 || sidecar.recipe !== INVENTORY_COMPAT_RECIPE
      || sidecar.target !== 'x86_64-unknown-linux-gnu' || !sha(sidecar.binary_sha256)
      || !sha(sidecar.key) || !sidecar.inputs || typeof sidecar.inputs !== 'object') return fallback;
  if (!semanticInputs(sidecar.inputs, sidecar.target)) return fallback;
  if (compatibilityKey(sidecar.inputs) !== sidecar.key) throw Error('inventory compatibility input digest mismatch');
  return { mode: 'semantic', identity: { format: 1, recipe: sidecar.recipe, target: sidecar.target, key: sidecar.key } };
}
