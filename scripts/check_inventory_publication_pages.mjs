// SPDX-License-Identifier: AGPL-3.0-or-later
// Publication pages must bind a weapon before looking up its measurement keys.
import assert from 'node:assert/strict';
import { mkdtempSync,rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { openCatalog } from './incremental_inventory.mjs';
import { InventoryRelations } from './inventory_relations.mjs';
import { InventoryPublication,PUBLICATION_ROWS_SQL } from './inventory_publication.mjs';

const root = mkdtempSync(join(tmpdir(),'publication-pages-'));
const catalog = openCatalog(join(root,'catalog'),{ engine:'0123456789ab',contract:{ v:1 },create:true });
const broken = `SELECT f.identity FROM state_catalog_publication_measurements f INDEXED BY rel_publication_facts_rows
  JOIN state_catalog_publication_sources s ON s.candidate=f.identity AND s.weapon=? AND (s.admit & ?)<>0
  WHERE f.ruler=? AND f.retired=0 AND (f.identity,f.mode)>(?,?) ORDER BY f.identity,f.mode LIMIT ?`;
function bounded(plan) {
  const searches = plan.filter(row => /SEARCH [sf] USING/.test(row.detail)).map(row => row.detail);
  assert.match(searches[0],/SEARCH s .*weapon=\? AND candidate>\?/);
  assert.match(searches[1],/SEARCH f .*identity=\?/);
}
try {
  const index = new InventoryRelations(catalog,{ v:1,entry_share:.2,rulers:[{ id:'r',scene:'' }] });
  new InventoryPublication(index,root,{});
  const args = ['fixture',1n,'r','','','',-1,-1,256];
  let added = 0;
  for (const size of [1000,10000]) {
    index.transaction(() => { for (let n = added; n < size; n++) {
      const id = String(n).padStart(8,'0');
      index.q('INSERT INTO entity_catalog_sources(id,candidate,body) VALUES(?,?,?)').run(id,id,'{}');
      const seq = index.q('SELECT seq FROM entity_catalog_sources WHERE id=?').get(id).seq;
      index.q('INSERT INTO state_catalog_publication_sources VALUES(?,?,?,?,?,?)')
        .run(id,seq,id,n === 0 ? 'fixture' : 'unrelated',1n,'[{"id":"base","mode":"base"}]');
      index.q('INSERT INTO state_catalog_publication_measurements VALUES(?,?,?,?,?,?)').run(id,'r','base','at',0,'{}');
    } }); added = size;
    assert.throws(() => bounded(catalog.db.prepare('EXPLAIN QUERY PLAN ' + broken).all('fixture',1n,'r','','',256)));
    bounded(catalog.db.prepare('EXPLAIN QUERY PLAN ' + PUBLICATION_ROWS_SQL).all(...args));
    assert.equal(catalog.db.prepare(PUBLICATION_ROWS_SQL).all(...args).length,1);
  }
  console.log('ok: the actual fact-first query fails the access-path check; weapon-first pages retain point measurement lookups with ten times the unrelated history');
} finally { catalog.close(); rmSync(root,{ recursive:true,force:true }); }
