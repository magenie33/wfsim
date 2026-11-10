// SPDX-License-Identifier: AGPL-3.0-or-later
// Every site release executes build, simulate and solve with its actual module and packs.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { loadEngine } from './wasm_engine.mjs';

const fixture = JSON.parse(readFileSync(process.env.WFSIM_RELEASE_SMOKE_FIXTURE || 'tests/client_search.json', 'utf8'));
const engine = loadEngine();
const build = { ...fixture.request, mods: fixture.request.starts[0].mods, arcanes: [], evolutions: [] };
const panel = engine.api('/api/panel', build);
assert.equal(panel.ok, true, JSON.stringify(panel));
assert(panel.forms?.length > 0, 'build: actual data produces a stats panel');
const simulation = engine.api('/api/simulate', build);
assert.equal(simulation.ok, true, JSON.stringify(simulation));
assert(simulation.panel?.modified_base > 0, 'simulate: actual fight carries the built weapon');
const solution = engine.optimize(fixture.request);
assert.notEqual(solution.ok, false, JSON.stringify(solution));
assert(solution.results?.some(row => row.mods?.length), 'solve: actual optimizer produces a complete build');
for (const route of ['/api/panel', '/api/simulate']) {
  assert.equal(engine.api(route, { ...build, weapon: '__missing_release_weapon__' }).ok, false);
}
assert.equal(engine.optimize({ ...fixture.request, weapon: '__missing_release_weapon__' }).ok, false);
console.log('Release core PASS: build, simulate and solve on the shipping WASM/data; unchanged fixture fight and search budgets');
