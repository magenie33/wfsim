// SPDX-License-Identifier: AGPL-3.0-or-later
// A disposable, durable question-id union; business tables remain authoritative.
import { DatabaseSync } from 'node:sqlite';
import { createReadStream, statSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createHash, randomUUID } from 'node:crypto';
import { canon } from '../worker/task_contract.js';

const ENGINE = /^[a-f0-9]{12}$/, QID = /^[a-f0-9]{32}$/;
const CHUNK = 256, OWNER_LIMIT = 64;
const active = new Set(['open', 'disputed', 'nondeterministic']);
const sql = `
  CREATE TABLE IF NOT EXISTS meta(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS records(id INTEGER PRIMARY KEY,build_id TEXT NOT NULL UNIQUE,stored TEXT,fallback TEXT);
  CREATE TABLE IF NOT EXISTS owners(id INTEGER PRIMARY KEY,owner TEXT NOT NULL UNIQUE,candidate INTEGER,scope TEXT,weapon TEXT,riven INTEGER,source INTEGER);
  CREATE INDEX IF NOT EXISTS owners_candidate ON owners(candidate);
  CREATE INDEX IF NOT EXISTS owners_weapon ON owners(weapon,riven);
  CREATE TABLE IF NOT EXISTS symbols(id INTEGER PRIMARY KEY,category TEXT NOT NULL,name TEXT NOT NULL,UNIQUE(category,name));
  CREATE TABLE IF NOT EXISTS members(owner INTEGER NOT NULL,candidate INTEGER NOT NULL,ruler INTEGER NOT NULL,mode INTEGER NOT NULL,
    qid BLOB NOT NULL,queued INTEGER NOT NULL CHECK(queued IN(0,1)),PRIMARY KEY(owner,candidate,ruler,mode)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS questions(id BLOB PRIMARY KEY,board_refs INTEGER NOT NULL DEFAULT 0 CHECK(board_refs>=0),
    queued_refs INTEGER NOT NULL DEFAULT 0 CHECK(queued_refs>=0 AND queued_refs<=board_refs),state TEXT,fact_current INTEGER NOT NULL DEFAULT 0,
    other_kind INTEGER,verb INTEGER,served_answered INTEGER NOT NULL DEFAULT 0,kind INTEGER,effective_verb INTEGER,queued INTEGER NOT NULL DEFAULT 0,
    imported INTEGER NOT NULL DEFAULT 0) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS totals(kind INTEGER NOT NULL,verb INTEGER NOT NULL,n INTEGER NOT NULL,queued INTEGER NOT NULL,
    PRIMARY KEY(kind,verb)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS jobs(key TEXT PRIMARY KEY,scope TEXT NOT NULL,requested TEXT NOT NULL,epoch TEXT NOT NULL,cursor TEXT) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS job_inputs(job TEXT NOT NULL,scope TEXT NOT NULL,key TEXT NOT NULL,revision TEXT NOT NULL,
    PRIMARY KEY(job,scope,key)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS receipts(scope TEXT NOT NULL,key TEXT NOT NULL,revision TEXT NOT NULL,PRIMARY KEY(scope,key)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS epoch_tokens(scope TEXT NOT NULL,key TEXT NOT NULL,revision TEXT NOT NULL,processed INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(scope,key)) WITHOUT ROWID;
`;

function text(value, name) {
  if (typeof value !== 'string' || !value || value.length > 4096) throw new Error(`invalid inventory ${name}`);
  return value;
}
function change(row) {
  return { scope: text(row.scope, 'scope'), key: text(row.key, 'key'), revision: text(row.revision, 'revision') };
}
function id(value) {
  if (!QID.test(value)) throw new Error('invalid inventory question id');
  return Buffer.from(value, 'hex');
}
function integer(value, name) {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`invalid inventory ${name}`);
  return value;
}

export async function* readInventoryRows(path) {
  const input = createReadStream(path), reader = createInterface({ input, crlfDelay: Infinity });
  input.on('error', error => reader.emit('error', error));
  try { for await (const line of reader) if (line.trim()) yield JSON.parse(line); }
  finally { reader.close(); input.destroy(); }
}

