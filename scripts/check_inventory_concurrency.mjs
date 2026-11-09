// Real independent PostgreSQL transactions exercise writer and receipt fences.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { randomUUID, createHash } from 'node:crypto';
import { setTimeout as pause } from 'node:timers/promises';
import postgres from './pg/node_modules/postgres/src/index.js';
import { inventoryCursor, acknowledgeInventory } from './inventory_pipeline.mjs';

const name = `wfsim-inventory-check-${randomUUID()}`;
const docker = (...args) => execFileSync('docker', args, { encoding: 'utf8', timeout: 120000 });
const ddl = name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), 'utf8');
const source = ddl('inventory');
const definition = name => source.match(new RegExp(`CREATE OR REPLACE FUNCTION ${name}\\([\\s\\S]*?END \\$\\$;`))[0];
const oldDefinition = name => definition(name).replace("PERFORM set_config('lock_timeout','1s',true);",
  "PERFORM set_config('lock_timeout','1s',true); LOCK TABLE inventory_dirty IN SHARE MODE;");
const engine = '0123456789ab', contract = { v: 1, covers: ['board'] };
let a, b, generation, checks = 0;
const call = (name, args = []) => a.unsafe(`SELECT ${name}(${args.map((_, i) => `$${i + 1}::text`).join(',')}) value`,
  args.map(v => v === null || typeof v === 'string' ? v : JSON.stringify(v))).then(r => r[0].value);
