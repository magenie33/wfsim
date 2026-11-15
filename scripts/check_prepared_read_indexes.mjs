// SPDX-License-Identifier: AGPL-3.0-or-later
// Repeated mode validation reads only exceptional rows, even for a large single weapon.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {readFileSync} from 'node:fs';
import {PreparedBoardInputs} from './prepared_board_inputs.mjs';
import {PreparedBoardReader} from './prepared_board_reader.mjs';
import {ensurePreparedReadIndexes} from './prepared_read_indexes.mjs';

const source=readFileSync(new URL('inventory_publication.mjs',import.meta.url),'utf8'),sql=source.match(/`(SELECT 1 FROM state_catalog_publication_sources WHERE weapon=\? AND \(admit & \?\)<>0 AND json_array_length\(modes\)=0 LIMIT 1)`/)[1];
for(const history of [1000,10000]) {
 const db=new DatabaseSync(':memory:');new PreparedBoardInputs(db,{engine:'fixture'});ensurePreparedReadIndexes(db);ensurePreparedReadIndexes(db);
 db.exec("CREATE TEMP VIEW state_catalog_publication_sources AS SELECT weapon,admit,modes FROM entity_prepared_board_versions WHERE kind='source' AND publication=1 AND body IS NOT NULL AND first_cut<=1 AND (last_cut IS NULL OR last_cut>1)");
 const put=db.prepare("INSERT INTO entity_prepared_board_versions(kind,key,first_cut,body,sha,weapon,admit,modes,publication) VALUES('source',?,1,x'00','fixture','target',1,?,1)");
 db.exec('BEGIN');for(let n=0;n<history;n++)put.run(String(n),'[{"id":"base","mode":""}]');db.exec('COMMIT');
 const verify=()=>{
  const plan=db.prepare('EXPLAIN QUERY PLAN '+sql).all('target',1n);
  assert(plan.some(row=>/SEARCH .*prepared_board_empty_modes \(weapon=/.test(row.detail)),JSON.stringify(plan));return plan;
 };
 assert.equal(db.prepare(sql).get('target',1n),undefined);const plan=verify();
 db.exec("UPDATE entity_prepared_board_versions SET modes='[]' WHERE key='0'");assert(db.prepare(sql).get('target',1n));
 db.exec("UPDATE entity_prepared_board_versions SET admit=0 WHERE key='0'");assert.equal(db.prepare(sql).get('target',1n),undefined);
 db.exec("UPDATE entity_prepared_board_versions SET admit=1,last_cut=1 WHERE key='0'");assert.equal(db.prepare(sql).get('target',1n),undefined);
 db.exec('DROP INDEX prepared_board_empty_modes');assert.throws(verify);ensurePreparedReadIndexes(db);verify();
 assert.equal(db.prepare('SELECT count(*) n FROM entity_prepared_board_versions').get().n,history);
 db.exec('DROP INDEX prepared_board_empty_modes;CREATE INDEX prepared_board_empty_modes ON entity_prepared_board_versions(key)');
 assert.throws(()=>ensurePreparedReadIndexes(db),/differs/);
 console.log(JSON.stringify({same_weapon_sources:history,plan}));db.close();
}
console.log('ok: actual empty-mode validation uses only its exceptional-row index at1k/10k same-weapon history; missing/wrong index fails, invalid modes reject, zero-admit/expired rows stay excluded and all rows remain');
