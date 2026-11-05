// SPDX-License-Identifier: AGPL-3.0-or-later
// Real Worker initialization must load its data before answering any query.
import assert from 'node:assert/strict';
import { register } from 'node:module';

register(new URL('./mcp_test_loader.mjs', import.meta.url));
const { default: worker } = await import('../mcp/index.js');
const { wasm_bindgen: engine } = await import('../mcp/engine.js');
const { HEADLESS_QUERIES, headlessToolName } = await import('../mcp/headless.js');
let meta, names;
const api = (p, b) => JSON.parse(engine.api(p, JSON.stringify(b ?? {})));
const host = { meta: () => meta ??= api('/api/meta'), names: () => names ??= Object.values(api('/api/i18n')),
  api: async (p, b) => api(p, b), tr: s => s, origin: 'https://wfsim.app', screen_build: () => null, screen_weapon: () => null };
const cases = [
  ['builder.stats.read', { build: { weapon: 'kuva_bramma', mods: ['magazine_warp'] } }],
  ['builder.stats.read', { build: { weapon: 'venato', mods: ['sentient_incision'] } }],
  ['builder.weapons.find', { query: '月神' }],
];
for (const [id, args] of cases) {
  const q = HEADLESS_QUERIES.find(q => q.id === id);
  const response = await worker.fetch(new Request('https://mcp.wfsim.app/mcp', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: headlessToolName(id), arguments: args } }),
  }), { ADDRESS_LIMIT: { limit: async () => ({ success: true }) } });
  assert.equal(response.status, 200);
  const actual = (await response.json()).result?.structuredContent;
  assert.equal(actual?.ok, true, JSON.stringify(actual));
  if (id === 'builder.stats.read') assert(actual.forms?.length > 0, 'a real panel needs installed game data');
  else assert(actual.rows?.some(w => w.id === 'soma'), 'the Chinese lookup needs the text pack');
  assert.deepEqual(actual, { ok: true, ...await q.run(args, host) });
  console.log(`  ok  real MCP entry: ${id} ${JSON.stringify(args)}`);
}
