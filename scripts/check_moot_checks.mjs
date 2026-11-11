import { seedComputeWork } from "./pg/fixtures.mjs";
// AN OFFICIAL CHECK OF ANOTHER ENGINE IS NONE (worker/pg/sync.sql
// `factory_moot_checks`, the settler's `moot` tick, worker/tasks.js
// `demandAll`): a dispute or a non-deterministic question no answer of the
// served engine is in goes back to the volunteers where a live demand waits,
// its answers and none of its lease kept, and is withdrawn where none does; a
// spot of another engine's fact is that fact again; a dispute of the served
// engine and its spots stay the official machines'. Reopened, it is no dispute
// again until the served engine's own answers fill the cap; two of them
// agreeing make the fact, and so does one reproducing an old answer.
//   node scripts/check_moot_checks.mjs
import { facts, VERBS } from "../worker/factory.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const L = await testDb("library");
const env = { LIBRARY: L, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) } };
const NOW = Date.parse("2026-11-01T12:00:00.000Z"), iso = (ms) => new Date(ms).toISOString(), AT = iso(NOW - 3_600_000);
// Engine joins are tested without the random third witness covered by check_tasks.
VERBS.simulate.recheck = 0;
const f = facts();

const question = (id, state, { fact_engine = null, lease = false } = {}) => (async (fixtureDb,args)=>{await seedComputeWork(fixtureDb,{id:args[0],verb:"simulate",request:"{}",state:args[1],due_at:args[2],opened_at:args[3],canon:args[4],fact_engine:args[5],fact_at:args[6],lease:args[7],lease_until:args[8],leased_to:args[9]});})(L,[id,state,AT,AT,fact_engine ? "c1" : null,fact_engine,fact_engine ? AT : null,lease ? "l" : null,lease ? iso(NOW + 6 * 3_600_000) : null,lease ? "official:settle-0" : null]);
const demand = (question, state) => L.run(
  `INSERT INTO relation_task_consumers (producer, ref, question, state, asked_at, due_at, payload, served_at) VALUES ('sweep', ?, ?, ?, ?, ?, ?, ?)`,
  `ref-${question}`, question, state, AT, AT, JSON.stringify({ identity: `b-${question}`, ruler: "r", mode: "m" }), state === "served" ? AT : null);
let n = 0;
const answer = (question, canon, engine, owner = `o${n}`) => L.run(
  `INSERT INTO record_candidate_answers (question, engine, device, owner, net, canon, result, work, at) VALUES (?, ?, ?, ?, ?, ?, '{"score":1,"metric":"dps"}', 100, ?)`,
  question, engine, `d${++n}`, owner, `n${n}`, canon, AT);
const stateOf = async (id) => (await L.get("SELECT state, lease, canon FROM state_compute_work WHERE id = ?", id)) || {};
const moot = async () => Number((await L.get("SELECT factory_moot_checks('e1', 1000) AS n")).n);

for(const [id,state]of [['asked','disputed'],['odd','nondeterministic'],['spot-old','spot']]) {
 await question(id,state,{lease:true,...(state==='spot'?{fact_engine:'e0'}:{})});await demand(id,'live');await answer(id,'c1','e0');
}
const before=await L.all('SELECT * FROM state_compute_work ORDER BY id');
check('an engine deployment creates no recalibration round',await moot()===0);
check('existing disputed, nondeterministic and spot entries retain their leases and results',JSON.stringify(await L.all('SELECT * FROM state_compute_work ORDER BY id'))===JSON.stringify(before));
check('repeated maintenance remains a no-op',await moot()===0);
console.log(failures?`\n${failures} failed`:'\nrelease metadata cannot reopen or replace queued work');
process.exitCode=failures?1:0;
