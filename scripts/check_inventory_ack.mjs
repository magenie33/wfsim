// ACK pages amortize transport while every existing SQL and durable-receipt bound remains intact.
import assert from 'node:assert/strict';
import { acknowledgeInventory } from './inventory_pipeline.mjs';
import { openCatalog } from './incremental_inventory.mjs';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

async function exercise(maxGroups) {
  const total = 1048577, pending = new Set(); let processed = 0, received = 0, requests = 0, peak = 0;
  const catalog = {
    recordReceipts(rows) { assert(rows.length <= 256); for (const row of rows) pending.add(row.revision); peak = Math.max(peak, pending.size); },
    acknowledge(rows) { for (const row of rows) { assert(pending.delete(row.revision)); processed++; } },
  };
  const sql = { unsafe: async (_query, [generation, body]) => {
    assert.equal(generation, 'generation');
    if (++requests > 40) throw Error('ACK round-trip budget exhausted');
    const groups = JSON.parse(body); assert(groups.length <= 128); assert(Buffer.byteLength(body) <= 4 * 1024 ** 2);
    for (const group of groups) {
      assert(group.length <= 256);
      for (const row of group) {
        assert(pending.has(row.revision), 'all page receipts must be durable before SQL');
        assert.equal(row.revision, String(++received));
        assert.equal(row.key, received.toString(16).padStart(32, '0'));
      }
    }
    return [];
  } };
  async function* chunks() {
    for (let i = 1; i <= total;) {
      const rows = [];
      while (rows.length < 256 && i <= total) { rows.push({ scope: 'question', key: i.toString(16).padStart(32, '0'), revision: String(i) }); i++; }
      yield rows;
    }
  }
  await acknowledgeInventory(sql, catalog, 'generation', chunks(), () => {}, maxGroups);
  assert.equal(processed, total); assert.equal(received, total); assert.equal(pending.size, 0); assert(peak <= 32768);
  return { processed, requests, peak_receipts: peak };
}
await assert.rejects(exercise(1), /ACK round-trip budget exhausted/);
console.log(`ok: ${JSON.stringify(await exercise(128))}; original 256-key requests fail the same budget`);

const pages = [], catalog = { recordReceipts() {}, acknowledge() {} };
const sql = { unsafe: async (_query, args) => { pages.push(JSON.parse(args[1])); return []; } };
const wide = i => [{ scope: 'question', key: '界'.repeat(400000), revision: String(i) }];
await acknowledgeInventory(sql, catalog, 'g', [wide(1), wide(2), wide(3), wide(4)]);
assert.deepEqual(pages.map(page => page.length), [3, 1]);
assert(pages.every(page => Buffer.byteLength(JSON.stringify(page)) <= 4 * 1024 ** 2));
pages.length = 0;
const large = [wide(1)[0], wide(2)[0], wide(3)[0], wide(4)[0]];
await acknowledgeInventory(sql, catalog, 'g', [wide(5), large, wide(6)]);
assert.deepEqual(pages.map(page => page.length), [1, 1, 1]); assert.deepEqual(pages[1][0], large);
await assert.rejects(acknowledgeInventory(sql, catalog, 'g', [Array(257).fill(wide(1)[0])]), /chunk exceeds bound/);
await assert.rejects(acknowledgeInventory(sql, catalog, 'g', [], () => {}, 129), /page bound/);
console.log('ok: UTF-8 byte sizing separates wide pages, preserves an existing oversized chunk and rejects unbounded groups');

const dir = mkdtempSync(join(tmpdir(), 'inventory-ack-')), path = join(dir, 'catalog.sqlite');
const engine = '0123456789ab', contract = { v: 1 }, generation = 'g'; let disk;
try {
  disk = openCatalog(path, { engine, contract, generation, create: true });
  disk.setMeta({ ...disk.meta, complete: true });
  disk.beginEpoch({ status: 'sealed', engine, contract, generation, epoch: '1', snapshot: '1:1:',
    started_at: '2026-11-09T00:00:00Z', manifest_sha: 'a'.repeat(64), change_count: 513 }); disk.freezeEpoch();
  const rows = Array.from({ length: 513 }, (_, i) => ({ scope: 'question', key: String(i), revision: String(i + 1) }));
  const groups = [rows.slice(0, 256), rows.slice(256, 512), rows.slice(512)];
  let calls = 0; const localAck = disk.acknowledge.bind(disk);
  disk.acknowledge = changes => { if (++calls === 2) throw Error('interrupted local ACK'); localAck(changes); };
  const accepted = { unsafe: async () => [] };
  await assert.rejects(acknowledgeInventory(accepted, disk, generation, groups), /interrupted local ACK/);
  disk.close(); disk = openCatalog(path);
  assert.equal(disk.meta.epoch.tokens_processed, 256);
  assert.equal(disk.statement('SELECT count(*) n FROM receipts').get().n, 257);
  await acknowledgeInventory(accepted, disk, generation, groups);
  assert.equal(disk.meta.epoch.tokens_processed, 513); assert.equal(disk.pendingAcks().length, 0);
  disk.finishEpoch();
  console.log('ok: reopening midway through local ACK completion replays a database-accepted page without losing or double-counting tokens');
} finally { disk?.close(); rmSync(dir, { recursive: true, force: true }); }