const lockTimeout = error => error.code === '55P03';
const body = () => ({ v: 1, generation, contract, source: 'fixture', engine, total: 0, registered: 0, orders: [], verbs: {} });
const check = async (name, fn) => { await fn(); checks++; console.log(`ok: ${name}`); };
try {
  docker('run', '--rm', '-d', '--name', name, '-e', 'POSTGRES_HOST_AUTH_METHOD=trust', '-p', '127.0.0.1::5432', 'postgres:18');
  const port = docker('port', name, '5432/tcp').trim().split(':').at(-1);
  const connect = () => postgres(`postgres://postgres@127.0.0.1:${port}/postgres`, {
    max: 1, fetch_types: false, connect_timeout: 2, onnotice: () => {}, connection: { statement_timeout: 5000 } });
  a = connect(); b = connect();
  for (let attempt = 0; ; attempt++) {
    try { await a.unsafe('SELECT 1'); break; }
    catch (error) { if (attempt === 60) throw error; await pause(250); }
  }
  await a.unsafe(['schema', 'library', 'sync'].map(ddl).join('\n') + '\n' + source);
  await check('old begin fails under a writer; new begin preserves its late dirty key', async () => {
    await a.unsafe(oldDefinition('inventory_begin'));
    await b.unsafe("BEGIN; INSERT INTO builds(id,at,record) VALUES ('late','at','{}')");
    await assert.rejects(call('inventory_begin', [engine, contract, null]), lockTimeout);
    await a.unsafe(definition('inventory_begin'));
    generation = (await call('inventory_begin', [engine, contract, null])).generation;
    await b.unsafe('COMMIT');
    assert.equal((await a.unsafe('SELECT * FROM inventory_changes(256)'))[0].key, 'late');
  });
  let cut;
  await check('sealed cut publishes alongside an open writer and leaves its later commit dirty', async () => {
    await a.unsafe('BEGIN ISOLATION LEVEL REPEATABLE READ');
    cut = await call('inventory_epoch_begin', [generation]);
    const [{ value: rows }] = await a.unsafe('SELECT inventory_epoch_seal($1::text,1::bigint,$2::text) value', [cut.epoch, 'a'.repeat(64)]);
    assert.equal(rows.status, 'sealed');
    await a.unsafe('COMMIT');
    await b.unsafe("BEGIN; INSERT INTO builds(id,at,record) VALUES ('after-cut','at','{}')");
    await a.unsafe(oldDefinition('inventory_publish_cut'));
    await assert.rejects(call('inventory_publish_cut', [cut.epoch, body()]), lockTimeout);
    await a.unsafe(definition('inventory_publish_cut'));
    const published = await call('inventory_publish_cut', [cut.epoch, body()]);
    assert.equal(published.snapshot, cut.snapshot); assert.equal(published.started_at, cut.started_at);
    await b.unsafe('COMMIT');
    assert((await a.unsafe('SELECT * FROM inventory_changes(256)')).some(r => r.key === 'after-cut'));
  });
  await check('acknowledgement rechecks the revision after a concurrent writer commits', async () => {
    const changes = await a.unsafe("SELECT * FROM inventory_changes(256) WHERE key='late'");
    await b.unsafe("BEGIN; UPDATE builds SET at='changed' WHERE id='late'");
    const acknowledged = call('inventory_ack', [generation, changes]);
    await pause(100); await b.unsafe('COMMIT');
    assert.equal(await acknowledged, '0');
    const kept = (await a.unsafe("SELECT * FROM inventory_changes(256) WHERE key='late'"))[0];
    assert.notEqual(kept.revision, changes[0].revision);
  });
  await check('capture still fences a generation change, and a replaced generation rejects old receipts', async () => {
    await b.unsafe('BEGIN ISOLATION LEVEL REPEATABLE READ');
    await b.unsafe('SELECT inventory_epoch_begin($1::text)', [generation]);
    await assert.rejects(call('inventory_begin', [engine, contract, generation]), lockTimeout);
    await b.unsafe('ROLLBACK');
    const previous = generation;
    generation = (await call('inventory_begin', [engine, contract, generation])).generation;
    await assert.rejects(call('inventory_begin', [engine, contract, previous]), /generation changed/);
    await assert.rejects(call('inventory_ack', [previous, []]), /generation changed/);
    await assert.rejects(call('inventory_publish_cut', [cut.epoch, { ...body(), generation: previous }]), /fence changed/);
  });
  await check('adaptive fetches preserve a frozen cursor across late writes and separate statement budgets', async () => {
    await a.unsafe('CREATE TABLE capture_fixture(id integer PRIMARY KEY, value text)');
    await a.unsafe("INSERT INTO capture_fixture SELECT i,'before' FROM generate_series(1,200001) i");
    const pages = []; let count = 0;
    await a.begin('isolation level repeatable read read only', async tx => {
      await tx.unsafe("SET LOCAL statement_timeout='1s'");
      for await (const row of inventoryCursor(tx, 'capture_fixture', 'SELECT id,value FROM capture_fixture ORDER BY id', [],
        { maxRows: 80000, report: page => pages.push(page) })) {
        assert.equal(row.id, ++count); assert.equal(row.value, 'before');
        if (count === 5000) {
          await b.unsafe("UPDATE capture_fixture SET value='after'; INSERT INTO capture_fixture VALUES (200002,'after')");
          await pause(1100);
        }
      }
    });
    assert.equal(count, 200001); assert(pages.length <= 5);
    assert(pages.some(page => page.page_rows === 80000));
    assert.equal((await b.unsafe('SELECT count(*)::integer n FROM capture_fixture'))[0].n, 200002);
  });
  await check('owned cursor streams without a whole-backlog spool and retains exact late-write ACK fences', async () => {
    await a.unsafe("INSERT INTO inventory_dirty(scope,key) SELECT 'question','stream-'||lpad(i::text,6,'0') FROM generate_series(1,200001) i");
    await a.unsafe('ANALYZE inventory_dirty');
    const expected = await a.unsafe('SELECT scope,key,revision::text FROM inventory_dirty ORDER BY scope,key');
    const digest = rows => { const hash = createHash('sha256'); for (const row of rows) hash.update(JSON.stringify(row)+'\n'); return hash.digest('hex'); };
    const expectedHash = digest(expected), count = expected.length;
    await assert.rejects(a.begin('isolation level repeatable read', async tx => {
      await tx.unsafe("SET LOCAL work_mem='64kB'; SET LOCAL temp_file_limit='4MB'");
      const [{ cut }] = await tx.unsafe('SELECT inventory_epoch_begin($1::text) cut', [generation]);
      await tx.unsafe('DECLARE old_tokens NO SCROLL CURSOR FOR SELECT * FROM inventory_epoch_changes($1::text)', [cut.epoch]);
      await tx.unsafe('FETCH FORWARD 5000 FROM old_tokens');
    }), error => error.code === '53400');
    await a.unsafe(`CREATE ROLE inventory_stream_reader;
      GRANT EXECUTE ON FUNCTION inventory_epoch_begin(text),inventory_epoch_open(text),inventory_epoch_seal(text,bigint,text)
      TO inventory_stream_reader`);
    let frozen, portal; const hash = createHash('sha256'); let consumed = 0;
    await a.begin('isolation level repeatable read', async tx => {
      await tx.unsafe("SET LOCAL work_mem='64kB'; SET LOCAL temp_file_limit='4MB'; SET LOCAL ROLE inventory_stream_reader");
      const refused = async (sql, args, pattern) => {
        await assert.rejects(tx.savepoint(inner => inner.unsafe(sql, args)), pattern);
      };
      await refused('SELECT * FROM inventory_dirty', [], /permission denied/);
      await refused("UPDATE inventory_epochs SET status='sealed'", [], /permission denied/);
      await refused('SELECT inventory_epoch_open($1::text)', ['99999999'], /does not own this snapshot/);
      const [{ cut }] = await tx.unsafe('SELECT inventory_epoch_begin($1::text) cut', [generation]);
      [{ portal }] = await tx.unsafe('SELECT inventory_epoch_open($1::text) portal', [cut.epoch]);
      for await (const row of inventoryCursor(tx, portal, null, [], { maxRows: 80000 })) {
        consumed++; hash.update(JSON.stringify(row)+'\n');
        if (consumed === 5000) await b.unsafe("UPDATE inventory_dirty SET revision=nextval('inventory_revision') WHERE key='stream-100000'; INSERT INTO inventory_dirty(scope,key) VALUES ('question','late-stream')");
      }
      assert.equal(consumed, count); assert.equal(hash.digest('hex'), expectedHash);
      await refused('SELECT inventory_epoch_seal($1::text,$2::bigint,$3::text)', [cut.epoch, count-1, expectedHash], /changes incomplete/);
      [{ frozen }] = await tx.unsafe('SELECT inventory_epoch_seal($1::text,$2::bigint,$3::text) frozen', [cut.epoch, count, expectedHash]);
      await refused('SELECT inventory_epoch_open($1::text)', [cut.epoch], /does not own this snapshot/);
    });
    assert.equal(frozen.status, 'sealed');
    await assert.rejects(a.unsafe(`FETCH FORWARD 1 FROM ${portal}`), error => error.code === '34000');
    await assert.rejects(call('inventory_epoch_open', [frozen.epoch]), /does not own this snapshot/);
    const selected = expected.find(row => row.key === 'stream-100001');
    let acknowledged = 0;
    const catalog = { recordReceipts() {}, acknowledge(rows) { acknowledged += rows.length; } };
    await assert.rejects(acknowledgeInventory(a, catalog, generation, [[selected], [{ ...selected, revision: '0' }]]), /invalid inventory change key/);
    assert.equal(acknowledged, 0);
    assert.equal((await a.unsafe("SELECT count(*)::integer n FROM inventory_dirty WHERE key='stream-100001'"))[0].n, 1,
      'one invalid chunk must roll back earlier deletions in the same ACK page');
    const groups = [];
    for (let i = 0; i < expected.length; i += 256) groups.push(expected.slice(i, i + 256));
    await acknowledgeInventory(a, catalog, generation, groups);
    assert.equal(acknowledged, expected.length);
    assert.equal((await a.unsafe('SELECT count(*)::integer n FROM inventory_dirty'))[0].n, 2);
    assert.equal((await a.unsafe("SELECT count(*)::integer n FROM inventory_dirty WHERE key IN ('late-stream','stream-100000')"))[0].n, 2);
  });
  await check('health SQL reads real PostgreSQL activity without changing inventory', async () => {
    const sql = execFileSync('python3', ['-c', 'import sys; sys.path.insert(0,"scripts"); import pg_health; print(pg_health.SQL)'], { encoding: 'utf8' });
    const before = await call('inventory_status');
    await a.unsafe(sql);
    assert.deepEqual(await call('inventory_status'), before);
  });
  console.log(`${checks} inventory concurrency checks passed (including both broken-lock controls)`);
} finally {
  await Promise.allSettled([a?.end({ timeout: 1 }), b?.end({ timeout: 1 })]);
  docker('rm', '-f', name);
}
