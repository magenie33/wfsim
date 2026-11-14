// SPDX-License-Identifier: AGPL-3.0-or-later
// Board rendering owns its metadata; workload inventory never writes this checkpoint.
import { canon } from '../worker/task_contract.js';

export class BoardProjectionState {
  constructor(records,contract) {
    this.records=records;this.db=records.db;
    this.db.exec('CREATE TABLE IF NOT EXISTS state_board_projection_generation(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL)');
    this.db.exec('CREATE TABLE IF NOT EXISTS state_board_projection_dirty(weapon TEXT PRIMARY KEY) WITHOUT ROWID');
    const prior=this.db.prepare('SELECT body FROM state_board_projection_generation WHERE id=1').get();
    if(prior) {
      this.meta=JSON.parse(prior.body);
      if(canon(this.meta.contract)!==canon(contract))throw Error('board projection contract changed');
    } else { this.meta={v:1,contract,complete:false};this.setMeta(this.meta); }
  }
  setMeta(value) {
    this.db.prepare('INSERT OR REPLACE INTO state_board_projection_generation VALUES(1,?)').run(canon(value));
    this.meta=value;
  }
  bodyText(value) { return this.records.bodyText(value); }
  recordBody(reference) { return this.records.recordBody(reference); }
  transaction(fn) {
    const prior=this.meta;
    this.db.exec('BEGIN IMMEDIATE');
    try {const value=fn();this.db.exec('COMMIT');return value;}
    catch(error){this.db.exec('ROLLBACK');this.meta=prior;throw error;}
  }
  index(nativeContract) {
    return {catalog:this,contract:nativeContract,prepared:true,
      q:sql=>this.db.prepare(sql.replace(/ INDEXED BY rel_publication_(?:sources_rows|facts_rows)/g,'')
        .replaceAll('state_catalog_publication_dirty','state_board_projection_dirty')),
      transaction:fn=>this.transaction(fn),
      publicationDirty:weapon=>this.db.prepare('INSERT OR IGNORE INTO state_board_projection_dirty VALUES(?)').run(weapon),
    };
  }
}
