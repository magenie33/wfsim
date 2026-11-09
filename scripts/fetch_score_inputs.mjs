// SPDX-License-Identifier: AGPL-3.0-or-later
// A scoring run reads only the calculations its official witness has leased.
import postgres from './pg/node_modules/postgres/src/index.js';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, openSync, writeFileSync, closeSync, renameSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { BOARD_KINDS } from '../worker/board_kinds.js';
import { pipelineMonitor } from './pipeline_monitor.mjs';

export async function captureScoreInputs({ sql, claim, directory, engine, max = 40000, report = () => {} }) {
  if (!Number.isSafeInteger(max) || max < 1 || max > 40000) throw Error('invalid official claim budget');
  const scratch = mkdtempSync(join(directory, '.score-inputs-'));
  const db = new DatabaseSync(join(scratch, 'inputs.sqlite'));
  db.exec(`PRAGMA cache_size=-16384; PRAGMA temp_store=FILE; PRAGMA mmap_size=0;
    CREATE TABLE claimed(id TEXT PRIMARY KEY) WITHOUT ROWID;
    CREATE TABLE wanted(build_id TEXT,ruler TEXT,mode TEXT,PRIMARY KEY(build_id,ruler,mode)) WITHOUT ROWID;
    CREATE TABLE rows(build_id TEXT,ruler TEXT,mode TEXT,batch TEXT,at TEXT,PRIMARY KEY(build_id,ruler,mode)) WITHOUT ROWID;
    CREATE TABLE records(id TEXT PRIMARY KEY,body TEXT) WITHOUT ROWID;
    CREATE TABLE facts(id TEXT,ruler TEXT,mode TEXT,body TEXT,PRIMARY KEY(id,ruler,mode)) WITHOUT ROWID;`);
  let leased = 0;
  try {
    while (leased < max) {
      report('claiming', { leased });
      const limit = Math.min(2000, max - leased), tasks = await claim({ witness: 'official:scorer', engine,
        states: ['open'], producers: BOARD_KINDS, max: limit, lease_ms: 43200000 });
      if (!Array.isArray(tasks) || tasks.length > limit) throw Error('invalid factory claim response');
      for (const task of tasks) {
        if (typeof task.question !== 'string' || task.verb !== 'simulate' || !Array.isArray(task.demands)) throw Error('invalid claimed calculation');
        db.prepare('INSERT INTO claimed VALUES(?)').run(task.question);
        for (const demand of task.demands) {
          if (!BOARD_KINDS.includes(demand.producer)) continue;
          const p = demand.payload;
          if (!p || ![p.identity, p.ruler, p.mode].every(x => typeof x === 'string' && x.length > 0)
              || demand.ref !== `${p.identity}|${p.ruler}|${p.mode}`) throw Error('invalid claimed board reference');
          db.prepare('INSERT OR IGNORE INTO wanted VALUES(?,?,?)').run(p.identity, p.ruler, p.mode);
        }
      }
      leased += tasks.length;
      report('claimed', { leased });
      if (tasks.length < limit) break;
    }
    report('capture', { leased });
    const snapshot = await sql.begin('isolation level repeatable read read only', async tx => {
      await tx.unsafe("SET LOCAL statement_timeout='180s'");
      const [stamp] = await tx.unsafe('SELECT pg_current_snapshot()::text snapshot');
      let after = ['', '', ''], processed = 0;
      for (;;) {
        const keys = db.prepare('SELECT * FROM wanted WHERE (build_id,ruler,mode)>(?,?,?) ORDER BY build_id,ruler,mode LIMIT 256').all(...after);
        if (!keys.length) break;
        // Parameterized primary-key probes cannot turn a small claim into a queue scan.
        const rows = await tx.unsafe(`SELECT q.*,b.at,source.record,f.fact FROM
          jsonb_to_recordset($1::jsonb) AS k(build_id text,ruler text,mode text)
          CROSS JOIN LATERAL (SELECT q.batch,q.build_id,q.ruler,q.mode FROM queue q
            WHERE q.build_id=k.build_id AND q.ruler=k.ruler AND q.mode=k.mode LIMIT 1) q
          JOIN batches b ON b.id=q.batch
          CROSS JOIN LATERAL (SELECT record FROM builds WHERE id=q.build_id LIMIT 1) source
          LEFT JOIN LATERAL (SELECT row_to_json(f) fact FROM board_facts f WHERE
            f.identity=q.build_id AND f.ruler=q.ruler AND f.mode=q.mode LIMIT 1) f ON true`, [JSON.stringify(keys)]);
        db.exec('BEGIN');
        try {
          for (const row of rows) {
            const record = typeof row.record === 'string' ? row.record : JSON.stringify(row.record);
            const parsed = record && JSON.parse(record);
            if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw Error('invalid claimed source record');
            db.prepare('INSERT INTO rows VALUES(?,?,?,?,?)').run(row.build_id, row.ruler, row.mode, row.batch, row.at);
            db.prepare('INSERT OR IGNORE INTO records VALUES(?,?)').run(row.build_id, record);
            if (row.fact) db.prepare('INSERT INTO facts VALUES(?,?,?,?)').run(row.build_id, row.ruler, row.mode, JSON.stringify(row.fact));
          }
          db.exec('COMMIT');
        } catch (error) { db.exec('ROLLBACK'); throw error; }
        const last = keys.at(-1); after = [last.build_id, last.ruler, last.mode];
        processed += keys.length; report('capture', { leased, processed });
      }
      return stamp.snapshot;
    });
    report('export', { snapshot });
    const emit = (name, query, render, prefix = '', suffix = '') => {
      const fd = openSync(join(scratch, name), 'w');
      try {
        writeFileSync(fd, prefix); let i = 0;
        for (const row of db.prepare(query).iterate()) writeFileSync(fd, render(row, i++));
        writeFileSync(fd, suffix);
      } finally { closeSync(fd); }
    };
    emit('library.json', 'SELECT body FROM records ORDER BY id', (r, i) => (i ? ',' : '') + r.body, '[', ']');
    emit('queue.ndjson', 'SELECT batch,build_id,ruler,mode FROM rows ORDER BY at,batch,build_id,ruler,mode', r => JSON.stringify(r) + '\n');
    emit('facts-known.ndjson', 'SELECT body FROM facts ORDER BY id,ruler,mode', r => r.body + '\n');
    const counts = Object.fromEntries(['wanted', 'rows', 'records', 'facts'].map(table => [table, db.prepare(`SELECT count(*) n FROM ${table}`).get().n]));
    for (const name of ['library.json', 'queue.ndjson', 'facts-known.ndjson']) renameSync(join(scratch, name), join(directory, name));
    return { leased, ...counts, snapshot };
  } finally { db.close(); rmSync(scratch, { recursive: true, force: true }); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  if (!process.env.PG_URL || !process.env.ENGINE || !process.env.FACTORY_TOKEN) throw Error('PG_URL, ENGINE and FACTORY_TOKEN required');
  const sql = postgres(process.env.PG_URL, { max: 1, fetch_types: false, connect_timeout: 15, onnotice: () => {},
    connection: { application_name: 'wfsim-score-inputs' } });
  const claim = async body => {
    const response = await fetch(`${process.env.FACTORY_ORIGIN || 'https://wfsim.app'}/api/tasks/official/claim`, {
      method: 'POST', headers: { authorization: `Bearer ${process.env.FACTORY_TOKEN}`, 'content-type': 'application/json' },
      body: JSON.stringify(body), signal: AbortSignal.timeout(180000) });
    if (!response.ok) throw Error(`official claim HTTP ${response.status}`);
    const result = await response.json(); if (!result.ok) throw Error('official claim refused');
    return result.tasks;
  };
  const monitor = pipelineMonitor(join(process.cwd(), 'diagnostics'), 'score-inputs', { source: process.env.GITHUB_SHA, engine: process.env.ENGINE });
  try {
    const result = await captureScoreInputs({ sql, claim, directory: process.cwd(), engine: process.env.ENGINE, report: monitor.report });
    monitor.report('exported', result); monitor.finish(); console.log(JSON.stringify(result));
  } catch (error) { monitor.finish('failed', error); process.exitCode = 1; }
  finally { await sql.end({ timeout: 5 }); }
}
