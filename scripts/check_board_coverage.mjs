// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { Readable } from 'node:stream';
import postgres from './pg/node_modules/postgres/src/index.js';
import { boardCoverageSchemaSql } from './board_coverage_schema.mjs';
import { pgDb, postgresClient, POSTGRES_OPTIONS } from '../worker/db.js';
import { factory, canon } from '../worker/tasks.js';
import { BUILD_WORKFLOW_PRODUCER } from '../worker/build_workflows.js';
import { computationCanon, computationId } from '../worker/computation_input.js';
import { prepareBoardCoverageBatch, admitBoardCoverageBatch, readBoardCoverageBatch,
  BoardCoverageReadbackRequired, coverageSession } from './board_coverage.mjs';

const url = process.env.WFSIM_COVERAGE_TEST_URL;
assert(url && ['127.0.0.1', 'localhost', '[::1]'].includes(new URL(url).hostname), 'use an isolated local PostgreSQL');
const schema = 'coverage_' + randomBytes(6).toString('hex');
const connect = () => postgres(url, { ...POSTGRES_OPTIONS, max: 1, fetch_types: false, onnotice() {},
  connection: { search_path: schema, statement_timeout: 10_000 } });
const writer = connect(), reader = connect();
const hash = value => createHash('sha256').update(computationCanon(value)).digest('hex');
const manifest = 'a'.repeat(64), namespace = 'coverage-' + manifest.slice(0,16);
const native = 'b'.repeat(64), engine = 'c'.repeat(12);
const at = '2026-11-20T00:00:00.000Z';
const build = digit => digit.repeat(32);
const record = { weapon: 'braton', mods: ['serration'] };
const readSql = name => readFileSync(new URL('../worker/pg/' + name + '.sql', import.meta.url), 'utf8');

async function cell(ordinal, identity, ruler, seed, mode = 'base') {
  const source = [namespace, identity, ruler, mode].join('/');
  const template = { ...record, runs: 1, seed };
  const computation = await computationId('simulate', template);
  return { source, target: { source, ordinal, identity, ruler, mode, weapon: record.weapon,
    engine, computation, witness: ['anchor', 'base'] },
  plan: { id: identity, record, at, from: [source], initiator: 'system', derived: false, flow: {
    schema: 1, scope: { boards: [{ id: ruler, scenario: { kind: 'test' } }], modes: { braton: [mode] } },
    builds: [{ id: identity, record }], templates: [template], patches: [{ set: {}, remove: [] }],
    nodes: [{ kind: 'measure', identity, ruler, mode, computation, template: 0, patch: 0 }],
  } } };
}

function inputs(items, label = '') {
  const checkpoint = { manifest, next: items[0].target.ordinal - 1, enqueued: 0, target: 10_000_000 };
  const rows = items.map(({target: { engine: _engine, computation: _computation, ...target }}) => target);
  const proposal = { before: hash(checkpoint), after: { ...checkpoint, next: checkpoint.next + items.length }, rows };
  if (label) proposal.after.test = label;
  proposal.id = hash(proposal);
  return { checkpoint, proposal, sealed: { batch: proposal.id, manifest, namespace, native, engine, items } };
}

async function prepare(items, label) {
  const { checkpoint, proposal, sealed } = inputs(items, label);
  return prepareBoardCoverageBatch(checkpoint, proposal, sealed, { native, engine, epoch });
}

const lookup = async (statement, args = []) => (await reader.unsafe(statement, args))[0];
const counts = async () => lookup(`SELECT
  (SELECT count(*)::int FROM entity_compute_tasks) tasks,
  (SELECT count(*)::int FROM state_build_workflow_nodes) nodes,
  (SELECT count(*)::int FROM entity_build_workflows) parents,
  (SELECT count(*)::int FROM relation_task_consumers) consumers,
  (SELECT count(*)::int FROM record_board_coverage_receipts) receipts`);
const write = (tx, prepared) => tx.unsafe('SELECT board_coverage_admit(($1::text)::jsonb,$2,$3)',
  [JSON.stringify(prepared.batch), prepared.epoch, prepared.batch.engine]);

