// Transactional dirty keys and publication fences retain authoritative inputs.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { PGlite } from './pg/node_modules/@electric-sql/pglite/dist/index.js';

const pg = new PGlite();
const ddl = name => readFileSync(new URL(`../worker/pg/${name}.sql`, import.meta.url), 'utf8');
const inventorySql = ddl('inventory');
const engine = '0ae1bf8edb0f', covers = ['board'], contract = { v: 1, covers, engine_rules: 'fixture' };
const query = (sql, params = []) => pg.query(sql, params).then(r => r.rows);
const scalar = async (sql, params = []) => Object.values((await query(sql, params))[0])[0];
const call = (name, args = []) => scalar(`SELECT ${name}(${args.map((_, i) => `$${i + 1}::text`).join(',')}) value`, args.map(x => typeof x === 'string' || x === null ? x : JSON.stringify(x)));
const changes = () => query('SELECT * FROM inventory_changes(256)');
let generation;
const ack = async () => call('inventory_ack', [generation, await changes()]);
const build = (id, weapon = 'torid') => query('INSERT INTO builds(id,at,record) VALUES ($1,\'day\',$2)', [id, JSON.stringify({ weapon, mods: [] })]);
const question = (id, state = 'open') => query(`INSERT INTO questions(id,verb,request,state,due_at,opened_at,fact_engine)
  VALUES ($1,'simulate','{}',$2,'due','opened',$3)`, [id, state, engine]);
const demand = (ref, id, producer = 'other') => query(`INSERT INTO demands(producer,ref,question,state,asked_at,due_at)
  VALUES ($1,$2,$3,'live','at','due')`, [producer, ref, id]);
const readInputs = async rows => {
  await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
  try { const value = await call('inventory_inputs', [engine, covers, rows]); await pg.exec('COMMIT'); return value; }
  catch (error) { await pg.exec('ROLLBACK'); throw error; }
};
const readCohort = async scopes => {
  await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
  try {
    const value = await query('SELECT * FROM inventory_cohort_rows($1::text,$2::text,$3::text)', [engine, JSON.stringify(covers), JSON.stringify(scopes)]);
    await pg.exec('COMMIT'); return value;
  } catch (error) { await pg.exec('ROLLBACK'); throw error; }
};
const body = () => ({ v: 1, generation, contract, source: 'fixture', engine, total: 7, registered: 2,
  orders: [{ kind: 'board', total: 7 }], verbs: { simulate: 7 }, started_at: '1900-01-01', snapshot: 'untrusted' });
const publish = value => call('inventory_publish', [value || body()]);
let passed = 0;
const check = async (name, fn) => { await fn(); passed++; console.log(`ok: ${name}`); };

