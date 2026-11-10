// Scoped calibration reads and counter totals preserve the individual-payment oracle.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { testDb } from './pg/test_db.mjs';
import { COUNTS, countsOf, population, addQuestion, pricedPopulation, populationDocument } from './work_population.mjs';
import { measurementProof } from './work_reuse.mjs';
import { officialRoute } from '../worker/factory.js';

const since = '2026-10-31T00:00:00.000Z', until = '2026-11-07T00:00:00.000Z', at = '2026-11-01T00:00:00.000Z';
const signature = 'factory_calibration_page(text,text,text,bigint,bigint,boolean,boolean)';
const db = await testDb('library');
try {
  for (let i = 1; i <= 9; i++) {
    const id = 'q'+i;
    await db.run("INSERT INTO state_compute_work(id,verb,request,state,due_at,opened_at) VALUES(?1,?2,?3,'open',?4,?4)",
      id, i === 4 ? 'optimize' : 'simulate', JSON.stringify({ record: { weapon: 'braton' }, fixture: i }), at);
    await db.run(`INSERT INTO record_candidate_answers(question,engine,device,owner,canon,result,work,at,counts,credited,official,void)
      VALUES(?1,'e',?2,?2,'c','{}',?3,?4,?5,?6,?7,?8)`, id, 'device-'+i, 100+i,
      i === 7 ? since : i === 8 ? '2026-11-08T00:00:00.000Z' : at,
      i === 3 ? null : JSON.stringify({ runs: i, shots: i+1 }), i === 9 ? 0 : 1, i === 6 ? 1 : 0, i === 5 ? 1 : 0);
  }
  await db.run(`INSERT INTO record_candidate_answers(question,engine,device,owner,canon,result,work,at,counts,credited)
    VALUES('q2','e','device-extra','owner-extra','c','{}',999,?1,'{"runs":99}',1)`, at);
  const oracle = await db.all(`SELECT q.id,q.request,max(a.work) work,max(a.counts) counts,
    '['||string_agg(to_json(a.device)::text,',' ORDER BY a.id)||']' devices
    FROM record_candidate_answers a JOIN state_compute_work q ON q.id=a.question
    WHERE a.credited=1 AND a.official=0 AND a.void=0 AND q.verb='simulate' AND a.at>?1 AND a.at<=?2
    GROUP BY q.id ORDER BY q.id`, since, until);
  const report = body => new Request('https://wfsim.test/api/tasks/official/report', { method: 'POST',
    headers: { 'content-type': 'application/json', authorization: 'Bearer calibration-check' },
    body: JSON.stringify({ witness: 'official:calibration', report: 'calibration', since, until, ...body }) });
  const env = { LIBRARY: db, FACTORY_TOKEN: 'calibration-check' }, route = '/api/tasks/official/report';
  assert.equal((await officialRoute(report({}), { ...env, FACTORY_TOKEN: '' }, route)).status, 401);
  for (const [body, expected] of [[{ offset: 1 }, oracle.slice(1)], [{ after: 'q1' }, oracle.filter(q => q.id > 'q1')]]) {
    const response = await officialRoute(report(body), env, route);
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { ok: true, rows: expected });
  }
  await db.exec('CREATE ROLE wfsim_pipeline NOLOGIN; CREATE ROLE calibration_stranger NOLOGIN;');
  const schema = (await db.get('SELECT current_schema() name')).name;
  await db.exec(`GRANT USAGE ON SCHEMA "${schema}" TO wfsim_pipeline,calibration_stranger`);
  await db.exec(readFileSync(new URL('../deploy/work_calibration_grants.sql', import.meta.url), 'utf8'));
  const page = async (after = '', limit = 2, details = false, inclusive = false, offset = 0) =>
    (await db.get('SELECT factory_calibration_page(?1,?2,?3,?4::bigint,?5::bigint,?6::boolean,?7::boolean) rows',
      since, until, after, offset, limit, details, inclusive)).rows;
  await db.exec('SET ROLE calibration_stranger');
  await assert.rejects(page(), /permission denied/);
  await db.exec('RESET ROLE; SET ROLE wfsim_pipeline');
  await assert.rejects(db.all('SELECT * FROM record_candidate_answers'), /permission denied/);
  await db.exec('CREATE TEMP TABLE state_compute_work(id text)');
  let after = '', actual = [];
  for (;;) {
    const rows = await page(after);
    actual.push(...rows);
    if (rows.length < 2) break;
    assert(rows.at(-1).id > after); after = rows.at(-1).id;
  }
  assert.deepEqual(actual, oracle.map(q => ({ ...q, request: null })));
  assert.deepEqual(await page('', 2, true, false, 1), oracle.slice(1, 3));
  assert.deepEqual(await page('q2', 1, true, true), oracle.filter(q => q.id === 'q2'));
  assert.deepEqual(await page(oracle.at(-1).id), []);
  await db.exec('DROP TABLE pg_temp.questions');
  await db.exec('RESET ROLE');
  await db.run(`INSERT INTO state_compute_work(id,verb,request,state,due_at,opened_at)
    SELECT 'z'||lpad(i::text,5,'0'),'simulate','{}','open',?1,?1 FROM generate_series(1,2003)i`, at);
  await db.run(`INSERT INTO record_candidate_answers(question,engine,device,owner,canon,result,work,at,credited)
    SELECT 'z'||lpad(i::text,5,'0'),'e','d'||i,'o'||i,'c','{}',1,?1,1 FROM generate_series(1,2003)i`, at);
  const capped = await page('z', 100000);
  assert.equal(capped.length, 2000); assert.equal((await page(capped.at(-1).id, 100000)).length, 3);
  assert.equal((await page('z', 0)).length, 1);
  const security = await db.get(`SELECT prosecdef,proconfig FROM pg_proc WHERE oid=?1::regprocedure`, signature);
  assert(security.prosecdef && security.proconfig.some(setting => setting.startsWith('search_path=')));
  assert(security.proconfig.includes('plan_cache_mode=force_custom_plan'), 'cursor reads need a plan for their current bounds');
  console.log('ok: calibration API and pages match the grouped oracle, preserve offsets, cap rows and restrict table access');
} finally { await db.close(); }

