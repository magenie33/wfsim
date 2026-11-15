// SPDX-License-Identifier: AGPL-3.0-or-later
// Local cursor and completed audit form a durable acknowledgement outbox for the source owner.
import { canon } from '../worker/task_contract.js';

export class PreparedBoardConsumer {
  constructor(db,reader,name,contract) {
    this.db=db;this.reader=reader;this.name=name;
    db.exec(`CREATE TABLE IF NOT EXISTS state_prepared_input_consumption(name TEXT PRIMARY KEY,source TEXT NOT NULL,
      contract TEXT NOT NULL,cut INTEGER NOT NULL DEFAULT 0,cursor INTEGER NOT NULL DEFAULT 0,
      acknowledged INTEGER NOT NULL DEFAULT 0,audit TEXT)`);
    const held=this.state();
    if(held && (held.source!==canon(reader.contract)||held.contract!==canon(contract)))throw Error('prepared consumer contract changed');
    if(!held)db.prepare('INSERT INTO state_prepared_input_consumption(name,source,contract) VALUES(?,?,?)').run(name,canon(reader.contract),canon(contract));
  }
  state() { return this.db.prepare('SELECT * FROM state_prepared_input_consumption WHERE name=?').get(this.name); }
  begin() {
    const state=this.state();
    if(state.cut>state.acknowledged)return this.reader.cut(state.cut);
    const next=this.reader.next(state.acknowledged);if(!next)return null;
    this.db.prepare('UPDATE state_prepared_input_consumption SET cut=?,cursor=0,audit=NULL WHERE name=?').run(next.id,this.name);
    return next;
  }
  page(limit=256) {
    const state=this.state();if(state.cut<=state.acknowledged)return [];
    return this.reader.readPage(state.cut,state.cursor,limit);
  }
  progress(cursor) {
    const state=this.state(),cut=this.reader.cut(state.cut);
    if(!Number.isSafeInteger(cursor)||cursor<state.cursor||cursor>state.cursor+256||cursor>cut.input_count)
      throw Error('invalid local consumer progress');
    this.db.prepare('UPDATE state_prepared_input_consumption SET cursor=? WHERE name=?').run(cursor,this.name);
  }
  complete(audit) {
    const state=this.state(),cut=this.reader.cut(state.cut);
    if(state.cursor!==cut.input_count||!audit?.complete||audit.cut!==cut.id
      ||audit.snapshot!==cut.receipt.snapshot||audit.engine!==cut.receipt.engine)throw Error('local consumer audit is incomplete');
    const output={...audit,input_count:cut.input_count,input_digest:cut.sealed_digest};
    this.db.prepare('UPDATE state_prepared_input_consumption SET acknowledged=?,audit=? WHERE name=?').run(cut.id,canon(output),this.name);
    return output;
  }
  acknowledgement() { const state=this.state();return state.audit?JSON.parse(state.audit):null; }
}
