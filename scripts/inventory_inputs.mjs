// SPDX-License-Identifier: AGPL-3.0-or-later
// Partition an immutable snapshot with disk indexes, retaining its raw audit inputs.
import { DatabaseSync } from 'node:sqlite';
import { openSync, closeSync, writeSync, fsyncSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync, readFileSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { canon } from '../worker/task_contract.js';
import { readInventoryRows } from './incremental_inventory.mjs';
import { classifyInventory } from './classify_compute_inventory.mjs';

const CACHE_KIB = 16384, OPEN_FILES = 32;
const keyOf = (weapon, riven) => createHash('sha256').update(JSON.stringify([weapon, !!riven])).digest('hex').slice(0, 24);
function sync(path) {
  const fd = openSync(path, 'r');
  try { fsyncSync(fd); } finally { closeSync(fd); }
}
function commitDirectory(staging, out, paths) {
  const directories = new Set([staging]);
  for (const path of new Set(paths)) { sync(path); directories.add(dirname(path)); }
  sync(join(staging, 'manifest.json'));
  for (const directory of directories) sync(directory);
  renameSync(staging, out); sync(dirname(resolve(out)));
}
function database(path) {
  const db = new DatabaseSync(path);
  db.exec(`PRAGMA cache_size=-${CACHE_KIB};PRAGMA temp_store=FILE;PRAGMA mmap_size=0;PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;
    CREATE TABLE sources(id TEXT PRIMARY KEY,ordinal INTEGER NOT NULL) WITHOUT ROWID;
    CREATE TABLE canonical(id TEXT PRIMARY KEY) WITHOUT ROWID;`);
  return db;
}

class Files {
  constructor(root) { this.root = root; this.open = new Map(); this.counts = new Map(); }
  write(name, row) {
    let fd = this.open.get(name);
    if (fd == null) {
      if (this.open.size >= OPEN_FILES) {
        const [old, handle] = this.open.entries().next().value; closeSync(handle); this.open.delete(old);
      }
      mkdirSync(dirname(join(this.root, name)), { recursive: true });
      fd = openSync(join(this.root, name), 'a');
    } else this.open.delete(name);
    this.open.set(name, fd);
    const bytes = Buffer.from(JSON.stringify(row) + '\n');
    let position = 0;
    while (position < bytes.length) position += writeSync(fd, bytes, position, bytes.length - position);
    this.counts.set(name, (this.counts.get(name) || 0) + 1);
  }
  close() { for (const fd of this.open.values()) closeSync(fd); this.open.clear(); }
}

async function indexSources(db, builds) {
  const add = db.prepare('INSERT INTO sources VALUES(?,?)');
  let count = 0;
  db.exec('BEGIN');
  try {
    for await (const row of readInventoryRows(builds)) {
      if (typeof row.id !== 'string' || !row.record) throw new Error('invalid inventory source build');
      add.run(row.id, count++);
      if (count % 256 === 0) db.exec('COMMIT;BEGIN');
    }
    db.exec('COMMIT');
  } catch (error) { db.exec('ROLLBACK'); throw error; }
  return count;
}

function footer(end, expected, owners, rows) {
  if (!end || end.v !== 1 || end.engine !== expected.engine || canon(end.contract) !== canon(expected.contract)
      || end.generation !== expected.generation || end.snapshot !== expected.snapshot || end.owners !== owners || end.rows !== rows
      || expected.epoch && end.epoch !== expected.epoch) throw new Error('incomplete immutable inventory output');
}

export async function shardInventory({ builds, facts, queue, tasks, owners, out, engine, contract, snapshot, generation, epoch }) {
  if (existsSync(out)) {
    const prior = JSON.parse(readFileSync(join(out, 'manifest.json'), 'utf8'));
    if (prior.engine !== engine || canon(prior.contract) !== canon(contract) || prior.snapshot !== snapshot
        || prior.generation !== generation || prior.epoch !== epoch || prior.builds !== resolve(builds) || prior.rawQueue !== resolve(queue)
        || !prior.scopes.every(scope => ['builds', 'facts', 'queue'].every(name => existsSync(scope[name]))))
      throw new Error('inventory input manifest changed');
    return prior;
  }
  mkdirSync(dirname(resolve(out)), { recursive: true });
  const staging = mkdtempSync(join(dirname(resolve(out)), '.inventory-inputs-'));
  const db = database(join(staging, 'inputs.sqlite')), files = new Files(staging);
  let done = false;
  try {
    db.exec(`CREATE TABLE owners(owner TEXT PRIMARY KEY,candidate TEXT,weapon TEXT,riven INTEGER) WITHOUT ROWID;
      CREATE TABLE identities(id TEXT NOT NULL,weapon TEXT NOT NULL,riven INTEGER NOT NULL,PRIMARY KEY(id,weapon,riven)) WITHOUT ROWID;
      CREATE TABLE scopes(key TEXT PRIMARY KEY,weapon TEXT NOT NULL,riven INTEGER NOT NULL) WITHOUT ROWID;`);
    const count = await indexSources(db, builds);
    const source = db.prepare('SELECT ordinal FROM sources WHERE id=?');
    const addOwner = db.prepare('INSERT INTO owners VALUES(?,?,?,?)');
    const addIdentity = db.prepare('INSERT OR IGNORE INTO identities VALUES(?,?,?)');
    const addScope = db.prepare('INSERT OR IGNORE INTO scopes VALUES(?,?,?)');
    const seenCanonical = db.prepare('INSERT OR IGNORE INTO canonical VALUES(?)');
    let ownerCount = 0, end = null;
    db.exec('BEGIN');
    for await (const row of readInventoryRows(owners)) {
      if (end) throw new Error('inventory owner data after footer');
      if (row.type === 'end') { end = row; continue; }
      if (row.type !== 'owner' || !source.get(row.owner)) throw new Error('invalid inventory owner mapping');
      ownerCount++;
      const valid = row.build_id != null && typeof row.weapon === 'string' && typeof row.riven === 'boolean';
      addOwner.run(row.owner, row.build_id, valid ? row.weapon : null, valid ? Number(row.riven) : null);
      if (valid) {
        const key = keyOf(row.weapon, row.riven);
        addScope.run(key, row.weapon, Number(row.riven));
        for (const identity of [row.owner, row.build_id]) addIdentity.run(identity, row.weapon, Number(row.riven));
        if (!source.get(row.build_id) && seenCanonical.run(row.build_id).changes) {
          if (!row.record) throw new Error('unbound canonical inventory identity');
          const binding = source.get(row.binding_owner || row.owner);
          if (!binding) throw new Error('canonical inventory source is missing');
          files.write('canonical.ndjson', { id: row.build_id, record: row.record, source: binding.ordinal });
        }
      }
      if (ownerCount % 256 === 0) db.exec('COMMIT;BEGIN');
    }
    db.exec('COMMIT');
    footer(end, { engine, contract, snapshot, generation, epoch }, ownerCount, 0);
    if (ownerCount !== count) throw new Error('incomplete inventory owner baseline');
    const scopeOfOwner = db.prepare('SELECT weapon,riven FROM owners WHERE owner=? AND weapon IS NOT NULL');
    for await (const row of readInventoryRows(builds)) {
      const scope = scopeOfOwner.get(row.id);
      if (scope) files.write(`${keyOf(scope.weapon, scope.riven)}/builds.ndjson`, row);
    }
    const scopesFor = db.prepare('SELECT weapon,riven FROM identities WHERE id=? ORDER BY weapon,riven');
    const counts = { builds: count, owners: ownerCount, facts: 0, queue: 0, unmapped_facts: 0, unmapped_queue: 0 };
    for (const [name, input, identity] of [['facts', facts, 'identity'], ['queue', queue, 'build_id']]) {
      for await (const row of readInventoryRows(input)) {
        counts[name]++;
        let mapped = false;
        for (const scope of scopesFor.iterate(row[identity])) {
          mapped = true;
          files.write(`${keyOf(scope.weapon, scope.riven)}/${name}.ndjson`, row);
        }
        if (!mapped) { counts[`unmapped_${name}`]++; files.write(`unmapped.${name}.ndjson`, row); }
      }
    }
    const scopes = db.prepare('SELECT key,weapon,riven FROM scopes ORDER BY weapon,riven').all().map(row => ({ ...row, riven: !!row.riven,
      scope: { weapon: row.weapon, riven: !!row.riven }, counts: Object.fromEntries(['builds', 'facts', 'queue'].map(name => [name, files.counts.get(`${row.key}/${name}.ndjson`) || 0])),
      builds: join(resolve(out), row.key, 'builds.ndjson'), facts: join(resolve(out), row.key, 'facts.ndjson'), queue: join(resolve(out), row.key, 'queue.ndjson') }));
    for (const scope of scopes) for (const name of ['builds', 'facts', 'queue']) {
      const relative = `${scope.key}/${name}.ndjson`, path = join(staging, relative);
      if (!existsSync(path)) { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, ''); }
    }
    if (!existsSync(join(staging, 'canonical.ndjson'))) writeFileSync(join(staging, 'canonical.ndjson'), '');
    const manifest = { v: 1, engine, contract, snapshot, generation, ...(epoch ? { epoch } : {}), counts, scopes,
      canonical: join(resolve(out), 'canonical.ndjson'), rawQueue: resolve(queue), builds: resolve(builds), tasks: resolve(tasks), index: join(resolve(out), 'inputs.sqlite') };
    files.close(); db.close();
    writeFileSync(join(staging, 'manifest.json'), JSON.stringify(manifest) + '\n');
    commitDirectory(staging, out, [join(staging, 'inputs.sqlite'), join(staging, 'canonical.ndjson'),
      ...files.counts.keys().map(name => join(staging, name)),
      ...scopes.flatMap(scope => ['builds', 'facts', 'queue'].map(name => join(staging, scope.key, `${name}.ndjson`)))]);
    done = true;
    return manifest;
  } finally {
    if (!done) { files.close(); try { db.close(); } catch {} rmSync(staging, { recursive: true, force: true }); }
  }
}

