// THE LIBRARY KEEPS A WINDOW (worker/pg/sync.sql `factory_forget_past`, the
// settler's `past` tick, worker/tasks.js `KEPT_MS`): a fact of an engine no
// longer served and a withdrawn question, settled before the window, go with
// their answers and board demands; the served engine's facts, a question a
// demand still waits on or another producer asked, a refused question and
// anything recent stay; a call takes at most its batch and one it cannot take
// never blocks the rest; a row forgotten is asked again as new; a refusal
// reaches back no further than the window.
//   node scripts/check_forget_past.mjs
import { facts } from "../worker/factory.js";
import { KEPT_MS } from "../worker/tasks.js";
import { BOARD_KINDS } from "../worker/board_kinds.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const L = await testDb("library");
const env = { LIBRARY: L, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) } };
const NOW = Date.parse("2026-11-01T12:00:00.000Z"), iso = (ms) => new Date(ms).toISOString(), DAY = 86_400_000;
const OLD = iso(NOW - 20 * DAY), RECENT = iso(NOW - 3 * DAY), BEFORE = iso(NOW - KEPT_MS);
const f = facts();

const question = (id, state, { fact_engine = null, at = OLD } = {}) => L.run(
  `INSERT INTO questions (id, verb, request, state, due_at, opened_at, canon, fact_engine, fact_at)
   VALUES (?, 'simulate', '{}', ?, ?, ?, ?, ?, ?)`, id, state, at, at, state === "fact" ? "c" : null, fact_engine, state === "fact" ? at : null);
const demand = (question, state, producer = "sweep", at = OLD) => L.run(
  `INSERT INTO demands (producer, ref, question, state, asked_at, due_at, payload, served_at) VALUES (?, ?, ?, ?, ?, ?, '{}', ?)`,
  producer, `ref-${question}-${producer}`, question, state, at, at, state === "served" ? at : null);
const answer = (question, device, at = OLD, engine = "e0") => L.run(
  `INSERT INTO answers (question, engine, device, owner, canon, result, work, at, credited) VALUES (?, ?, ?, ?, 'c', '{}', 100, ?, 1)`,
  question, engine, device, `o-${device}`, at);
const forget = async (limit = 1000) => Number((await L.get("SELECT factory_forget_past(?::text, ?::text, ?::text, ?::bigint) AS n",
  BEFORE, "e1", JSON.stringify(BOARD_KINDS), limit)).n);
const has = async (id) => !!(await L.get("SELECT 1 AS x FROM questions WHERE id = ?", id));
const count = async (table, id) => Number((await L.get(`SELECT count(*) AS n FROM ${table} WHERE question = ?`, id)).n);

// ---- what goes, what stays ----
await question("stale", "fact", { fact_engine: "e0" }); await demand("stale", "served"); await answer("stale", "d1"); await answer("stale", "d2");
await question("served-engine", "fact", { fact_engine: "e1" }); await demand("served-engine", "served"); await answer("served-engine", "d1", OLD, "e1");
await question("recent", "fact", { fact_engine: "e0", at: RECENT }); await demand("recent", "served", "sweep", RECENT);
await question("waited", "fact", { fact_engine: "e0" }); await demand("waited", "live");
await question("survey", "fact", { fact_engine: "e0" }); await demand("survey", "served", "survey");
await question("withdrawn", "withdrawn"); await demand("withdrawn", "withdrawn");
await question("refused", "withdrawn");
await L.run("INSERT INTO refusal_withdrawals (question, engine, owners, at) VALUES ('refused', 'e0', 2, ?)", OLD);
await question("open", "open"); await demand("open", "live");
await question("disputed", "disputed");

const gone = await forget();
check("a past fact of another engine and a past withdrawn question go, and nothing else", gone === 2, `forgot ${gone}`);
check("…the fact with its answers and its served demand",
  !(await has("stale")) && (await count("answers", "stale")) === 0 && (await count("demands", "stale")) === 0);
check("…the withdrawn question with its withdrawn demand", !(await has("withdrawn")) && (await count("demands", "withdrawn")) === 0);
for (const [id, why] of [["served-engine", "a fact of the served engine is the cache a row asked again is told"],
  ["recent", "a fact inside the window"], ["waited", "a question a live demand waits on"],
  ["survey", "a question a producer outside the board asked"], ["refused", "a question its engine refused, which must stay withdrawn"],
  ["open", "an open question"], ["disputed", "a disputed question"]]) {
  check(`stays: ${why}`, await has(id));
}
check("…and the served engine's fact keeps its answers", (await count("answers", "served-engine")) === 1);
check("a second call finds nothing more", (await forget()) === 0);

// ---- the batch, and what it cannot take ----
for (let i = 0; i < 5; i++) { await question(`b${i}`, "fact", { fact_engine: "e0" }); await demand(`b${i}`, "served"); }
for (let i = 0; i < 3; i++) { await question(`k${i}`, "fact", { fact_engine: "e0" }); await demand(`k${i}`, "served", "chat"); }
check("a call takes at most its batch", (await forget(2)) === 2);
check("…and the questions it may not take never fill a batch: the rest go in later calls",
  (await forget(2)) === 2 && (await forget(2)) === 1 && (await forget(2)) === 0);

// ---- asked again ----
const ask = { producer: "sweep", ref: "again", verb: "simulate", request: { record: { weapon: "w" }, ruler: "r", mode: "m" }, due_in_ms: 3_600_000, payload: {} };
const [qa] = await f.demandAll(env, [ask], "e0", NOW - 20 * DAY);
await L.run("UPDATE questions SET state = 'fact', canon = 'c', fact_engine = 'e0', fact_at = ? WHERE id = ?", OLD, qa);
await L.run("UPDATE demands SET state = 'served', served_at = ? WHERE ref = 'again'", OLD);
await forget();
await f.demandAll(env, [ask], "e1", NOW);
check("a row forgotten is asked again as a new open question with a live demand",
  (await L.get("SELECT state FROM questions WHERE id = ?", qa))?.state === "open"
  && (await L.get("SELECT state FROM demands WHERE ref = 'again'"))?.state === "live");

// ---- a refusal reaches back the window ----
await question("liar-old", "fact", { fact_engine: "e1" }); await demand("liar-old", "served"); await answer("liar-old", "liar", OLD, "e1");
await question("liar-new", "fact", { fact_engine: "e1", at: RECENT }); await demand("liar-new", "served", "sweep", RECENT); await answer("liar-new", "liar", RECENT, "e1");
const out = await f.refuse(env, "liar", NOW);
check("a refusal reopens the fact it witnessed inside the window", (await L.get("SELECT state FROM questions WHERE id = 'liar-new'")).state === "open");
check("…and leaves standing one it witnessed before it", (await L.get("SELECT state FROM questions WHERE id = 'liar-old'")).state === "fact");
check("…and takes back only the work of the window", out.taken_back === 100 && out.reopened === 1, JSON.stringify(out));

console.log(failures ? `\n${failures} failed` : "\nthe library keeps a window, and only what the window and the board still need");
process.exit(failures ? 1 : 0);
