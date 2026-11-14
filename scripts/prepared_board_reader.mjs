// SPDX-License-Identifier: AGPL-3.0-or-later
// Consumers attach canonical inputs read-only and keep their writes in their own database.
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { canon } from '../worker/task_contract.js';
import { PreparedBoardInputs } from './prepared_board_inputs.mjs';

export class PreparedBoardReader {
  constructor(db,path,contract) {
    this.raw=db;
    db.prepare('ATTACH DATABASE ? AS prepared').run(pathToFileURL(resolve(path)).href+'?mode=ro');
    this.db={prepare:sql=>db.prepare(sql.replace(/\b(?:entity|record|state)_prepared_board_[a-z_]+\b/g,name=>'prepared.'+name))};
    const held=this.db.prepare('SELECT body FROM state_prepared_board_generation WHERE id=1').get();
    if(!held || contract && held.body!==canon(contract))throw Error('prepared reader generation changed');
    this.contract=JSON.parse(held.body);
  }
  cut(id) { return PreparedBoardInputs.prototype.cut.call(this,id); }
  value(kind,key,id) { return PreparedBoardInputs.prototype.value.call(this,kind,key,id); }
  rows(kind,id) { return PreparedBoardInputs.prototype.rows.call(this,kind,id); }
  readPage(id,cursor,limit=256) { return PreparedBoardInputs.prototype.readPage.call(this,id,cursor,limit); }
  retained(id) { return PreparedBoardInputs.prototype.retained.call(this,id); }
  next(after) {
    const row=this.db.prepare("SELECT id FROM record_prepared_board_cuts WHERE id>? AND state='sealed' ORDER BY id LIMIT 1").get(after);
    return row?this.cut(row.id):null;
  }
  recordBody(reference) {
    if(!reference?.startsWith('@'))return reference;
    const row=this.raw.prepare('SELECT body FROM prepared.entity_inventory_records WHERE id=?').get(reference.slice(1));
    if(!row)throw Error('canonical source record is unavailable');
    const body=typeof row.body==='string'?row.body:inflateRawSync(row.body).toString();
    if(createHash('sha256').update(body).digest('hex')!==reference.slice(1))throw Error('canonical source record changed');
    return body;
  }
  sourceRecords() {
    return {reference:key=>this.value('source',key,this.readCut)?.value?.raw_record_ref,
      count:()=>this.raw.prepare('SELECT count(*) n FROM entity_catalog_sources').get().n,
      rows:(after,limit)=>this.raw.prepare('SELECT id,body FROM entity_catalog_sources WHERE id>? ORDER BY id LIMIT ?').all(after||'',limit)
        .map(row=>({id:row.id,record:JSON.parse(this.recordBody(JSON.parse(inflateRawSync(row.body)).raw_record_ref))}))};
  }
  bindViews(id) {
    this.retained(id);if(this.cut(id).state!=='sealed')throw Error('prepared read cut is not sealed');
    this.readCut=id;if(this.views)return;
    this.raw.function('prepared_board_cut',()=>this.readCut);
    const visible="first_cut<=prepared_board_cut() AND (last_cut IS NULL OR last_cut>prepared_board_cut())";
    this.raw.exec(`
      CREATE TEMP VIEW entity_catalog_sources AS SELECT source_order seq,key id,candidate,body
        FROM prepared.entity_prepared_board_versions WHERE kind='source' AND body IS NOT NULL AND ${visible};
      CREATE TEMP VIEW state_catalog_publication_sources AS SELECT key id,source_order seq,candidate,weapon,admit,modes
        FROM prepared.entity_prepared_board_versions WHERE kind='source' AND body IS NOT NULL AND publication=1 AND ${visible};
      CREATE TEMP VIEW state_catalog_publication_measurements AS SELECT identity,ruler,mode,at,retired,body
        FROM prepared.entity_prepared_board_versions WHERE kind='fact' AND body IS NOT NULL AND ${visible};
    `);
    this.views=true;
  }
}
