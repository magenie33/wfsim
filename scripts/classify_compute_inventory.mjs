// SPDX-License-Identifier: AGPL-3.0-or-later
// Task identity, rather than result rows or delivery attempts, is the counting unit.
import { DatabaseSync } from 'node:sqlite';
import { createReadStream, createWriteStream, mkdtempSync, renameSync, rmSync } from 'node:fs';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { canon, questionId } from '../worker/task_contract.js';
import { deflateRawSync, inflateRawSync } from 'node:zlib';

export const INVENTORY_MIN_CACHE_KIB = 16 * 1024;
export const INVENTORY_MAX_CACHE_KIB = 16 * 1024 * 1024;

async function* rows(path) {
  if (typeof path !== 'string') { yield* path; return; }
  for await (const line of createInterface({ input: createReadStream(path), crlfDelay: Infinity })) {
    if (line.trim()) { const row = JSON.parse(line); if (!row || typeof row !== 'object') throw new Error('invalid inventory row'); yield row; }
  }
}
async function exportQuestions(db, path) {
  const partial = typeof path === 'string' ? `${path}.partial-${process.pid}` : null;
  const out = partial ? createWriteStream(partial) : null, hash = createHash('sha256');
  let failed = null;
  out?.on('error', error => { failed = error; });
  try {
    for (const row of db.prepare('SELECT lower(hex(id)) id,kind,verb,queued FROM tasks ORDER BY id').iterate()) {
      const text = JSON.stringify(row) + '\n';
      if (failed) throw failed;
      hash.update(text);
      if (out && !out.write(text)) await once(out, 'drain');
    }
    if (failed) throw failed;
    if (out) { out.end(); await once(out, 'finish'); }
    if (failed) throw failed;
    if (partial) renameSync(partial, path);
    return hash.digest('hex');
  } finally { out?.destroy(); if (partial) rmSync(partial, { force: true }); }
}

