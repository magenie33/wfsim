// SPDX-License-Identifier: AGPL-3.0-or-later
// Real intake and shipping retain completed batches and retry unacknowledged arrivals.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import postgres from './pg/node_modules/postgres/src/index.js';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { query, close } from './db.mjs';
import { LiveMailbox } from './live_mailbox.mjs';
import { liveIntake } from './live_intake.mjs';

const { intakeBatches, runIntakeCommand } = await import(process.env.WFSIM_INTAKE_MODULE
  ? pathToFileURL(process.env.WFSIM_INTAKE_MODULE).href : './intake_batches.mjs');
const root = fileURLToPath(new URL('../', import.meta.url));
const intake = process.env.WFSIM_INTAKE_BIN || ['release', 'debug']
  .map(profile => join(root, 'target', profile, process.platform === 'win32' ? 'wfsim-intake.exe' : 'wfsim-intake')).find(existsSync);
assert(intake, 'build wfsim-intake before running this check');
const directory = mkdtempSync(join(tmpdir(), 'intake-batches-check-'));
const connection = process.env.WFSIM_INTAKE_PG_URL;
if (connection) assert(['127.0.0.1', 'localhost'].includes(new URL(connection).hostname), 'use a local test database');
const client = connection ? postgres(connection, { max: 1, fetch_types: false, onnotice: () => {} }) : null;
const schema = 'intake_batches_' + randomUUID().replaceAll('-', '');
const env = Object.fromEntries(['PG_URL', 'FACTORY_ORIGIN', 'FACTORY_TOKEN'].map(key => [key, process.env[key]]));
const ddl = ['schema', 'library', 'sync'].map(name => readFileSync(join(root, 'worker/pg', `${name}.sql`), 'utf8')).join('\n');
let askMode = 'ok', familyMode = 'ok';
const requests = [], admitted = new Set();
const server = createServer(async (req, response) => {
  let raw = ''; for await (const chunk of req) raw += chunk;
  const body = raw ? JSON.parse(raw) : null;
  requests.push({ path: req.url, body });
  response.setHeader('content-type', 'application/json');
  if (req.url === '/release.json') { response.end(JSON.stringify({ engine: 'fixture-engine' })); return; }
  if (req.url === '/api/board/family') {
    if (familyMode === 'refuse') { response.writeHead(400); response.end('{}'); return; }
    for (const job of body.jobs) admitted.add(JSON.stringify([job.source, job.weapon, job.ruler, body.engine]));
    response.end(JSON.stringify({ ok: true, engine: body.engine, accepted: body.jobs.length })); return;
  }
  assert.equal(req.url, '/api/board/ask');
  if (askMode === 'refuse') { response.writeHead(400); response.end('{}'); return; }
  if (askMode === 'lost') { response.destroy(); return; }
  response.end(JSON.stringify({ ok: true, asked: askMode === 'partial' ? 0 : body.rows.length, skipped: 0 }));
});
const file = (name, text) => { const path = join(directory, name); writeFileSync(path, text); return path; };
const mods = ['hellfire', 'primary_acuity', 'galvanized_aptitude', 'hammer_shot', 'speed_trigger', 'vigilante_fervor', 'vital_sense', 'magnetic_capacity'];
const arrival = (id, weapon = 'braton_prime', extra = {}) => ({ id, at: new Date(Date.UTC(2026, 10, 1, 0, 0, id.length)).toISOString(),
  record: { weapon, mods, benchmark: 'standard_single_target', mode: 'base', arcanes: ['primary_deadhead'],
    evolutions: ['braton_prime_evo1_incarnon_form', 'braton_prime_daring_reverie', 'braton_prime_voids_guidance', 'braton_prime_prelude_of_might'] }, ...extra });
