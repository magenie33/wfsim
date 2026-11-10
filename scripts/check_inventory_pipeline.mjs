// A frozen Postgres cut crosses the actual ranker, catalog and full classifier.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, openSync, closeSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { DatabaseSync } from 'node:sqlite';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';
import { runInventoryCycle } from './inventory_pipeline.mjs';
import { captureInventory } from './capture_compute_inventory.mjs';
import { classifyInventory } from './classify_compute_inventory.mjs';
import { openCatalog } from './incremental_inventory.mjs';
import { questionId } from '../worker/task_contract.js';
import { seedComputeWork,replaceComputeResult } from './pg/fixtures.mjs';
import { placeholders } from '../worker/db.js';

const supplied = process.env.WFSIM_INVENTORY_BOARD_BIN;
const binary = resolve(supplied || 'target/debug/wfsim-board');
if (!existsSync(binary)) throw new Error('build wfsim-board or set WFSIM_INVENTORY_BOARD_BIN for the actual inventory protocol check');
const parent = process.env.WFSIM_INVENTORY_PIPELINE_EVIDENCE || tmpdir();
mkdirSync(parent, { recursive: true });
const dir = mkdtempSync(join(parent, 'inventory-pipeline-')), bin = join(dir, 'bin'), work = join(dir, 'work');
mkdirSync(bin); mkdirSync(work);
copyFileSync(binary, join(bin, 'wfsim-board'));
const engine = '0ae1bf8edb0f';
writeFileSync(join(bin, 'ENGINE'), engine); writeFileSync(join(bin, 'VERSION'), 'inventory-pipeline-fixture');
const file = (name, body) => { const path = join(dir, name); writeFileSync(path, typeof body === 'string' ? body : JSON.stringify(body)); return path; };
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const rows = path => readFileSync(path, 'utf8').split('\n').filter(Boolean).map(line => JSON.parse(line));
const roster = execFileSync(binary, ['--rulers'], { encoding: 'utf8', timeout: 30000 }).trim().split(/\s+/);
const seed = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(r => r.record);
const canonical = execFileSync(binary, ['--inventory-builds'], { input: JSON.stringify(seed), encoding: 'utf8', timeout: 30000 })
  .trim().split('\n').map(line => JSON.parse(line));
