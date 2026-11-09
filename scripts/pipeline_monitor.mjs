// Durable phase evidence; heartbeats never advance the last work-progress clock.
import { randomUUID } from 'node:crypto';
import { appendFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const fields = new Set(['generation', 'epoch', 'rows', 'chunks', 'changes', 'scopes', 'total', 'leased', 'processed', 'records', 'facts', 'started_at', 'snapshot',
  'normalized', 'groups', 'leaders', 'range_rows', 'candidates', 'outputs']);
export function pipelineMonitor(directory, name, context = {}, { now = Date.now, heartbeatMs = 30000, limit = 2 * 1024 ** 2, log = console.error } = {}) {
  mkdirSync(directory, { recursive: true });
  const path = join(directory, `${name}.json`), history = join(directory, `${name}.ndjson`);
  const time = () => new Date(now()).toISOString();
  let previous;
  try { previous = JSON.parse(readFileSync(path, 'utf8')); } catch { /* first run has no receipt */ }
  const state = { v: 1, event: 'pipeline', pipeline: name, run_id: randomUUID(), pid: process.pid,
    source: context.source || null, engine: context.engine || null, status: 'running', stage: 'start',
    run_started_at: time(), last_progress_at: time(), previous_run: previous?.status === 'running' ? { run_id: previous.run_id, stage: previous.stage, updated_at: previous.updated_at } : null };
  function emit(event) {
    state.updated_at = time();
    const row = { ...state, event, resources: process.memoryUsage() }, data = JSON.stringify(row) + '\n';
    writeFileSync(`${path}.partial`, data); renameSync(`${path}.partial`, path);
    if (existsSync(history) && statSync(history).size + Buffer.byteLength(data) > limit) {
      rmSync(`${history}.2`, { force: true });
      if (existsSync(`${history}.1`)) renameSync(`${history}.1`, `${history}.2`);
      renameSync(history, `${history}.1`);
    }
    appendFileSync(history, data); log(data.trim());
  }
  function report(stage, values = {}) {
    state.stage = stage; state.last_progress_at = time();
    for (const [key, value] of Object.entries(values)) if (fields.has(key)) state[key] = value;
    emit('progress');
  }
  emit(previous?.status === 'running' ? 'interrupted' : 'start');
  const timer = setInterval(() => emit('heartbeat'), heartbeatMs); timer.unref();
  return { report, heartbeat: () => emit('heartbeat'), finish(status = 'complete', error) {
    clearInterval(timer); state.status = status;
    if (error) state.error_code = /^[A-Z0-9_]{2,40}$/.test(error.code || '') ? error.code : error.name || 'Error';
    emit(status);
  } };
}