const family = id => arrival(id);
async function seed(rows) {
  for (const row of rows) await query(`INSERT INTO ${row.id.startsWith('equipment:') ? 'equipment_inbox' : 'inbox'}(id,at,record) VALUES(?,?,?)`,
    [row.id, row.at, JSON.stringify(row.record)]);
  return file('inbox.ndjson', rows.map(row => JSON.stringify(row)).join('\n') + '\n');
}
const pending = async () => (await query('SELECT id FROM inbox UNION ALL SELECT id FROM equipment_inbox ORDER BY id')).map(row => row.id);
const saved = () => query('SELECT id,at,record FROM builds ORDER BY id');
const owed = () => query('SELECT batch,build_id,ruler,mode,initiator FROM queue ORDER BY build_id,ruler,mode');
async function clear() {
  await query('DELETE FROM inbox'); await query('DELETE FROM equipment_inbox');
  await query('DELETE FROM builds'); await query('DELETE FROM equipment_builds');
  await query('DELETE FROM queue'); await query('DELETE FROM batches');
  askMode = familyMode = 'ok'; requests.length = 0; admitted.clear();
}
async function execute(input, options = {}) {
  await close();
  return intakeBatches(input, { intake, maxRows: 1, ...options });
}
async function trip(table, enabled) {
  await query(`DROP TRIGGER IF EXISTS reject_batch ON ${table}`);
  if (enabled) {
    await query(`CREATE OR REPLACE FUNCTION reject_batch_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture write refused'; END $$`);
    await query(`CREATE TRIGGER reject_batch BEFORE ${table === 'inbox' ? 'DELETE' : 'INSERT'} ON ${table} FOR EACH ROW EXECUTE FUNCTION reject_batch_write()`);
  }
}
try {
  if (client) {
    await client.unsafe(`CREATE SCHEMA ${schema}; SET search_path TO ${schema}`).simple();
    await client.unsafe(ddl).simple();
    const url = new URL(connection); url.searchParams.set('options', `-c search_path=${schema},pg_catalog,pg_temp`); process.env.PG_URL = url.href;
  } else {
    process.env.PG_URL = `pglite:${join(directory, 'db')}`;
    const pg = new PGlite(join(directory, 'db')); try { await pg.exec(ddl); } finally { await pg.close(); }
  }
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  process.env.FACTORY_ORIGIN = `http://127.0.0.1:${server.address().port}`; process.env.FACTORY_TOKEN = 'local-fixture';
  console.log(client ? 'intake batches: postgres.js TCP transport' : 'intake batches: PGlite transport');

  const different = arrival('bb'); different.record.mods = mods.map(mod => mod === 'speed_trigger' ? 'vile_acceleration' : mod);
  const rows = [arrival('a'), different, arrival('ccc', 'braton_prime', { initiator: 'user:first' }),
    arrival('dddd', 'braton_prime', { initiator: 'user:later' }), family('eeeee'), arrival('bad', 'not_a_weapon'),
    arrival('equipment:bad', 'torid', { record: { kind: 'companion', build: { companion: 'sentinel', mods: [] } } })];
  const input = await seed(rows);
  const reference = spawnSync(intake, [], { input: rows.map(row => JSON.stringify(row)).join('\n'), encoding: 'utf8' });
  assert.equal(reference.status, 0, reference.stderr);
  const referenceBuilds = reference.stdout.split('\n').filter(Boolean).map(JSON.parse);
  const expected = referenceBuilds.map(({ id, at, record }) => ({ id, at, record: JSON.stringify(record) })).sort((a, b) => a.id.localeCompare(b.id));
  const expectedFamily = new Set(referenceBuilds.flatMap(build => build.family || []).map(job => JSON.stringify([job.source, job.weapon, job.ruler, 'fixture-engine'])));
  const familyCount = referenceBuilds.find(build => build.record.mods.includes('speed_trigger')).family.length;
  assert(familyCount > 0, 'the failure fixture must exercise family admission');
  let calls = 0;
  await assert.rejects(execute(input, { run: async (...args) => {
    if (args[0] === intake && ++calls === 2) throw new Error('cut before second batch');
    return runIntakeCommand(...args);
  } }), /cut before second batch/);
  assert(!((await pending()).includes('a')), 'completed prefix was not spent');
  assert((await pending()).includes('bb'), 'unprocessed arrival was spent');
  assert.equal((await saved()).length, 1); assert.equal((await owed()).length, 1);
  await execute(input);
  assert.deepEqual(await pending(), []);
  assert.deepEqual(await saved(), expected, 'batching changed canonical records or original clocks');
  const queue = await owed(), torid = (await saved()).find(row => JSON.parse(row.record).mods.includes('speed_trigger'));
  assert.equal(queue.find(row => row.build_id === torid.id).initiator, 'user:first', 'first nonempty initiator changed');
  assert.equal((await query('SELECT sum(total) AS n FROM batches'))[0].n, queue.length, 'replay counted existing work');
  assert.deepEqual(admitted, expectedFamily); assert(requests.filter(row => row.path === '/api/board/ask').every(row => row.body.rows.every(item => item.named === true)));
  console.log('ok: a completed prefix survives a cut; replay matches full intake clocks, records, first initiator, family and unique work counts');

  for (const fault of ['build', 'family', 'queue', 'ask', 'lost', 'partial', 'delete']) {
    await clear();
    const source = await seed([family('source')]);
    if (fault === 'build') await trip('builds', true);
    if (fault === 'queue') await trip('queue', true);
    if (fault === 'delete') await trip('inbox', true);
    if (fault === 'family') familyMode = 'refuse';
    if (fault === 'ask') askMode = 'refuse';
    if (['lost', 'partial'].includes(fault)) askMode = fault;
    await assert.rejects(execute(source), undefined, `${fault} failure was acknowledged`);
    assert.deepEqual(await pending(), ['source'], `${fault} failure spent its source`);
    if (fault !== 'build') assert.equal((await saved()).length, 1);
    if (['ask', 'lost', 'partial', 'delete'].includes(fault)) assert.equal((await owed()).length, 1, 'pending work was lost');
    for (const table of ['builds', 'queue', 'inbox']) await trip(table, false);
    familyMode = askMode = 'ok';
    await execute(source);
    assert.deepEqual(await pending(), []); assert.equal((await saved()).length, 1); assert.equal((await owed()).length, 1);
    assert.equal((await query('SELECT sum(total) AS n FROM batches'))[0].n, 1);
    assert.equal(admitted.size, familyCount);
  }
  console.log('ok: build/family/queue/ask/partial ACK/lost reply/delete failures retain arrivals; retries admit one calculation');

  await clear();
  const source = await seed([arrival('one')]);
  await assert.rejects(execute(source, { run: async (...args) => {
    await runIntakeCommand(...args);
    if (args[1].includes(join(root, 'scripts/ship_builds.sh')) && args[1].at(-2).endsWith('/empty')) throw new Error('delete reply lost');
  } }), /delete reply lost/);
  assert.deepEqual(await pending(), []);
  const before = await owed(); await execute(source);
  assert.deepEqual(await owed(), before); assert.equal((await query('SELECT sum(total) AS n FROM batches'))[0].n, 1);
  console.log('ok: losing the final delete reply cannot duplicate work on replay');

  await clear();
  const fifo = await seed([arrival('fifo', 'braton_prime', { initiator: 'user:new' })]);
  const canonical = spawnSync(intake, [], { input: readFileSync(fifo), encoding: 'utf8' });
  assert.equal(canonical.status, 0, canonical.stderr);
  const identity = JSON.parse(canonical.stdout.split('\n')[0]).id;
  await query("INSERT INTO batches(id,at,total) VALUES('existing','2000-01-01',1)");
  await query("INSERT INTO queue(batch,build_id,ruler,mode,initiator) VALUES('existing',?,'standard_single_target','base','user:original')", [identity]);
  await execute(fifo);
  assert.deepEqual((await owed()).map(({ batch, initiator }) => ({ batch, initiator })), [{ batch: 'existing', initiator: 'user:original' }]);
  assert.equal((await query('SELECT sum(total) AS n FROM batches'))[0].n, 1);
  console.log('ok: an existing calculation retains its FIFO batch and original initiator');

  await clear();
  const hooked = await seed([arrival('hooked', 'braton_prime', { initiator: 'user:hook' })]);
  const mailboxWork = join(directory, 'mailbox'); const mailbox = new LiveMailbox(mailboxWork);
  try {
    await assert.rejects(execute(hooked, { onCommitted: async () => {
      assert.equal((await saved()).length, 1); assert.equal((await owed()).length, 1);
      assert.deepEqual(await pending(), ['hooked']);
      assert(requests.some(row => row.path === '/api/board/ask' && row.body.rows.every(item => item.named)));
      throw Error('mailbox unavailable');
    } }), /mailbox unavailable/);
    assert.deepEqual(await pending(), ['hooked']);
    await assert.rejects(execute(hooked, { onCommitted: async builds => {
      await mailbox.append(builds); throw Error('mailbox committed reply lost');
    } }), /mailbox committed reply lost/);
    assert.deepEqual(await pending(), ['hooked']);
    const committed = readFileSync(join(mailboxWork, 'arrived-builds.ndjson'), 'utf8');
    await execute(hooked, { onCommitted: builds => mailbox.append(builds) });
    assert.deepEqual(await pending(), []); assert.equal(readFileSync(join(mailboxWork, 'arrived-builds.ndjson'), 'utf8'), committed);
    assert.equal(committed.trim().split('\n').length, 1); assert.equal((await owed())[0].initiator, 'user:hook');
    await mailbox.take(); assert.equal(readFileSync(join(mailboxWork, 'new-builds.ndjson'), 'utf8'), committed);
  } finally { mailbox.close(); }
  await clear(); const incremental = await seed([arrival('incremental')]); await close();
  await liveIntake(incremental, { work: join(directory, 'incremental'), bin: join(intake, '..'), incremental: 1, maxRows: 1,
    onCommitted: () => assert.fail('incremental intake must rely on the database dirty boundary') });
  assert.deepEqual(await pending(), []);
  console.log('ok: source/family and strict named ACK precede the durable legacy hook and inbox deletion; hook failure/lost reply/replay preserve one identity; incremental intake uses no mailbox');

  await clear();
  const boundRows = [arrival('byte-a'), arrival('byte-b'), arrival('byte-c')];
  boundRows.forEach(row => { row.note = '雪"\\'.repeat(20); });
  const boundInput = await seed(boundRows), sizes = [];
  const maxBytes = Math.max(...boundRows.map(row => Buffer.byteLength(JSON.stringify(row) + '\n')));
  await execute(boundInput, { maxRows: 2, maxBytes, run: async (...args) => {
    if (args[0] === intake) sizes.push(Buffer.byteLength(readFileSync(args[2].input)));
    return runIntakeCommand(...args);
  } });
  assert.equal(sizes.length, 3); assert(sizes.every(size => size <= maxBytes));
  await assert.rejects(execute(boundInput, { maxBytes: 1 }), /byte limit/);
  let launched = false;
  assert.equal((await execute(boundInput, { deadlineSeconds: 0, run: () => { launched = true; } })).batches, 0);
  assert.equal(launched, false);
  console.log('ok: row/UTF-8 byte bounds and the pass deadline stop new work without spending it');
} finally {
  await close(); await new Promise(resolve => server.close(resolve));
  for (const [key, value] of Object.entries(env)) if (value === undefined) delete process.env[key]; else process.env[key] = value;
  try { if (client) await client.unsafe(`DROP SCHEMA IF EXISTS ${schema} CASCADE`); }
  finally { if (client) await client.end({ timeout: 5 }); rmSync(directory, { recursive: true, force: true }); }
}
