// SPDX-License-Identifier: AGPL-3.0-or-later
// Independent raw-input classification must reject altered projected membership.
import assert from 'node:assert/strict';
import { mkdtempSync,writeFileSync,readFileSync,rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join,resolve } from 'node:path';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';
import { auditInventoryProjection } from './inventory_projection.mjs';

const root = mkdtempSync(join(tmpdir(),'projection-audit-'));
const operator = await openInventoryOperator(resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board'));
const engine = '0ae1bf8edb0f', catalog = openCatalog(join(root,'catalog.sqlite'), {
  engine,contract:{ v:1,record_storage:'interned-deflate-v1' },create:true });
try {
  const source = JSON.parse(readFileSync(new URL('../tests/gas/rows.json',import.meta.url)))[0].record;
  const [d] = await operator.call('describe',[{ id:'source',record:source }]);
  const index = new InventoryRelations(catalog,operator.contract), row = { id:d.candidate,record:d.record };
  const raw = Object.fromEntries(['builds','queue','tasks'].map(name => [name,join(root,name)]));
  writeFileSync(raw.builds,JSON.stringify(row) + '\n'); writeFileSync(raw.queue,''); writeFileSync(raw.tasks,'');
  await index.sources([row],operator.call);
  while (index.pending()) await index.step(operator.call,{ budgetMs:100 });
  assert(catalog.summary().rows > 0);
  const cut = { raw,meta:{ engine,snapshot:'fixture',started_at:'2026-11-14T00:00:00.000Z',counts:{ library:1,queue:0,tasks:0 } },
    receipt:{ snapshot:'fixture',started_at:'2026-11-14T00:00:00.000Z' } };
  await auditInventoryProjection(index,cut,root);
  const mode = catalog.statement('SELECT mode FROM relation_inventory_owner_members LIMIT 1').get().mode;
  const old = catalog.name(mode);
  catalog.statement('UPDATE entity_inventory_symbols SET name=? WHERE id=?').run('invalid_fixture_mode',mode);
  await assert.rejects(auditInventoryProjection(index,cut,root), /digest mismatch/);
  catalog.statement('UPDATE entity_inventory_symbols SET name=? WHERE id=?').run('',mode);
  await assert.rejects(auditInventoryProjection(index,cut,root), /invalid inventory candidate/);
  catalog.statement('UPDATE entity_inventory_symbols SET name=? WHERE id=?').run(old,mode);
  await auditInventoryProjection(index,cut,root);
  console.log('ok: actual missing-work membership passes the independent audit; changed and absent modes fail; restored membership passes');
} finally { catalog.close(); await operator.close(); rmSync(root,{ recursive:true,force:true }); }
