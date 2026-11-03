// SPDX-License-Identifier: AGPL-3.0-or-later
// Bind current engine identities to existing library inputs before asking their rows.
import { createReadStream, readFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { query, close } from './db.mjs';
async function* rows(path) {
  for await (const line of createInterface({ input: createReadStream(path), crlfDelay: Infinity })) if (line.trim()) yield JSON.parse(line);
}
export async function bindInventoryBuilds({ builds, canonical, missing, queue, orphans, rulers }, run = query) {
  const known = new Set(), sources = [], needed = new Set(), bindings = new Map(), excluded = new Set(), roster = new Set(rulers);
  const key = r => JSON.stringify([r.build_id, r.ruler, r.mode]);
  for await (const r of rows(builds)) { known.add(r.id); sources.push(r.at); }
  for await (const r of rows(orphans)) excluded.add(key(r));
  for (const path of [missing, queue]) for await (const r of rows(path)) {
    if (roster.has(r.ruler) && !excluded.has(key(r)) && !known.has(r.build_id)) needed.add(r.build_id);
  }
  for await (const r of rows(canonical)) {
    if (!needed.has(r.id)) continue;
    const at = Number.isInteger(r.source) && sources[r.source];
    if (typeof at !== 'string' || !r.record) throw new Error('incomplete canonical build binding');
    const prior = bindings.get(r.id);
    if (!prior || at < prior.at) bindings.set(r.id, { id: r.id, at, record: JSON.stringify(r.record) });
  }
  if (bindings.size !== needed.size) throw new Error('inventory contains an unbound build identity');
  const values = [...bindings.values()];
  for (let i = 0; i < values.length; i += 500) {
    const batch = values.slice(i, i + 500);
    await run(`INSERT INTO builds(id,at,record) SELECT id,at,record FROM jsonb_to_recordset(($1::text)::jsonb) x(id text,at text,record text)
      ON CONFLICT(id) DO NOTHING`, [JSON.stringify(batch)]);
    const got = await run('SELECT id FROM builds WHERE id IN (SELECT jsonb_array_elements_text(($1::text)::jsonb))', [JSON.stringify(batch.map(r => r.id))]);
    if (got.length !== batch.length) throw new Error('canonical build binding did not land');
  }
  return values.length;
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [builds, canonical, missing, queue, orphans, roster] = process.argv.slice(2);
  if (!roster) throw new Error('usage: bind_inventory_builds.mjs builds canonical missing queue orphans roster');
  try { console.log(`inventory: ${await bindInventoryBuilds({ builds, canonical, missing, queue, orphans,
    rulers: readFileSync(roster, 'utf8').trim().split(/\s+/) })} current build identities bound`); }
  finally { await close(); }
}
