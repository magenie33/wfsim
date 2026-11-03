// SPDX-License-Identifier: AGPL-3.0-or-later
// Task identity, rather than result rows or delivery attempts, is the counting unit.
import { DatabaseSync } from 'node:sqlite';
import { createReadStream, mkdtempSync, rmSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { canon, questionId } from '../worker/task_contract.js';

async function* rows(path) {
  for await (const line of createInterface({ input: createReadStream(path), crlfDelay: Infinity })) {
    if (line.trim()) { const row = JSON.parse(line); if (!row || typeof row !== 'object') throw new Error('invalid inventory row'); yield row; }
  }
}
export async function classifyInventory({ queue, missing, orphans, builds, canonical, tasks, meta, rulers }) {
  if (!meta?.snapshot || !meta.engine || !meta.started_at || !Array.isArray(rulers) || !rulers.length
    || !Number.isSafeInteger(meta.counts?.queue) || meta.counts.queue < 0) throw new Error('incomplete inventory snapshot');
  const records = new Map(), states = new Map(), excluded = new Set(), roster = new Set(rulers);
  let n = 0;
  for await (const r of rows(builds)) { if (typeof r.id !== 'string' || !r.record) throw new Error('invalid inventory build'); records.set(r.id, canon(r.record)); n++; }
  if (n !== meta.counts.library) throw new Error('incomplete inventory builds');
  if (canonical) for await (const r of rows(canonical)) {
    if (typeof r.id !== 'string' || !r.record) throw new Error('invalid canonical inventory build');
    if (!records.has(r.id)) records.set(r.id, canon(r.record));
  }
  n = 0;
  for await (const q of rows(tasks)) {
    if (!/^[a-f0-9]{32}$/.test(q.id)) throw new Error('invalid inventory task');
    if (states.has(q.id)) throw new Error('duplicate inventory task state');
    states.set(q.id, q.state === 'fact' && q.fact_engine === meta.engine ? 0 : q); n++;
  }
  if (n !== meta.counts.tasks) throw new Error('incomplete inventory task states');
  const key = r => {
    if (!['build_id', 'ruler', 'mode'].every(k => typeof r[k] === 'string' && r[k])) throw new Error('invalid inventory candidate');
    return JSON.stringify([r.build_id, r.ruler, r.mode]);
  };
  for await (const r of rows(orphans)) excluded.add(key(r));
  const dir = mkdtempSync(join(tmpdir(), 'compute-classified-')), db = new DatabaseSync(join(dir, 'tasks.db'));
  try {
    db.exec('PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA cache_size=-1048576;CREATE TABLE tasks(id BLOB PRIMARY KEY,kind TEXT,verb TEXT,queued INTEGER) WITHOUT ROWID;BEGIN');
    const insert = db.prepare(`INSERT INTO tasks VALUES (?,?,?,?) ON CONFLICT(id) DO UPDATE SET
      queued=max(tasks.queued,excluded.queued),kind=CASE WHEN excluded.kind='verification' THEN excluded.kind ELSE tasks.kind END`);
    let sample = 0;
    const add = (id, kind, verb, queued) => insert.run(Buffer.from(id, 'hex'), kind, verb, queued);
    for (const [path, queued] of [[queue, 1], [missing, 0]]) {
      let read = 0;
      for await (const r of rows(path)) {
        read++;
        const k = key(r); if (!roster.has(r.ruler)) { if (!queued) throw new Error('unknown inventory ruler'); continue; }
        if (excluded.has(k)) continue;
        const record = records.get(r.build_id); if (!record) throw new Error(`inventory build unavailable: ${r.build_id}`);
        const input = `simulate\n{"mode":${JSON.stringify(r.mode)},"record":${record},"ruler":${JSON.stringify(r.ruler)}}`;
        const id = createHash('sha256').update(input).digest('hex').slice(0, 32), q = states.get(id);
        if (sample++ < 10 && id !== await questionId('simulate', { mode: r.mode, record: JSON.parse(record), ruler: r.ruler })) throw new Error('inventory identity mismatch');
        if (q === 0 || q?.state === 'refused') continue;
        add(id, q?.state === 'spot' && q.fact_engine === meta.engine ? 'verification' : 'board', 'simulate', queued);
      }
      if (queued && read !== meta.counts.queue) throw new Error('incomplete inventory queue');
    }
    for (const q of states.values()) {
      if (q === 0) continue;
      const check = q.state === 'spot' && q.fact_engine === meta.engine
        || ['disputed', 'nondeterministic'].includes(q.state) && q.served_answered;
      if (check) add(q.id, 'verification', q.verb, 1);
      else if (q.kind && ['open', 'disputed', 'nondeterministic'].includes(q.state)) add(q.id, q.kind, q.verb, 1);
    }
    db.exec('COMMIT');
    const total = db.prepare('SELECT count(*) n,coalesce(sum(queued),0) queued FROM tasks').get();
    const orders = db.prepare('SELECT kind,count(*) total FROM tasks GROUP BY kind ORDER BY kind').all();
    const verbs = Object.fromEntries(db.prepare('SELECT verb,count(*) n FROM tasks GROUP BY verb').all().map(r => [r.verb, r.n]));
    return { v: 2, rows: total.n, queued: total.queued, orders, verbs, started_at: meta.started_at, snapshot: meta.snapshot, engine: meta.engine };
  } finally { db.close(); rmSync(dir, { recursive: true, force: true }); }
}
