// SPDX-License-Identifier: AGPL-3.0-or-later
// Sealed normalized inputs retain versions until every registered consumer acknowledges them.
import { createHash } from 'node:crypto';
import { deflateRawSync,inflateRawSync } from 'node:zlib';
import { canon } from '../worker/task_contract.js';

const schema = `
  CREATE TABLE IF NOT EXISTS state_prepared_board_generation(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS record_prepared_board_cuts(id INTEGER PRIMARY KEY,epoch TEXT NOT NULL UNIQUE,
    receipt TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN('preparing','sealed')),input_count INTEGER NOT NULL DEFAULT 0,checkpoint TEXT,sealed_digest TEXT);
  CREATE TABLE IF NOT EXISTS entity_prepared_board_versions(id INTEGER PRIMARY KEY,kind TEXT NOT NULL,key TEXT NOT NULL,
    first_cut INTEGER NOT NULL,last_cut INTEGER,body BLOB,sha TEXT NOT NULL,source_order INTEGER,
    identity TEXT,candidate TEXT,weapon TEXT,admit INTEGER,modes TEXT,ruler TEXT,mode TEXT,
    score REAL,at TEXT,retired INTEGER,gate_ruler TEXT,scene TEXT,publication INTEGER NOT NULL DEFAULT 0);
  CREATE UNIQUE INDEX IF NOT EXISTS prepared_board_current ON entity_prepared_board_versions(kind,key) WHERE last_cut IS NULL;
  CREATE INDEX IF NOT EXISTS prepared_board_key ON entity_prepared_board_versions(kind,key,first_cut DESC);
  CREATE INDEX IF NOT EXISTS prepared_board_sources ON entity_prepared_board_versions(weapon,candidate,source_order,first_cut) WHERE kind='source';
  CREATE INDEX IF NOT EXISTS prepared_board_candidates ON entity_prepared_board_versions(candidate,source_order,first_cut) WHERE kind='source';
  CREATE INDEX IF NOT EXISTS prepared_board_source_order ON entity_prepared_board_versions(source_order) WHERE kind='source';
  CREATE INDEX IF NOT EXISTS prepared_board_measurements ON entity_prepared_board_versions(identity,ruler,mode,first_cut DESC) WHERE kind='fact';
  CREATE INDEX IF NOT EXISTS prepared_board_expired ON entity_prepared_board_versions(last_cut,id) WHERE last_cut IS NOT NULL;
  CREATE TABLE IF NOT EXISTS record_prepared_board_changes(cut INTEGER NOT NULL,ordinal INTEGER NOT NULL,
    count INTEGER NOT NULL,versions BLOB NOT NULL,input_sha TEXT NOT NULL,PRIMARY KEY(cut,ordinal)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_prepared_board_consumers(name TEXT PRIMARY KEY,cut INTEGER NOT NULL DEFAULT 0,
    cursor INTEGER NOT NULL DEFAULT 0,acknowledged INTEGER NOT NULL DEFAULT 0,audit TEXT);
  CREATE TABLE IF NOT EXISTS state_prepared_board_retention(id INTEGER PRIMARY KEY CHECK(id=1),floor INTEGER NOT NULL);
  INSERT OR IGNORE INTO state_prepared_board_retention VALUES(1,0);
  CREATE TABLE IF NOT EXISTS entity_prepared_board_candidate_buckets(id TEXT PRIMARY KEY,bucket TEXT NOT NULL) WITHOUT ROWID;
`;
const hash = body => createHash('sha256').update(body).digest('hex');
export const preparedBatchHash = items => hash(canon(items.map(({kind,key,value})=>({kind,key,value}))));
const unpack = (body,sha) => {
  const text=body==null?'null':inflateRawSync(body).toString();
  if(sha && hash(text)!==sha)throw Error('prepared input body changed');
  return JSON.parse(text);
};
const mask = value => value == null ? null : BigInt.asIntN(64,BigInt('0x' + value));
const keys = new Set(['source','fact','reset','work']);
const versionColumns = `id,kind,key,first_cut,last_cut,body,sha,source_order,identity,candidate,weapon,
  CASE WHEN admit IS NULL THEN NULL ELSE printf('%016llx',admit) END admit,modes,ruler,mode,score,at,retired,gate_ruler,scene,publication`;

