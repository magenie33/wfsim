// SPDX-License-Identifier: AGPL-3.0-or-later
// Frozen packages survive response loss while projection and stage controls advance independently.
import assert from 'node:assert/strict';
import { mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BoardPipeline } from './board_pipeline.mjs';
import { pushLive } from '../worker/live_board.js';

const root = mkdtempSync(join(tmpdir(),'board-handoff-')), boardDir = join(root,'verified'); mkdirSync(boardDir);
const token = process.env.BOARD_PUSH_TOKEN; process.env.BOARD_PUSH_TOKEN = 'fixture-only';
let producer = new BoardPipeline(root), consumer = new BoardPipeline(root), success = false;
const remote = new Map(), calls = [], stamps = [];
const env = { BOARD_PUSH_TOKEN:'fixture-only', PUBLIC:{ put:async (key,body) => {
  remote.set(key,Buffer.from(body).toString()); if (key === 'board.meta.json') stamps.push(new Map(remote));
} } };
const send = async (url,init) => {
  calls.push(new URL(url).pathname);
  return pushLive(new Request(url,init),env,new URL(url).pathname.split('/api/board/live/')[1]);
};
const write = n => {
  writeFileSync(join(boardDir,'braton_prime.json'),JSON.stringify([{ score:n,mods:[] }]));
  writeFileSync(join(boardDir,'meta.json'),JSON.stringify({ stamp:'round-' + n }));
};
async function drain() { for (let n = 0; n < 20 && consumer.status().publication.pending; n++) await consumer.deliver({ send,maxFiles:1 });
  assert.equal(consumer.status().publication.pending,0); }
try {
  write(1);
  await assert.rejects(producer.enqueue({ epoch:'one',boardDir,fault:async () => { throw Error('lost before commit'); } }), /lost before commit/);
  assert.equal(consumer.status().publication.pending,0);
  const first = await producer.enqueue({ epoch:'one',boardDir });
  assert.equal((await producer.enqueue({ epoch:'one',boardDir })).seq,first.seq);
  consumer.control('publication',true);
  assert.deepEqual(await consumer.deliver({ send }),{ paused:true });
  write(2); await producer.enqueue({ epoch:'two',boardDir });
  assert.equal(consumer.status().publication.pending,2);
  consumer.control('publication',false);
  await assert.rejects(consumer.deliver({ send,fault:async () => { throw Error('lost upload reply'); } }), /lost upload reply/);
  assert.equal(calls.length,1); assert.equal(stamps.length,0);
  consumer.close(); consumer = new BoardPipeline(root);
  await drain();
  assert.equal(calls.filter(name => name.endsWith('/braton_prime.json')).length,3);
  assert.equal(stamps.length,2);
  assert.equal(JSON.parse(stamps[0].get('board/braton_prime.json'))[0].score,1);
  assert.equal(JSON.parse(stamps[0].get('board.meta.json')).stamp,'round-1');
  assert.equal(JSON.parse(stamps[1].get('board/braton_prime.json'))[0].score,2);
  console.log('ok: crash before commit, paused publication, mutable next projection and lost upload reply preserve exact package bytes; actual receiver sees each stamp last');

  write(3); await producer.enqueue({ epoch:'three',boardDir });
  let entered,release;
  const started = new Promise(resolve => { entered=resolve; }), hold = new Promise(resolve => { release=resolve; });
  const delivery = consumer.deliver({ send:async (...args) => { entered(); await hold; return send(...args); } });
  await started;
  write(4); await producer.enqueue({ epoch:'four',boardDir });
  assert.equal(producer.status().publication.pending,2,'a slow uploader held projection or lost its input');
  const contender = new BoardPipeline(root);
  try { assert.equal((await contender.deliver({ send })).busy,true); } finally { contender.close(); }
  release(); await delivery; await drain();
  assert.equal(stamps.length,4);
  assert.equal(JSON.parse(stamps[2].get('board/braton_prime.json'))[0].score,3);
  assert.equal(JSON.parse(stamps[3].get('board/braton_prime.json'))[0].score,4);
  console.log('ok: slow publication permits another committed projection; a concurrent publisher cannot reorder packages');

  const limited = new BoardPipeline(join(root,'limited'),{ maxBytes:1 });
  try {
    await assert.rejects(limited.enqueue({ epoch:'full',boardDir }), error => error.code === 'PUBLICATION_BACKPRESSURE');
    assert.equal(limited.status().publication.pending,0);
    assert.equal(readFileSync(join(boardDir,'meta.json'),'utf8'),'{"stamp":"round-4"}');
  } finally { limited.close(); }
  producer.control('projection',true); assert(consumer.paused('projection')); assert(!consumer.paused('intake'));
  producer.control('projection',false);
  write(5); await producer.enqueue({ epoch:'five',boardDir });
  const row = producer.db.prepare('SELECT sha FROM relation_board_publication_files WHERE package=(SELECT max(seq) FROM record_board_publication_packages) AND name=?').get('braton_prime.json');
  writeFileSync(join(producer.blobs,row.sha),'tampered');
  await assert.rejects(consumer.deliver({ send }), /immutable publication artifact changed/);
  assert.equal(consumer.status().publication.pending,1);
  console.log('ok: disk backpressure and corrupted immutable artifacts retain pending work; controls remain independent');
  success = true;
} finally {
  producer.close(); consumer.close();
  if (token === undefined) delete process.env.BOARD_PUSH_TOKEN; else process.env.BOARD_PUSH_TOKEN=token;
  if (success) rmSync(root,{ recursive:true,force:true }); else console.error('board handoff evidence: ' + root);
}