try {
  await pg.exec(['schema', 'library', 'sync'].map(ddl).join('\n') + '\n' + inventorySql);
  await check('source writes are tracked before the first generation exists', async () => {
    await build('a');
    assert.deepEqual((await changes()).map(r => [r.scope, r.key]), [['build', 'a']]);
    const state = await call('inventory_begin', [engine, contract, null]);
    generation = state.generation;
    assert.equal(typeof generation, 'string'); assert.equal(state.status, 'initializing');
    assert.equal((await changes()).length, 1);
    assert.deepEqual(await call('inventory_status'), state);
  });
  await check('changes coalesce per source statement and no-op writes retain their token', async () => {
    const prior = (await changes())[0];
    await query("UPDATE builds SET record=record WHERE id='a'");
    assert.deepEqual(await changes(), [prior]);
    await query("INSERT INTO batches(id,at,total) VALUES ('batch','at',1000)");
    await query("INSERT INTO queue(batch,build_id,ruler,mode) SELECT 'batch','a','r'||i,'base' FROM generate_series(1,1000) i");
    const held = await changes(); assert.equal(held.length, 1); assert.notEqual(held[0].revision, prior.revision);
    assert.equal(await ack(), 1);
  });
  await check('lease, wait, heartbeat and credit updates do not invalidate counts', async () => {
    await question('q'); await ack();
    await query("UPDATE questions SET lease='lease',lease_until='later',waits=1,due_at='later' WHERE id='q'");
    assert.deepEqual(await changes(), []);
    await query("INSERT INTO answers(id,question,device,owner,engine,canon,result,official,work,at) VALUES (100,'q','d','d',$1,'{}','{}',0,1,'at')", [engine]);
    assert.deepEqual((await changes()).map(r => r.key), ['q']); await ack();
    await query("UPDATE answers SET credited=1,credit_work=1,credit_at='at' WHERE id=100");
    assert.deepEqual(await changes(), []);
  });
  await check('lease-only SQL never dispatches the eligible-row trigger, while the old transition trigger does', async () => {
    await pg.exec(`BEGIN;
      CREATE OR REPLACE FUNCTION inventory_hot_changed() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN RAISE EXCEPTION 'eligible inventory update entered'; END $$;`);
    await query("UPDATE questions SET lease='lease-fast',lease_until='later' WHERE id='q'");
    await pg.exec('SAVEPOINT eligible_update');
    await assert.rejects(query("UPDATE questions SET state='spot' WHERE id='q'"), /eligible inventory update entered/);
    await pg.exec('ROLLBACK TO eligible_update');
    await pg.exec(`DROP TRIGGER inventory_update ON questions;
      CREATE OR REPLACE FUNCTION inventory_source_changed() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN RAISE EXCEPTION 'eligible inventory update entered'; END $$;
      CREATE TRIGGER inventory_update AFTER UPDATE ON questions REFERENCING OLD TABLE AS inventory_old NEW TABLE AS inventory_new
      FOR EACH STATEMENT EXECUTE FUNCTION inventory_source_changed()`);
    await assert.rejects(async () => assert.doesNotReject(query("UPDATE questions SET lease='old-transition' WHERE id='q'")), /eligible inventory update entered/);
    await pg.exec('ROLLBACK');
  });
  await check('a stale acknowledgement cannot erase a newer source mutation', async () => {
    await query("UPDATE builds SET record='{\"weapon\":\"braton\"}' WHERE id='a'");
    const old = await changes();
    await query("UPDATE builds SET record='{\"weapon\":\"torid\"}' WHERE id='a'");
    const newer = await changes();
    assert.notEqual(newer[0].revision, old[0].revision);
    assert.equal(await call('inventory_ack', [generation, old]), 0);
    assert.deepEqual(await changes(), newer); await ack();
  });
  await check('rolled-back revisions are never reused and rolled-back changes disappear', async () => {
    await pg.exec('BEGIN'); await build('rolled-back');
    const token = (await changes())[0].revision;
    await pg.exec('ROLLBACK'); assert.deepEqual(await changes(), []);
    await build('after-rollback'); assert.notEqual((await changes())[0].revision, token); await ack();
  });
  await check('batch validity changes dirty every affected build while batch metadata does not', async () => {
    await query("DELETE FROM batches WHERE id='batch'");
    assert.deepEqual((await changes()).map(r => r.key), ['a']); await ack();
    await query("INSERT INTO batches(id,at,total) VALUES ('batch','at',1000)");
    assert.deepEqual((await changes()).map(r => r.key), ['a']); await ack();
    await query("UPDATE batches SET total=2000,why='metadata' WHERE id='batch'"); assert.deepEqual(await changes(), []);
  });
  await check('queue replacement dirties old and new identities; deletion dirties its payable source', async () => {
    await query("INSERT INTO queue(batch,build_id,ruler,mode) VALUES ('batch','old-key','r','base')"); await ack();
    await query("UPDATE queue SET build_id='new-key' WHERE build_id='old-key'");
    assert.deepEqual(new Set((await changes()).map(r => r.key)), new Set(['old-key', 'new-key'])); await ack();
    await query("DELETE FROM queue WHERE build_id='new-key'"); assert.deepEqual((await changes()).map(r => r.key), ['new-key']); await ack();
  });
  await check('score and review-retirement changes invalidate the build and export its removal fact', async () => {
    await query(`INSERT INTO scores(identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at,review_id)
      VALUES ('a','r','base','old',10,'kpm',1,'before','finished','review')`);
    assert.deepEqual((await changes()).map(r => r.key), ['a']); await ack();
    await query(`INSERT INTO board_reviews(ref,id,round_at,identity,ruler,mode,engine,question,record,state,
      baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at)
      VALUES ('ref','review','round','a','r','base',$1,'review-question','{}','waiting',10,'kpm',1,'old','before','finished','at')`, [engine]);
    assert.deepEqual((await changes()).map(r => r.key), ['a']); await ack();
    await query("UPDATE board_reviews SET invalidated_at='retired' WHERE id='review'");
    const input = await readInputs(await changes()); assert.equal(input.facts[0].retired_at, 'retired'); await ack();
    await query("UPDATE board_reviews SET state='succeeded' WHERE id='review'");
    assert.equal((await readInputs(await changes())).facts[0].retired_at, null); await ack();
    await query("DELETE FROM scores WHERE identity='a'"); assert.deepEqual((await changes()).map(r => r.key), ['a']); await ack();
  });
  await check('keyed RR inputs clear missing builds and lifecycle states explicitly', async () => {
    await build('deleted'); await question('cached', 'fact'); await question('other'); await demand('other-ref', 'other');
    await question('covered'); await demand('covered-ref', 'covered', 'board');
    await question('disputed', 'disputed');
    await query("INSERT INTO answers(id,question,device,owner,engine,canon,result,official,work,at) VALUES (101,'disputed','d','d',$1,'{}','{}',0,1,'at')", [engine]);
    await query("DELETE FROM builds WHERE id='deleted'");
    const keys = await changes(); const input = await readInputs(keys);
    assert.equal(input.generation, generation); assert.deepEqual(input.contract, contract); assert(input.snapshot);
    assert.deepEqual(input.builds.find(r => r.id === 'deleted'), { id: 'deleted', deleted: true });
    assert.equal(input.tasks.find(r => r.id === 'cached').state, 'fact');
    assert.equal(input.tasks.find(r => r.id === 'other').kind, 'other');
    assert.deepEqual(input.tasks.find(r => r.id === 'covered'), { id: 'covered', deleted: true });
    assert.equal(input.tasks.find(r => r.id === 'disputed').served_answered, true);
    await query("UPDATE demands SET state='withdrawn' WHERE ref='other-ref'");
    const cleared = await readInputs(await changes()); assert.deepEqual(cleared.tasks.find(r => r.id === 'other'), { id: 'other', deleted: true });
    await ack();
  });
  await check('refusals and deleted questions match the full lifecycle projection', async () => {
    await query("INSERT INTO refusal_withdrawals(question,engine,owners,at) VALUES ('refused',$1,2,'at')", [engine]);
    const input = await readInputs(await changes());
    assert.equal(input.tasks[0].state, 'refused');
    await question('refused');
    const withdrawn = await readInputs(await changes()); assert.deepEqual(withdrawn.tasks[0], { id: 'refused', deleted: true });
    await query("DELETE FROM questions WHERE id='refused'");
    assert.equal((await readInputs(await changes())).tasks[0].state, 'refused'); await ack();
  });
  await check('cohorts include siblings, retired-ruler queue and explicit canonical aliases only', async () => {
    await build('sibling'); await build('unrelated', 'braton');
    await query("INSERT INTO queue(batch,build_id,ruler,mode) VALUES ('batch','canonical-alias','retired-ruler','base'),('missing-batch','a','r','base')");
    const rows = await readCohort({ weapons: ['torid'], identities: ['canonical-alias'] });
    assert(rows.some(r => r.kind === 'builds' && r.body.id === 'sibling'));
    assert(!rows.some(r => r.kind === 'builds' && r.body.id === 'unrelated'));
    assert(rows.some(r => r.kind === 'queue' && r.body.build_id === 'canonical-alias' && r.body.ruler === 'retired-ruler'));
    assert(!rows.some(r => r.kind === 'queue' && r.body.batch === 'missing-batch')); await ack();
  });
  await check('read APIs reject incoherent transaction settings, fences and unbounded keys', async () => {
    await assert.rejects(call('inventory_inputs', [engine, covers, []]), /read-only repeatable-read/);
    await assert.rejects(query('SELECT * FROM inventory_changes(257)'), /1\.\.256/);
    await assert.rejects(readInputs([{ scope: 'build', key: 'a' }]), /invalid inventory change/);
    await assert.rejects(readInputs(Array.from({ length: 257 }, () => ({ scope: 'build', key: 'a', revision: '1' }))), /invalid inventory changes/);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');
    await assert.rejects(call('inventory_inputs', ['ffffffffffff', covers, []]), /read fence/); await pg.exec('ROLLBACK');
    await assert.rejects(readCohort({ weapons: [], identities: Array(257).fill('a') }), /invalid inventory cohort/);
  });
  await check('publication rejects pending changes and ignores caller freshness metadata', async () => {
    await build('pending'); await assert.rejects(publish(), /changes remain/); await ack();
    const saved = await publish(); assert.equal(saved.total, 7); assert.equal(saved.registered, 2);
    assert.notEqual(saved.snapshot, 'untrusted'); assert.notEqual(saved.started_at, '1900-01-01');
    assert.equal(Date.parse(saved.expires_at) - Date.parse(saved.started_at), 3 * 60 * 60000);
    assert.equal((await call('inventory_status')).status, 'active');
    assert.deepEqual(JSON.parse(await scalar("SELECT body FROM factory_findings WHERE kind='task_inventory'")), saved);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
    await assert.rejects(publish(), /requires read committed/); await pg.exec('ROLLBACK');
  });
  await check('count validation fails closed before writing a finding', async () => {
    const prior = await scalar("SELECT body FROM factory_findings WHERE kind='task_inventory'");
    for (const bad of [{ ...body(), total: -1 }, { ...body(), total: '7' }, { ...body(), orders: [] },
      { ...body(), registered: 8 }, { ...body(), verbs: { simulate: 6 } }, { ...body(), verbs: null },
      Object.fromEntries(Object.entries(body()).filter(([k]) => k !== 'total'))]) {
      await assert.rejects(publish(bad), /invalid inventory|count mismatch/);
    }
    assert.equal(await scalar("SELECT body FROM factory_findings WHERE kind='task_inventory'"), prior);
  });
  await check('new generations fence old ACK/publication and preserve the last published value', async () => {
    const oldGeneration = generation, prior = await scalar("SELECT body FROM factory_findings WHERE kind='task_inventory'");
    await build('fenced');
    const pending = await changes();
    await assert.rejects(call('inventory_begin', [engine, contract, null]), /generation changed/);
    generation = (await call('inventory_begin', [engine, contract, oldGeneration])).generation;
    assert.notEqual(generation, oldGeneration); assert.deepEqual(await changes(), pending);
    await assert.rejects(call('inventory_ack', [oldGeneration, pending]), /generation changed/);
    await assert.rejects(publish({ ...body(), generation: oldGeneration }), /fence changed/);
    assert.equal(await scalar("SELECT body FROM factory_findings WHERE kind='task_inventory'"), prior);
    await ack(); await publish();
  });
  await check('idempotent SQL installation preserves dirty keys, fences and revision sequences', async () => {
    await build('reinstall'); const prior = await changes(), state = await call('inventory_status');
    await pg.exec(inventorySql); assert.deepEqual(await changes(), prior); assert.deepEqual(await call('inventory_status'), state);
    await query("UPDATE builds SET record='{\"weapon\":\"braton\"}' WHERE id='reinstall'");
    assert.notEqual((await changes())[0].revision, prior[0].revision); await ack();
  });
  await check('an unsealed epoch rolls back with its receipt and cannot resume in another transaction', async () => {
    await assert.rejects(call('inventory_epoch_begin', [generation]), /repeatable read receipt transaction/);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
    const cut = await call('inventory_epoch_begin', [generation]);
    assert.equal(cut.status, 'capturing');
    await call('inventory_inputs', [engine, covers, []]);
    await pg.exec('ROLLBACK');
    assert.equal(await call('inventory_epoch_status', [cut.epoch]), null);
    await assert.rejects(call('inventory_epoch_changes', [cut.epoch]), /does not own this snapshot/);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
    await call('inventory_epoch_begin', [generation]);
    await assert.rejects(build('forbidden-epoch-write'), /may write receipts only/); await pg.exec('ROLLBACK');
    assert.equal(await scalar("SELECT count(*) FROM builds WHERE id='forbidden-epoch-write'"), 0);
  });
  let frozen, captured;
  await check('one frozen RR epoch exports every token in bounded cursor pages and seals its durable manifest', async () => {
    await query("INSERT INTO builds(id,at,record) SELECT 'epoch-'||i,'day','{\"weapon\":\"torid\"}' FROM generate_series(1,400) i");
    assert.equal((await changes()).length, 256);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
    const cut = await call('inventory_epoch_begin', [generation]);
    await query('DECLARE epoch_keys NO SCROLL CURSOR FOR SELECT * FROM inventory_epoch_changes($1::text)', [cut.epoch]);
    captured = [];
    for (;;) {
      const page = await query('FETCH FORWARD 97 FROM epoch_keys'); captured.push(...page);
      if (page.length < 97) break;
    }
    await pg.exec('CLOSE epoch_keys'); assert.equal(captured.length, 400);
    const input = await call('inventory_inputs', [engine, covers, captured.slice(0, 256)]);
    assert.equal(input.snapshot, cut.snapshot); assert.equal(input.started_at, cut.started_at);
    await pg.exec('SAVEPOINT invalid_seal');
    await assert.rejects(query('SELECT inventory_epoch_seal($1::text,$2::bigint,$3::text)', [cut.epoch, 399, 'a'.repeat(64)]), /changes incomplete/);
    await pg.exec('ROLLBACK TO invalid_seal');
    frozen = await scalar('SELECT inventory_epoch_seal($1::text,$2::bigint,$3::text)', [cut.epoch, 400, 'a'.repeat(64)]);
    assert.equal(frozen.change_count, 400); assert.equal(frozen.status, 'sealed');
    await pg.exec('SAVEPOINT sealed_business');
    await assert.rejects(build('write-after-seal'), /may write receipts only/); await pg.exec('ROLLBACK TO sealed_business');
    await assert.rejects(query("UPDATE questions SET lease='after-seal' WHERE id='q'"), /may write receipts only/);
    await pg.exec('ROLLBACK TO sealed_business');
    await pg.exec('COMMIT'); assert.deepEqual(await call('inventory_epoch_status', [cut.epoch]), frozen);
    await assert.rejects(call('inventory_epoch_changes', [cut.epoch]), /does not own this snapshot/);
  });
  await check('later writes survive frozen-token ACK and cannot starve publication of its original cut', async () => {
    await query("UPDATE builds SET record='{\"weapon\":\"braton\"}' WHERE id='epoch-1'");
    await question('later-question');
    let matched = 0;
    for (let i = 0; i < captured.length; i += 256) matched += await call('inventory_ack', [generation, captured.slice(i, i + 256)]);
    assert.equal(matched, 399);
    assert.deepEqual(new Set((await changes()).map(r => r.key)), new Set(['epoch-1', 'later-question']));
    await assert.rejects(publish(), /changes remain/);
    const saved = await call('inventory_publish_cut', [frozen.epoch, body()]);
    assert.equal(saved.started_at, frozen.started_at); assert.equal(saved.snapshot, frozen.snapshot);
    assert.equal(saved.manifest_sha, frozen.manifest_sha); assert.equal(saved.epoch, frozen.epoch);
    assert.equal(Date.parse(saved.expires_at) - Date.parse(frozen.started_at), 3 * 60 * 60000);
    assert.equal((await changes()).length, 2); await ack();
  });
  await check('unsealed, wrong-engine and obsolete-generation cuts cannot publish', async () => {
    await assert.rejects(call('inventory_publish_cut', ['99999999', body()]), /fence changed/);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
    const unsealed = await call('inventory_epoch_begin', [generation]); await pg.exec('COMMIT');
    await assert.rejects(call('inventory_publish_cut', [unsealed.epoch, body()]), /fence changed/);
    await query(`INSERT INTO factory_snapshot(id,at,engine,open,urgent,asking,computing,queue_rows,settle_queue_rows,daemon_at,daemon_commit)
      VALUES (1,'at','ffffffffffff',0,0,0,0,0,0,'at','commit')`);
    await assert.rejects(call('inventory_publish_cut', [frozen.epoch, body()]), /fence changed/);
    await query('DELETE FROM factory_snapshot');
    generation = (await call('inventory_begin', [engine, contract, generation])).generation;
    await assert.rejects(call('inventory_publish_cut', [frozen.epoch, body()]), /fence changed/);
  });
  await check('an expired sealed cut cannot acquire fresh publication time', async () => {
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
    const cut = await call('inventory_epoch_begin', [generation]);
    await scalar('SELECT inventory_epoch_seal($1::text,0::bigint,$2::text)', [cut.epoch, 'b'.repeat(64)]); await pg.exec('COMMIT');
    await query("UPDATE inventory_epochs SET started_at='2000-01-01T00:00:00.000Z' WHERE epoch::text=$1", [cut.epoch]);
    await assert.rejects(call('inventory_publish_cut', [cut.epoch, body()]), /cut expired/);
  });
  await check('older epochs cannot overwrite a newer cut when server timestamps tie', async () => {
    const cuts = [];
    for (let i = 0; i < 2; i++) {
      await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ');
      const cut = await call('inventory_epoch_begin', [generation]);
      cuts.push(await scalar('SELECT inventory_epoch_seal($1::text,0::bigint,$2::text)', [cut.epoch, 'd'.repeat(64)]));
      await pg.exec('COMMIT');
    }
    await query('UPDATE inventory_epochs SET started_at=$1 WHERE epoch::text=ANY($2::text[])', [cuts[1].started_at, cuts.map(c => c.epoch)]);
    await call('inventory_publish_cut', [cuts[1].epoch, body()]);
    await assert.rejects(call('inventory_publish_cut', [cuts[0].epoch, body()]), /newer inventory cut/);
    const fresh = await publish({ ...body(), epoch: 'forged', manifest_sha: 'forged' });
    assert(!('epoch' in fresh)); assert(!('manifest_sha' in fresh));
  });
  await check('restricted API grants cannot write sources, forge receipts or call private publication helpers', async () => {
    await pg.exec(`CREATE ROLE inventory_pipeline_check;
      GRANT EXECUTE ON FUNCTION inventory_status(),inventory_changes(bigint),inventory_inputs(text,text,text)
      TO inventory_pipeline_check;
      SET ROLE inventory_pipeline_check`);
    assert.equal((await call('inventory_status')).generation, generation);
    await assert.rejects(query('SELECT * FROM inventory_dirty'), /permission denied/);
    await assert.rejects(query("SELECT nextval('inventory_revision')"), /permission denied/);
    await assert.rejects(build('forged-source'), /permission denied/);
    await assert.rejects(query("UPDATE inventory_epochs SET status='sealed'"), /permission denied/);
    await assert.rejects(query('SELECT inventory_store_body(($1::text)::jsonb,$2::text,$3::text)', [JSON.stringify(body()), '2026-01-01', 'forged']), /permission denied/);
    await pg.exec(`CREATE TEMP TABLE inventory_state(generation bigint,engine text,contract jsonb,status text,id bigint);
      INSERT INTO pg_temp.inventory_state VALUES (999,'forged','{}','active',1);
      CREATE TEMP TABLE inventory_dirty(scope text,key text,revision bigint);
      INSERT INTO pg_temp.inventory_dirty VALUES ('question','forged',999);
      SET search_path TO pg_temp,public`);
    assert.equal((await call('inventory_status')).generation, generation);
    assert.deepEqual(await changes(), []);
    await pg.exec('DROP TABLE pg_temp.inventory_state,pg_temp.inventory_dirty; RESET search_path; RESET ROLE');
  });
  await check('the old post-seal source-write gap fails the same receipt-only assertion', async () => {
    const sourceGuard = inventorySql.match(/CREATE OR REPLACE FUNCTION inventory_check_source_write\(\)[^]*?END \$\$;/)[0];
    const oldGuard = sourceGuard.replace('WHERE e.capture_tx=txid_current()::text)', "WHERE e.capture_tx=txid_current()::text AND e.status='capturing')");
    assert.notEqual(oldGuard, sourceGuard);
    await pg.exec('BEGIN ISOLATION LEVEL REPEATABLE READ'); await pg.exec(oldGuard);
    const cut = await call('inventory_epoch_begin', [generation]);
    await scalar('SELECT inventory_epoch_seal($1::text,0::bigint,$2::text)', [cut.epoch, 'c'.repeat(64)]);
    await assert.rejects(async () => assert.rejects(build('old-post-seal-gap'), /may write receipts only/), /Missing expected rejection/);
    await pg.exec('ROLLBACK');
  });
  await check('old missing invalidation fails the same source-mutation assertion', async () => {
    await pg.exec('DROP TRIGGER inventory_update ON questions');
    await query("UPDATE questions SET state='spot' WHERE id='q'");
    const requireDirty = async () => assert((await changes()).some(r => r.scope === 'question' && r.key === 'q'), 'a lifecycle mutation disappeared');
    await assert.rejects(requireDirty(), /lifecycle mutation disappeared/);
    await pg.exec(inventorySql);
    await query("UPDATE questions SET state='fact' WHERE id='q'"); await requireDirty(); await ack();
  });
  await check('old unchecked publisher fails the pending-change rejection', async () => {
    const unchecked = inventorySql.replace("IF EXISTS(SELECT 1 FROM inventory_dirty) THEN RAISE EXCEPTION 'inventory changes remain'; END IF;", 'NULL;');
    assert.notEqual(unchecked, inventorySql); await pg.exec(unchecked); await build('unchecked');
    await assert.rejects(async () => assert.rejects(publish(), /changes remain/), /Missing expected rejection/);
    await pg.exec(inventorySql); await assert.rejects(publish(), /changes remain/); await ack();
  });
  console.log(`\n${passed} incremental inventory PG checks passed`);
} finally { await pg.close(); }