assert.equal(canonical.length, 3);
const [c, b, d] = canonical, alias = 'a'.repeat(32), future = await questionId('future_verb', { input: 'future' });
const pg = new PGlite();
const q = (text, args = []) => pg.query(text, args).then(result => result.rows);
const fixtureDb={run:(text,...args)=>q(placeholders(text),args),get:async(text,...args)=>(await q(placeholders(text),args))[0]};
const scalar = async (text, args = []) => Object.values((await q(text, args))[0])[0];
let afterCapture = null, ackCalls = 0, failCohort = false, failAckReply = false, child, replies, reader, succeeded = false;
const commandLog = [], ackEvidence = [];
const statePath = join(work, 'inventory/state.json');
const unsafe = async (text, args = []) => {
  if (/\binventory_ack\(/.test(text)) {
    const state = json(statePath), db = new DatabaseSync(state.catalog, { readOnly: true });
    try {
      const meta = JSON.parse(db.prepare("SELECT body FROM state_inventory_generation WHERE id=1").get().body);
      assert.equal(meta.complete, true, 'ACK preceded a complete durable catalog');
      assert.equal(db.prepare("SELECT count(*) n FROM state_inventory_jobs").get().n, 0, 'ACK preceded all frozen cohort jobs');
      const changes = JSON.parse(args[1]).flat();
      for (const row of changes) assert(db.prepare("SELECT 1 FROM record_inventory_applied_changes WHERE scope=? AND key=? AND revision=?")
        .get(row.scope, row.key, row.revision), 'ACK preceded durable local receipt');
      ackEvidence.push({ generation: args[0], rows: changes.length }); ackCalls++;
    } finally { db.close(); }
  }
  if (!args.length && text.includes(';')) { await pg.exec(text); return []; }
  const result = await q(text, args);
  if (/\binventory_ack\(/.test(text) && failAckReply) { failAckReply = false; throw new Error('injected lost ACK reply'); }
  return result;
};
const sql = { unsafe, begin: async (options, fn) => {
  await pg.exec(`BEGIN ${options}`);
  try {
    const result = await fn({ unsafe }); await pg.exec('COMMIT');
    if (afterCapture && result?.receipt?.status === 'sealed') { const hook = afterCapture; afterCapture = null; await hook(result); }
    return result;
  } catch (error) { await pg.exec('ROLLBACK'); throw error; }
} };
const call = async (request, output) => {
  commandLog.push({ op: request.op, epoch: request.epoch, base_epoch: request.base_epoch, output });
  if (failCohort && request.op === 'cohort') { failCohort = false; throw new Error('injected engine cut before durable cohort'); }
  const path = `${output}.request.json`; writeFileSync(path, JSON.stringify(request));
  child.stdin.write(`inventory ${path} ${output}\n`);
  let timer;
  try {
    const reply = await Promise.race([replies.next(), new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('inventory test ranker deadline')), 30000); })]);
    assert(!reply.done && reply.value.startsWith('ok inventory '), `ranker refused ${request.op}: ${reply.value}`);
  } finally { clearTimeout(timer); }
};
const mature = () => { const state = json(statePath); state.published_at = '2000-01-01T00:00:00.000Z'; writeFileSync(statePath, JSON.stringify(state)); };
const catalogState = () => {
  const state = json(statePath), catalog = openCatalog(state.catalog);
  try { return { state, meta: structuredClone(catalog.meta), digest: catalog.digest(), summary: catalog.summary(), tuples: [...catalog.tuples()] }; }
  finally { catalog.close(); }
};
async function fullOracle(raw, meta, tag, workers) {
  const out = join(dir, `oracle-${tag}`); mkdirSync(out);
  const stderr = openSync(join(out, 'stderr'), 'w');
  try {
    execFileSync(binary, ['--each-ruler', '--dry-run', '--gate', '--queue-in', raw.queue, '--facts-in', raw.facts,
      '--inventory-builds', join(out, 'canonical'), '--queue-missing', join(out, '{id}.missing'), '--queue-orphans', join(out, '{id}.orphans'),
      ...(workers === undefined ? [] : ['--inventory-workers', String(workers)])],
    { input: readFileSync(raw.library), stdio: ['pipe', 'pipe', stderr], timeout: 60000 });
  } finally { closeSync(stderr); }
  for (const suffix of ['missing', 'orphans']) writeFileSync(join(out, suffix), roster.map(r => readFileSync(join(out, `${r}.${suffix}`), 'utf8')).join(''));
  return classifyInventory({ ...raw, meta, rulers: roster, canonical: join(out, 'canonical'), missing: join(out, 'missing'),
    orphans: join(out, 'orphans'), questions: join(out, 'questions') });
}
async function currentOracle(tag) {
  const root = join(dir, `capture-oracle-${tag}`); mkdirSync(root);
  const raw = Object.fromEntries(['library', 'builds', 'facts', 'queue', 'tasks', 'meta'].map(name => [name, join(root, name)]));
  const meta = await captureInventory(sql, raw, engine);
  return fullOracle(raw, meta, tag);
}
function equalPlan(expected, actual) {
  assert.equal(actual.digest, expected.question_digest, 'incremental tuples differ from full each-ruler/classifier oracle');
  for (const key of ['rows', 'queued', 'orders', 'verbs']) assert.deepEqual(actual.summary[key], JSON.parse(JSON.stringify(expected[key])), `full ${key} differ`);
}
async function cycle(instance = 'working', budgetMs = 15000) {
  return runInventoryCycle({ work, bin, instance, sql, call, budgetMs });
}

