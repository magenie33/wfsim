// SPDX-License-Identifier: AGPL-3.0-or-later
// Large library exports stay within a bounded heap and preserve source records.
import assert from 'node:assert/strict';
import { mkdtempSync, openSync, closeSync, writeSync, readFileSync, writeFileSync, rmSync, createReadStream } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { libraryArray } from './library_array.mjs';

const dir = mkdtempSync(join(tmpdir(), 'inventory-library-'));
try {
  const input = join(dir, 'keyed.ndjson'), output = join(dir, 'library.json');
  const records = [{ weapon: 'torid', rolls: [0.1, 1.0000000000000002], text: '中文\n"' }, { nested: [null, { rank: 0 }] }, null];
  writeFileSync(input, records.map((v, k) => JSON.stringify({ k, v })).join('\n') + '\n');
  assert.equal(await libraryArray(input, output), records.length);
  assert.deepEqual(JSON.parse(readFileSync(output, 'utf8')), records);
  const completed = readFileSync(output, 'utf8');
  writeFileSync(input, '{"v":{"weapon":"valid"}}\ninvalid\n');
  await assert.rejects(libraryArray(input, output), SyntaxError);
  assert.equal(readFileSync(output, 'utf8'), completed);
  writeFileSync(input, '');
  assert.equal(await libraryArray(input, output), 0);
  assert.deepEqual(JSON.parse(readFileSync(output, 'utf8')), []);

  const record = { weapon: 'torid', context: 'x'.repeat(4096) }, encoded = JSON.stringify(record);
  const source = openSync(input, 'w'), expected = createHash('sha256');
  expected.update('[');
  try {
    for (let i = 0; i < 20000; i++) {
      writeSync(source, JSON.stringify({ k: i, v: record }) + '\n');
      expected.update((i ? ',' : '') + encoded);
    }
  } finally { closeSync(source); }
  expected.update(']\n');
  const count = execFileSync(process.execPath, ['--max-old-space-size=24', new URL('./library_array.mjs', import.meta.url).pathname, input, output],
    { encoding: 'utf8', timeout: 30000 });
  assert.equal(count.trim(), '20000');
  const actual = createHash('sha256');
  for await (const block of createReadStream(output)) actual.update(block);
  assert.equal(actual.digest('hex'), expected.digest('hex'));
  console.log('ok: records, number precision, empty input, failed export and 80MiB input under a 24MiB heap');
} finally { rmSync(dir, { recursive: true, force: true }); }