export class PreparedBoardInputs {
  constructor(db,contract,{ consumers = ['inventory','projection'] } = {}) {
    this.db=db;this.db.exec(schema);
    const held=this.db.prepare('SELECT body FROM state_prepared_board_generation WHERE id=1').get();
    if (held && held.body!==canon(contract)) throw Error('prepared source generation changed');
    if (!held) this.db.prepare('INSERT INTO state_prepared_board_generation VALUES(1,?)').run(canon(contract));
    for (const name of consumers) {
      if (!/^[a-z][a-z_]+$/.test(name)) throw Error('invalid prepared consumer');
      if (!this.db.prepare('SELECT 1 FROM state_prepared_board_consumers WHERE name=?').get(name)
        && this.db.prepare('SELECT floor FROM state_prepared_board_retention WHERE id=1').get().floor)
        throw Error('a new consumer requires an explicit retained baseline');
      this.db.prepare('INSERT OR IGNORE INTO state_prepared_board_consumers(name) VALUES(?)').run(name);
    }
    this.contract=contract;
  }
  transaction(fn) {
    this.db.exec('BEGIN IMMEDIATE');
    try { const value=fn();this.db.exec('COMMIT');return value; }
    catch(error) { this.db.exec('ROLLBACK');throw error; }
  }
  begin(receipt) {
    if (!receipt?.epoch || !receipt.snapshot || !receipt.started_at || receipt.engine!==this.contract.engine
      || canon(receipt.normalizer)!==canon(this.contract.normalizer)) throw Error('invalid prepared source receipt');
    const prior=this.db.prepare('SELECT * FROM record_prepared_board_cuts WHERE epoch=?').get(String(receipt.epoch));
    if (prior) { if (prior.receipt!==canon(receipt)) throw Error('prepared epoch changed');return prior.id; }
    if (this.db.prepare("SELECT 1 FROM record_prepared_board_cuts WHERE state='preparing'").get()) throw Error('a prepared cut is unfinished');
    return Number(this.db.prepare("INSERT INTO record_prepared_board_cuts(epoch,receipt,state) VALUES(?,?,'preparing')")
      .run(String(receipt.epoch),canon(receipt)).lastInsertRowid);
  }
  cut(id) {
    const row=this.db.prepare('SELECT * FROM record_prepared_board_cuts WHERE id=?').get(id);
    if (!row) throw Error('unknown prepared cut');return { ...row,receipt:JSON.parse(row.receipt) };
  }
  value(kind,key,cut) {
    this.retained(cut);
    const row=this.db.prepare(`SELECT ${versionColumns} FROM entity_prepared_board_versions WHERE kind=? AND key=?
      AND first_cut<=? AND (last_cut IS NULL OR last_cut>?) ORDER BY first_cut DESC,id DESC LIMIT 1`).get(kind,key,cut,cut);
    return row ? { ...row,value:unpack(row.body,row.sha) } : null;
  }
  *rows(kind,cut) {
    this.retained(cut);
    let after='';
    for (;;) {
      const page=this.db.prepare(`SELECT ${versionColumns} FROM entity_prepared_board_versions WHERE kind=? AND body IS NOT NULL
        AND key>? AND first_cut<=? AND (last_cut IS NULL OR last_cut>?) ORDER BY key LIMIT 256`).all(kind,after,cut,cut);
      if (!page.length)return;
      for(const row of page)yield { ...row,value:unpack(row.body,row.sha) };
      after=page.at(-1).key;
    }
  }
  append(id,items,{beforeWrite,checkpoint}={}) {
    if (!Array.isArray(items) || items.length>256) throw Error('prepared input batch exceeds bound');
    if(beforeWrite?.constructor?.name==='AsyncFunction')throw Error('prepared writes cannot await another stage');
    return this.transaction(()=>{
      const cut=this.cut(id);if(cut.state!=='preparing') throw Error('prepared cut is sealed');
      if(beforeWrite?.()?.then)throw Error('prepared writes cannot await another stage');
      if(!items.length) {
        if(checkpoint)this.db.prepare('UPDATE record_prepared_board_cuts SET checkpoint=? WHERE id=?').run(canon(checkpoint),id);
        return cut.input_count;
      }
      const start=items[0].ordinal,inputSha=preparedBatchHash(items);
      const prior=this.db.prepare('SELECT input_sha,count FROM record_prepared_board_changes WHERE cut=? AND ordinal=?').get(id,start);
      if(prior) {
        if(prior.input_sha!==inputSha || prior.count!==items.length)throw Error('prepared input replay changed');
        return cut.input_count;
      }
      let count=cut.input_count;
      const versions=[];
      for(const item of items) {
        const {kind,key,ordinal,value}=item;
        if(!keys.has(kind) || typeof key!=='string' || !key || value===undefined || !Number.isSafeInteger(ordinal) || ordinal<0)
          throw Error('invalid prepared input identity');
        if(ordinal!==count)throw Error('prepared input cursor gap');
        if(kind==='source' && value?.candidate && value.bucket) {
          const bucket=canon(value.bucket),held=this.db.prepare('SELECT bucket FROM entity_prepared_board_candidate_buckets WHERE id=?').get(value.candidate);
          if(held && held.bucket!==bucket)throw Error('canonical prepared bucket changed');
          this.db.prepare('INSERT OR IGNORE INTO entity_prepared_board_candidate_buckets VALUES(?,?)').run(value.candidate,bucket);
        }
        if(kind==='reset')this.db.prepare("UPDATE entity_prepared_board_versions SET last_cut=? WHERE kind='fact' AND identity=? AND last_cut IS NULL").run(id,value.id);
        const old=this.db.prepare(`SELECT ${versionColumns} FROM entity_prepared_board_versions WHERE kind=? AND key=? AND last_cut IS NULL`).get(kind,key);
        let version=old?.id;
        const text=canon(value),sha=hash(text);
        if(old?.sha!==sha) {
          const at=value?.retired_at ?? value?.finished_at ?? '';
          const stale=kind==='fact' && value && old?.body && (old.at>at || old.at===at && old.retired && !value.retired_at);
          if(!stale && old)this.db.prepare('UPDATE entity_prepared_board_versions SET last_cut=? WHERE id=?').run(id,old.id);
          const sourceOrder=kind==='source' ? old?.body ? old.source_order
            : this.db.prepare("SELECT coalesce(max(source_order),0)+1 n FROM entity_prepared_board_versions WHERE kind='source'").get().n : null;
          const result=this.db.prepare(`INSERT INTO entity_prepared_board_versions(kind,key,first_cut,last_cut,body,sha,source_order,
            identity,candidate,weapon,admit,modes,ruler,mode,score,at,retired,gate_ruler,scene,publication) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
            .run(kind,key,id,stale?id:null,value==null?null:deflateRawSync(text,{level:1}),sha,sourceOrder,
              value?.identity ?? value?.id ?? value?.build_id ?? null,value?.candidate ?? null,value?.bucket?.[0] ?? null,
              mask(value?.admit),value?.publication_modes?canon(value.publication_modes):null,value?.ruler ?? null,value?.mode ?? null,
              value?.score ?? null,at,Number(value?.retired_at!=null),value?.gate_ruler ?? null,value?.scene ?? null,Number(!!value?.publication));
          version=Number(result.lastInsertRowid);
          if(kind==='work')this.db.prepare('UPDATE entity_prepared_board_versions SET last_cut=? WHERE id=?').run(id,version);
        }
        versions.push(version);
        count++;
      }
      this.db.prepare('INSERT INTO record_prepared_board_changes VALUES(?,?,?,?,?)')
        .run(id,start,items.length,deflateRawSync(canon(versions),{level:1}),inputSha);
      this.db.prepare('UPDATE record_prepared_board_cuts SET input_count=?,checkpoint=? WHERE id=?').run(count,checkpoint?canon(checkpoint):null,id);
      return count;
    });
  }
  seal(id,{ inputCount,digest }={}) {
    const cut=this.cut(id);
    if(cut.input_count!==inputCount || !/^[a-f0-9]{64}$/.test(digest || ''))throw Error('incomplete prepared cut');
    const actual=createHash('sha256');
    for(let offset=0;;) {
      const page=this.db.prepare('SELECT ordinal,count,input_sha FROM record_prepared_board_changes WHERE cut=? AND ordinal>=? ORDER BY ordinal LIMIT 4096').all(id,offset);
      if(!page.length)break;
      for(const row of page)actual.update(row.input_sha+'\n');
      offset=page.at(-1).ordinal+page.at(-1).count;
    }
    if(actual.digest('hex')!==digest)throw Error('prepared input digest mismatch');
    this.db.prepare("UPDATE record_prepared_board_cuts SET state='sealed',sealed_digest=? WHERE id=?").run(digest,id);
  }
  next(name) {
    const consumer=this.consumer(name);
    return this.db.prepare("SELECT * FROM record_prepared_board_cuts WHERE id>? AND state='sealed' ORDER BY id LIMIT 1").get(consumer.acknowledged) || null;
  }
  consumer(name) {
    const row=this.db.prepare('SELECT * FROM state_prepared_board_consumers WHERE name=?').get(name);
    if(!row)throw Error('unknown prepared consumer');return row;
  }
  page(name,id,limit=256) {
    if(!Number.isSafeInteger(limit)||limit<1||limit>256)throw Error('invalid prepared page bound');
    const consumer=this.consumer(name),cut=this.cut(id);
    if(cut.state!=='sealed'||this.next(name)?.id!==id)throw Error('prepared consumer cut is fenced');
    const cursor=consumer.cut===id?consumer.cursor:0;
    return this.readPage(id,cursor,limit);
  }
  readPage(id,cursor,limit=256) {
    this.retained(id);
    const cut=this.cut(id);
    if(cut.state!=='sealed'||!Number.isSafeInteger(cursor)||cursor<0||cursor>cut.input_count
      ||!Number.isSafeInteger(limit)||limit<1||limit>256)throw Error('invalid prepared read cursor');
    if(cursor===cut.input_count)return [];
    const first=this.db.prepare('SELECT ordinal FROM record_prepared_board_changes WHERE cut=? AND ordinal<=? ORDER BY ordinal DESC LIMIT 1').get(id,cursor);
    if(!first)throw Error('prepared input is missing');
    const blocks=this.db.prepare(`SELECT * FROM record_prepared_board_changes WHERE cut=? AND ordinal>=?
      AND ordinal<=? ORDER BY ordinal LIMIT 256`).all(id,first.ordinal,cursor+limit-1);
    const refs=[],allRefs=[],decoded=[];
    for(const block of blocks) {
      const values=JSON.parse(inflateRawSync(block.versions));
      if(values.length!==block.count)throw Error('prepared input block changed');
      decoded.push({block,values});allRefs.push(...values);
      for(let n=0;n<values.length;n++)if(block.ordinal+n>=cursor && refs.length<limit) {
        if(block.ordinal+n!==cursor+refs.length)throw Error('prepared input is missing');
        refs.push(values[n]);
      }
    }
    if(!refs.length)throw Error('prepared input is missing');
    const found=this.db.prepare(`SELECT ${versionColumns} FROM entity_prepared_board_versions WHERE id IN(SELECT value FROM json_each(?))`).all(JSON.stringify(allRefs));
    const values=new Map(found.map(row=>[row.id,row]));
    for(const {block,values:ids}of decoded) {
      const items=ids.map(id=>{const row=values.get(id);if(!row)throw Error('prepared input is missing');return {kind:row.kind,key:row.key,value:unpack(row.body,row.sha)};});
      if(preparedBatchHash(items)!==block.input_sha)throw Error('prepared input block changed');
    }
    return refs.map((ref,n)=>{
      const row=values.get(ref);if(!row)throw Error('prepared input is missing');
      return {...row,ordinal:cursor+n,version_id:ref,value:unpack(row.body,row.sha)};
    });
  }
  progress(name,id,cursor) {
    const consumer=this.consumer(name),cut=this.cut(id);
    const from=consumer.cut===id?consumer.cursor:0;
    if(this.next(name)?.id!==id || !Number.isSafeInteger(cursor) || cursor<0 || cursor>cut.input_count
      || cursor<from || cursor>from+256)throw Error('invalid prepared consumer progress');
    this.db.prepare('UPDATE state_prepared_board_consumers SET cut=?,cursor=?,audit=NULL WHERE name=?').run(id,cursor,name);
  }
  acknowledge(name,id,audit) {
    const consumer=this.consumer(name),cut=this.cut(id);
    if(consumer.cut!==id || consumer.cursor!==cut.input_count || !audit?.complete || audit.cut!==id
      || audit.snapshot!==cut.receipt.snapshot || audit.engine!==cut.receipt.engine)throw Error('prepared consumer audit is incomplete');
    this.db.prepare('UPDATE state_prepared_board_consumers SET acknowledged=?,audit=? WHERE name=?').run(id,canon(audit),name);
  }
  observeAcknowledgement(name,audit) {
    const consumer=this.consumer(name),cut=this.cut(audit.cut);
    if(cut.state!=='sealed'||!audit.complete||audit.snapshot!==cut.receipt.snapshot||audit.engine!==cut.receipt.engine
      ||audit.input_count!==cut.input_count||audit.input_digest!==cut.sealed_digest||audit.cut<consumer.acknowledged)throw Error('invalid prepared acknowledgement');
    this.db.prepare('UPDATE state_prepared_board_consumers SET cut=?,cursor=?,acknowledged=?,audit=? WHERE name=?')
      .run(audit.cut,cut.input_count,audit.cut,canon(audit),name);
  }
  retained(cut) {
    if(cut<this.db.prepare('SELECT floor FROM state_prepared_board_retention WHERE id=1').get().floor)
      throw Error('prepared snapshot was retired');
  }
  collect(limit=4096) {
    if(!Number.isSafeInteger(limit)||limit<1||limit>10000)throw Error('invalid prepared retention bound');
    const floor=this.db.prepare('SELECT min(acknowledged) floor FROM state_prepared_board_consumers').get().floor;
    if(!floor)return {floor:0,versions:0};
    return this.transaction(()=>{
      this.db.prepare(`DELETE FROM record_prepared_board_changes WHERE (cut,ordinal) IN(
        SELECT cut,ordinal FROM record_prepared_board_changes WHERE cut<=? ORDER BY cut,ordinal LIMIT ?)`).run(floor,limit);
      const result=this.db.prepare(`DELETE FROM entity_prepared_board_versions WHERE id IN(
        SELECT id FROM entity_prepared_board_versions WHERE last_cut IS NOT NULL AND last_cut<=?
        ORDER BY last_cut,id LIMIT ?)`).run(floor,limit);
      this.db.prepare('UPDATE state_prepared_board_retention SET floor=? WHERE id=1').run(floor);
      return {floor,versions:result.changes};
    });
  }
}