try {
  await pg.exec(['schema', 'library', 'sync', 'inventory'].map(name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), 'utf8')).join('\n'));
  for (const item of [c, b, d]) await q('INSERT INTO entity_weapon_builds(id,at,record) VALUES ($1,\'day\',$2)', [item.id, JSON.stringify(item.record)]);
  await q('INSERT INTO entity_weapon_builds(id,at,record) VALUES ($1,\'day\',$2)', [alias, JSON.stringify({ ...c.record, alias_note: 'raw source' })]);
  await q("INSERT INTO entity_weapon_builds(id,at,record) VALUES ('invalid-source','day','{\"weapon\":\"not-a-weapon\"}')");
  await q("INSERT INTO entity_compute_batches(id,at,total) VALUES ('one','at',2),('two','at',2)");
  await q(`INSERT INTO state_board_pending_work(batch,build_id,ruler,mode) VALUES ('one',$1,$3,'base'),('two',$1,$3,'base'),
    ('one',$2,'retired-ruler','base'),('one','orphan',$3,'base')`, [c.id, b.id, roster[0]]);
  await q(`INSERT INTO entity_measurement_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,baseline_score,
    baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at)
    VALUES ('ref','review','round',$1,$2,'base',$3,'review-question','{}','waiting',100,'kpm',1,'old',
      '2026-11-01T00:00:00.000Z','2026-11-01T00:00:01.000Z','at')`, [d.id, roster[0], engine]);
  await q(`INSERT INTO state_board_measurement_selection(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at,review_id)
    VALUES ($1,$2,'base','old',100,'kpm',1,'2026-11-01T00:00:00.000Z','2026-11-01T00:00:01.000Z','review')`, [d.id, roster[0]]);
  await seedComputeWork(fixtureDb,{id:future,verb:'future_verb',request:'{}',state:'open',due_at:'due',opened_at:'at'});
  await q(`INSERT INTO relation_task_consumers(producer,ref,question,state,asked_at,due_at) VALUES ('future_owner','future',$1,'live','at','due')`, [future]);
  const errorFd = openSync(join(dir, 'ranker.stderr'), 'w');
  child = spawn(binary, ['--serve', join(dir, 'published-board'), '--library', file('empty-library', []), '--builds', file('empty-builds', ''),
    '--facts-in', file('empty-facts', ''), '--state', join(dir, 'board-state.yaml')], { stdio: ['pipe', 'pipe', errorFd] }); closeSync(errorFd);
  reader = createInterface({ input: child.stdout }); replies = reader[Symbol.asyncIterator]();
  let readyTimer;
  try { assert.match((await Promise.race([replies.next(), new Promise((_, reject) => {
    readyTimer = setTimeout(() => reject(new Error('inventory test ranker startup deadline')), 30000);
  })])).value, /^ok ready /); } finally { clearTimeout(readyTimer); }

  failCohort = true;
  const initialDirty = Number(await scalar('SELECT count(*) FROM state_compute_inventory_invalidations'));
  await assert.rejects(cycle('injected-crash'), /injected engine cut/);
  assert.equal(ackCalls, 0); assert.equal(Number(await scalar('SELECT count(*) FROM state_compute_inventory_invalidations')), initialDirty);
  assert.equal(Number(await scalar("SELECT count(*) FROM state_factory_findings WHERE kind='task_inventory'")), 0);
  console.log('ok: a cut before durable cohort completion neither ACKs nor publishes');

  let bootstrapRaw, bootstrapMeta, originalReceipt, bootstrapCapture;
  afterCapture = async cut => {
    const frozen = join(dir, 'frozen-bootstrap'); mkdirSync(frozen);
    bootstrapRaw = Object.fromEntries(Object.entries(cut.raw).map(([name, path]) => {
      const copied = join(frozen, name); copyFileSync(path, copied); return [name, copied];
    }));
    bootstrapMeta = cut.meta; originalReceipt = cut.receipt; bootstrapCapture = dirname(cut.capture);
    await replaceComputeResult(fixtureDb,future,{state:'fact',canon:'{}',fact_engine:engine,fact_at:'2026-11-01T00:00:01.000Z'});
  };
  failAckReply = true;
  await assert.rejects(cycle(), /injected lost ACK reply/);
  const interrupted = openCatalog(json(statePath).catalog);
  try { assert(interrupted.pendingAcks().length > 0, 'lost reply must retain durable receipts'); }
  finally { interrupted.close(); }
  const boot = await cycle(); assert.equal(boot.pending, false);
  assert.equal(boot.published.started_at, originalReceipt.started_at); assert.equal(boot.published.snapshot, originalReceipt.snapshot);
  assert.equal(Number(await scalar("SELECT count(*) FROM state_compute_inventory_invalidations WHERE scope='question' AND key=$1", [future])), 1);
  const firstOracle = await fullOracle(bootstrapRaw, bootstrapMeta, 'bootstrap'); equalPlan(firstOracle, catalogState());
  for (const workers of [1, 2]) {
    const limited = await fullOracle(bootstrapRaw, bootstrapMeta, `workers-${workers}`, workers);
    for (const key of ['rows', 'queued', 'orders', 'verbs', 'question_digest']) assert.deepEqual(limited[key], firstOracle[key],
      `worker limit ${workers} changes classified ${key}`);
  }
  assert.equal(catalogState().meta.audit_digest, firstOracle.question_digest);
  assert(!existsSync(bootstrapCapture), 'completed bootstrap payload was not reclaimed');
  const publication = json(join(work, 'inventory/last-publication.json'));
  assert.equal(publication.receipt.epoch, originalReceipt.epoch); assert(publication.capture.files.length > 0);
  console.log('ok: actual ranker bootstrap matches every full-classifier tuple; later writes retain the original cut and remain dirty');

  mature(); await cycle(); equalPlan(await currentOracle('question-delta'), catalogState());
  assert.equal(Number(await scalar('SELECT count(*) FROM state_compute_inventory_invalidations')), 0);
  console.log('ok: the next epoch consumes the later lifecycle mutation exactly');

  await q('DELETE FROM entity_weapon_builds WHERE id=$1', [c.id]);
  await q("UPDATE entity_measurement_reviews SET invalidated_at='2026-11-06T00:00:00.000Z' WHERE id='review'");
  mature(); await cycle(); equalPlan(await currentOracle('delete-retire'), catalogState());
  const canonicalQueued = await questionId('simulate', { record: c.record, ruler: roster[0], mode: 'base' });
  assert(catalogState().tuples.some(row => row.id === canonicalQueued && row.queued === 1));
  console.log('ok: deleting a stored canonical record retains a payable alias and its queued row; fact retirement matches the full oracle');

  for (let i = 0; i < 130; i++) await q('INSERT INTO entity_weapon_builds(id,at,record) VALUES ($1,\'day\',$2)', [`new-owner-${String(i).padStart(3, '0')}`, JSON.stringify(c.record)]);
  mature(); const beforeAck = ackCalls; let next = await cycle('working', 0), turns = 1;
  assert.equal(next.pending, true); assert.equal(ackCalls, beforeAck, 'partial owner chunks acknowledged a frozen cut');
  const active = json(statePath).pending, activeDir = dirname(active.capture);
  assert(existsSync(active.capture));
  for (const scope of active.scopes) for (const name of ['facts', 'queue']) assert(existsSync(scope[name]), 'GC removed an active cohort payload');
  while (next.pending && turns++ < 10) next = await cycle('working', 0);
  assert.equal(next.pending, false); assert(turns > 1);
  assert(commandLog.filter(row => row.op === 'apply' && row.epoch === json(statePath).engine_epoch).length >= 3);
  equalPlan(await currentOracle('multi-chunk'), catalogState());
  console.log('ok: multiple apply and owner chunks complete one immutable epoch before durable ACK');
  assert(!existsSync(activeDir), 'completed chunk payload was not reclaimed'); assert.equal(json(statePath).pending, null);
  assert.equal(json(join(work, 'inventory/last-publication.json')).receipt.epoch, active.receipt.epoch);
  console.log('ok: GC preserves pending input payloads and reclaims completed cuts after durable publication');
  file('receipt.json', { v: 1, actual_binary: binary, cases: 6, ackEvidence, commandLog, final: catalogState().summary });
  succeeded = true;
} catch (error) { console.error(`inventory pipeline fixture retained: ${dir}`); throw error; }
finally {
  reader?.close();
  if (child) {
    child.stdin.end();
    if (child.exitCode === null) {
      await Promise.race([onceExit(child), new Promise(resolve => setTimeout(resolve, 2000))]);
      if (child.exitCode === null) child.kill('SIGKILL');
    }
  }
  await pg.close();
  if (succeeded && !process.env.WFSIM_INVENTORY_PIPELINE_EVIDENCE) rmSync(dir, { recursive: true, force: true });
}
function onceExit(process) { return new Promise(resolve => process.once('exit', resolve)); }
