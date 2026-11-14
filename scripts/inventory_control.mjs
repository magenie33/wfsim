// SPDX-License-Identifier: AGPL-3.0-or-later
// Rebuild requests are immutable; a kernel-held SQLite writer lock covers each controller turn.
import { DatabaseSync } from 'node:sqlite';
import { createHash, randomUUID } from 'node:crypto';
import { createReadStream, existsSync, lstatSync, mkdirSync, readFileSync, statfsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { canon } from '../worker/task_contract.js';

export const INVENTORY_RESERVE = 8 * 1024 ** 3;
export class InventoryActionRequired extends Error {
  constructor(stage, message) { super(message); this.name = 'InventoryActionRequired'; this.code = 'INVENTORY_ACTION_REQUIRED'; this.stage = stage; }
}
export const needsAction = (stage, message) => { throw new InventoryActionRequired(stage, message); };
const hash = async path => {
  const value = createHash('sha256'); for await (const data of createReadStream(path)) value.update(data); return value.digest('hex');
};
const stamp = path => { const s = statSync(path, { bigint: true }); return [s.dev, s.ino, s.size, s.mtimeNs, s.ctimeNs].map(String); };
const selected = `SELECT t.id,t.body,s.phase,s.generation,s.error FROM state_inventory_rebuild_selection p
  JOIN entity_inventory_rebuild_tasks t ON t.id=p.task JOIN state_inventory_rebuild_tasks s ON s.task=t.id WHERE p.id=1`;
const current = db => { const row = db.prepare(selected).get(); return row ? { ...JSON.parse(row.body), id: row.id, phase: row.phase, generation: row.generation, error: row.error } : null; };
export function inspectInventoryTask(root) {
  const path = join(root, 'controller.sqlite'); if (!existsSync(path)) return null;
  const db = new DatabaseSync(path, { readOnly: true }); try { return current(db); } finally { db.close(); }
}

export class InventoryControl {
  constructor(root) {
    this.root = resolve(root); mkdirSync(this.root, { recursive: true });
    this.lock = new DatabaseSync(join(this.root, 'controller-lock.sqlite'));
    try { this.lock.exec('PRAGMA busy_timeout=0;BEGIN IMMEDIATE'); }
    catch (error) { this.lock.close(); needsAction('controller_busy', 'another inventory controller owns this working directory'); }
    try {
      this.db = new DatabaseSync(join(this.root, 'controller.sqlite'));
      const version = this.db.prepare('PRAGMA user_version').get().user_version;
      if (version !== 0 && version !== 1) throw Error('unsupported inventory controller version');
      this.db.exec(`PRAGMA foreign_keys=ON;PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS entity_inventory_rebuild_tasks(id TEXT PRIMARY KEY,body TEXT NOT NULL);
        CREATE TRIGGER IF NOT EXISTS inventory_task_immutable BEFORE UPDATE ON entity_inventory_rebuild_tasks BEGIN SELECT RAISE(ABORT,'immutable rebuild request');END;
        CREATE TRIGGER IF NOT EXISTS inventory_task_retained BEFORE DELETE ON entity_inventory_rebuild_tasks BEGIN SELECT RAISE(ABORT,'rebuild requests are retained');END;
        CREATE TABLE IF NOT EXISTS state_inventory_rebuild_tasks(task TEXT PRIMARY KEY REFERENCES entity_inventory_rebuild_tasks(id),phase TEXT NOT NULL,generation TEXT,error TEXT);
        CREATE TABLE IF NOT EXISTS state_inventory_rebuild_selection(id INTEGER PRIMARY KEY CHECK(id=1),task TEXT NOT NULL REFERENCES entity_inventory_rebuild_tasks(id));
        CREATE TABLE IF NOT EXISTS record_inventory_rebuild_events(id INTEGER PRIMARY KEY,task TEXT NOT NULL,event TEXT NOT NULL,at TEXT NOT NULL,body TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS record_inventory_capture_verifications(id INTEGER PRIMARY KEY,manifest TEXT NOT NULL,body TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS inventory_verified_manifest ON record_inventory_capture_verifications(manifest);
        PRAGMA user_version=1;`);
    } catch (error) { this.close(); throw error; }
  }
  transaction(fn) { this.db.exec('BEGIN IMMEDIATE'); try { const result = fn(); this.db.exec('COMMIT'); return result; } catch (error) { this.db.exec('ROLLBACK'); throw error; } }
  current() {
    return current(this.db);
  }
  event(id, event, body = {}) { this.db.prepare('INSERT INTO record_inventory_rebuild_events(task,event,at,body) VALUES(?,?,?,?)').run(id, event, new Date().toISOString(), canon(body)); }
  phase(id, phase, generation = null, error = null) {
    this.transaction(() => { this.db.prepare('UPDATE state_inventory_rebuild_tasks SET phase=?,generation=COALESCE(?,generation),error=? WHERE task=?').run(phase, generation, error, id); this.event(id, phase, { generation, error }); });
  }
  request({ mode, engine, contract, expectedGeneration = null, reason, maxNewBytes, reserveBytes = INVENTORY_RESERVE }) {
    const prior = this.current();
    if (prior && !['complete', 'cancelled'].includes(prior.phase)) needsAction('task_exists', 'cancel the existing rebuild task before requesting another');
    if (!['classic', 'relational', 'board'].includes(mode) || !/^[a-f0-9]{12}$/.test(engine) || !contract || !reason?.trim()
        || !Number.isSafeInteger(maxNewBytes) || maxNewBytes < 1 || !Number.isSafeInteger(reserveBytes) || reserveBytes < INVENTORY_RESERVE)
      throw Error('an explicit engine, contract, reason and positive disk budget are required');
    const s = statfsSync(this.root), free = s.bavail * s.bsize;
    if (free < maxNewBytes + reserveBytes) needsAction('needs_space', 'rebuild budget and reserve exceed available space');
    const id = randomUUID(), task = { mode, engine, contract, expected_generation: expectedGeneration,
      reason, requested_at: new Date().toISOString(), max_new_bytes: maxNewBytes, reserve_bytes: reserveBytes,
      initial_free_bytes: free, filesystem_device: String(statSync(this.root).dev) };
    this.transaction(() => {
      this.db.prepare('INSERT INTO entity_inventory_rebuild_tasks VALUES(?,?)').run(id, canon(task));
      this.db.prepare("INSERT INTO state_inventory_rebuild_tasks(task,phase) VALUES(?,'requested')").run(id);
      this.db.prepare('INSERT INTO state_inventory_rebuild_selection VALUES(1,?) ON CONFLICT(id) DO UPDATE SET task=excluded.task').run(id);
      this.event(id, 'requested', task);
    });
    return this.current();
  }
  cancel(id, reason) {
    if (!id || this.current()?.id !== id || !reason?.trim()) throw Error('the selected task id and cancellation reason are required');
    this.phase(id, 'cancelled', null, reason); return this.current();
  }
  budget(task) {
    const s = statfsSync(this.root), free = s.bavail * s.bsize;
    if (String(statSync(this.root).dev) !== task.filesystem_device || free < task.reserve_bytes
        || task.initial_free_bytes - free > task.max_new_bytes) needsAction('needs_space', 'rebuild disk budget or filesystem reserve was reached');
  }
  authorize({ mode, engine, contract, status }) {
    const task = this.current();
    if (!task || task.phase !== 'requested') needsAction('needs_bootstrap', 'an explicit rebuild task is required; existing files were retained');
    if (task.mode !== mode || task.engine !== engine || canon(task.contract) !== canon(contract)
        || task.expected_generation !== (status?.generation || null)) needsAction('request_changed', 'rebuild request no longer matches the exact engine, contract and prior generation');
    this.budget(task);
    const fs = statfsSync(this.root);
    if (fs.bavail * fs.bsize < task.max_new_bytes + task.reserve_bytes) needsAction('needs_space', 'rebuild admission must be refreshed against current free space');
    return task;
  }
  bound({ mode, engine, contract, status }) {
    const task = this.current();
    return task?.phase === 'created' && task.generation === status?.generation && task.mode === mode && task.engine === engine
      && canon(task.contract) === canon(contract) ? { task: task.id, generation: task.generation } : null;
  }
  checkTask(state) {
    if (!state.rebuild_task) return;
    const task = this.current();
    if (!task || task.id !== state.rebuild_task || task.generation !== state.generation || ['starting', 'needs_action', 'cancelled'].includes(task.phase))
      needsAction('needs_resume', 'the durable rebuild task requires operator attention');
    if (task.phase !== 'complete') this.budget(task);
  }
  async begin(context, sql) {
    const task = this.authorize(context); this.phase(task.id, 'starting');
    try {
      const [{ generation }] = await sql.unsafe('SELECT inventory_begin($1::text,$2::text,$3::text) generation',
        [context.engine, JSON.stringify(context.contract), context.status?.generation || null]);
      this.phase(task.id, 'created', generation.generation); return { task: task.id, generation: generation.generation };
    } catch (error) { this.phase(task.id, 'needs_action', null, 'generation start failed or its committed outcome is uncertain'); throw error; }
  }
  async capture(state, capture, save) {
    const task = this.current();
    if (!task || task.id !== state.rebuild_task || !['created', 'active'].includes(task.phase))
      needsAction('needs_resume', 'a full capture may not restart after an interrupted or failed attempt');
    this.budget(task); this.phase(task.id, 'capturing');
    try { state.pending = await capture(); save(); this.phase(task.id, 'active'); }
    catch (error) { this.phase(task.id, 'needs_action', null, 'full capture failed; partial files retained, explicit new task required'); throw error; }
  }
  async verifyCapture(cut, context) {
    const r = cut?.receipt;
    if (!r || r.status !== 'sealed' || r.generation !== context.generation || r.engine !== context.engine
        || canon(r.contract) !== canon(context.contract) || r.manifest_sha !== cut.manifest_sha || !cut.capture
        || !existsSync(cut.capture) || lstatSync(cut.capture).isSymbolicLink()
        || await hash(cut.capture) !== cut.manifest_sha) needsAction('needs_restore', 'only an unchanged sealed input cut can resume');
    const manifest = JSON.parse(readFileSync(cut.capture, 'utf8'));
    if (!Array.isArray(manifest.files) || ['epoch', 'generation', 'engine', 'contract', 'snapshot', 'started_at'].some(key => canon(manifest.receipt?.[key]) !== canon(r[key]))
        || ['raw', 'changes', 'applies', 'delta', 'bootstrap'].some(key => canon(manifest[key]) !== canon(cut[key])))
      needsAction('needs_restore', 'sealed capture manifest no longer matches its receipt and input references');
    const signatures = [];
    for (const file of manifest.files) {
      if (!existsSync(file.path) || lstatSync(file.path).isSymbolicLink()) needsAction('needs_restore', 'sealed capture input is missing or linked');
      signatures.push({ path: file.path, sha256: file.sha256, stamp: stamp(file.path) });
    }
    const cached = this.db.prepare('SELECT 1 FROM record_inventory_capture_verifications WHERE manifest=? AND body=?').get(cut.manifest_sha, canon(signatures));
    if (!cached) {
      for (const file of signatures) {
        if (await hash(file.path) !== file.sha256 || canon(stamp(file.path)) !== canon(file.stamp)) needsAction('needs_restore', 'sealed capture input hash differs');
      }
      this.db.prepare('INSERT INTO record_inventory_capture_verifications(manifest,body) VALUES(?,?)').run(cut.manifest_sha, canon(signatures));
    }
    const task = this.current();
    if (task?.id === context.rebuild_task && task.phase === 'capturing') this.phase(task.id, 'active');
  }
  complete(state) {
    const task = this.current(); if (state.rebuild_task === task?.id && task.phase !== 'complete') this.phase(task.id, 'complete', state.generation);
  }
  close() { this.db?.close(); if (this.lock) { this.lock.exec('ROLLBACK'); this.lock.close(); this.lock = null; } }
}