let epoch;
try {
  await writer.unsafe('CREATE SCHEMA ' + schema);
  for (const name of ['schema', 'library', 'sync', 'inventory', 'build_workflows', 'board_admission']) {
    await writer.unsafe(readSql(name));
  }
  assert.throws(() => boardCoverageSchemaSql(), /capacity-qualified/);
  let installation = boardCoverageSchemaSql('pg_default').replace("SET LOCAL ROLE wfsim;", "");
  if (process.argv.includes('--break-readback-proof')) {
    const guarded = `JOIN relation_compute_task_sources linked ON linked.trigger_id=tr.id
              AND linked.producer='build_workflow' AND linked.ref=n.workflow||'|'||n.id
           JOIN entity_compute_tasks task ON task.id=linked.task_id`;
    assert(installation.includes(guarded));
    installation = installation.replace(guarded,
      "JOIN entity_compute_tasks task ON task.id=(r.disposition->>'task_id')::bigint");
  }
  if (process.argv.includes('--break-total-width')) {
    const guarded = 'IF sources NOT BETWEEN 1 AND 32';
    assert(installation.includes(guarded));
    installation = installation.replace(guarded, 'IF sources NOT BETWEEN 1 AND 512');
  }
  await writer.unsafe(installation);
  await writer.unsafe(installation);
  await writer.unsafe(readSql('order_cut_indexes').replaceAll('INDEX CONCURRENTLY', 'INDEX'));
  await writer.unsafe(`
    INSERT INTO state_task_pipeline_controls(name,paused,changed_at,changed_by) VALUES('admission',true,'test','test');
    INSERT INTO state_factory_parameters(name,value,set_at,set_by) VALUES('board_admission',0,'test','test')
      ON CONFLICT(name) DO UPDATE SET value=0;
    SELECT compute_order_cut_stage('{"id":"coverage-cut","cut_at":"2026-11-16T01:53:48.726Z",
      "task_cutoff":1,"keep_boundary":[3,1,"fixture"],"source":{}}');
    SELECT compute_order_cut_capture('coverage-cut');
    SELECT compute_order_cut_activate('coverage-cut',0,'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
    SELECT setval(pg_get_serial_sequence('entity_compute_tasks','id'),1,true);
  `);
  ({ epoch } = await lookup('SELECT compute_order_cut_epoch() epoch'));
  for (const id of ['1','2','3','4','5'].map(build)) {
    await writer.unsafe('INSERT INTO entity_weapon_builds(id,at,record) VALUES($1,$2,$3)', [id,at,JSON.stringify(record)]);
    await writer.unsafe(`INSERT INTO state_board_measurement_selection
      (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at)
      VALUES($1,'anchor','base','verified:test',1,'kpm',0,$2,$2)`, [id,at]);
  }

  const items = await Promise.all([cell(1,build('1'),'r1',1), cell(2,build('1'),'r2',1),
    cell(3,build('2'),'r3',2), cell(4,build('1'),'r4',3)]);
  const batch = await prepare(items);
  assert.deepEqual(batch.batch.groups.map(group => group.targets.length), [2,1,1]);
  const malformed = inputs(items);
  malformed.checkpoint.next++;
  await assert.rejects(prepareBoardCoverageBatch(malformed.checkpoint,malformed.proposal,malformed.sealed,
    { native,engine,epoch }));
  assert.equal(await readBoardCoverageBatch(reader,batch), null);

  const once = await admitBoardCoverageBatch(writer,reader,batch);
  assert.deepEqual(once.dispositions.map(row => row.outcome), ['enqueued','joined_shared_task','enqueued','enqueued']);
  assert.deepEqual(once.receipt.rows.map(row => row.outcome), ['enqueued','skipped_prior_work','enqueued','enqueued']);
  assert.equal(once.taskIds.length,3);
  assert.equal(once.dispositions[0].task_id,once.dispositions[1].task_id);
  assert.notEqual(once.dispositions[0].node,once.dispositions[1].node);
  assert.deepEqual({ ...await counts() }, { tasks:3,nodes:4,parents:3,consumers:4,receipts:4 });
  await assert.rejects(writer.unsafe("UPDATE record_board_coverage_receipts SET fingerprint=repeat('d',64)"),
    /captured intake obligation is immutable/);
  const queue = await reader.unsafe('SELECT id FROM state_compute_scheduling ORDER BY zone_position');
  assert.deepEqual(queue.map(row=>row.id),[items[0],items[2],items[3]].map(item=>item.target.computation));

  const lostCommit = { begin: async fn => {
    await writer.begin(fn);
    throw new Error('commit acknowledgement lost');
  } };
  assert.deepEqual(await admitBoardCoverageBatch(lostCommit,reader,batch),once);
  assert.deepEqual(await readBoardCoverageBatch(reader,batch),once);

  const fresh = await prepare(await Promise.all([cell(5,build('3'),'r5',5),cell(6,build('4'),'r6',6)]));
  const bad = structuredClone(fresh);
  bad.batch.groups[1].plan.flow.scope.modes.extra_weapon = ['unexpected'];
  await assert.rejects(admitBoardCoverageBatch(writer,reader,bad),/scope does not match/);
  assert.deepEqual({ ...await counts() }, { tasks:3,nodes:4,parents:3,consumers:4,receipts:4 });

  const aborted = { begin: fn => writer.begin(async tx => {
    await fn(tx);
    throw new Error('transaction rolled back before readback');
  }) };
  await assert.rejects(admitBoardCoverageBatch(aborted,reader,fresh),BoardCoverageReadbackRequired);
  assert.equal(await readBoardCoverageBatch(reader,fresh),null);

  await assert.rejects(writer.begin(async tx => {
    await write(tx,fresh);
    await write(tx,fresh);
  }),/only one coverage batch per transaction/);
  assert.equal(await readBoardCoverageBatch(reader,fresh),null);

  const maxItems = await Promise.all(Array.from({length:32},(_,i)=>cell(7+i,build('5'),'maximum-'+i,20+i)));
  const maxBatch = await prepare(maxItems);
  const extra = await prepare([await cell(39,build('4'),'over-limit',71)]);
  const tooWide = structuredClone(maxBatch);
  tooWide.batch.groups.push(extra.batch.groups[0]);
  tooWide.batch.last = extra.batch.last;
  await assert.rejects(writer.begin(tx => write(tx,tooWide)),/exceeds source limit/);
  assert.deepEqual({ ...await counts() }, { tasks:3,nodes:4,parents:3,consumers:4,receipts:4 });

  const overlap = await prepare([items[3]],'overlap');
  const joined = await admitBoardCoverageBatch(writer,reader,overlap);
  assert.equal(joined.taskIds.length,0);
  assert.equal(joined.dispositions[0].batch,batch.batch.batch);

  const missingReader = { begin: async () => { throw new Error('readback connection lost'); } };
  await assert.rejects(admitBoardCoverageBatch(writer,missingReader,fresh),BoardCoverageReadbackRequired);
  const recovered = await readBoardCoverageBatch(reader,fresh);
  assert.equal(recovered.taskIds.length,2);
  assert.equal((await counts()).tasks,5);

  const leftTarget = await cell(7,build('5'),'overlap-left',7);
  const middleTarget = await cell(8,build('5'),'overlap-middle',8);
  const rightTarget = await cell(9,build('5'),'overlap-right',9);
  const left = await prepare([leftTarget,middleTarget]);
  const right = await prepare([middleTarget,rightTarget]);
  const otherWriter = connect(), otherReader = connect();
  try {
    const outcomes = await Promise.all([
      admitBoardCoverageBatch(writer,reader,left),
      admitBoardCoverageBatch(otherWriter,otherReader,right),
    ]);
    assert.equal(outcomes[0].taskIds.length + outcomes[1].taskIds.length,3);
    assert.equal((await counts()).tasks,8);
  } finally {
    await Promise.all([otherWriter.end(),otherReader.end()]);
  }
  const legacy = await prepare([await cell(10,build('5'),'previous-writer',10)]);
  await writer.unsafe("INSERT INTO relation_build_intake_sources SELECT $1,id FROM entity_build_intake_intents LIMIT 1",
    [legacy.batch.groups[0].targets[0].source]);
  await assert.rejects(admitBoardCoverageBatch(writer,reader,legacy),/belongs to the previous writer/);

  const service = factory({
    verbs: { simulate: { cap: 2, lease_ms: 60_000, canon: (result, work) => canon({ ...result, work }) } },
    producers: { build_workflow: BUILD_WORKFLOW_PRODUCER }, random: () => 1,
  });
  await writer.unsafe("UPDATE state_compute_scheduling SET required_capability='fixture-unavailable' WHERE id<>$1",
    [items[0].target.computation]);
  const now = Date.now();
  const env = { LIBRARY: pgDb(postgresClient(writer)), ASSETS: { fetch: async () => Response.json({ engine }) } };
  for (const suffix of ['a','b']) {
    const device = 'coverage-peer-' + suffix;
    const witness = { device, owner: 'owner-'+suffix, net: 'network-'+suffix,
      siblings: [device,'cap:fixed-board-input-v1'] };
    await writer.unsafe("INSERT INTO entity_compute_devices(id,seen) VALUES($1,$2)",[device,new Date(now).toISOString()]);
    const lease = await service.next(env,witness,engine,now);
    assert.equal(lease?.question,items[0].target.computation);
    assert.equal((await service.answer(env,witness,{ lease:lease.lease,engine, result:{score:37,metric:'kpm'},work:7 },now)).taken,true);
  }
  const lateAlias = await prepare([await cell(11,build('1'),'after-fact',1)]);
  const late = await admitBoardCoverageBatch(writer,reader,lateAlias);
  assert.equal(late.taskIds.length,0);
  assert.equal(late.dispositions[0].task_id,once.taskIds[0]);
  for (let pass=0;pass<2;pass++) await service.redeliver(env,Date.now());
  const settled = await lookup(`SELECT
    (SELECT count(*)::int FROM entity_compute_results) results,
    (SELECT count(*)::int FROM record_compute_result_proofs) proofs,
    (SELECT count(*)::int FROM record_candidate_answers WHERE credited=1) credits,
    (SELECT coalesce(sum(work),0)::int FROM entity_compute_devices) work,
    (SELECT count(*)::int FROM state_board_measurement_selection WHERE score=37) delivered`);
  assert.deepEqual({...settled},{results:1,proofs:1,credits:2,work:14,delivered:3});

  const frame = (operation, cells) => JSON.stringify({ operation, ...inputs(cells), pins:{native,engine,epoch} })+'\n';
  let opens = 0;
  const replies = [];
  const nextTarget = await cell(12,build('4'),'session-would-admit',12);
  await coverageSession(Readable.from([
    frame('read',items), frame('read',[nextTarget]), frame('admit',[nextTarget]),
  ]), { write: reply => replies.push(JSON.parse(reply)) }, () => { opens++; return connect(); });
  assert.equal(opens,2);
  assert.deepEqual(replies.map(reply=>reply.verified),[once,null]);
  assert.equal(await readBoardCoverageBatch(reader,await prepare([nextTarget])),null);

  const alreadyQueued = await prepare([
    await cell(13,build('4'),'ordinary-first',89), await cell(14,build('4'),'ordinary-second',90),
  ]);
  const ordinaryPlan = { ...alreadyQueued.batch.groups[0].plan,
    from: ['ordinary-explicit-source'], initiator: 'anonymous' };
  await writer.unsafe('SELECT build_intake_batch($1::text)',[JSON.stringify([ordinaryPlan])]);
  const admissionTime = new Date(Date.now()+1000).toISOString();
  for (const [kind,width] of [['intake',1],['expansion',2]]) {
    const [{job}] = await writer.unsafe('SELECT build_admission_claim($1,$2) job',[kind,admissionTime]);
    assert(job);
    const [{result}] = await writer.unsafe('SELECT build_admission_run($1,$2,$3,$4,$5) result',
      [kind,job.key,job.lease,width,admissionTime]);
    assert.equal(result.processed,width);
    assert.equal(result.pending,false);
    if (kind==='intake') {
      const pending = await counts();
      await assert.rejects(admitBoardCoverageBatch(writer,reader,alreadyQueued),/still owns this scope/);
      assert.deepEqual(await counts(),pending);
    }
  }
  const oldParent = await lookup(`SELECT f.id FROM entity_build_workflows f
    JOIN record_build_workflow_triggers t ON t.workflow=f.id
    WHERE t.source#>>'{inbox,0}'='ordinary-explicit-source'`);
  assert.equal((await lookup("SELECT count(*)::int n FROM state_build_admission_work WHERE key=$1",
    [oldParent.id+':root'])).n,0);
  const beforeCoalescing = await counts();
  const reusedParent = await admitBoardCoverageBatch(writer,reader,alreadyQueued);
  assert.equal(reusedParent.taskIds.length,0);
  assert(reusedParent.dispositions.every(row=>row.outcome==='joined_shared_task' && row.workflow===oldParent.id));
  assert.deepEqual({...await counts()},{...beforeCoalescing,receipts:beforeCoalescing.receipts+2});
  assert.deepEqual(await readBoardCoverageBatch(reader,alreadyQueued),reusedParent);

  await writer.unsafe('DELETE FROM relation_compute_task_sources WHERE task_id=$1',[once.taskIds[0]]);
  assert.equal(await readBoardCoverageBatch(reader,batch),null,'readback requires the independent task source');
  console.log('board coverage: exact cursor, alias, active cut, missing proof, lost commit and transaction rollback pass');
} finally {
  await reader.end();
  await writer.unsafe('DROP SCHEMA IF EXISTS ' + schema + ' CASCADE');
  await writer.end();
}