async function importBatches(input, apply) {
  let batch = [];
  for await (const row of typeof input === 'string' ? readInventoryRows(input) : input) {
    batch.push(row);
    if (batch.length === CHUNK) { apply(batch); batch = []; }
  }
  if (batch.length) apply(batch);
}

// This is the full classifier's precedence, including independent verification.
export function classifyQuestion(row) {
  if (row.state === 'fact' && row.fact_current) return null;
  const verification = row.state === 'spot' && row.fact_current
    || ['disputed', 'nondeterministic'].includes(row.state) && row.served_answered;
  if (verification) return { kind: 'verification', verb: row.verb || 'simulate', queued: 1 };
  const other = row.other_kind && active.has(row.state);
  if (row.board_refs && row.state !== 'refused')
    return { kind: 'board', verb: 'simulate', queued: Number(row.queued_refs > 0 || !!other) };
  return other ? { kind: row.other_kind, verb: row.verb, queued: 1 } : null;
}

export class InventoryCatalog {
  constructor(path, { engine, contract, generation, create = false } = {}) {
    this.path = path;
    if (!create) statSync(path);
    this.db = new DatabaseSync(path);
    this.statements = new Map();
    this.symbolIds = new Map(); this.symbolNames = new Map();
    try {
      this.db.exec(`PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA cache_size=-16384;
        PRAGMA temp_store=FILE;PRAGMA mmap_size=0;PRAGMA wal_autocheckpoint=256;PRAGMA journal_size_limit=16777216;
        PRAGMA max_page_count=2097152;PRAGMA busy_timeout=5000;${sql}`);
      const prior = this.statement('SELECT body FROM meta WHERE id=1').get();
      if (prior) {
        this.meta = JSON.parse(prior.body);
        if (this.meta.v !== 1 || engine && engine !== this.meta.engine || contract && canon(contract) !== canon(this.meta.contract)
            || generation && generation !== this.meta.generation) throw new Error('inventory catalog generation mismatch');
      } else {
        if (!create || !ENGINE.test(engine) || !contract || typeof contract !== 'object') throw new Error('invalid inventory generation');
        this.meta = { v: 1, engine, contract, generation: generation || randomUUID(), complete: false };
        this.saveMeta();
      }
    } catch (error) { this.db.close(); throw error; }
  }
  statement(query) {
    if (!this.statements.has(query)) this.statements.set(query, this.db.prepare(query));
    return this.statements.get(query);
  }
  saveMeta() { this.statement('INSERT OR REPLACE INTO meta VALUES(1,?)').run(JSON.stringify(this.meta)); }
  setMeta(next, mutate = () => {}) {
    this.transaction(() => { mutate(); this.statement('INSERT OR REPLACE INTO meta VALUES(1,?)').run(JSON.stringify(next)); });
    this.meta = next;
  }
  transaction(fn) {
    this.db.exec('BEGIN IMMEDIATE');
    try { const value = fn(); this.db.exec('COMMIT'); return value; }
    catch (error) { this.db.exec('ROLLBACK'); this.symbolIds.clear(); this.symbolNames.clear(); throw error; }
  }
  symbol(category, name) {
    text(name, category);
    const key = JSON.stringify([category, name]);
    if (this.symbolIds.has(key)) return this.symbolIds.get(key);
    this.statement('INSERT OR IGNORE INTO symbols(category,name) VALUES(?,?)').run(category, name);
    const value = this.statement('SELECT id FROM symbols WHERE category=? AND name=?').get(category, name).id;
    if (this.symbolIds.size >= CHUNK) this.symbolIds.clear();
    if (this.symbolNames.size >= CHUNK) this.symbolNames.clear();
    this.symbolIds.set(key, value); this.symbolNames.set(value, name);
    return value;
  }
  name(symbol) {
    if (symbol == null) return null;
    if (this.symbolNames.has(symbol)) return this.symbolNames.get(symbol);
    const name = this.statement('SELECT name FROM symbols WHERE id=?').get(symbol)?.name;
    if (this.symbolNames.size >= CHUNK) this.symbolNames.clear();
    this.symbolNames.set(symbol, name);
    return name;
  }
  record(build) {
    const row = this.statement('SELECT id,coalesce(stored,fallback) record FROM records WHERE build_id=?').get(build);
    if (!row?.record) throw new Error(`inventory build unavailable: ${build}`);
    return row;
  }
  putRecords(rows, { stored = true } = {}) {
    if (!Array.isArray(rows) || rows.length > CHUNK) throw new Error('inventory record batch exceeds bound');
    return this.transaction(() => {
      for (const row of rows) {
        text(row.id, 'build');
        if (!row.deleted && (!row.record || typeof row.record !== 'object')) throw new Error('invalid inventory record');
        const column = stored ? 'stored' : 'fallback';
        const update = stored ? 'stored=excluded.stored,fallback=NULL'
          : 'fallback=CASE WHEN records.stored IS NULL THEN excluded.fallback ELSE NULL END';
        this.statement(`INSERT INTO records(build_id,${column}) VALUES(?,?) ON CONFLICT(build_id) DO UPDATE SET ${update}`)
          .run(row.id, row.deleted ? null : canon(row.record));
      }
    });
  }
  importRecords(input, options) { return importBatches(input, batch => this.putRecords(batch, options)); }
  importLifecycle(input, options) { return importBatches(input, batch => this.applyLifecycle(batch, options)); }
  beginEpoch(receipt) {
    if (!receipt || receipt.status !== 'sealed' || receipt.generation !== this.meta.generation || receipt.engine !== this.meta.engine
        || canon(receipt.contract) !== canon(this.meta.contract) || !receipt.snapshot || !Number.isFinite(Date.parse(receipt.started_at))
        || !/^[a-f0-9]{64}$/.test(receipt.manifest_sha) || !Number.isSafeInteger(receipt.change_count) || receipt.change_count < 0)
      throw new Error('invalid inventory epoch receipt');
    if (this.meta.epoch?.epoch === receipt.epoch) {
      if (['manifest_sha', 'snapshot', 'started_at', 'change_count'].some(key => this.meta.epoch[key] !== receipt[key]))
        throw new Error('inventory epoch manifest changed');
      return;
    }
    if (this.meta.epoch && this.meta.epoch.phase !== 'complete' || this.jobs(1).length || this.pendingAcks(1).length)
      throw new Error('inventory epoch already in progress');
    text(receipt.epoch, 'epoch');
    this.setMeta({ ...this.meta, epoch: { ...receipt, phase: 'preparing', tokens_processed: 0 } }, () => this.statement('DELETE FROM epoch_tokens').run());
  }
  freezeEpoch() {
    if (!this.meta.epoch || !['preparing', 'processing'].includes(this.meta.epoch.phase)) throw new Error('inventory epoch is not prepared');
    this.setMeta({ ...this.meta, epoch: { ...this.meta.epoch, phase: 'processing' } });
  }
  finishEpoch() {
    const epoch = this.meta.epoch;
    if (epoch?.phase === 'complete') return this.plan();
    if (!epoch || epoch.phase !== 'processing' || !this.meta.complete || this.jobs(1).length || this.pendingAcks(1).length
        || epoch.tokens_processed !== epoch.change_count) throw new Error('inventory epoch is not complete');
    this.setMeta({ ...this.meta, snapshot: epoch.snapshot, started_at: epoch.started_at, epoch: { ...epoch, phase: 'complete' } });
    return this.plan();
  }
  owner(owner) {
    const row = this.statement(`SELECT o.owner,c.build_id,o.weapon,o.riven,o.source,o.scope,s.stored stored_record,
      coalesce(c.stored,c.fallback) canonical_record FROM owners o LEFT JOIN records c ON c.id=o.candidate
      LEFT JOIN records s ON s.build_id=o.owner WHERE o.owner=?`).get(owner);
    return row ? { ...row, scope: JSON.parse(row.scope), riven: row.riven == null ? null : !!row.riven,
      stored_record: row.stored_record ? JSON.parse(row.stored_record) : null,
      canonical_record: row.canonical_record ? JSON.parse(row.canonical_record) : null } : null;
  }
  ownerRows(after = null, limit = CHUNK) {
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > CHUNK) throw new Error('invalid inventory owner bound');
    return this.statement(`SELECT build_id id,stored record FROM records WHERE stored IS NOT NULL AND build_id>?
      ORDER BY build_id LIMIT ?`).all(after || '', limit).map(row => ({ id: row.id, record: JSON.parse(row.record) }));
  }
  adjustTotal(kind, verb, n, queued) {
    const prior = this.statement('SELECT n,queued FROM totals WHERE kind=? AND verb=?').get(kind, verb);
    const next = (prior?.n || 0) + n, waiting = (prior?.queued || 0) + queued;
    if (next < 0 || waiting < 0 || waiting > next) throw new Error('inventory reference count underflow');
    if (!next) this.statement('DELETE FROM totals WHERE kind=? AND verb=?').run(kind, verb);
    else this.statement('INSERT OR REPLACE INTO totals VALUES(?,?,?,?)').run(kind, verb, next, waiting);
  }
  reclassify(qid) {
    const row = this.statement('SELECT * FROM questions WHERE id=?').get(qid);
    const next = classifyQuestion({ ...row, other_kind: this.name(row.other_kind), verb: this.name(row.verb) });
    const kind = next ? this.symbol('kind', next.kind) : null, verb = next ? this.symbol('verb', next.verb) : null;
    if (row.kind === kind && row.effective_verb === verb && row.queued === (next?.queued || 0)) return;
    if (row.kind != null) this.adjustTotal(row.kind, row.effective_verb, -1, -row.queued);
    this.statement('UPDATE questions SET kind=?,effective_verb=?,queued=? WHERE id=?').run(kind, verb, next?.queued || 0, qid);
    if (next) this.adjustTotal(kind, verb, 1, next.queued);
  }
  applyLifecycle(rows, { bootstrap = false } = {}) {
    if (!Array.isArray(rows) || rows.length > CHUNK) throw new Error('inventory lifecycle batch exceeds bound');
    this.transaction(() => {
      for (const row of rows) {
        const qid = id(row.id);
        const other = !row.deleted && row.kind ? this.symbol('kind', row.kind) : null;
        const verb = !row.deleted && row.verb ? this.symbol('verb', row.verb) : null;
        const state = row.deleted ? null : row.state || null, current = Number(!row.deleted && row.fact_engine === this.meta.engine);
        const served = Number(!row.deleted && !!row.served_answered);
        const prior = bootstrap ? this.statement('SELECT * FROM questions WHERE id=?').get(qid) : null;
        if (prior?.imported) {
          if (prior.state !== state || prior.fact_current !== current || prior.other_kind !== other || prior.verb !== verb || prior.served_answered !== served)
            throw new Error('bootstrap inventory task state changed');
          continue;
        }
        this.statement(`INSERT INTO questions(id,state,fact_current,other_kind,verb,served_answered,imported) VALUES(?,?,?,?,?,?,?)
          ON CONFLICT(id) DO UPDATE SET state=excluded.state,fact_current=excluded.fact_current,other_kind=excluded.other_kind,
          verb=excluded.verb,served_answered=excluded.served_answered,imported=max(questions.imported,excluded.imported)`)
          .run(qid, state, current, other, verb, served, Number(bootstrap));
        this.reclassify(qid);
      }
    });
  }
  queueJob(key, scope, changes = []) {
    text(key, 'job');
    if (!scope || typeof scope !== 'object' || !Array.isArray(changes) || changes.length > CHUNK) throw new Error('invalid inventory job');
    this.transaction(() => {
      let moved = !this.statement('SELECT key FROM jobs WHERE key=?').get(key);
      for (const raw of changes) {
        const row = change(raw), prior = this.statement('SELECT revision FROM job_inputs WHERE job=? AND scope=? AND key=?').get(key, row.scope, row.key);
        if (prior?.revision !== row.revision) moved = true;
        this.statement('INSERT OR REPLACE INTO job_inputs VALUES(?,?,?,?)').run(key, row.scope, row.key, row.revision);
      }
      if (moved && this.meta.epoch && this.meta.epoch.phase !== 'preparing') throw new Error('frozen inventory epoch cannot receive new work');
      if (moved) {
        const epoch = randomUUID();
        this.statement(`INSERT INTO jobs VALUES(?,?,?,?,NULL) ON CONFLICT(key) DO UPDATE SET scope=excluded.scope,requested=excluded.requested`)
          .run(key, canon(scope), epoch, epoch);
      }
    });
  }
  jobs(limit = 1) {
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > CHUNK) throw new Error('invalid inventory job limit');
    return this.statement('SELECT * FROM jobs ORDER BY key LIMIT ?').all(limit).map(row => ({ key: row.key, scope: JSON.parse(row.scope),
      epoch: row.epoch, after_owner: row.cursor, changes: this.statement('SELECT scope,key,revision FROM job_inputs WHERE job=? ORDER BY scope,key LIMIT 256').all(row.key) }));
  }
  adjustRefs(qid, board, queued) {
    this.statement('INSERT OR IGNORE INTO questions(id) VALUES(?)').run(qid);
    this.statement('UPDATE questions SET board_refs=board_refs+?,queued_refs=queued_refs+? WHERE id=?').run(board, queued, qid);
    this.reclassify(qid);
  }
  beginOwner(header, scope, { replace = true } = {}) {
    text(header.owner, 'owner');
    if (header.riven != null && typeof header.riven !== 'boolean' || header.source != null && (!Number.isSafeInteger(header.source) || header.source < 0))
      throw new Error('invalid inventory owner metadata');
    let candidate = null;
    if (header.build_id != null) {
      text(header.build_id, 'candidate');
      if (header.record) this.statement(`INSERT INTO records(build_id,fallback) VALUES(?,?)
        ON CONFLICT(build_id) DO UPDATE SET fallback=CASE WHEN records.stored IS NULL THEN excluded.fallback ELSE NULL END`)
        .run(header.build_id, canon(header.record));
      candidate = this.record(header.build_id).id;
    }
    this.statement(`INSERT INTO owners(owner,candidate,scope,weapon,riven,source) VALUES(?,?,?,?,?,?)
      ON CONFLICT(owner) DO UPDATE SET candidate=excluded.candidate,scope=excluded.scope,weapon=excluded.weapon,riven=excluded.riven,source=excluded.source`)
      .run(header.owner, candidate, canon(scope), header.weapon || null, header.riven == null ? null : Number(!!header.riven), header.source ?? null);
    const owner = this.statement('SELECT id FROM owners WHERE owner=?').get(header.owner).id;
    if (replace) {
      for (const row of this.statement('SELECT qid,queued FROM members WHERE owner=?').iterate(owner)) this.adjustRefs(row.qid, -1, -row.queued);
      this.statement('DELETE FROM members WHERE owner=?').run(owner);
    }
    return owner;
  }
  async applyOwners(path, { snapshot = this.meta.epoch?.snapshot, op } = {}) {
    if (!['bootstrap', 'apply'].includes(op) || this.meta.epoch?.phase !== 'preparing') throw new Error('inventory metadata is not prepared');
    const before = statSync(path);
    let count = 0, end = null;
    for await (const item of readInventoryRows(path)) {
      if (end) throw new Error('inventory data after end marker');
      if (item.type === 'owner') { text(item.owner, 'owner'); count++; }
      else if (item.type === 'end') end = item;
      else throw new Error('inventory metadata contains member rows');
    }
    const after = statSync(path);
    if (!end || end.v !== 1 || end.engine !== this.meta.engine || canon(end.contract) !== canon(this.meta.contract)
        || end.generation !== this.meta.generation || end.epoch !== this.meta.epoch.epoch || end.snapshot !== snapshot
        || end.owners !== count || end.rows !== 0 || before.size !== after.size || before.mtimeMs !== after.mtimeMs)
      throw new Error('incomplete inventory owner metadata');
    let batch = [];
    const flush = () => this.transaction(() => {
      for (const item of batch) this.beginOwner(item, item.weapon ? { weapon: item.weapon, riven: !!item.riven } : {},
        { replace: op !== 'bootstrap' || !this.meta.complete });
    });
    for await (const item of readInventoryRows(path)) if (item.type === 'owner') {
      batch.push(item);
      if (batch.length === OWNER_LIMIT) { flush(); batch = []; }
    }
    if (batch.length) flush();
    return { owners: count, rows: 0 };
  }
  addMember(owner, row, prefixes) {
    if (typeof row.queued !== 'boolean' && ![0, 1].includes(row.queued)) throw new Error('invalid inventory queued flag');
    const record = this.record(text(row.build_id, 'candidate'));
    const ruler = this.symbol('ruler', row.ruler), mode = this.symbol('mode', row.mode);
    const prefixKey = JSON.stringify([record.id, row.mode]);
    if (!prefixes.has(prefixKey)) prefixes.set(prefixKey, createHash('sha256')
      .update(`simulate\n{"mode":${JSON.stringify(row.mode)},"record":${record.record},"ruler":`));
    const qid = prefixes.get(prefixKey).copy().update(JSON.stringify(row.ruler) + '}').digest().subarray(0, 16);
    const prior = this.statement('SELECT qid,queued FROM members WHERE owner=? AND candidate=? AND ruler=? AND mode=?').get(owner, record.id, ruler, mode);
    if (prior) {
      if (!Buffer.from(prior.qid).equals(qid)) throw new Error('inventory record changed within owner');
      if (row.queued && !prior.queued) {
        this.statement('UPDATE members SET queued=1 WHERE owner=? AND candidate=? AND ruler=? AND mode=?').run(owner, record.id, ruler, mode);
        this.adjustRefs(qid, 0, 1);
      }
    } else {
      this.statement('INSERT INTO members VALUES(?,?,?,?,?,?)').run(owner, record.id, ruler, mode, qid, Number(row.queued));
      this.adjustRefs(qid, 1, Number(row.queued));
    }
  }
  async replaceOwners(path, { job, limit = OWNER_LIMIT, snapshot, generation = this.meta.generation } = {}) {
    if (!job || !Number.isSafeInteger(limit) || limit < 1 || limit > OWNER_LIMIT) throw new Error('invalid inventory engine chunk');
    const current = this.statement('SELECT * FROM jobs WHERE key=?').get(job.key);
    if (!current || current.epoch !== job.epoch || current.cursor !== job.after_owner || current.scope !== canon(job.scope))
      throw new Error('superseded inventory engine chunk');
    let owner = null, ownerName = null, ownerRows = 0, owners = 0, rows = 0, end = null, prior = job.after_owner;
    const prefixes = new Map();
    this.db.exec('BEGIN IMMEDIATE');
    try {
      for await (const item of readInventoryRows(path)) {
        if (end) throw new Error('inventory data after end marker');
        if (item.type === 'owner') {
          if (++owners > limit || prior != null && item.owner <= prior) throw new Error('inventory owner order/bound violated');
          prior = ownerName = item.owner; ownerRows = 0; prefixes.clear(); owner = this.beginOwner(item, job.scope);
        } else if (item.type === 'row') {
          if (owner == null || item.owner !== ownerName || ++ownerRows > 4096) throw new Error('invalid inventory owner rows');
          this.addMember(owner, item, prefixes); rows++;
        } else if (item.type === 'end') end = item;
        else throw new Error('invalid inventory engine output');
      }
      if (!end || end.v !== 1 || end.engine !== this.meta.engine || canon(end.contract) !== canon(this.meta.contract)
          || end.generation !== generation || !end.snapshot || snapshot && end.snapshot !== snapshot
          || this.meta.epoch && end.epoch !== this.meta.epoch.epoch
          || canon(end.scope) !== canon(job.scope) || end.owners !== owners || end.rows !== rows || typeof end.complete !== 'boolean'
          || (end.complete ? end.next_owner != null : !owners || end.next_owner !== prior)) throw new Error('incomplete inventory engine output');
      if (!end.complete) this.statement('UPDATE jobs SET cursor=? WHERE key=?').run(end.next_owner, job.key);
      else if (current.requested !== current.epoch) this.statement('UPDATE jobs SET epoch=requested,cursor=NULL WHERE key=?').run(job.key);
      else {
        if (this.meta.epoch) {
          if (this.statement(`SELECT 1 FROM job_inputs i JOIN epoch_tokens e ON e.scope=i.scope AND e.key=i.key
            WHERE i.job=? AND i.revision<>e.revision LIMIT 1`).get(job.key)) throw new Error('sealed inventory token changed');
          this.statement(`INSERT OR IGNORE INTO epoch_tokens(scope,key,revision) SELECT scope,key,revision FROM job_inputs WHERE job=?`).run(job.key);
          this.statement(`INSERT INTO receipts SELECT i.scope,i.key,i.revision FROM job_inputs i
            JOIN epoch_tokens e ON e.scope=i.scope AND e.key=i.key WHERE i.job=? AND e.processed=0
            ON CONFLICT(scope,key) DO UPDATE SET revision=excluded.revision`).run(job.key);
        } else this.statement(`INSERT INTO receipts SELECT scope,key,revision FROM job_inputs WHERE job=?
          ON CONFLICT(scope,key) DO UPDATE SET revision=excluded.revision`).run(job.key);
        this.statement('DELETE FROM job_inputs WHERE job=?').run(job.key);
        this.statement('DELETE FROM jobs WHERE key=?').run(job.key);
      }
      this.db.exec('COMMIT');
      return { owners, rows, complete: end.complete, next_owner: end.next_owner, pending: this.jobs(1).length > 0 };
    } catch (error) { this.db.exec('ROLLBACK'); this.symbolIds.clear(); this.symbolNames.clear(); throw error; }
  }
  pendingAcks(limit = CHUNK) {
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > CHUNK) throw new Error('invalid inventory acknowledgment bound');
    if (this.meta.epoch && this.jobs(1).length) return [];
    return this.statement('SELECT * FROM receipts ORDER BY scope,key LIMIT ?').all(limit);
  }
  recordReceipts(changes) {
    if (!Array.isArray(changes) || changes.length > CHUNK || this.jobs(1).length) throw new Error('inventory receipts are not ready');
    this.transaction(() => { for (const raw of changes) {
      const row = change(raw);
      if (this.meta.epoch) {
        const prior = this.statement('SELECT revision,processed FROM epoch_tokens WHERE scope=? AND key=?').get(row.scope, row.key);
        if (prior && prior.revision !== row.revision) throw new Error('sealed inventory token changed');
        if (prior?.processed) continue;
        this.statement('INSERT OR IGNORE INTO epoch_tokens(scope,key,revision) VALUES(?,?,?)').run(row.scope, row.key, row.revision);
      }
      this.statement('INSERT OR REPLACE INTO receipts VALUES(?,?,?)').run(row.scope, row.key, row.revision);
    } });
  }
  acknowledge(changes) {
    if (!Array.isArray(changes) || changes.length > CHUNK) throw new Error('inventory acknowledgment batch exceeds bound');
    let next = this.meta;
    this.transaction(() => { let processed = 0; for (const raw of changes) {
      const row = change(raw);
      const removed = this.statement('DELETE FROM receipts WHERE scope=? AND key=? AND revision=?').run(row.scope, row.key, row.revision).changes;
      if (this.meta.epoch && removed) processed += Number(this.statement(`UPDATE epoch_tokens SET processed=1
        WHERE scope=? AND key=? AND revision=? AND processed=0`).run(row.scope, row.key, row.revision).changes);
    } if (this.meta.epoch) {
      next = { ...this.meta, epoch: { ...this.meta.epoch, tokens_processed: this.meta.epoch.tokens_processed + processed } };
      this.statement('INSERT OR REPLACE INTO meta VALUES(1,?)').run(JSON.stringify(next));
    } });
    this.meta = next;
  }
  summary() {
    let rows = 0, queued = 0;
    const kinds = new Map(), verbs = {};
    for (const row of this.statement(`SELECT k.name kind,v.name verb,t.n,t.queued FROM totals t
      JOIN symbols k ON k.id=t.kind JOIN symbols v ON v.id=t.verb ORDER BY k.name,v.name`).iterate()) {
      rows += row.n; queued += row.queued;
      kinds.set(row.kind, (kinds.get(row.kind) || 0) + row.n); verbs[row.verb] = (verbs[row.verb] || 0) + row.n;
    }
    return { rows, queued, orders: [...kinds].map(([kind, total]) => ({ kind, total })), verbs };
  }
  *tuples() {
    for (const row of this.statement(`SELECT q.id,k.name kind,v.name verb,q.queued FROM questions q
      JOIN symbols k ON k.id=q.kind JOIN symbols v ON v.id=q.effective_verb WHERE q.kind IS NOT NULL ORDER BY q.id`).iterate())
      yield { id: Buffer.from(row.id).toString('hex'), kind: row.kind, verb: row.verb, queued: row.queued };
  }
  digest() {
    const hash = createHash('sha256');
    for (const row of this.tuples()) hash.update(JSON.stringify(row) + '\n');
    return hash.digest('hex');
  }
  seal({ snapshot, started_at, expected } = {}) {
    text(snapshot, 'snapshot');
    if (!Number.isFinite(Date.parse(started_at)) || !expected || this.jobs(1).length || !this.meta.epoch && this.pendingAcks(1).length)
      throw new Error('incomplete inventory bootstrap');
    const actual = { ...this.summary(), owners: this.statement('SELECT count(*) n FROM owners').get().n,
      records: this.statement('SELECT count(*) n FROM records WHERE stored IS NOT NULL').get().n,
      tasks: this.statement('SELECT count(*) n FROM questions WHERE imported=1').get().n };
    for (const key of ['owners', 'records', 'tasks', 'rows', 'queued'])
      if (integer(expected[key], key) !== actual[key]) throw new Error(`incomplete inventory bootstrap ${key}`);
    const digest = this.digest();
    if (!/^[a-f0-9]{64}$/.test(expected.digest) || expected.digest !== digest) throw new Error('inventory bootstrap digest mismatch');
    this.setMeta({ ...this.meta, complete: true, snapshot, started_at, audit_digest: digest });
    return { ...actual, digest };
  }
  plan({ snapshot = this.meta.snapshot, started_at = this.meta.started_at, source = '' } = {}) {
    if (!this.meta.complete || this.jobs(1).length || this.pendingAcks(1).length || this.meta.epoch && this.meta.epoch.phase !== 'complete')
      throw new Error('inventory is not complete');
    if (this.meta.epoch && (snapshot !== this.meta.epoch.snapshot || started_at !== this.meta.epoch.started_at))
      throw new Error('inventory epoch input time cannot advance');
    text(snapshot, 'snapshot');
    if (!Number.isFinite(Date.parse(started_at))) throw new Error('invalid inventory input time');
    return { v: 2, ...this.summary(), engine: this.meta.engine, contract: this.meta.contract, generation: this.meta.generation,
      snapshot, started_at, source, ...(this.meta.epoch ? { epoch: this.meta.epoch.epoch, manifest_sha: this.meta.epoch.manifest_sha } : {}) };
  }
  audit(other) {
    if (this.meta.engine !== other.meta.engine || canon(this.meta.contract) !== canon(other.meta.contract))
      throw new Error('inventory audit generation semantics differ');
    const left = this.tuples(), right = other.tuples();
    let rows = 0;
    try { for (;;) {
      const a = left.next(), b = right.next();
      if (a.done || b.done) return { equal: a.done === b.done, rows, digest: this.digest() };
      if (canon(a.value) !== canon(b.value)) return { equal: false, rows, first: { active: a.value, shadow: b.value } };
      rows++;
    } } finally { left.return(); right.return(); }
  }
  activate(receipt) {
    this.plan();
    if (!receipt || receipt.generation !== this.meta.generation || receipt.engine !== this.meta.engine
        || canon(receipt.contract) !== canon(this.meta.contract) || !receipt.snapshot || !Number.isFinite(Date.parse(receipt.started_at)))
      throw new Error('inventory publication receipt mismatch');
    const summary = this.summary();
    if (this.meta.epoch && (receipt.epoch !== this.meta.epoch.epoch || receipt.snapshot !== this.meta.epoch.snapshot
        || receipt.started_at !== this.meta.epoch.started_at)) throw new Error('inventory publication epoch mismatch');
    if (receipt.total != null && receipt.total !== summary.rows || receipt.registered != null && receipt.registered !== summary.queued)
      throw new Error('inventory publication counts mismatch');
    this.setMeta({ ...this.meta, published: receipt });
  }
  close() { this.db.close(); }
}

export const openCatalog = (path, options) => new InventoryCatalog(path, options);
