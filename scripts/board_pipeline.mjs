// SPDX-License-Identifier: AGPL-3.0-or-later
// Durable stage controls and immutable publication references; remote ACKs follow each file.
import { DatabaseSync } from 'node:sqlite';
import { createHash } from 'node:crypto';
import { mkdirSync, existsSync, readFileSync, readdirSync, statSync, statfsSync, lstatSync,
  copyFileSync, renameSync, rmSync, openSync, closeSync, fsyncSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { canon } from '../worker/task_contract.js';
import { fileHash, syncDirectory } from './inventory_publication_io.mjs';

const STAGES = ['intake', 'normalization', 'inventory', 'projection', 'publication'], RESERVE = 8 * 1024 ** 3, FILE_LIMIT = 32 * 1024 ** 2;
const load = path => JSON.parse(readFileSync(path, 'utf8'));
function files(root, prefix = '') {
  const result = [];
  for (const entry of readdirSync(join(root, prefix), { withFileTypes: true })) {
    const name = prefix + entry.name;
    if (entry.isSymbolicLink()) throw Error('publication artifacts cannot contain links');
    if (entry.isDirectory()) result.push(...files(root, name + '/'));
    else if (entry.isFile() && name.endsWith('.json')) result.push(name);
  }
  return result.sort();
}
export class BoardPipeline {
  constructor(work, { maxBytes = Number(process.env.WFSIM_BOARD_OUTBOX_BYTES || 2 * 1024 ** 3) } = {}) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) throw Error('invalid publication outbox disk bound');
    this.root = join(resolve(work), 'publication'); this.maxBytes = maxBytes;
    this.blobs = join(this.root, 'blobs'); mkdirSync(this.blobs, { recursive: true });
    this.db = new DatabaseSync(join(this.root, 'pipeline.sqlite'));
    this.db.exec(`PRAGMA busy_timeout=5000;PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA cache_size=-2048;
      CREATE TABLE IF NOT EXISTS state_board_pipeline_controls(name TEXT PRIMARY KEY,paused INTEGER NOT NULL,changed_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS state_board_pipeline_progress(name TEXT PRIMARY KEY,at TEXT NOT NULL,body TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS record_board_publication_packages(seq INTEGER PRIMARY KEY,epoch TEXT NOT NULL UNIQUE,
        manifest TEXT NOT NULL,created_at TEXT NOT NULL,delivered_at TEXT);
      CREATE TABLE IF NOT EXISTS relation_board_publication_files(package INTEGER NOT NULL,name TEXT NOT NULL,sha TEXT NOT NULL,
        bytes INTEGER NOT NULL,served INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(package,name)) WITHOUT ROWID;
      CREATE INDEX IF NOT EXISTS board_publication_files_sha ON relation_board_publication_files(sha,package);
      CREATE TABLE IF NOT EXISTS entity_board_publication_blobs(sha TEXT PRIMARY KEY,bytes INTEGER NOT NULL) WITHOUT ROWID;
      CREATE TABLE IF NOT EXISTS state_board_remote_files(name TEXT PRIMARY KEY,sha TEXT NOT NULL) WITHOUT ROWID;
      CREATE TABLE IF NOT EXISTS state_board_source_files(name TEXT PRIMARY KEY,stamp TEXT NOT NULL,sha TEXT NOT NULL) WITHOUT ROWID;`);
    if (!this.db.prepare('SELECT 1 FROM record_board_publication_packages LIMIT 1').get() && existsSync(join(work, 'pushed.json'))) {
      const ledger = load(join(work, 'pushed.json'));
      if (!ledger || Array.isArray(ledger) || typeof ledger !== 'object') throw Error('invalid pushed ledger');
      this.transaction(() => { for (const [name, sha] of Object.entries(ledger)) if (/^[a-f0-9]{64}$/.test(sha))
        this.db.prepare('INSERT OR IGNORE INTO state_board_remote_files VALUES(?,?)').run(name,sha); });
    }
  }
  transaction(fn) {
    this.db.exec('BEGIN IMMEDIATE');
    try { const value = fn(); this.db.exec('COMMIT'); return value; }
    catch (error) { this.db.exec('ROLLBACK'); throw error; }
  }
  paused(name) {
    if (!STAGES.includes(name)) throw Error('unknown board pipeline stage');
    return !!this.db.prepare('SELECT paused FROM state_board_pipeline_controls WHERE name=?').get(name)?.paused;
  }
  control(name, paused) {
    this.paused(name);
    this.db.prepare(`INSERT INTO state_board_pipeline_controls VALUES(?,?,?) ON CONFLICT(name)
      DO UPDATE SET paused=excluded.paused,changed_at=excluded.changed_at`).run(name, Number(paused), new Date().toISOString());
  }
  progress(name, body) {
    this.db.prepare('INSERT OR REPLACE INTO state_board_pipeline_progress VALUES(?,?,?)').run(name,new Date().toISOString(),canon(body));
  }
  status() {
    return { stages: STAGES.map(name => ({ name, paused: this.paused(name),
      progress: this.db.prepare('SELECT at,body FROM state_board_pipeline_progress WHERE name=?').get(name) || null })),
      publication: this.db.prepare(`SELECT count(*) pending,min(created_at) oldest FROM record_board_publication_packages WHERE delivered_at IS NULL`).get(),
      stored_bytes: this.db.prepare('SELECT coalesce(sum(bytes),0) n FROM entity_board_publication_blobs').get().n, max_bytes: this.maxBytes };
  }
  collect() {
    for (const name of readdirSync(this.blobs)) if ((/^[a-f0-9]{64}$/.test(name) || /^[a-f0-9]{64}\.partial$/.test(name))
      && !this.db.prepare('SELECT 1 FROM entity_board_publication_blobs WHERE sha=?').get(name)) rmSync(join(this.blobs,name));
    const latest = this.db.prepare('SELECT max(seq) id FROM record_board_publication_packages WHERE delivered_at IS NOT NULL').get().id || 0;
    for (const row of this.db.prepare(`SELECT b.sha FROM entity_board_publication_blobs b WHERE NOT EXISTS(
      SELECT 1 FROM relation_board_publication_files f JOIN record_board_publication_packages p ON p.seq=f.package
      WHERE f.sha=b.sha AND (p.delivered_at IS NULL OR p.seq=?))`).all(latest)) {
      // New references commit only in the coordinator, which alone collects blobs.
      rmSync(join(this.blobs,row.sha), { force: true });
      this.db.prepare('DELETE FROM entity_board_publication_blobs WHERE sha=?').run(row.sha);
    }
  }
  async enqueue({ epoch, boardDir, receipt, fault = async () => {} }) {
    if (typeof epoch !== 'string' || !epoch) throw Error('publication epoch is required');
    const held = this.db.prepare('SELECT * FROM record_board_publication_packages WHERE epoch=?').get(epoch);
    if (held) return { seq: held.seq, replay: true };
    if (!existsSync(join(boardDir, 'meta.json'))) throw Error('a publication requires its completed stamp');
    this.collect();
    const manifest = [];
    let used = readdirSync(this.blobs).reduce((n,name) => n + statSync(join(this.blobs,name)).size,0);
    for (const name of files(boardDir)) {
      const path = join(boardDir,name), stat = lstatSync(path,{ bigint: true });
      if (!stat.isFile() || stat.size > BigInt(FILE_LIMIT)) throw Error('publication file exceeds the existing receiver bound: ' + name);
      const stamp = [stat.dev,stat.ino,stat.size,stat.mtimeNs,stat.ctimeNs].map(String).join(':');
      const cache = this.db.prepare('SELECT * FROM state_board_source_files WHERE name=?').get(name);
      const sha = cache?.stamp === stamp ? cache.sha : await fileHash(path), bytes = Number(stat.size), blob = join(this.blobs,sha);
      if (!existsSync(blob)) {
        const fs = statfsSync(this.root);
        if (used + bytes > this.maxBytes || fs.bavail * fs.bsize - bytes < RESERVE)
          throw Object.assign(Error('publication outbox disk bound reached; projection retained'), { code: 'PUBLICATION_BACKPRESSURE' });
        const partial = blob + '.partial'; copyFileSync(path, partial);
        if (await fileHash(partial) !== sha) throw Error('publication source changed while freezing');
        const fd = openSync(partial,'r'); try { fsyncSync(fd); } finally { closeSync(fd); }
        renameSync(partial,blob); used += bytes;
      }
      manifest.push({ name,sha,bytes });
      this.db.prepare('INSERT OR REPLACE INTO state_board_source_files VALUES(?,?,?)').run(name,stamp,sha);
    }
    syncDirectory(join(this.blobs, 'entry'));
    if(receipt?.artifact_scope==='native-board-v1') {
      const core=manifest.filter(file=>file.name!=='meta.json'&&!file.name.startsWith('heads/')).map(({name,sha})=>({name,sha}));
      if(createHash('sha256').update(canon(core)).digest('hex')!==receipt.digest)
        throw Error('publication files differ from the independent board audit');
    }
    await fault('after-publication-files-before-commit');
    const body = canon({ epoch,receipt,files: manifest });
    return this.transaction(() => {
      const seq = this.db.prepare('INSERT INTO record_board_publication_packages(epoch,manifest,created_at) VALUES(?,?,?)')
        .run(epoch,body,new Date().toISOString()).lastInsertRowid;
      for (const file of manifest) {
        this.db.prepare('INSERT OR IGNORE INTO entity_board_publication_blobs VALUES(?,?)').run(file.sha,file.bytes);
        this.db.prepare('INSERT INTO relation_board_publication_files(package,name,sha,bytes) VALUES(?,?,?,?)').run(seq,file.name,file.sha,file.bytes);
      }
      return { seq: Number(seq), files: manifest.length, manifest_sha256: createHash('sha256').update(body).digest('hex') };
    });
  }
  async deliver({ send = fetch, site = process.env.WFSIM_SITE || 'https://wfsim.app', maxFiles = 32,
    requestMs = 30000, fault = async () => {}, signal } = {}) {
    if (!Number.isSafeInteger(maxFiles) || maxFiles < 1 || !Number.isSafeInteger(requestMs) || requestMs < 1)
      throw Error('invalid publication delivery bounds');
    if (this.paused('publication')) return { paused: true };
    const lock = new DatabaseSync(join(this.root,'publisher-lock.sqlite'));
    try {
      try { lock.exec('PRAGMA busy_timeout=0;BEGIN IMMEDIATE'); }
      catch { return { busy: true }; }
      const pack = this.db.prepare('SELECT seq,epoch FROM record_board_publication_packages WHERE delivered_at IS NULL ORDER BY seq LIMIT 1').get();
      if (!pack) return { pending: false, sent: 0 };
      let sent = 0;
      for (const file of this.db.prepare(`SELECT * FROM relation_board_publication_files WHERE package=? AND served=0
        ORDER BY (name='meta.json'),name LIMIT ?`).all(pack.seq,maxFiles)) {
        signal?.throwIfAborted();
        if (this.paused('publication')) return { paused: true, sent };
        if (this.db.prepare('SELECT sha FROM state_board_remote_files WHERE name=?').get(file.name)?.sha !== file.sha) {
          const path = join(this.blobs,file.sha);
          if (statSync(path).size !== file.bytes || await fileHash(path) !== file.sha) throw Error('immutable publication artifact changed');
          const timeout = AbortSignal.timeout(requestMs), requestSignal = signal ? AbortSignal.any([timeout,signal]) : timeout;
          const response = await send(site + '/api/board/live/' + file.name, { method: 'PUT', signal: requestSignal,
            headers: { authorization: 'Bearer ' + process.env.BOARD_PUSH_TOKEN, 'content-type': 'application/json' }, body: readFileSync(path) });
          if (!response.ok) throw Error('publication upload failed: ' + file.name + ' (' + response.status + ')');
          sent++;
          await fault('after-upload-before-ack', file);
        }
        this.transaction(() => {
          this.db.prepare('INSERT OR REPLACE INTO state_board_remote_files VALUES(?,?)').run(file.name,file.sha);
          this.db.prepare('UPDATE relation_board_publication_files SET served=1 WHERE package=? AND name=?').run(pack.seq,file.name);
        });
      }
      const pending = !!this.db.prepare('SELECT 1 FROM relation_board_publication_files WHERE package=? AND served=0 LIMIT 1').get(pack.seq);
      if (!pending) this.db.prepare('UPDATE record_board_publication_packages SET delivered_at=? WHERE seq=?').run(new Date().toISOString(),pack.seq);
      const result = { epoch: pack.epoch, sent, pending };
      this.progress('publication', result); return result;
    } catch (error) { this.progress('publication',{ error: error.message }); throw error; }
    finally { if (lock.isTransaction) lock.exec('ROLLBACK'); lock.close(); }
  }
  close() { this.db.close(); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [command,work,stage] = process.argv.slice(2);
  if (!work) throw Error('usage: board_pipeline.mjs status|pause|resume|deliver <work> [stage]');
  const pipeline = new BoardPipeline(work);
  try {
    if (command === 'status') console.log(JSON.stringify(pipeline.status()));
    else if (command === 'pause' || command === 'resume') pipeline.control(stage,command === 'pause');
    else if (command === 'deliver') console.log(JSON.stringify(await pipeline.deliver()));
    else if (command === 'serve') {
      const controller = new AbortController(), stop = () => controller.abort();
      process.on('SIGTERM',stop); process.on('SIGINT',stop);
      try { while (!controller.signal.aborted) {
        try { await pipeline.deliver({ signal: controller.signal }); }
        catch (error) { if (!controller.signal.aborted) console.error('publication: ' + error.message); }
        if (!controller.signal.aborted) await new Promise(resolve => {
          const timer = setTimeout(done,1000);
          function done() { clearTimeout(timer); controller.signal.removeEventListener('abort',done); resolve(); }
          controller.signal.addEventListener('abort',done,{ once:true });
        });
      } } finally { process.off('SIGTERM',stop); process.off('SIGINT',stop); }
    }
    else throw Error('unknown board pipeline command');
  } finally { pipeline.close(); }
}
