// SPDX-License-Identifier: AGPL-3.0-or-later
// Consumers attach canonical inputs read-only and keep their writes in their own database.
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import {statSync,lstatSync} from 'node:fs';
import { canon } from '../worker/task_contract.js';
import { PreparedBoardInputs } from './prepared_board_inputs.mjs';

export const PREPARED_WEAPON_ROWS_SQL=`SELECT key,candidate,source_order FROM entity_prepared_board_versions
  WHERE kind='source' AND weapon=? AND candidate IS NOT NULL AND publication=1 AND body IS NOT NULL
  AND first_cut<=? AND (last_cut IS NULL OR last_cut>?) AND candidate>=? AND (candidate,source_order)>(?,?)
  ORDER BY candidate,source_order LIMIT 256`;

export class PreparedBoardReader {
  constructor(db,path,contract) {
    this.raw=db;
    db.prepare('ATTACH DATABASE ? AS prepared').run(pathToFileURL(resolve(path)).href+'?mode=ro');
    this.db={prepare:sql=>db.prepare(sql.replace(/\b(?:entity|record|state)_prepared_board_[a-z_]+\b/g,name=>'prepared.'+name))};
    const held=this.db.prepare('SELECT body FROM state_prepared_board_generation WHERE id=1').get();
    if(!held || contract && held.body!==canon(contract))throw Error('prepared reader generation changed');
    this.contract=JSON.parse(held.body);
    if(this.contract.record_base) {
      const base=this.contract.record_base;
      if(base.codec!=='interned-deflate-v1'||!base.path||lstatSync(base.path).isSymbolicLink()
        ||!Array.isArray(base.files)||!base.files.length||base.files[0].path!==resolve(base.path))throw Error('invalid shared record base');
      for(const file of base.files) {
        const s=statSync(file.path,{bigint:true}),stamp=[s.dev,s.ino,s.size,s.mtimeNs,s.ctimeNs].map(String).join(':');
        if(stamp!==file.stamp)throw Error('shared record base changed');
      }
      db.prepare('ATTACH DATABASE ? AS record_base').run(pathToFileURL(resolve(base.path)).href+'?mode=ro');
    }
  }
  cut(id) { return PreparedBoardInputs.prototype.cut.call(this,id); }
  value(kind,key,id) { return PreparedBoardInputs.prototype.value.call(this,kind,key,id); }
  rows(kind,id) { return PreparedBoardInputs.prototype.rows.call(this,kind,id); }
  *weaponRows(weapon,id) {
    this.retained(id);let after=['',0];
    for(;;) {
      const page=this.db.prepare(PREPARED_WEAPON_ROWS_SQL).all(weapon,id,id,after[0],...after);
      if(!page.length)return;
      for(const row of page)yield this.value('source',row.key,id);
      const last=page.at(-1);after=[last.candidate,last.source_order];
    }
  }
  sourceCount() { return this.cut(this.readCut).source_count; }
  readPage(id,cursor,limit=256) { return PreparedBoardInputs.prototype.readPage.call(this,id,cursor,limit); }
  retained(id) { return PreparedBoardInputs.prototype.retained.call(this,id); }
  next(after) {
    const row=this.db.prepare("SELECT id FROM record_prepared_board_cuts WHERE id>? AND state='sealed' ORDER BY id LIMIT 1").get(after);
    return row?this.cut(row.id):null;
  }
  recordBody(reference) {
    if(!reference?.startsWith('@'))return reference;
    const row=this.raw.prepare('SELECT body FROM prepared.entity_inventory_records WHERE id=?').get(reference.slice(1))
      ||(this.contract.record_base?this.raw.prepare('SELECT body FROM record_base.entity_inventory_records WHERE id=?').get(reference.slice(1)):null);
    if(!row)throw Error('canonical source record is unavailable');
    const body=typeof row.body==='string'?row.body:inflateRawSync(row.body).toString();
    if(createHash('sha256').update(body).digest('hex')!==reference.slice(1))throw Error('canonical source record changed');
    return body;
  }
  sourceRecords() {
    return {reference:key=>this.value('source',key,this.readCut)?.value?.raw_record_ref,
      count:()=>this.sourceCount(),
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
