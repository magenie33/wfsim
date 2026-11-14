// SPDX-License-Identifier: AGPL-3.0-or-later
// Indexed admission state; the engine owns normalization and the final gate.
import { canon } from '../worker/task_contract.js';

const schema = `
  CREATE TABLE IF NOT EXISTS entity_catalog_sources(seq INTEGER PRIMARY KEY,id TEXT NOT NULL UNIQUE,candidate TEXT,body TEXT NOT NULL);
  CREATE INDEX IF NOT EXISTS rel_sources_candidate ON entity_catalog_sources(candidate,seq);
  CREATE TABLE IF NOT EXISTS entity_catalog_candidates(id TEXT PRIMARY KEY,bucket TEXT NOT NULL,admit INTEGER NOT NULL,default_id TEXT,body TEXT NOT NULL) WITHOUT ROWID;
  CREATE INDEX IF NOT EXISTS rel_candidates_default ON entity_catalog_candidates(default_id,id);
  CREATE TABLE IF NOT EXISTS state_catalog_measurements(identity TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,score REAL NOT NULL,
    scene TEXT NOT NULL,bucket TEXT,at TEXT NOT NULL,retired INTEGER NOT NULL,gate_ruler TEXT NOT NULL,PRIMARY KEY(identity,ruler,mode)) WITHOUT ROWID;
  CREATE INDEX IF NOT EXISTS rel_facts_score ON state_catalog_measurements(bucket,gate_ruler,mode,score,identity) WHERE retired=0;
  CREATE TABLE IF NOT EXISTS state_catalog_pending_work(identity TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,
    PRIMARY KEY(identity,ruler,mode)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_catalog_leaders(bucket TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,target INTEGER NOT NULL,score REAL,
    PRIMARY KEY(bucket,ruler,mode,target)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_catalog_groups(bucket TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,
    PRIMARY KEY(bucket,ruler,mode)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_catalog_ranges(bucket TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,target INTEGER NOT NULL,
    low REAL NOT NULL,high REAL NOT NULL,after_score REAL NOT NULL,after_id TEXT NOT NULL,
    PRIMARY KEY(bucket,ruler,mode,target)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_catalog_jobs(id TEXT PRIMARY KEY) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_catalog_contract(id INTEGER PRIMARY KEY CHECK(id=1),body TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS state_catalog_publication_sources(id TEXT PRIMARY KEY,seq INTEGER NOT NULL UNIQUE,candidate TEXT NOT NULL,
    weapon TEXT NOT NULL,admit INTEGER NOT NULL,modes TEXT NOT NULL) WITHOUT ROWID;
  CREATE INDEX IF NOT EXISTS rel_publication_sources_rows ON state_catalog_publication_sources(weapon,candidate,seq);
  CREATE TABLE IF NOT EXISTS state_catalog_publication_measurements(identity TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,
    at TEXT NOT NULL,retired INTEGER NOT NULL,body TEXT NOT NULL,PRIMARY KEY(identity,ruler,mode)) WITHOUT ROWID;
  CREATE TABLE IF NOT EXISTS state_catalog_publication_dirty(weapon TEXT PRIMARY KEY) WITHOUT ROWID;
`;
const mask = hex => BigInt.asIntN(64, BigInt(`0x${hex}`));
const bit = target => BigInt.asIntN(64, 1n << BigInt(target));
const allowed = (hex, target) => (mask(hex) & bit(target)) !== 0n;