export async function auditInventoryInputs({ builds, queue, tasks, meta, rulers, engineOutputs, out, contract, generation, epoch }) {
  if (existsSync(out)) {
    const prior = JSON.parse(readFileSync(join(out, 'manifest.json'), 'utf8'));
    if (prior.engine !== meta.engine || prior.snapshot !== meta.snapshot || prior.generation !== generation || prior.epoch !== epoch
        || canon(prior.contract) !== canon(contract) || canon(prior.input_counts) !== canon(meta.counts)
        || !['missing', 'orphans', 'canonical', 'questions'].every(name => existsSync(prior[name]))) throw new Error('inventory audit manifest changed');
    return prior;
  }
  mkdirSync(dirname(resolve(out)), { recursive: true });
  const staging = mkdtempSync(join(dirname(resolve(out)), '.inventory-audit-'));
  const db = database(join(staging, 'audit.sqlite')), files = new Files(staging);
  let done = false;
  try {
    db.exec('CREATE TABLE valid_queue(build_id TEXT NOT NULL,ruler TEXT NOT NULL,mode TEXT NOT NULL,PRIMARY KEY(build_id,ruler,mode)) WITHOUT ROWID');
    const sourceCount = await indexSources(db, builds), roster = new Set(rulers);
    if (sourceCount !== meta.counts.library) throw new Error('incomplete inventory audit builds');
    const source = db.prepare('SELECT ordinal FROM sources WHERE id=?'), seen = db.prepare('INSERT OR IGNORE INTO canonical VALUES(?)');
    const addQueued = db.prepare('INSERT OR IGNORE INTO valid_queue VALUES(?,?,?)');
    db.exec('BEGIN');
    let memberCount = 0;
    for await (const path of engineOutputs) {
      let owners = 0, rows = 0, end = null, owner = null;
      for await (const row of readInventoryRows(path)) {
        if (end) throw new Error('inventory audit data after footer');
        if (row.type === 'owner') {
          owner = row.owner; owners++;
          if (row.build_id != null && !source.get(row.build_id) && seen.run(row.build_id).changes) {
            const origin = source.get(row.binding_owner || row.owner);
            if (!origin || !row.record) throw new Error('unbound audit canonical identity');
            files.write('canonical.ndjson', { id: row.build_id, record: row.record, source: origin.ordinal });
          }
        } else if (row.type === 'row') {
          if (row.owner !== owner || ![true, false, 0, 1].includes(row.queued)) throw new Error('invalid inventory audit membership');
          rows++; memberCount++;
          if (row.queued) addQueued.run(row.build_id, row.ruler, row.mode);
          else files.write('missing.ndjson', { build_id: row.build_id, ruler: row.ruler, mode: row.mode });
          if (memberCount % 256 === 0) db.exec('COMMIT;BEGIN');
        } else if (row.type === 'end') end = row;
        else throw new Error('invalid inventory audit output');
      }
      footer(end, { engine: meta.engine, contract, snapshot: meta.snapshot, generation, epoch }, owners, rows);
    }
    db.exec('COMMIT');
    const valid = db.prepare('SELECT 1 FROM valid_queue WHERE build_id=? AND ruler=? AND mode=?');
    let raw = 0;
    for await (const row of readInventoryRows(queue)) {
      raw++;
      if (roster.has(row.ruler) && !valid.get(row.build_id, row.ruler, row.mode))
        files.write('orphans.ndjson', { build_id: row.build_id, ruler: row.ruler, mode: row.mode });
    }
    if (raw !== meta.counts.queue) throw new Error('incomplete inventory audit queue');
    files.close();
    for (const name of ['missing', 'orphans', 'canonical']) if (!existsSync(join(staging, `${name}.ndjson`))) writeFileSync(join(staging, `${name}.ndjson`), '');
    const paths = Object.fromEntries(['missing', 'orphans', 'canonical', 'questions'].map(name => [name, join(staging, `${name}.ndjson`)]));
    const plan = await classifyInventory({ builds, queue, tasks, meta, rulers, ...paths });
    db.close();
    const result = { plan, engine: meta.engine, snapshot: meta.snapshot, contract, generation, ...(epoch ? { epoch } : {}), input_counts: meta.counts,
      ...Object.fromEntries(Object.keys(paths).map(name => [name, join(resolve(out), `${name}.ndjson`)])) };
    writeFileSync(join(staging, 'manifest.json'), JSON.stringify(result) + '\n');
    commitDirectory(staging, out, [join(staging, 'audit.sqlite'), ...Object.values(paths)]); done = true;
    return result;
  } finally {
    if (!done) { files.close(); try { db.close(); } catch {} rmSync(staging, { recursive: true, force: true }); }
  }
}
