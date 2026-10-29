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

const question = (id, state, { fact_engine = null, lease = false } = {}) => L.run(
  `INSERT INTO questions (id, verb, request, state, due_at, opened_at, canon, fact_engine, fact_at, lease, lease_until, leased_to)
   VALUES (?, 'simulate', '{}', ?, ?, ?, ?, ?, ?, ?, ?, ?)`, id, state, AT, AT, fact_engine ? "c1" : null, fact_engine, fact_engine ? AT : null,
  lease ? "l" : null, lease ? iso(NOW + 6 * 3_600_000) : null, lease ? "official:settle-0" : null);
const demand = (question, state) => L.run(
  `INSERT INTO demands (producer, ref, question, state, asked_at, due_at, payload, served_at) VALUES ('sweep', ?, ?, ?, ?, ?, ?, ?)`,
  `ref-${question}`, question, state, AT, AT, JSON.stringify({ identity: `b-${question}`, ruler: "r", mode: "m" }), state === "served" ? AT : null);
let n = 0;
const answer = (question, canon, engine, owner = `o${n}`) => L.run(
  `INSERT INTO answers (question, engine, device, owner, net, canon, result, work, at) VALUES (?, ?, ?, ?, ?, ?, '{"score":1,"metric":"dps"}', 100, ?)`,
  question, engine, `d${++n}`, owner, `n${n}`, canon, AT);
const stateOf = async (id) => (await L.get("SELECT state, lease, canon FROM questions WHERE id = ?", id)) || {};
const moot = async () => Number((await L.get("SELECT factory_moot_checks('e1', 1000) AS n")).n);

// ---- the settler's pass ----
await question("asked", "disputed", { lease: true }); await demand("asked", "live"); await answer("asked", "c1", "e0"); await answer("asked", "c2", "e0");
await question("unasked", "disputed"); await demand("unasked", "served"); await answer("unasked", "c1", "e0"); await answer("unasked", "c2", "e0");
await question("odd", "nondeterministic"); await demand("odd", "live"); for (const c of ["c1", "c2", "c3"]) await answer("odd", c, "e0");
await question("served", "disputed"); await demand("served", "live"); await answer("served", "c1", "e1"); await answer("served", "c2", "e1");
await question("spot-old", "spot", { fact_engine: "e0" }); await demand("spot-old", "served");
await question("spot-served", "spot", { fact_engine: "e1" }); await demand("spot-served", "served");

check("one pass moves the two disputes, the non-deterministic one and the old spot", (await moot()) === 4);
const asked = await stateOf("asked");
check("a dispute of another engine a live demand waits on is open again, its official lease gone", asked.state === "open" && asked.lease === null, JSON.stringify(asked));
check("…its answers carried", Number((await L.get("SELECT count(*) AS n FROM answers WHERE question = 'asked'")).n) === 2);
check("a dispute of another engine nobody asks for any more is withdrawn", (await stateOf("unasked")).state === "withdrawn");
check("a non-deterministic question of another engine is open again", (await stateOf("odd")).state === "open");
check("a dispute of the served engine stays the official machines'", (await stateOf("served")).state === "disputed");
const back = await stateOf("spot-old");
check("a spot of another engine's fact is that fact again", back.state === "fact" && back.canon === "c1", JSON.stringify(back));
check("a spot of the served engine's fact stays a spot", (await stateOf("spot-served")).state === "spot");
check("a second pass finds nothing", (await moot()) === 0);

// ---- reopened, settled by the served engine ----
await answer("asked", "c3", "e0");
await answer("asked", "c9", "e1");
await f.settle(env, "asked", "e1", NOW);
check("the old answers do not fill the cap: one answer of the served engine matching none leaves it open", (await stateOf("asked")).state === "open");
await answer("asked", "c9", "e1");
await f.settle(env, "asked", "e1", NOW);
const settled = await stateOf("asked");
check("…and two of it agreeing make the fact", ["fact", "spot"].includes(settled.state) && settled.canon === "c9", JSON.stringify(settled));
await answer("odd", "c2", "e1");
await f.settle(env, "odd", "e1", NOW);
const joined = await stateOf("odd");
check("one answer of the served engine reproducing an old one makes the fact with it", ["fact", "spot"].includes(joined.state) && joined.canon === "c2", JSON.stringify(joined));

// ---- asked again ----
const ask = { producer: "sweep", ref: "again", verb: "simulate", request: { record: { weapon: "w" }, ruler: "r", mode: "m" }, due_in_ms: 3_600_000, payload: {} };
const [qa] = await f.demandAll(env, [ask], "e0", NOW - 7_200_000);
await L.run("UPDATE questions SET state = 'disputed' WHERE id = ?", qa);
await answer(qa, "c1", "e0"); await answer(qa, "c2", "e0");
await f.demandAll(env, [ask], "e1", NOW);
check("a dispute of another engine asked again is open again at once", (await stateOf(qa)).state === "open");
await L.run("UPDATE questions SET state = 'disputed' WHERE id = ?", qa);
await answer(qa, "c1", "e1"); await answer(qa, "c2", "e1");
await f.demandAll(env, [ask], "e1", NOW);
check("…and a dispute of the served engine asked again stays a dispute", (await stateOf(qa)).state === "disputed");

console.log(failures ? `\n${failures} failed` : "\nan official check of another engine is none, and goes back to whom it can still mean something");
process.exit(failures ? 1 : 0);