export async function classifyInventory({ queue, missing, orphans, builds, canonical, tasks, meta, rulers, questions, cacheKiB = INVENTORY_MIN_CACHE_KIB, databaseBytes, check = () => {}, scratchRoot = tmpdir() }) {
  if (!Number.isSafeInteger(cacheKiB) || cacheKiB < INVENTORY_MIN_CACHE_KIB || cacheKiB > INVENTORY_MAX_CACHE_KIB)
    throw new Error('invalid inventory cache budget');
  if (!meta?.snapshot || !meta.engine || !meta.started_at || !Array.isArray(rulers) || !rulers.length
    || !Number.isSafeInteger(meta.counts?.queue) || meta.counts.queue < 0) throw new Error('incomplete inventory snapshot');
  const roster = new Set(rulers);
  const dir = mkdtempSync(join(scratchRoot, 'compute-classified-')), db = new DatabaseSync(join(dir, 'tasks.db'));
  try {
    if(databaseBytes!==undefined) {
      const pageSize=db.prepare('PRAGMA page_size').get().page_size,pages=Math.floor(databaseBytes/pageSize);
      if(!Number.isSafeInteger(databaseBytes)||pages<1||pages>4294967294
        ||db.prepare('PRAGMA max_page_count='+pages).get().max_page_count!==pages)throw Error('invalid classifier database bound');
    }
    db.exec(`PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;PRAGMA temp_store=FILE;PRAGMA cache_size=-${cacheKiB};
      CREATE TABLE records(id TEXT PRIMARY KEY,record BLOB NOT NULL) WITHOUT ROWID;
      CREATE TABLE states(id BLOB UNIQUE,ordinal INTEGER PRIMARY KEY,state TEXT,fact_engine TEXT,kind TEXT,verb TEXT,served_answered INTEGER);
      CREATE TABLE excluded(build TEXT,ruler TEXT,mode TEXT,PRIMARY KEY(build,ruler,mode)) WITHOUT ROWID;
      CREATE TABLE tasks(id BLOB PRIMARY KEY,kind TEXT,verb TEXT,queued INTEGER) WITHOUT ROWID;BEGIN`);
    const recordPut = db.prepare('INSERT INTO records VALUES (?,?) ON CONFLICT(id) DO UPDATE SET record=excluded.record');
    const canonicalPut = db.prepare('INSERT OR IGNORE INTO records VALUES (?,?)');
    const recordGet = db.prepare('SELECT record FROM records WHERE id=?');
    const statePut = db.prepare('INSERT OR IGNORE INTO states VALUES (?,?,?,?,?,?,?)');
    const stateGet = db.prepare('SELECT state,fact_engine,kind,verb,served_answered FROM states WHERE id=?');
    let n = 0;
    for await (const r of rows(builds)) {
      if (typeof r.id !== 'string' || !r.record) throw new Error('invalid inventory build');
      recordPut.run(r.id, deflateRawSync(canon(r.record), { level: 1 })); if (++n % 256 === 0) check();
    }
    if (n !== meta.counts.library) throw new Error('incomplete inventory builds');
    let canonicalRows = 0;
    if (canonical) for await (const r of rows(canonical)) {
      if (typeof r.id !== 'string' || !r.record) throw new Error('invalid canonical inventory build');
      if (!recordGet.get(r.id)) canonicalPut.run(r.id, deflateRawSync(canon(r.record), { level: 1 }));
      if (++canonicalRows % 256 === 0) check();
    }
    n = 0;
    for await (const q of rows(tasks)) {
      if (!/^[a-f0-9]{32}$/.test(q.id)) throw new Error('invalid inventory task');
      if (!statePut.run(Buffer.from(q.id, 'hex'), n, q.state ?? null, q.fact_engine ?? null, q.kind ?? null,
        q.verb ?? null, Number(Boolean(q.served_answered))).changes) throw new Error('duplicate inventory task state');
      n++; if (n % 256 === 0) check();
    }
    if (n !== meta.counts.tasks) throw new Error('incomplete inventory task states');
    const key = r => {
      if (!['build_id', 'ruler', 'mode'].every(k => typeof r[k] === 'string' && r[k])) throw new Error('invalid inventory candidate');
      return [r.build_id, r.ruler, r.mode];
    };
    const exclude = db.prepare('INSERT OR IGNORE INTO excluded VALUES (?,?,?)');
    const excluded = db.prepare('SELECT 1 FROM excluded WHERE build=? AND ruler=? AND mode=?');
    let excludedCount = 0;
    for await (const r of rows(orphans)) { exclude.run(...key(r)); if (++excludedCount % 256 === 0) check(); }
    const insert = db.prepare(`INSERT INTO tasks VALUES (?,?,?,?) ON CONFLICT(id) DO UPDATE SET
      queued=max(tasks.queued,excluded.queued),kind=CASE WHEN excluded.kind='verification' THEN excluded.kind ELSE tasks.kind END`);
    let sample = 0;
    const add = (id, kind, verb, queued) => insert.run(id, kind, verb, queued);
    for (const [path, queued] of [[queue, 1], [missing, 0]]) {
      let read = 0;
      for await (const r of rows(path)) {
        read++; if (read % 256 === 0) check();
        const k = key(r); if (!roster.has(r.ruler)) { if (!queued) throw new Error('unknown inventory ruler'); continue; }
        if (excluded.get(...k)) continue;
        const compressed = recordGet.get(r.build_id)?.record; if (!compressed) throw new Error(`inventory build unavailable: ${r.build_id}`);
        const record = inflateRawSync(compressed).toString();
        const input = `simulate\n{"mode":${JSON.stringify(r.mode)},"record":${record},"ruler":${JSON.stringify(r.ruler)}}`;
        const id = createHash('sha256').update(input).digest().subarray(0, 16), q = stateGet.get(id);
        if (sample++ < 10 && id.toString('hex') !== await questionId('simulate', { mode: r.mode, record: JSON.parse(record), ruler: r.ruler })) throw new Error('inventory identity mismatch');
        if (q?.state === 'fact' && q.fact_engine === meta.engine || q?.state === 'refused') continue;
        add(id, q?.state === 'spot' && q.fact_engine === meta.engine ? 'verification' : 'board', 'simulate', queued);
      }
      if (queued && read !== meta.counts.queue) throw new Error('incomplete inventory queue');
    }
    for (const q of db.prepare('SELECT * FROM states ORDER BY ordinal').iterate()) {
      if (q.state === 'fact' && q.fact_engine === meta.engine) continue;
      const check = q.state === 'spot' && q.fact_engine === meta.engine
        || ['disputed', 'nondeterministic'].includes(q.state) && q.served_answered;
      if (check) add(q.id, 'verification', q.verb, 1);
      else if (q.kind && ['open', 'disputed', 'nondeterministic'].includes(q.state)) add(q.id, q.kind, q.verb, 1);
    }
    db.exec('COMMIT');
    const total = db.prepare('SELECT count(*) n,coalesce(sum(queued),0) queued FROM tasks').get();
    const orders = db.prepare('SELECT kind,count(*) total FROM tasks GROUP BY kind ORDER BY kind').all();
    const verbs = Object.fromEntries(db.prepare('SELECT verb,count(*) n FROM tasks GROUP BY verb').all().map(r => [r.verb, r.n]));
    const question_digest = questions ? await exportQuestions(db, questions) : undefined;
    return { v: 2, rows: total.n, queued: total.queued, orders, verbs, started_at: meta.started_at, snapshot: meta.snapshot, engine: meta.engine,
      ...(questions ? { question_digest } : {}) };
  } finally { db.close(); rmSync(dir, { recursive: true, force: true }); }
}
