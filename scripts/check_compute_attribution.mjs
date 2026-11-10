import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { computationSchemaSql } from './computation_schema.mjs';
import { testDb } from './pg/test_db.mjs';
import { replaceComputeResult } from './pg/fixtures.mjs';
import { factory } from '../worker/tasks.js';
import { VERBS } from '../worker/factory.js';
import { BOARD_PRODUCERS } from '../worker/board_tasks.js';
import { contributorProfilesRoute, computationNames } from '../worker/contributor_profiles.js';
import { contributionRoute } from '../worker/contribution.js';
import { witnessNames } from '../worker/contributor_identity.js';
import { acceptedWitnesses } from '../worker/task_contract.js';
import { purgeDeleted, sha256 } from '../worker/accounts.js';
import { computationWrites, purgeComputationRecords, COMPUTATION_KEEP_MS } from '../worker/computation_records.js';

const library = await testDb('library'), accounts = await testDb('accounts');
const engine = '123456789abc', identity = '1'.repeat(32);
const env = { LIBRARY: library, ACCOUNTS: accounts, AUTH_SECRET: 'attribution-check',
  ASSETS: { fetch: async () => Response.json({ engine }) } };
const f = factory({ verbs: VERBS, producers: { ...BOARD_PRODUCERS, search: { on_fact: async () => {} } }, spot: 0, recheck: 0,
  on_fact_writes: (env, q, fact) => computationWrites(env.LIBRARY, q, fact.witnesses, fact.engine, fact.at) });
const users = ['ann', 'bob', 'cy', 'dan'];
const devices = Object.fromEntries(users.map((person, i) => [person, String(i + 1).repeat(24)]));
let passed = 0;
const check = async (name, fn) => { await fn(); passed++; console.log('  ok  ' + name); };
const pipelineSchema = (await library.get('SELECT current_schema() AS name')).name;
await library.exec(`CREATE ROLE wfsim_pipeline; GRANT USAGE ON SCHEMA ${pipelineSchema} TO wfsim_pipeline;
  GRANT SELECT, INSERT, UPDATE, DELETE ON state_board_measurement_selection TO wfsim_pipeline;`);
