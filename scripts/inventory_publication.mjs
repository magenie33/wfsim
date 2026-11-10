// SPDX-License-Identifier: AGPL-3.0-or-later
// Native publication decisions over a frozen disk projection; no weapon-sized arrays.
import { existsSync, statSync, readFileSync, writeFileSync, mkdirSync, renameSync, rmSync, openSync, closeSync, fsyncSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { jsonArrayRows } from './json_array_stream.mjs';
import { outputWriter, fileHash, syncDirectory } from './inventory_publication_io.mjs';

const LIMIT = 256;
const schema = `
  CREATE TABLE IF NOT EXISTS pub_files(dir TEXT NOT NULL,weapon TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(dir,weapon)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS pub_history(dir TEXT NOT NULL,weapon TEXT NOT NULL,retain INTEGER NOT NULL,sha TEXT,size INTEGER,
    PRIMARY KEY(dir,weapon)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS pub_carry(dir TEXT NOT NULL,weapon TEXT NOT NULL,ordinal INTEGER NOT NULL,page TEXT NOT NULL,
    benchmark TEXT,mode TEXT,riven INTEGER,score REAL,PRIMARY KEY(dir,weapon,ordinal)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS pub_rows(seq INTEGER PRIMARY KEY,ruler INTEGER NOT NULL,identity TEXT NOT NULL,mode TEXT NOT NULL,
    mode_order INTEGER NOT NULL,source INTEGER NOT NULL,shape TEXT NOT NULL,floor TEXT NOT NULL,def INTEGER NOT NULL,
    score REAL NOT NULL,riven INTEGER NOT NULL,page TEXT NOT NULL,UNIQUE(ruler,identity,mode));
  CREATE INDEX IF NOT EXISTS pub_rows_order ON pub_rows(ruler,(-score),(-def),source,mode_order,seq);
  CREATE TABLE IF NOT EXISTS pub_shapes(seq INTEGER PRIMARY KEY,ruler INTEGER NOT NULL,shape TEXT NOT NULL,row_id INTEGER NOT NULL,
    score REAL NOT NULL,def INTEGER NOT NULL,source INTEGER NOT NULL,mode_order INTEGER NOT NULL,UNIQUE(ruler,shape));
  CREATE TABLE IF NOT EXISTS pub_max(ruler INTEGER NOT NULL,floor TEXT NOT NULL,score REAL NOT NULL,PRIMARY KEY(ruler,floor)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS pub_final(row_id INTEGER PRIMARY KEY,keep INTEGER NOT NULL);
  CREATE TABLE IF NOT EXISTS pub_leaders(dir TEXT NOT NULL,weapon TEXT NOT NULL,benchmark TEXT NOT NULL,mode TEXT NOT NULL,
    riven INTEGER NOT NULL,score REAL NOT NULL,page TEXT NOT NULL,PRIMARY KEY(dir,weapon,benchmark,mode,riven)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS pub_listed(ruler INTEGER NOT NULL,weapon TEXT NOT NULL,n INTEGER NOT NULL,PRIMARY KEY(ruler,weapon)) WITHOUT ROWID;
`;
const number = value => value == null ? -Infinity : Number(value);
const order = row => [row.ruler, -row.score, -row.def, row.source, row.mode_order, row.seq];
const startOrder = [-1, 0, 0, 0, 0, 0];
const safeName = value => typeof value === 'string' && /^[a-z0-9_]{1,80}$/.test(value);

export class InventoryPublication {
  constructor(index, work, context = {}, fault = () => {}) {
    this.index = index; this.catalog = index.catalog; this.q = sql => index.q(sql);
    this.work = work; this.context = context; this.fault = fault;
    this.root = join(work, 'verified'); this.statePath = join(work, 'pub/data/board_state.yaml');
    this.catalog.db.exec(schema);
    this.catalog.db.exec('CREATE INDEX IF NOT EXISTS rel_publication_facts_rows ON rel_publication_facts(ruler,retired,identity,mode);');
  }
  save(file, state) {
    this.q('UPDATE pub_files SET body=? WHERE dir=? AND weapon=?').run(JSON.stringify(state), file.dir, file.weapon);
    file.state = state;
  }
  compatibility(key) {
    if (this.catalog.meta.publication_compat === key) return false;
    const initial = !this.catalog.meta.publication && !this.catalog.meta.publication_compat;
    this.context.writer?.close(); this.context.writer = null; this.context.legacyPath = null;
    this.index.transaction(() => {
      if (!initial) {
        for (const table of ['pub_files','pub_rows','pub_shapes','pub_max','pub_final','pub_leaders','pub_listed']) this.q(`DELETE FROM ${table}`).run();
        this.q(`INSERT OR IGNORE INTO rel_publication_dirty SELECT DISTINCT weapon FROM rel_publication_sources
          UNION SELECT weapon FROM pub_history`).run();
      }
      this.catalog.setMeta({ ...this.catalog.meta, publication_compat: key,
        ...(!initial ? { publication: null, publication_refresh: { key, forceHeads: true } } : {}) });
    });
    return !initial;
  }
  leader(file, page, metadata) {
    if (!metadata) return;
    const score = number(metadata.score);
    this.q(`INSERT INTO pub_leaders VALUES(?,?,?,?,?,?,?) ON CONFLICT(dir,weapon,benchmark,mode,riven)
      DO UPDATE SET score=excluded.score,page=excluded.page WHERE excluded.score>pub_leaders.score`)
      .run(file.dir, file.weapon, metadata.benchmark, metadata.mode, Number(metadata.riven), score, page);
  }
  begin(receipt) {
    const round = receipt.projectionId ?? receipt.epoch;
    if (typeof round !== 'string' || !round) throw Error('publication round identity required');
    const prior = this.catalog.meta.publication;
    if (prior?.round === round) return;
    if (prior && prior.phase !== 'complete') throw Error('a board publication round is unfinished');
    this.index.transaction(() => {
      this.q('DELETE FROM pub_files').run();
      const dirs = [...new Set(this.index.contract.rulers.map(r => r.scene))];
      for (const { weapon } of this.q('SELECT weapon FROM rel_publication_dirty ORDER BY weapon').iterate()) {
        if (!safeName(weapon)) throw Error('invalid publication weapon');
        for (const dir of dirs) {
          if (dir && !safeName(dir)) throw Error('invalid publication directory');
          this.q('INSERT INTO pub_files VALUES(?,?,?)').run(dir, weapon, JSON.stringify({ phase: 'start' }));
        }
      }
      this.catalog.setMeta({ ...this.catalog.meta, publication: { round, phase: 'files', index: 0 } });
    });
  }
  async initialize(file) {
    const previous = this.q('SELECT retain FROM pub_history WHERE dir=? AND weapon=?').get(file.dir, file.weapon);
    const path = join(this.root, file.dir, file.weapon + '.json');
    const original = !previous && existsSync(path) ? { size: statSync(path).size, sha: await fileHash(path) } : null;
    this.index.transaction(() => {
      for (const table of ['pub_rows','pub_shapes','pub_max','pub_final']) this.q(`DELETE FROM ${table}`).run();
      this.q('DELETE FROM pub_leaders WHERE dir=? AND weapon=?').run(file.dir, file.weapon);
      for (let r = 0; r < this.index.contract.rulers.length; r++) if (this.index.contract.rulers[r].scene === file.dir)
        this.q('DELETE FROM pub_listed WHERE ruler=? AND weapon=?').run(r, file.weapon);
      this.save(file, { phase: previous ? 'rows' : 'legacy', legacyOffset: 0, legacyOrdinal: 0,
        retain: previous?.retain ?? Number(existsSync(path)), target: 0, input: ['', '', -1, -1],
        cursor: 0, written: 0, size: 0, sha: null, original });
    });
  }
  async legacy(file, operate) {
    const path = join(this.root, file.dir, file.weapon + '.json'), state = file.state;
    if (!state.original) { this.save(file, { ...state, phase: 'rows' }); return; }
    if (statSync(path).size !== state.original.size) throw Error('carried publication source changed');
    if (this.context.legacyPath !== path) {
      if (await fileHash(path) !== state.original.sha) throw Error('carried publication source changed');
      this.context.legacyPath = path;
    }
    const rows = [];
    for await (const row of jsonArrayRows(path, { offset: state.legacyOffset, withOffsets: true })) {
      rows.push(row); if (rows.length === LIMIT) break;
    }
    const values = await operate('carry', rows.map(row => ({ dir: file.dir, raw: row.raw })));
    this.index.transaction(() => {
      for (let i = 0; i < rows.length; i++) if (values[i].keep) {
        const metadata = values[i].index;
        this.q('INSERT OR REPLACE INTO pub_carry VALUES(?,?,?,?,?,?,?,?)')
          .run(file.dir, file.weapon, state.legacyOrdinal + i, rows[i].raw, metadata?.benchmark ?? null,
            metadata?.mode ?? null, metadata ? Number(metadata.riven) : null, metadata ? number(metadata.score) : null);
      }
      const done = !rows.length || rows.at(-1).done;
      this.save(file, { ...state, phase: done ? 'rows' : 'legacy', legacyOffset: rows.at(-1)?.offset ?? state.legacyOffset,
        legacyOrdinal: state.legacyOrdinal + rows.length });
    });
  }
  async rows(file, operate) {
    const state = file.state, rulers = this.index.contract.rulers;
    let target = state.target;
    while (target < rulers.length && rulers[target].scene !== file.dir) target++;
    if (target === rulers.length) { this.save(file, { ...state, phase: 'max', cursor: 0 }); return; }
    const bit = BigInt.asIntN(64, 1n << BigInt(target));
    const empty = this.q(`SELECT 1 FROM rel_publication_sources WHERE weapon=? AND (admit & ?)<>0 AND json_array_length(modes)=0 LIMIT 1`)
      .get(file.weapon, bit);
    if (empty) throw Error('admitted publication source has no mode');
    const page = this.q(`SELECT f.identity,f.mode fact_mode,f.body fact,s.seq source,s.id owner,d.body descriptor,
        CAST(m.key AS INTEGER) mode_order,m.value mode
      FROM rel_publication_facts f INDEXED BY rel_publication_facts_rows
      JOIN rel_publication_sources s ON s.candidate=f.identity AND s.weapon=? AND (s.admit & ?)<>0
      JOIN rel_sources d ON d.id=s.id JOIN json_each(s.modes) m ON json_extract(m.value,'$.id')=f.mode
      WHERE f.ruler=? AND f.retired=0 AND (f.identity,f.mode,s.seq,CAST(m.key AS INTEGER))>(?,?,?,?)
      ORDER BY f.identity,f.mode,s.seq,CAST(m.key AS INTEGER) LIMIT ?`)
      .all(file.weapon, bit, rulers[target].id, ...state.input, LIMIT);
    const pending = page.filter(row => !this.q('SELECT 1 FROM pub_rows WHERE ruler=? AND identity=? AND mode=?')
      .get(target, row.identity, JSON.parse(row.mode).mode));
    const seen = new Set(), unique = pending.filter(row => {
      const key = JSON.stringify([row.identity, row.fact_mode]); if (seen.has(key)) return false; seen.add(key); return true;
    });
    const rendered = await operate('render', unique.map(row => {
      const d = JSON.parse(row.descriptor), mode = JSON.parse(row.mode);
      return { benchmark: rulers[target].id, mode: mode.mode, row: { ...d.record, ...d.publication }, fact: JSON.parse(row.fact) };
    }));
    this.index.transaction(() => {
      for (let i = 0; i < unique.length; i++) {
        const row = unique[i], value = rendered[i], mode = JSON.parse(row.mode);
        const result = this.q('INSERT INTO pub_rows(ruler,identity,mode,mode_order,source,shape,floor,def,score,riven,page) VALUES(?,?,?,?,?,?,?,?,?,?,?)')
          .run(target, row.identity, mode.mode, row.mode_order, row.source, value.shape, value.floor, Number(value.default), value.score, Number(value.riven), value.page);
        this.q(`INSERT INTO pub_shapes(ruler,shape,row_id,score,def,source,mode_order) VALUES(?,?,?,?,?,?,?)
          ON CONFLICT(ruler,shape) DO UPDATE SET row_id=excluded.row_id,score=excluded.score,def=excluded.def,source=excluded.source,mode_order=excluded.mode_order
          WHERE (excluded.score,excluded.def,-excluded.source,-excluded.mode_order)>(pub_shapes.score,pub_shapes.def,-pub_shapes.source,-pub_shapes.mode_order)`)
          .run(target, value.shape, result.lastInsertRowid, value.score, Number(value.default), row.source, row.mode_order);
      }
      const last = page.at(-1), done = page.length < LIMIT;
      this.save(file, { ...state, target: done ? target + 1 : target,
        input: done ? ['', '', -1, -1] : [last.identity, last.fact_mode, last.source, last.mode_order] });
    });
  }
  maxima(file) {
    const page = this.q(`SELECT s.seq,r.ruler,r.floor,r.score FROM pub_shapes s JOIN pub_rows r ON r.seq=s.row_id
      WHERE s.seq>? ORDER BY s.seq LIMIT ?`).all(file.state.cursor, LIMIT);
    this.index.transaction(() => {
      for (const row of page) this.q(`INSERT INTO pub_max VALUES(?,?,?) ON CONFLICT(ruler,floor)
        DO UPDATE SET score=max(pub_max.score,excluded.score)`).run(row.ruler, row.floor, row.score);
      this.save(file, { ...file.state, cursor: page.at(-1)?.seq ?? file.state.cursor,
        ...(page.length < LIMIT ? { phase: 'floor', cursor: 0 } : {}) });
    });
  }
  async floor(file, operate) {
    const page = this.q(`SELECT s.seq,r.seq row_id,r.score,m.score leader FROM pub_shapes s JOIN pub_rows r ON r.seq=s.row_id
      JOIN pub_max m ON m.ruler=r.ruler AND m.floor=r.floor WHERE s.seq>? ORDER BY s.seq LIMIT ?`).all(file.state.cursor, LIMIT);
    const decisions = await operate('floor', page.map(row => ({ score: row.score, leader: row.leader })));
    this.index.transaction(() => {
      for (let i = 0; i < page.length; i++) this.q('INSERT OR REPLACE INTO pub_final VALUES(?,?)').run(page[i].row_id, Number(decisions[i].keep));
      if (page.length < LIMIT && this.q('SELECT count(*) n FROM pub_shapes').get().n !== this.q('SELECT count(*) n FROM pub_final').get().n)
        throw Error('publication winner accounting mismatch');
      this.save(file, { ...file.state, cursor: page.at(-1)?.seq ?? file.state.cursor,
        ...(page.length < LIMIT ? { phase: 'write-legacy', cursor: -1, output: startOrder } : {}) });
    });
  }
  async write(file) {
    const state = file.state, path = join(this.root, file.dir, file.weapon + '.json');
    const writer = await outputWriter(this.context, path, state);
    if (writer.renamed) throw Error('publication file completion receipt is inconsistent');
    if (!state.size) writer.append('[');
    const legacy = state.phase === 'write-legacy';
    const page = legacy ? this.q('SELECT * FROM pub_carry WHERE dir=? AND weapon=? AND ordinal>? ORDER BY ordinal LIMIT ?')
      .all(file.dir, file.weapon, state.cursor, LIMIT)
      : this.q(`SELECT r.* FROM pub_rows r INDEXED BY pub_rows_order JOIN pub_final f ON f.row_id=r.seq AND f.keep=1
        WHERE (r.ruler,-r.score,-r.def,r.source,r.mode_order,r.seq)>(?,?,?,?,?,?)
        ORDER BY r.ruler,-r.score,-r.def,r.source,r.mode_order,r.seq LIMIT ?`).all(...state.output, LIMIT);
    let written = state.written;
    for (const row of page) { writer.append((written++ ? ',' : '') + row.page); }
    const receipt = writer.checkpoint(); await this.fault('after-output-before-checkpoint', { file, receipt });
    this.index.transaction(() => {
      for (const row of page) {
        this.leader(file, row.page, legacy ? row.benchmark == null ? null : row : {
          benchmark: this.index.contract.rulers[row.ruler].id, mode: row.mode, riven: row.riven, score: row.score,
        });
        if (!legacy) this.q(`INSERT INTO pub_listed VALUES(?,?,1) ON CONFLICT(ruler,weapon) DO UPDATE SET n=n+1`).run(row.ruler, file.weapon);
      }
      this.save(file, { ...state, ...receipt, written, cursor: legacy ? page.at(-1)?.ordinal ?? state.cursor : state.cursor,
        output: !legacy && page.length ? order(page.at(-1)) : state.output,
        ...(page.length < LIMIT ? { phase: legacy ? 'write-current' : 'close' } : {}) });
    });
  }
  async close(file) {
    const state = file.state, path = join(this.root, file.dir, file.weapon + '.json');
    const removed = state.closed && !state.written && !state.retain && !existsSync(path) && !existsSync(path + '.inventory-partial');
    const writer = removed ? { renamed: true, size: state.size, sha: state.sha } : await outputWriter(this.context, path, state);
    if (!state.closed) {
      if (writer.renamed) throw Error('publication file was renamed before completion');
      writer.append(']'); const receipt = writer.checkpoint();
      this.save(file, { ...state, ...receipt, closed: true });
    }
    const receipt = writer.renamed ? { size: writer.size, sha: writer.sha } : await writer.finish();
    if (!state.written && !state.retain) rmSync(path, { force: true });
    await this.fault('after-file-before-checkpoint', { file, receipt });
    this.index.transaction(() => {
      this.q('INSERT OR REPLACE INTO pub_history VALUES(?,?,?,?,?)').run(file.dir, file.weapon, state.retain, receipt.sha, receipt.size);
      this.save(file, { ...file.state, phase: 'complete' });
    });
  }
  async indexes(operate) {
    const publication = this.catalog.meta.publication;
    const dirs = [...new Set(this.index.contract.rulers.map(r => r.scene))].sort();
    const dir = dirs[publication.index];
    if (dir === undefined) {
      mkdirSync(dirname(this.statePath), { recursive: true });
      const partial = this.statePath + '.inventory-partial';
      writeFileSync(partial, existsSync(this.statePath) ? readFileSync(this.statePath) : 'boards: {}\n');
      const seen = this.q('SELECT count(*) n FROM rel_sources').get().n;
      const boards = this.index.contract.rulers.map((r, i) => [r.id, seen, this.q('SELECT coalesce(sum(n),0) n FROM pub_listed WHERE ruler=?').get(i).n]);
      await operate('state', [{ path: partial, boards }]);
      const fd = openSync(partial, 'r'); try { fsyncSync(fd); } finally { closeSync(fd); }
      renameSync(partial, this.statePath);
      syncDirectory(this.statePath);
      await this.fault('after-state-before-checkpoint', {});
      this.index.transaction(() => {
        this.q('DELETE FROM rel_publication_dirty').run();
        this.catalog.setMeta({ ...this.catalog.meta, publication: { ...publication, phase: 'complete' } });
      });
      return;
    }
    const path = join(this.root, dir, 'index.json');
    if (publication.indexClosing) {
      const writer = await outputWriter(this.context, path, publication.indexClosing);
      if (!writer.renamed) await writer.finish();
      await this.fault('after-index-before-checkpoint', { path });
      this.catalog.setMeta({ ...this.catalog.meta, publication: { round: publication.round, phase: 'files', index: publication.index + 1 } });
      return;
    }
    const output = publication.output || { size: 0, sha: null, weapon: '', key: null, rows: 0, open: false };
    const writer = await outputWriter(this.context, path, output);
    if (writer.renamed) throw Error('publication index completion receipt is inconsistent');
    if (!output.size) writer.append('{');
    const rows = this.q(`SELECT * FROM pub_leaders WHERE dir=? AND (weapon,benchmark,mode,riven)>(?,?,?,?)
      ORDER BY weapon,benchmark,mode,riven LIMIT ?`).all(dir, output.weapon, ...(output.key || ['', '', -1]), LIMIT);
    let weapon = output.weapon, open = output.open, count = output.rows;
    for (const row of rows) {
      if (row.weapon !== weapon) {
        if (open) writer.append(']');
        writer.append((open ? ',' : '') + JSON.stringify(row.weapon) + ':['); weapon = row.weapon; open = true; count = 0;
      }
      writer.append((count++ ? ',' : '') + row.page);
    }
    if (rows.length < LIMIT) {
      if (open) writer.append(']'); writer.append('}');
      const receipt = writer.checkpoint();
      this.catalog.setMeta({ ...this.catalog.meta, publication: { ...publication, indexClosing: receipt } });
      await writer.finish();
      await this.fault('after-index-before-checkpoint', { path });
      this.catalog.setMeta({ ...this.catalog.meta, publication: { ...publication, index: publication.index + 1 } });
    } else {
      const receipt = writer.checkpoint(), last = rows.at(-1);
      this.catalog.setMeta({ ...this.catalog.meta, publication: { ...publication,
        output: { ...receipt, weapon, key: [last.benchmark, last.mode, last.riven], rows: count, open } } });
    }
  }
  async step(receipt, operate, { budgetMs = 1000 } = {}) {
    this.begin(receipt);
    const until = performance.now() + budgetMs;
    do {
      const raw = this.q("SELECT * FROM pub_files WHERE json_extract(body,'$.phase')<>'complete' ORDER BY dir,weapon LIMIT 1").get();
      if (!raw) {
        if (this.catalog.meta.publication.phase === 'complete') return { pending: false };
        await this.indexes(operate);
      } else {
        const file = { ...raw, state: JSON.parse(raw.body) };
        if (file.state.phase === 'start') await this.initialize(file);
        else if (file.state.phase === 'legacy') await this.legacy(file, operate);
        else if (file.state.phase === 'rows') await this.rows(file, operate);
        else if (file.state.phase === 'max') this.maxima(file);
        else if (file.state.phase === 'floor') await this.floor(file, operate);
        else if (file.state.phase.startsWith('write-')) await this.write(file);
        else if (file.state.phase === 'close') await this.close(file);
        else throw Error('unknown board publication phase');
      }
    } while (performance.now() < until);
    return { pending: this.catalog.meta.publication.phase !== 'complete' };
  }
}