const all = population(), questions = [], weights = COUNTS.map((_, i) => i+1);
for (let i = 0; i < 140; i++) {
  const q = { id: 'p'+i, work: i === 7 ? 0 : 100+i, counts: i % 11 === 0 ? null : { runs: 1+i%5, shots: i%7, hits: i%3 },
    accounts: i%3 === 0 ? ['private-account-a','private-account-a'] : ['private-account-b','unclaimed'], request: 'private-body' };
  questions.push(q); addQuestion(all, q, 40, () => 0);
}
all.sample.forEach((q, i) => { q.exact = i%3 ? { gas: 1000+i, runs: 2+i%4, key: 'fixture' } : null; });
const document = populationDocument(all, { since, until }), result = pricedPopulation(document, weights), expected = new Map();
let priced = 0;
for (const q of questions) {
  const gas = q.exact ? q.exact.gas : q.counts ? countsOf(q).reduce((sum, x, i) => sum+x*weights[i], 0) : null;
  if (gas === null || !(q.work > 0)) continue;
  priced++;
  for (const account of q.accounts) {
    const row = expected.get(account) || { account, old: 0, answers: 0, gas: 0, runs: 0 };
    row.old += q.work; row.answers++; row.gas += gas; row.runs += q.exact ? q.exact.runs : q.counts.runs || 0;
    expected.set(account, row);
  }
}
assert.equal(result.priced, priced); assert.deepEqual(result.accounts, expected);
assert.equal(document.sample.length, 40); assert(!JSON.stringify(document).includes('private-body'));
const noCorrection = pricedPopulation({ ...document, sample: [] }, weights);
assert.notDeepEqual(noCorrection.accounts, expected, 'fixture must detect lost exact-sample corrections');
console.log('ok: bounded population sums equal individual pricing, including duplicate accounts and missing counters');

const metadata = { head_repository: { full_name: 'owner/repo' }, workflow_id: 7, event: 'workflow_dispatch', status: 'completed', head_sha: 'a'.repeat(40) };
const jobs = ['build','measure (1)','measure (2)','measure (3)','measure (4)'].map(name => ({ name, conclusion: 'success' }));
assert.equal(measurementProof(metadata, jobs, 'owner/repo', 7), 'a'.repeat(40));
assert.throws(() => measurementProof(metadata, jobs.slice(1), 'owner/repo', 7));
assert.throws(() => measurementProof(metadata, jobs, 'other/repo', 7));
assert.throws(() => measurementProof({ ...metadata, event: 'pull_request' }, jobs, 'owner/repo', 7));
console.log('ok: measurement reuse rejects missing jobs, foreign repositories and pull requests');

const dir = mkdtempSync(join(tmpdir(), 'wfsim-work-fit-'));
try {
  const small = population();
  for (let i = 1; i <= 40; i++) {
    const counts = { runs: i%5+1, shots: i%7+1 }, gas = counts.runs*2e6+counts.shots*3e6;
    addQuestion(small, { id: String(i), counts, accounts: ['private-account-a'], work: gas*0.05+counts.runs*400000,
      exact: { gas, runs: counts.runs, key: 'fixture' } }, 40);
  }
  const file = join(dir,'sample.json'); writeFileSync(file,JSON.stringify(populationDocument(small,{since,until})));
  const files = [];
  for (let i = 0; i < 4; i++) {
    const path = join(dir,`measure-${i}.json`);
    writeFileSync(path,JSON.stringify({ wasm:'fixture',machine:{cpu:'fixture',cores:4,node:'fixture'},rows:Array.from({length:20},(_,j)=>({
      key:String(j),gas:10e6+j*5e6,runs:j+1,seconds:(10+j*5)*0.001+(j+1)*0.005,
    })) })); files.push(path);
  }
  const output = execFileSync(process.execPath,['scripts/work_calibrate.mjs','fit',...files,`sample=${file}`],{encoding:'utf8'});
  assert.match(output,/NEW TOTAL \/ OLD TOTAL: 1\.015/); assert.match(output,/pub const RUN_PRICE: u64 = \d[\d_]*;/);
  assert(!output.includes('NaN') && !output.includes('private-account-a'));
  console.log('ok: complete fitting keeps the payout target and never prints account identities');
} finally { rmSync(dir,{recursive:true,force:true}); }
