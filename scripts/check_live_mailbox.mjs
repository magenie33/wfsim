// SPDX-License-Identifier: AGPL-3.0-or-later
// Lost replies cannot duplicate or lose the legacy coordinator's durable inputs.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, truncateSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { LiveMailbox } from './live_mailbox.mjs';

const root = mkdtempSync(join(tmpdir(), 'live-mailbox-'));
const rows = [{ id: 'a', at: 'first', record: { weapon: 'braton_prime' } },
  { id: 'equipment', at: 'first', record: { kind: 'companion' } },
  { id: 'b', at: 'next', record: { weapon: 'torid' } }, { id: 'a', at: 'later', record: { weapon: 'braton_prime' } }];
const input = join(root, 'input'); writeFileSync(input, rows.map(row => JSON.stringify(row) + '\n').join(''));
const expected = [rows[0], rows[2]].map(row => JSON.stringify(row) + '\n').join('');
try {
  for (const stage of ['after-mailbox-intent','after-mailbox-before-ack','after-mailbox-commit-before-reply']) {
    const work = join(root, stage); let lost = true;
    let mailbox = new LiveMailbox(work, async point => { if (point === stage && lost) { lost = false; throw Error(stage); } });
    await assert.rejects(mailbox.append(input), new RegExp(stage)); mailbox.close(); mailbox = new LiveMailbox(work);
    try {
      await mailbox.append(input); assert.equal(readFileSync(join(work, 'arrived-builds.ndjson'), 'utf8'), expected);
      await mailbox.take(); assert.equal(readFileSync(join(work, 'new-builds.ndjson'), 'utf8'), expected);
      await mailbox.append(input); await mailbox.take(); assert.equal(readFileSync(join(work, 'new-builds.ndjson'), 'utf8'), expected);
    } finally { mailbox.close(); }
  }
  for (const stage of ['after-take-intent','after-take-rename','after-take-spool-before-ack','after-mailbox-commit-before-reply']) {
    const work = join(root, 'take-' + stage); let mailbox = new LiveMailbox(work); await mailbox.append(input); mailbox.close();
    let lost = true; mailbox = new LiveMailbox(work, async point => { if (point === stage && lost) { lost = false; throw Error(stage); } });
    await assert.rejects(mailbox.take(), new RegExp(stage)); mailbox.close(); mailbox = new LiveMailbox(work);
    try { await mailbox.recover(); await mailbox.take(); assert.equal(readFileSync(join(work, 'new-builds.ndjson'), 'utf8'), expected); }
    finally { mailbox.close(); }
  }
  const work = join(root, 'fanout'); let mailbox = new LiveMailbox(work);
  try {
    const expanded = Array.from({ length: 257 }, (_, i) => ({ id: String(i), at: 'first', record: { weapon: 'braton_prime', source: i } }));
    writeFileSync(input, expanded.map(row => JSON.stringify(row) + '\n').join(''));
    await mailbox.append(input); await mailbox.take(); assert.deepEqual(readFileSync(join(work, 'new-builds.ndjson'), 'utf8').trim().split('\n').map(JSON.parse), expanded);
    writeFileSync(input, JSON.stringify({ id: 'new', record: { weapon: 'torid' } }) + '\n');
    await mailbox.append(input); mailbox.close();
    let failed = true; mailbox = new LiveMailbox(work, async stage => { if (stage === 'after-mailbox-intent' && failed) { failed = false; throw Error('cut'); } });
    writeFileSync(input, JSON.stringify({ id: 'last', record: { weapon: 'torid' } }) + '\n');
    await assert.rejects(mailbox.append(input), /cut/); mailbox.close(); truncateSync(join(work, 'arrived-builds.ndjson'), 0);
    mailbox = new LiveMailbox(work); await assert.rejects(mailbox.recover(), /prefix is truncated/);
    assert.equal(mailbox.db.prepare('SELECT 1 FROM record_live_mailbox_acceptances WHERE id=?').get('last'), undefined);
  } finally { mailbox.close(); }
  console.log('ok: seven append/take/lost-reply recovery points preserve FIFO/first identity with no duplicate; 257 fan-out builds stay complete; truncated durable prefixes refuse ACK');
} finally { rmSync(root, { recursive: true, force: true }); }