await check('the upgrade applies twice to a pre-attribution database and only shortens optimization details', async () => {
  await library.exec('DROP TABLE relation_measurement_witnesses,relation_computation_witnesses,state_public_computation_summaries; DROP TRIGGER scores_replace_witnesses ON state_board_measurement_selection; DROP FUNCTION score_witnesses_replaced(); ALTER TABLE entity_compute_devices DROP COLUMN signatures; ALTER TABLE record_hourly_compute_totals DROP COLUMN signatures');
  await accounts.exec('ALTER TABLE entity_accounts DROP COLUMN profile_id');
  for (const kind of ['simulation', 'optimization']) await library.run(`INSERT INTO entity_user_calculation_requests
    (id,task,owner,kind,engine,question,state,created_at,completed_at,expires_at) VALUES (?,?,?,?,'engine','question','succeeded','2026-01-01T00:00:00.000Z','2026-01-02T00:00:00.000Z','2026-02-01T00:00:00.000Z')`, kind, kind, 'owner', kind);
  for (let i = 0; i < 2; i++) { await library.exec(computationSchemaSql()); await accounts.exec(computationSchemaSql('accounts')); }
  assert.equal((await library.get("SELECT expires_at FROM entity_user_calculation_requests WHERE kind='optimization'")).expires_at, '2026-01-16T00:00:00.000Z');
  assert.equal((await library.get("SELECT expires_at FROM entity_user_calculation_requests WHERE kind='simulation'")).expires_at, '2026-02-01T00:00:00.000Z');
  await library.run('DELETE FROM entity_user_calculation_requests');
});
await check('the pipeline replaces a score and clears its old witnesses without reading private witness columns', async () => {
  await library.exec(`INSERT INTO state_board_measurement_selection VALUES ('pipeline-check','ruler','base','engine',1,'kpm',1,'old','old',NULL);
    INSERT INTO relation_measurement_witnesses VALUES ('pipeline-check','ruler','base','old','engine','private-owner','private-device',0,'old','old',1);
    SET ROLE wfsim_pipeline;`);
  try {
    await library.run("UPDATE state_board_measurement_selection SET finished_at='new' WHERE identity='pipeline-check'");
    const privateColumn = await library.get("SELECT has_column_privilege(current_user,'relation_measurement_witnesses','owner','SELECT') AS allowed");
    assert.equal(privateColumn.allowed, false);
  } finally { await library.exec('RESET ROLE'); }
  assert.equal((await library.get("SELECT count(*) AS n FROM relation_measurement_witnesses WHERE identity='pipeline-check'")).n, 0);
  await library.run("DELETE FROM state_board_measurement_selection WHERE identity='pipeline-check'");
});
const call = async (path, cookie = '', body) => {
  const request = new Request('https://wfsim.test' + path, { method: body ? 'POST' : 'GET', headers: { cookie, origin: 'https://wfsim.test', ...(body ? { 'content-type': 'application/json' } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
  const response = await contributorProfilesRoute(request, env, path.split('?')[0]);
  return { status: response.status, ...await response.json() };
};
const choose = async (person, named) => {
  const path = '/api/account/contribution';
  const response = await contributionRoute(new Request('https://wfsim.test' + path, { method: 'POST', headers: { cookie: 'wfsim_session=' + person, origin: 'https://wfsim.test', 'content-type': 'application/json' }, body: JSON.stringify({ named }) }), env, path);
  assert.equal(response.status, 200);
};
const mine = user => call('/api/account/contribution-profile', 'wfsim_session=' + user);
const accept = async (verb, request, who, result) => {
  const q = await f.demand(env, { producer: verb === 'simulate' ? 'new_build' : 'search', ref: crypto.randomUUID(), verb, request,
    payload: { identity, ruler: 'standard_single_target', mode: 'base', batch: 'batch' }, due_in_ms: 100000 }, engine);
  for (const [i, user] of who.entries()) {
    const device = devices[user], witness = { device, owner: user, net: user, siblings: [device, 'cap:calculation-reports-v1'] };
    const leased = await f.next(env, witness, engine);
    assert.equal(leased?.question, q);
    await f.answer(env, witness, { lease: leased.lease, engine, work: 100, result }, Date.now() + i);
  }
  return q;
};
for (const user of users) {
  await library.run("INSERT INTO entity_compute_devices (id,seen) VALUES (?,'2026-11-08')", devices[user]);
  await accounts.run("INSERT INTO entity_accounts (id,created_at,username,display_name) VALUES (?,'2026-01-01',?,?)", user, user, user.toUpperCase());
  await accounts.run("INSERT INTO state_login_sessions (token_hash,account,expires_at) VALUES (?,?,'2999-01-01')", await sha256(user), user);
  await accounts.run("INSERT INTO relation_account_compute_devices (verifier,account,claimed_at) VALUES (?,?,'2026-01-01')", devices[user], user);
  await accounts.run("INSERT INTO record_compute_device_ownership_intervals (verifier,account,from_hour) VALUES (?,?,'')", devices[user], user);
}
const build = { weapon: 'torid', mods: ['serration'], evolutions: [], arcanes: [], valence: '' };
await library.run("INSERT INTO entity_weapon_builds (id,at,record) VALUES (?,'2026-01-01',?)", identity, JSON.stringify(build));
await library.run("INSERT INTO entity_compute_batches (id,at,total) VALUES ('batch','2026-01-01',1)");
await library.run("INSERT INTO state_board_pending_work (batch,build_id,ruler,mode) VALUES ('batch',?,'standard_single_target','base')", identity);
await accept('simulate', { record: build, ruler: 'standard_single_target', mode: 'base' }, ['ann', 'bob'], { score: 42, metric: 'kpm' });
const measured = await library.get('SELECT * FROM state_board_measurement_selection WHERE identity=?', identity);
const ask = [{ identity, ruler: measured.ruler, mode: measured.mode, score: measured.score, measured_at: measured.finished_at, measured_by: engine }];
await check('public record lookups reject malformed rows and excessive batches', async () => {
  assert.equal((await call('/api/computations/names', '', { rows: [null] })).status, 400);
  assert.equal((await call('/api/computations/names', '', { rows: Array(201).fill(ask[0]) })).status, 400);
  assert.equal((await call('/api/computations/names', '', { rows: ask })).rows.length, 1);
});
await check('anonymous work is credited and retained under the same stable account', async () => {
  const own = await mine('ann');
  assert.equal(own.profile.signatures, 1);
  assert.equal(own.candidates.length, 1);
  assert.equal(own.profile.named, false);
  assert.equal((await call('/api/contributors/profile/' + own.profile.id)).status, 404);
  assert.ok((await computationNames(env, ask))[0].witnesses.every(w => w.name === null && w.profile === null));
});
await choose('ann', true); await choose('bob', true);
if (process.argv.includes('--negative-control')) await library.run('UPDATE relation_measurement_witnesses SET position=-position');
await check('public names follow actual completion order and expose no private identifiers', async () => {
  const result = (await computationNames(env, ask))[0];
  assert.deepEqual(result.witnesses.map(w => w.name), ['ANN', 'BOB']);
  assert.ok(result.witnesses.every(w => /^[a-f0-9]{32}$/.test(w.profile) && !('device' in w) && !('owner' in w)));
});
const publicId = (await mine('ann')).profile.id;
await check('identity and summary avoid record scans while retaining current privacy', async () => {
  const request = view => new Request('https://wfsim.test/api/contributors/profile/' + publicId + '?view=' + view);
  const identityResponse = await contributorProfilesRoute(request('identity'), { ...env, LIBRARY: null }, '/api/contributors/profile/' + publicId);
  const identityValue = await identityResponse.json();
  assert.equal(identityResponse.status, 200);
  assert.equal(identityValue.profile.name, 'ANN');
  assert.ok(!('points' in identityValue.profile) && !('candidates' in identityValue));
  const noRecords = { ...env, LIBRARY: { prepare(sql) {
    assert.ok(!/relation_measurement_witnesses|state_public_computation_summaries/.test(sql), 'summary must not query record tables');
    return library.prepare(sql);
  } } };
  const summaryResponse = await contributorProfilesRoute(request('summary'), noRecords, '/api/contributors/profile/' + publicId);
  const summary = await summaryResponse.json();
  assert.equal(summaryResponse.status, 200);
  assert.equal(summary.profile.signatures, 1);
  assert.ok(!('candidates' in summary));
  await choose('ann', false);
  for (const view of ['identity', 'summary', 'records']) assert.equal((await call('/api/contributors/profile/' + publicId + '?view=' + view)).status, 404);
  await choose('ann', true);
});
await check('record-only pages avoid contribution totals and enforce bounded page sizes', async () => {
  const boundedEnv = { ...env, LIBRARY: { prepare(sql) {
    assert.ok(!/\bverifiers\b|\bverifier_hours\b/.test(sql), 'records must not query aggregate counters');
    return library.prepare(sql);
  } } };
  const path = '/api/contributors/profile/' + publicId;
  const response = await contributorProfilesRoute(new Request('https://wfsim.test' + path + '?view=records&limit=1'), boundedEnv, path);
  const value = await response.json();
  assert.equal(response.status, 200); assert.equal(value.candidates.length, 1);
  assert.ok(!('points' in value.profile));
  for (const limit of ['0', '51', '1.5', 'bad']) assert.equal((await call(path + '?limit=' + limit)).status, 400);
});
await check('renaming changes existing signatures while preserving the profile address', async () => {
  await accounts.run("UPDATE entity_accounts SET display_name='New Ann' WHERE id='ann'");
  assert.equal((await computationNames(env, ask))[0].witnesses[0].name, 'New Ann');
  assert.equal((await mine('ann')).profile.id, publicId);
  assert.equal((await call('/api/contributors/profile/' + publicId)).profile.name, 'New Ann');
});
await check('visibility changes never remove lifetime credit', async () => {
  await choose('ann', false);
  assert.equal((await computationNames(env, ask))[0].witnesses[0].name, null);
  assert.equal((await mine('ann')).profile.signatures, 1);
  await choose('ann', true);
  assert.equal((await computationNames(env, ask))[0].witnesses[0].name, 'New Ann');
});
await check('a later calculation replaces the displayed cohort without removing past signature credit', async () => {
  await choose('cy', true); await choose('dan', true);
  await library.run("INSERT INTO state_board_pending_work (batch,build_id,ruler,mode) VALUES ('batch',?,'standard_single_target','base')", identity);
  await accept('simulate', { record: build, ruler: 'standard_single_target', mode: 'base', refresh: true }, ['cy', 'dan'], { score: 43, metric: 'kpm' });
  ask[0].measured_at = (await library.get('SELECT finished_at FROM state_board_measurement_selection WHERE identity=?', identity)).finished_at;
  ask[0].score = 43;
  assert.deepEqual((await computationNames(env, ask))[0].witnesses.map(w => w.name), ['CY', 'DAN']);
  assert.equal((await mine('ann')).candidates.length, 0);
  assert.equal((await mine('ann')).profile.signatures, 1);
  assert.equal((await mine('cy')).candidates.length, 1);
});
await check('a changed score within the same published minute cannot inherit another result’s names', async () => {
  await library.run('UPDATE state_board_measurement_selection SET score=44 WHERE identity=?', identity);
  assert.equal((await computationNames(env, ask)).length, 0);
  await library.run('UPDATE state_board_measurement_selection SET score=43 WHERE identity=?', identity);
});
await check('a newer unrecorded official measurement never inherits old volunteer names', async () => {
  await library.run('UPDATE state_board_measurement_selection SET finished_at=?,measured_by=? WHERE identity=?', new Date(Date.now() + 60000).toISOString(), engine, identity);
  assert.equal((await computationNames(env, ask)).length, 0);
  const newer = await library.get('SELECT * FROM state_board_measurement_selection WHERE identity=?', identity);
  const got = await computationNames(env, [{ ...ask[0], measured_at: newer.finished_at }]);
  assert.deepEqual(got[0].witnesses, [{ official: true, name: null }]);
  assert.equal((await mine('ann')).candidates.length, 0);
});
const optimization = await accept('optimize', { weapon: 'torid', private_input: 'must not be public' }, ['ann', 'bob'], { build: { mods: ['serration'] }, score: 5 });
await check('optimization records preserve attribution without exposing private input or report payloads', async () => {
  const got = await call('/api/computations/' + optimization);
  assert.equal(got.status, 200);
  assert.deepEqual(got.record.witnesses.map(w => w.name), ['New Ann', 'BOB']);
  assert.ok(!JSON.stringify(got).includes('private_input') && !('request' in got.record) && !('result' in got.record));
  const own = await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=ann');
  assert.equal(own.profile.signatures, 2);
  assert.equal(own.candidates[0].question, optimization);
});
await check('settling an unchanged proof again does not renew its fourteen day detail window', async () => {
  const original = await library.get('SELECT * FROM state_public_computation_summaries WHERE question=?', optimization);
  const later = new Date(Date.now() + 60000).toISOString();
  await replaceComputeResult(library,optimization,{fact_at:later});
  const q = await library.get('SELECT * FROM state_compute_work WHERE id=?', optimization);
  await library.batch(computationWrites(library, q, JSON.parse(q.witnesses), engine, later));
  const kept = await library.get('SELECT * FROM state_public_computation_summaries WHERE question=?', optimization);
  assert.equal(kept.completed_at, original.completed_at);
  assert.equal(kept.expires_at, original.expires_at);
});
await check('optimization display expires at fourteen days before physical cleanup', async () => {
  const row = await library.get('SELECT * FROM state_public_computation_summaries WHERE question=?', optimization);
  assert.equal(Date.parse(row.expires_at) - Date.parse(row.completed_at), COMPUTATION_KEEP_MS);
  await library.run('UPDATE state_public_computation_summaries SET expires_at=? WHERE question=?', new Date(Date.now() - 1).toISOString(), optimization);
  assert.equal((await call('/api/computations/' + optimization)).status, 410);
  assert.equal((await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=ann')).candidates.length, 0);
});
await check('detail cleanup and current-board removal retain permanent aggregate signatures', async () => {
  await purgeComputationRecords(env);
  assert.equal(await library.get('SELECT * FROM state_public_computation_summaries WHERE question=?', optimization), null);
  assert.equal((await library.get('SELECT count(*) AS n FROM relation_computation_witnesses WHERE question=?', optimization)).n, 0);
  assert.equal((await library.get('SELECT witnesses FROM state_compute_work WHERE id=?', optimization)).witnesses, '[]');
  await library.run('DELETE FROM state_board_measurement_selection WHERE identity=?', identity);
  assert.equal((await mine('ann')).profile.signatures, 2);
  assert.equal((await mine('ann')).candidates.length, 0);
});
await check('official and volunteer witnesses settle under the deployed daemon permissions', async () => {
  const schema = (await library.get('SELECT current_schema() AS name')).name;
  await library.exec(`CREATE ROLE wfsim_factoryd; GRANT USAGE ON SCHEMA ${schema} TO wfsim_factoryd;`);
  const grants = readFileSync(new URL('../deploy/factoryd_grants.sql', import.meta.url), 'utf8');
  await library.exec([...grants.matchAll(/GRANT (?!CONNECT)[^;]+;/g)].map(m => m[0]).join('\n'));
  const q = await f.demand(env, { producer: 'search', ref: 'official-check', verb: 'optimize', request: { weapon: 'torid', official_check: true }, due_in_ms: 100000 }, engine);
  const witness = { device: devices.ann, owner: 'ann', net: 'ann', siblings: [devices.ann] }, leased = await f.next(env, witness, engine);
  await f.answer(env, witness, { lease: leased.lease, engine, work: 100, result: { build: { mods: [] }, score: 7 } });
  await f.answerOfficial(env, { device: 'official:reference', owner: 'official:reference', official: true }, { question: q, engine, work: 100, result: { build: { mods: [] }, score: 7 } });
  const settled = await library.get('SELECT * FROM state_compute_work WHERE id=?', q);
  await library.run('DELETE FROM state_public_computation_summaries WHERE question=?', q);
  await library.exec('SET ROLE wfsim_factoryd');
  try {
    await library.batch(computationWrites(library, settled, JSON.parse(settled.witnesses), engine, settled.fact_at));
  } finally { await library.exec('RESET ROLE'); }
  const got = await call('/api/computations/' + q);
  assert.equal(got.record.witnesses[0].name, 'New Ann');
  assert.equal(got.record.witnesses[1].official, true);
  assert.equal(got.record.witnesses[1].name, 'reference');
  assert.equal((await mine('ann')).profile.signatures, 3);
});
await check('first claims and device transfers use the same credit hour as aggregate signatures', async () => {
  const device = '9'.repeat(24);
  await accounts.run("INSERT INTO record_compute_device_ownership_intervals (verifier,account,from_hour,until_hour) VALUES (?,'ann','','2026-10-01T02')", device);
  await accounts.run("INSERT INTO record_compute_device_ownership_intervals (verifier,account,from_hour,until_hour) VALUES (?,'cy','2026-10-01T02','2026-10-01T03')", device);
  const rows = ['01', '02', '03'].map(hour => ({ owner: device, device, official: 0,
    credited_at: '2026-10-01T' + hour + ':00:00.000Z', completed_at: '2026-10-01T01:00:00.000Z' }));
  assert.deepEqual((await witnessNames(env, rows)).map(w => w.name), ['New Ann', 'CY', null]);
  const answer = { id: 1, owner: 'ann', device, official: 0, at: rows[0].completed_at, credited: 1 };
  const snapshot = acceptedWitnesses([answer], rows[0].credited_at);
  assert.equal(acceptedWitnesses([answer], rows[1].credited_at, snapshot)[0].credited_at, rows[0].credited_at);
  const question = '9'.repeat(32), now = new Date().toISOString();
  await library.run("INSERT INTO state_public_computation_summaries (question,kind,engine,weapon,completed_at,expires_at) VALUES (?,'optimization',?,'torid',?,'2999-01-01')", question, engine, now);
  await library.run("INSERT INTO relation_computation_witnesses (question,owner,device,official,completed_at,credited_at,position) VALUES (?,?,?,0,?,?,0)", question, device, device, rows[1].completed_at, rows[1].credited_at);
  assert.ok(!(await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=ann')).candidates.some(r => r.question === question));
  assert.ok((await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=cy')).candidates.some(r => r.question === question));
});
await check('exclusive cursors paginate a contributor without repeating or skipping equal-time records', async () => {
  const at = new Date().toISOString();
  for (let i = 0; i < 51; i++) {
    const q = (1000 + i).toString(16).padStart(32, '0');
    await library.run("INSERT INTO state_public_computation_summaries (question,kind,engine,weapon,completed_at,expires_at) VALUES (?,'optimization',?,'test_weapon',?,'2999-01-01')", q, engine, at);
    await library.run("INSERT INTO relation_computation_witnesses (question,owner,device,official,completed_at,credited_at,position) VALUES (?,'ann',?,0,?,?,0)", q, devices.ann, at, at);
  }
  const first = await call('/api/account/contribution-profile?kind=optimization&weapon=test_weapon', 'wfsim_session=ann');
  assert.equal(first.candidates.length, 50);
  assert.ok(first.next);
  const next = await call('/api/account/contribution-profile?kind=optimization&weapon=test_weapon&before=' + encodeURIComponent(JSON.stringify(first.next)), 'wfsim_session=ann');
  assert.equal(next.candidates.length, 1);
  assert.equal(new Set([...first.candidates, ...next.candidates].map(row => row.question)).size, 51);
  assert.equal(next.next, null);
});
await check('unregistered work has no profile, first claims recover it and deleted ownership cannot be claimed again', async () => {
  const OriginalDate = Date;
  let clock = OriginalDate.parse('2026-11-08T12:30:00.000Z');
  globalThis.Date = class extends OriginalDate {
    constructor(...args) { super(...(args.length ? args : [clock])); }
    static now() { return clock; }
  };
  const guest = 'e'.repeat(24), result = { build: { mods: [] }, score: 9 };
  const complete = async (round, holder) => {
    const question = await f.demand(env, { producer: 'search', ref: 'claim-lifecycle-' + round, verb: 'optimize',
      request: { weapon: 'torid', claim_lifecycle: round }, due_in_ms: 100000 }, engine);
    for (const witness of [{ device: guest, owner: holder, net: 'guest', siblings: [guest] },
      { device: devices.bob, owner: 'bob', net: 'bob', siblings: [devices.bob] }]) {
      const leased = await f.next(env, witness, engine);
      assert.equal(leased?.question, question);
      await f.answer(env, witness, { lease: leased.lease, engine, work: 100, result });
    }
    return question;
  };
  const claim = async person => {
    const path = '/api/account/devices/claim';
    const response = await contributionRoute(new Request('https://wfsim.test' + path, { method: 'POST',
      headers: { 'content-type': 'application/json', cookie: 'wfsim_session=' + person }, body: JSON.stringify({ verifier: guest }) }), env, path);
    assert.equal(response.status, 200);
  };
  try {
    await library.run('INSERT INTO entity_compute_devices (id,seen) VALUES (?,?)', guest, new Date().toISOString());
    const question = await complete(1, guest), accountsBefore = (await accounts.get('SELECT count(*) AS n FROM entity_accounts')).n;
    assert.equal((await call('/api/account/contribution-profile')).status, 401);
    assert.equal((await accounts.get('SELECT count(*) AS n FROM entity_accounts')).n, accountsBefore);
    assert.equal((await library.get('SELECT signatures FROM entity_compute_devices WHERE id=?', guest)).signatures, 1);
    assert.equal((await call('/api/computations/' + question)).record.witnesses[0].profile, null);
    for (const person of ['claimant', 'later_holder']) {
      await accounts.run('INSERT INTO entity_accounts (id,created_at,username,display_name) VALUES (?,?,?,?)', person, new Date().toISOString(), person, person.toUpperCase());
      await accounts.run("INSERT INTO state_login_sessions (token_hash,account,expires_at) VALUES (?,?,'2999-01-01')", await sha256(person), person);
    }
    await claim('claimant');
    const own = await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=claimant');
    assert.equal(own.profile.signatures, 1);
    assert.deepEqual(own.candidates.map(row => row.question), [question]);
    assert.equal(own.profile.named, false);
    assert.equal((await call('/api/contributors/profile/' + own.profile.id)).status, 404);
    await choose('claimant', true);
    assert.equal((await call('/api/computations/' + question)).record.witnesses[0].name, 'CLAIMANT');
    await accounts.run("UPDATE entity_accounts SET delete_after='2000-01-01' WHERE id='claimant'");
    assert.equal(await purgeDeleted(env), 1);
    assert.equal((await call('/api/contributors/profile/' + own.profile.id)).status, 404);
    const anonymous = (await call('/api/computations/' + question)).record.witnesses[0];
    assert.equal(anonymous.name, null);
    assert.equal(anonymous.profile, null);
    assert.equal((await library.get('SELECT signatures FROM entity_compute_devices WHERE id=?', guest)).signatures, 1);
    assert.equal((await accounts.get('SELECT account FROM record_compute_device_ownership_intervals WHERE verifier=?', guest)).account, null);
    if (process.argv.includes('--negative-control-claim')) await accounts.run('DELETE FROM record_compute_device_ownership_intervals WHERE verifier=?', guest);
    await claim('later_holder');
    const later = await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=later_holder');
    assert.equal(later.profile.signatures, 0);
    assert.equal(later.candidates.length, 0);
    clock += 3600000;
    const subsequent = await complete(2, 'later_holder');
    const fresh = await call('/api/account/contribution-profile?kind=optimization', 'wfsim_session=later_holder');
    assert.equal(fresh.profile.signatures, 1);
    assert.deepEqual(fresh.candidates.map(row => row.question), [subsequent]);
    assert.equal((await call('/api/computations/' + question)).record.witnesses[0].name, null);
  } finally { globalThis.Date = OriginalDate; }
});
await accounts.close(); await library.close();
console.log(`\n${passed} attribution checks passed`);