export class InventoryRelations {
  constructor(catalog, contract) {
    if (contract.v !== 1 || !Number.isFinite(contract.entry_share) || contract.entry_share <= 0
        || !Array.isArray(contract.rulers) || !contract.rulers.length || contract.rulers.length > 64
        || new Set(contract.rulers.map(r => r.id)).size !== contract.rulers.length) throw Error('invalid relational engine contract');
    this.catalog = catalog; this.db = catalog.db; this.contract = contract;
    this.rulers = new Map(contract.rulers.map((r, i) => [r.id, { ...r, target: i }]));
    this.db.exec(schema);
    const held = this.q("SELECT body FROM state_catalog_contract WHERE id=1").get();
    if (held && held.body !== canon(contract)) throw Error('relational engine contract changed');
    if (!held) this.q("INSERT INTO state_catalog_contract VALUES(1,?)").run(canon(contract));
    this.metrics = { normalized: 0, groups: 0, leaders: 0, range_rows: 0, candidates: 0, outputs: 0 };
  }
  q(sql) { return this.catalog.statement(sql); }
  transaction(fn) { return this.catalog.transaction(fn); }
  publicationDirty(weapon) {
    if (weapon) this.q('INSERT OR IGNORE INTO state_catalog_publication_dirty VALUES(?)').run(weapon);
  }
  dirty(id) {
    this.q("INSERT OR IGNORE INTO state_catalog_jobs VALUES(?)").run(id);
    this.q("INSERT OR IGNORE INTO state_catalog_jobs SELECT id FROM entity_catalog_candidates WHERE default_id=?").run(id);
  }
  touchGroups(id) {
    this.q(`INSERT OR IGNORE INTO state_catalog_groups SELECT DISTINCT bucket,gate_ruler,mode FROM state_catalog_measurements
      WHERE identity=? AND bucket IS NOT NULL`).run(id);
  }
  candidate(id) {
    const old = this.q("SELECT body FROM entity_catalog_candidates WHERE id=?").get(id)?.body;
    let first = null, admit = 0n;
    for (const row of this.q("SELECT body FROM entity_catalog_sources WHERE candidate=? ORDER BY seq").iterate(id)) {
      const body = JSON.parse(this.catalog.bodyText(row.body)); first ||= body; admit |= mask(body.admit);
      if (canon(body.bucket) !== canon(first.bucket) || body.default !== first.default)
        throw Error('canonical identity has conflicting engine dependencies');
    }
    const next = first ? { bucket:first.bucket,default:first.default,
      ...(first.record_ref ? { record_ref:first.record_ref } : { record:first.record }),
      admit: BigInt.asUintN(64, admit).toString(16).padStart(16, '0') } : null;
    // Owner aliases do not invalidate their canonical calculation when its inputs agree.
    const body = next ? canon(next) : null;
    if (body === old || body === null && old === undefined) return;
    this.touchGroups(id);
    if (next) {
      this.q(`INSERT INTO entity_catalog_candidates VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET
        bucket=excluded.bucket,admit=excluded.admit,default_id=excluded.default_id,body=excluded.body`)
        .run(id, canon(next.bucket), admit, next.default, body);
      this.q("UPDATE state_catalog_measurements SET bucket=? WHERE identity=?").run(canon(next.bucket), id);
    } else {
      this.q("DELETE FROM entity_catalog_candidates WHERE id=?").run(id);
      this.q("UPDATE state_catalog_measurements SET bucket=NULL WHERE identity=?").run(id);
    }
    this.touchGroups(id); this.dirty(id);
  }
  async sources(rows, operate) {
    if (rows.length > 256) throw Error('relational source batch exceeds bound');
    const changed = rows.filter(row => {
      const held = this.catalog.recordBody(this.q("SELECT stored FROM entity_inventory_builds WHERE build_id=?").get(row.id)?.stored);
      return held !== (row.deleted || row.record == null ? null : canon(row.record));
    });
    if (!changed.length) return;
    const descriptions = await operate('describe', changed.map(row => ({ id: row.id, record: row.deleted ? null : row.record })));
    if (descriptions.length !== changed.length || descriptions.some((d, i) => d.id !== changed[i].id)) throw Error('incomplete engine descriptions');
    this.transaction(() => {
      const candidates = new Set();
      for (let i = 0; i < changed.length; i++) {
        const row = changed[i], d = { ...descriptions[i] }, old = this.q('SELECT candidate FROM entity_catalog_sources WHERE id=?').get(row.id);
        if (d.record) { d.record_ref = this.catalog.internRecord(d.record); delete d.record; }
        const descriptor = { ...d }; delete descriptor.id; delete descriptor.candidate; delete descriptor.publication_modes;
        this.publicationDirty(this.q('SELECT weapon FROM state_catalog_publication_sources WHERE id=?').get(row.id)?.weapon);
        if (old?.candidate) candidates.add(old.candidate);
        this.q(`INSERT INTO entity_inventory_builds(build_id,stored) VALUES(?,?) ON CONFLICT(build_id)
          DO UPDATE SET stored=excluded.stored,fallback=NULL`).run(row.id, row.deleted || row.record == null ? null : this.catalog.internRecord(row.record));
        if (row.deleted || row.record == null) this.q("DELETE FROM entity_catalog_sources WHERE id=?").run(row.id);
        else this.q(`INSERT INTO entity_catalog_sources(id,candidate,body) VALUES(?,?,?) ON CONFLICT(id)
          DO UPDATE SET candidate=excluded.candidate,body=excluded.body`).run(row.id, d.candidate, this.catalog.jsonBody(descriptor));
        this.q('DELETE FROM state_catalog_publication_sources WHERE id=?').run(row.id);
        if (!row.deleted && row.record != null && d.candidate && d.publication) {
          const seq = this.q('SELECT seq FROM entity_catalog_sources WHERE id=?').get(row.id).seq;
          this.q('INSERT INTO state_catalog_publication_sources VALUES(?,?,?,?,?,?)')
            .run(row.id, seq, d.candidate, d.bucket[0], mask(d.admit), canon(d.publication_modes));
          this.publicationDirty(d.bucket[0]);
        }
        if (d.candidate) candidates.add(d.candidate);
        // A stored canonical record takes precedence over all normalized aliases.
        this.dirty(row.id);
      }
      for (const id of candidates) { this.candidate(id); this.dirty(id); }
    });
    this.metrics.normalized += changed.length;
  }
  async facts(rows, operate, { replace = [] } = {}) {
    if (rows.length > 256 || replace.length > 256) throw Error('relational fact batch exceeds bound');
    const normalized = await operate('facts', rows);
    if (normalized.length !== rows.length) throw Error('incomplete engine fact keys');
    this.transaction(() => {
      for (const id of replace) {
        this.touchGroups(id); this.q('DELETE FROM state_catalog_measurements WHERE identity=?').run(id);
        this.q('DELETE FROM state_catalog_publication_measurements WHERE identity=?').run(id);
        for (const row of this.q('SELECT DISTINCT weapon FROM state_catalog_publication_sources WHERE candidate=?').iterate(id)) this.publicationDirty(row.weapon);
        this.dirty(id);
      }
      for (const row of normalized) {
        const score = Number(row.score);
        if (!row.mode || !Number.isFinite(score)) continue;
        const at = row.retired_at ?? row.finished_at ?? '', retired = Number(row.retired_at != null);
        const publication = this.q('SELECT at,retired FROM state_catalog_publication_measurements WHERE identity=? AND ruler=? AND mode=?')
          .get(row.identity, row.ruler, row.mode);
        if (!publication || publication.at < at || publication.at === at && (!publication.retired || retired)) {
          this.q(`INSERT INTO state_catalog_publication_measurements VALUES(?,?,?,?,?,?) ON CONFLICT(identity,ruler,mode) DO UPDATE SET
            at=excluded.at,retired=excluded.retired,body=excluded.body`)
            .run(row.identity, row.ruler, row.mode, at, retired, this.catalog.jsonBody(row));
          for (const source of this.q('SELECT DISTINCT weapon FROM state_catalog_publication_sources WHERE candidate=?').iterate(row.identity)) this.publicationDirty(source.weapon);
        }
        if (!row.gate_ruler) continue;
        const prior = this.q('SELECT * FROM state_catalog_measurements WHERE identity=? AND ruler=? AND mode=?').get(row.identity, row.ruler, row.mode);
        if (prior && (prior.at > at || prior.at === at && prior.retired && !retired)) continue;
        const bucket = this.q("SELECT bucket FROM entity_catalog_candidates WHERE id=?").get(row.identity)?.bucket || null;
        if (prior?.score === score && prior.retired === retired && prior.bucket === bucket) {
          this.q("UPDATE state_catalog_measurements SET at=? WHERE identity=? AND ruler=? AND mode=?").run(at, row.identity, row.ruler, row.mode);
          continue;
        }
        this.q(`INSERT INTO state_catalog_measurements VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(identity,ruler,mode) DO UPDATE SET
          score=excluded.score,scene=excluded.scene,bucket=excluded.bucket,at=excluded.at,retired=excluded.retired,gate_ruler=excluded.gate_ruler`)
          .run(row.identity, row.ruler, row.mode, score, row.scene, bucket, at, retired, row.gate_ruler);
        if (bucket) this.q("INSERT OR IGNORE INTO state_catalog_groups VALUES(?,?,?)").run(bucket, row.gate_ruler, row.mode);
        this.dirty(row.identity);
      }
    });
  }
  queue(rows, { replace = [] } = {}) {
    if (rows.length > 256 || replace.length > 256) throw Error('relational queue batch exceeds bound');
    this.transaction(() => {
      for (const id of replace) { this.q("DELETE FROM state_catalog_pending_work WHERE identity=?").run(id); this.dirty(id); }
      for (const row of rows) {
        // A retired ruler still counts as an already owed arrival in the full gate.
        if (this.q("INSERT OR IGNORE INTO state_catalog_pending_work VALUES(?,?,?)").run(row.build_id, row.ruler, row.mode).changes)
          this.dirty(row.build_id);
      }
    });
  }
  groups(limit) {
    for (let n = 0; n < limit; n++) {
      const g = this.q("SELECT * FROM state_catalog_groups ORDER BY bucket,ruler,mode LIMIT 1").get();
      if (!g) break;
      this.transaction(() => {
        for (let target = 0; target < this.contract.rulers.length; target++) {
          const args = [g.bucket, g.ruler, g.mode, target];
          const previous = this.q("SELECT score FROM state_catalog_leaders WHERE bucket=? AND ruler=? AND mode=? AND target=?").get(...args)?.score ?? null;
          const score = this.q(`SELECT f.score FROM state_catalog_measurements f JOIN entity_catalog_candidates c ON c.id=f.identity
            WHERE f.bucket=? AND f.gate_ruler=? AND f.mode=? AND f.retired=0 AND (c.admit & ?)<>0
            ORDER BY f.score DESC,f.identity DESC LIMIT 1`).get(g.bucket, g.ruler, g.mode, bit(target))?.score ?? null;
          if (score === previous) continue;
          this.q("INSERT OR REPLACE INTO state_catalog_leaders VALUES(?,?,?,?,?)").run(...args, score);
          const cutoff = value => value != null && value > 0 ? value * this.contract.entry_share : -Infinity;
          let low = Math.min(cutoff(previous), cutoff(score)), high = Math.max(cutoff(previous), cutoff(score));
          if (low === high) { low = -Infinity; high = Infinity; }
          // The range is conservative around floating division's inclusive boundary.
          if (Number.isFinite(low)) low -= Math.max(1, Math.abs(low)) * Number.EPSILON * 8;
          if (Number.isFinite(high)) high += Math.max(1, Math.abs(high)) * Number.EPSILON * 8;
          const pending = this.q("SELECT low,high FROM state_catalog_ranges WHERE bucket=? AND ruler=? AND mode=? AND target=?").get(...args);
          low = Math.min(low, pending?.low ?? low); high = Math.max(high, pending?.high ?? high);
          this.q("INSERT OR REPLACE INTO state_catalog_ranges VALUES(?,?,?,?,?,?,?,?)").run(...args, low, high, low, '');
          this.metrics.leaders++;
        }
        this.q("DELETE FROM state_catalog_groups WHERE bucket=? AND ruler=? AND mode=?").run(g.bucket, g.ruler, g.mode);
      });
      this.metrics.groups++;
    }
  }
  range(limit) {
    const g = this.q("SELECT * FROM state_catalog_ranges ORDER BY bucket,ruler,mode,target LIMIT 1").get();
    if (!g) return;
    const rows = this.q(`SELECT identity,score FROM state_catalog_measurements WHERE bucket=? AND gate_ruler=? AND mode=? AND retired=0
      AND score>=? AND score<=? AND (score,identity)>(?,?) ORDER BY score,identity LIMIT ?`)
      .all(g.bucket, g.ruler, g.mode, g.low, g.high, g.after_score, g.after_id, limit);
    this.transaction(() => {
      for (const row of rows) {
        if (this.q("SELECT 1 FROM entity_catalog_candidates WHERE id=? AND (admit & ?)<>0").get(row.identity, bit(g.target))) this.dirty(row.identity);
      }
      const key = [g.bucket, g.ruler, g.mode, g.target];
      if (rows.length < limit) this.q("DELETE FROM state_catalog_ranges WHERE bucket=? AND ruler=? AND mode=? AND target=?").run(...key);
      else this.q("UPDATE state_catalog_ranges SET after_score=?,after_id=? WHERE bucket=? AND ruler=? AND mode=? AND target=?")
        .run(rows.at(-1).score, rows.at(-1).identity, ...key);
    });
    this.metrics.range_rows += rows.length;
  }
  share(id, target, scene) {
    const best = this.q(`SELECT max(CASE WHEN l.score>0 THEN f.score/l.score ELSE 1.0 END) value
      FROM state_catalog_measurements f JOIN state_catalog_leaders l ON l.bucket=f.bucket AND l.ruler=f.gate_ruler AND l.mode=f.mode AND l.target=?
      WHERE f.identity=? AND f.retired=0 AND f.scene=?`).get(target, id, scene).value;
    return best ?? (scene ? this.share(id, target, '') : null);
  }
  async project(id, operate) {
    const stored = this.q("SELECT body FROM entity_catalog_candidates WHERE id=?").get(id);
    if (!stored) {
      this.transaction(() => {
        if (this.q("SELECT 1 FROM relation_inventory_owners WHERE owner=?").get(id)) this.catalog.beginOwner({ owner: id, build_id: null }, {});
        this.q("DELETE FROM state_catalog_jobs WHERE id=?").run(id);
      });
      return;
    }
    const candidate = JSON.parse(stored.body), requests = [], targets = [];
    const owed = !!this.q("SELECT 1 FROM state_catalog_pending_work WHERE identity=? LIMIT 1").get(id);
    for (let target = 0; target < this.contract.rulers.length; target++) {
      if (!allowed(candidate.admit, target)) continue;
      const ruler = this.contract.rulers[target];
      const present = candidate.default != null && !!this.q("SELECT 1 FROM entity_catalog_candidates WHERE id=? AND (admit & ?)<>0")
        .get(candidate.default, bit(target));
      targets.push(target); requests.push({ share: this.share(id, target, ruler.scene), owed,
        default_present: present, default_share: present ? this.share(candidate.default, target, ruler.scene) : null });
    }
    const decisions = await operate('gate', requests);
    if (decisions.length !== targets.length || decisions.some(d => typeof d.eligible !== 'boolean')) throw Error('incomplete engine gates');
    this.transaction(() => {
      const owner = this.catalog.beginOwner({ owner: id, build_id: id, record: candidate.record || JSON.parse(this.catalog.recordBody(candidate.record_ref)),
        weapon: candidate.bucket[0], riven: candidate.bucket[1] }, {}), prefixes = new Map();
      const variants = this.q("SELECT body FROM entity_catalog_sources WHERE candidate=? ORDER BY seq");
      for (const source of variants.iterate(id)) {
        const body = JSON.parse(this.catalog.bodyText(source.body));
        for (let i = 0; i < targets.length; i++) {
          const target = targets[i];
          if (!allowed(body.admit, target)) continue;
          const ruler = this.contract.rulers[target].id;
          for (const mode of body.modes) {
            const queued = !!this.q("SELECT 1 FROM state_catalog_pending_work WHERE identity=? AND ruler=? AND mode=?").get(id, ruler, mode);
            if (!queued && (!decisions[i].eligible || this.q("SELECT 1 FROM state_catalog_measurements WHERE identity=? AND ruler=? AND mode=? AND retired=0").get(id, ruler, mode))) continue;
            this.catalog.addMember(owner, { build_id: id, ruler, mode, queued }, prefixes); this.metrics.outputs++;
          }
        }
      }
      this.q("DELETE FROM state_catalog_jobs WHERE id=?").run(id);
    });
    this.metrics.candidates++;
  }
  pending() {
    return !!this.q("SELECT 1 FROM state_catalog_groups UNION ALL SELECT 1 FROM state_catalog_ranges UNION ALL SELECT 1 FROM state_catalog_jobs LIMIT 1").get();
  }
  async step(operate, { limit = 64, budgetMs = 1000 } = {}) {
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 256 || !Number.isFinite(budgetMs) || budgetMs < 0) throw Error('invalid relational execution budget');
    const until = performance.now() + budgetMs;
    do {
      if (this.q("SELECT 1 FROM state_catalog_groups LIMIT 1").get()) this.groups(1);
      else if (this.q("SELECT 1 FROM state_catalog_ranges LIMIT 1").get()) this.range(limit);
      else {
        const job = this.q("SELECT id FROM state_catalog_jobs ORDER BY id LIMIT 1").get();
        if (!job) break;
        await this.project(job.id, operate);
      }
    } while (performance.now() < until);
    return { pending: this.pending(), metrics: { ...this.metrics } };
  }
}
