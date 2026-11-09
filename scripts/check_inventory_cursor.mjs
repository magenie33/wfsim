// Token export stays complete at production scale without a round trip per tiny page.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventoryCursor } from './inventory_pipeline.mjs';

const total = 27038064;
if (process.argv[2] === 'scale') {
  const maxRows = Number(process.argv[3]); let fetched = 0, consumed = 0, calls = 0, closed = false, peakHeap = 0;
  const tx = { unsafe: async query => {
    if (query.startsWith('DECLARE')) return [];
    if (query.startsWith('CLOSE')) { closed = true; return []; }
    assert.equal(fetched, consumed, 'the next page cannot be prefetched while one is retained');
    const limit = Number(query.match(/^FETCH FORWARD (\d+) FROM scale_tokens$/)?.[1]);
    assert(limit > 0 && limit <= 80000);
    if (++calls > 400) throw new Error('round-trip budget exhausted');
    const rows = [];
    for (let n = Math.min(limit, total - fetched); n > 0; n--) {
      fetched++;
      rows.push({ scope: 'question', key: fetched.toString(16).padStart(32, '0'), revision: String(fetched) });
    }
    return rows;
  } };
  try {
    for await (const row of inventoryCursor(tx, 'scale_tokens', 'SELECT fixture', [], { maxRows,
      report: () => { peakHeap = Math.max(peakHeap, process.memoryUsage().heapUsed); } })) {
      consumed++;
      assert.equal(row.revision, String(consumed));
      assert.equal(row.key, consumed.toString(16).padStart(32, '0'));
    }
    assert.equal(consumed, total); assert(closed);
    assert(peakHeap < 160 * 1024 ** 2, 'token memory must stay independent of total cardinality');
    console.log(JSON.stringify({ rows: consumed, fetches: calls, peak_heap_bytes: peakHeap, peak_rss_bytes: process.resourceUsage().maxRSS * 1024 }));
  } catch (error) {
    assert(closed, 'failed capture must close its cursor');
    console.error(error.message); process.exitCode = 1;
  }
} else {
  const run = rows => spawnSync(process.execPath, ['--max-old-space-size=96', fileURLToPath(import.meta.url), 'scale', String(rows)],
    { encoding: 'utf8', timeout: 180000 });
  const broken = run(5000);
  assert.equal(broken.status, 1); assert.match(broken.stderr, /round-trip budget exhausted/);
  const repaired = run(80000);
  assert.equal(repaired.status, 0, repaired.stderr || repaired.error?.message);
  const evidence = JSON.parse(repaired.stdout);
  assert.equal(evidence.rows, total); assert(evidence.fetches <= 400);
  console.log(`ok: 27,038,064 exact tokens with 96MiB old-space limit: ${JSON.stringify(evidence)}; original 5k pages fail the same round-trip budget`);

  let position = 0, closed = false; const limits = [], events = [];
  const tx = { unsafe: async query => {
    if (query.startsWith('DECLARE')) return [];
    if (query.startsWith('CLOSE')) { closed = true; return []; }
    const limit = Number(query.match(/FETCH FORWARD (\d+)/)[1]); limits.push(limit);
    return Array.from({ length: Math.min(limit, 12001 - position) }, () => ({ key: '界'.repeat(1000), revision: String(++position) }));
  } };
  let consumed = 0;
  for await (const row of inventoryCursor(tx, 'wide_tokens', 'SELECT fixture', [], { maxRows: 80000, report: e => events.push(e) })) {
    assert.equal(row.revision, String(++consumed));
  }
  assert.equal(consumed, 12001); assert(closed); assert(limits[1] < limits[0]);
  assert(events.slice(1).every(e => e.page_bytes <= 8 * 1024 ** 2), 'observed UTF-8 width shrinks later pages');
  closed = false;
  for await (const row of inventoryCursor({ unsafe: async query => {
    if (query.startsWith('DECLARE')) return [];
    if (query.startsWith('CLOSE')) { closed = true; return []; }
    return [{ key: 'one' }];
  } }, 'aborted_tokens', 'SELECT fixture')) { assert.equal(row.key, 'one'); break; }
  assert(closed, 'a stopped consumer closes the cursor');
  console.log('ok: byte sizing handles wide UTF-8 rows and consumer interruption closes the cursor');
}
