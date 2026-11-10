// SPDX-License-Identifier: AGPL-3.0-or-later
// The legacy coordinator keeps an identity-idempotent, fsynced arrival mailbox.
import { DatabaseSync } from 'node:sqlite';
import { existsSync, openSync, closeSync, readSync, writeSync, ftruncateSync, fsyncSync, statSync, renameSync, mkdirSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { createReadStream } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { syncDirectory } from './inventory_publication_io.mjs';

export class LiveMailbox {
  constructor(work, fault = async () => {}) {
    this.work = resolve(work); mkdirSync(this.work, { recursive: true }); this.fault = fault;
    this.mail = join(this.work, 'arrived-builds.ndjson'); this.now = join(this.work, 'arrived-now.ndjson'); this.builds = join(this.work, 'new-builds.ndjson');
    this.db = new DatabaseSync(join(this.work, 'live-mailbox.sqlite'));
    this.db.exec(`PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;PRAGMA temp_store=FILE;
      CREATE TABLE IF NOT EXISTS seen(id TEXT PRIMARY KEY) WITHOUT ROWID;
      CREATE TABLE IF NOT EXISTS intent(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);`);
  }
  close() { this.db.close(); }
  intent(value) {
    if (value) this.db.prepare('INSERT OR REPLACE INTO intent VALUES(1,?)').run(JSON.stringify(value));
    else this.db.exec('DELETE FROM intent');
  }
  async recover() {
    const held = this.db.prepare('SELECT body FROM intent WHERE id=1').get(); if (!held) return;
    const value = JSON.parse(held.body);
    if (value.op === 'append') {
      const fd = openSync(this.mail, 'a+');
      try {
        if (statSync(this.mail).size < value.offset) throw Error('durable legacy mailbox prefix is truncated');
        ftruncateSync(fd, value.offset); const bytes = Buffer.from(value.body); let offset = 0;
        while (offset < bytes.length) offset += writeSync(fd, bytes, offset);
        fsyncSync(fd); syncDirectory(this.mail);
      } finally { closeSync(fd); }
      await this.fault('after-mailbox-before-ack', value);
      this.db.exec('BEGIN IMMEDIATE');
      try {
        for (const id of value.ids) this.db.prepare('INSERT OR IGNORE INTO seen VALUES(?)').run(id);
        this.intent(null); this.db.exec('COMMIT');
      } catch (error) { if (this.db.isTransaction) this.db.exec('ROLLBACK'); throw error; }
    } else if (value.op === 'take') {
      const matches = path => existsSync(path) && statSync(path).ino === value.ino && statSync(path).dev === value.dev && statSync(path).size === value.size;
      if (matches(this.mail)) renameSync(this.mail, this.now);
      else if (!matches(this.now)) throw Error('durable legacy mailbox take source changed');
      const empty = openSync(this.mail, 'a'); try { fsyncSync(empty); } finally { closeSync(empty); }
      syncDirectory(this.now);
      await this.fault('after-take-rename', value);
      const input = openSync(this.now, 'r'), output = openSync(this.builds, 'a+'), buffer = Buffer.allocUnsafe(65536);
      try {
        if (statSync(this.builds).size < value.offset) throw Error('durable legacy build spool prefix is truncated');
        ftruncateSync(output, value.offset); let n;
        while ((n = readSync(input, buffer, 0, buffer.length, null))) {
          let offset = 0; while (offset < n) offset += writeSync(output, buffer, offset, n - offset);
        }
        fsyncSync(output); syncDirectory(this.builds);
      } finally { closeSync(input); closeSync(output); }
      await this.fault('after-take-spool-before-ack', value); this.intent(null);
    } else throw Error('invalid legacy mailbox journal');
    await this.fault('after-mailbox-commit-before-reply', value);
  }
  async append(file) {
    await this.recover();
    let rows = [], ids = new Set(), bytes = 0;
    const flush = async () => {
      if (!rows.length) return;
      this.intent({ op: 'append', offset: existsSync(this.mail) ? statSync(this.mail).size : 0, body: rows.join(''), ids: [...ids] });
      await this.fault('after-mailbox-intent', {}); await this.recover(); rows = []; ids = new Set(); bytes = 0;
    };
    const input = createReadStream(file), lines = createInterface({ input, crlfDelay: Infinity });
    try {
      for await (const line of lines) if (line.trim()) {
        const row = JSON.parse(line);
        if (row.record?.kind != null || this.db.prepare('SELECT 1 FROM seen WHERE id=?').get(row.id) || ids.has(row.id)) continue;
        if (typeof row.id !== 'string' || !row.id) throw Error('legacy mailbox build identity required');
        const body = JSON.stringify(row) + '\n', size = Buffer.byteLength(body);
        if (size > 1024 ** 2) throw Error('legacy mailbox build exceeds byte bound');
        if (rows.length && (rows.length >= 100 || bytes + size > 1024 ** 2)) await flush();
        rows.push(body); ids.add(row.id); bytes += size;
      }
    } finally { lines.close(); input.destroy(); }
    await flush();
  }
  async take() {
    await this.recover(); const fd = openSync(this.mail, 'a'); closeSync(fd);
    const source = statSync(this.mail);
    this.intent({ op: 'take', ino: source.ino, dev: source.dev, size: source.size, offset: existsSync(this.builds) ? statSync(this.builds).size : 0 });
    await this.fault('after-take-intent', {}); await this.recover();
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [op, work, file] = process.argv.slice(2);
  if (!['append','take','recover'].includes(op) || !work || op === 'append' && !file) throw Error('usage: live_mailbox.mjs append|take|recover <work> [builds-file]');
  const mailbox = new LiveMailbox(work);
  try { if (op === 'append') await mailbox.append(file); else if (op === 'take') await mailbox.take(); else await mailbox.recover(); }
  finally { mailbox.close(); }
}
