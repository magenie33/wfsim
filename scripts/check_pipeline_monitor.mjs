// Progress clocks, crash evidence, redaction and retention are executable contracts.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pipelineMonitor } from './pipeline_monitor.mjs';

const directory = mkdtempSync(join(tmpdir(), 'wfsim-monitor-'));
let time = 1800000000000;
const options = { now: () => time, log: () => {}, limit: 1800 };
const read = () => JSON.parse(readFileSync(join(directory, 'inventory.json'), 'utf8'));
try {
  const monitor = pipelineMonitor(directory, 'inventory', { engine: 'engine', source: 'source' }, options);
  monitor.report('capture', { rows: 123, epoch: '4', page_rows: 123, page_bytes: 4567, fetch_ms: 89, next_rows: 20000, token: 'secret', query: 'secret' });
  const started = read(); time += 60000; monitor.heartbeat();
  assert.equal(started.page_rows, 123); assert.equal(started.page_bytes, 4567);
  assert.equal(started.fetch_ms, 89); assert.equal(started.next_rows, 20000);
  assert.equal(read().last_progress_at, started.last_progress_at);
  assert.notEqual(read().updated_at, started.updated_at);
  const restart = pipelineMonitor(directory, 'inventory', {}, options);
  assert.equal(read().event, 'interrupted'); assert.equal(read().previous_run.stage, 'capture');
  assert.equal(read().previous_run.run_id, started.run_id);
  monitor.finish();
  for (let i = 0; i < 30; i++) { time += 1000; restart.report('capture', { rows: i }); }
  restart.finish('failed', Object.assign(Error('postgres://private:secret@host'), { code: '55P03' }));
  assert.equal(read().error_code, '55P03'); assert.equal(read().status, 'failed');
  assert.equal(read().stage, 'capture');
  const files = readdirSync(directory);
  assert.equal(files.filter(f => f.includes('.ndjson')).length, 3);
  for (const file of files) {
    assert(!readFileSync(join(directory, file), 'utf8').includes('secret'));
    assert(statSync(join(directory, file)).size <= 1800);
  }
  console.log('pipeline monitor progress/crash/secret/rotation checks passed');
} finally { rmSync(directory, { recursive: true, force: true }); }
