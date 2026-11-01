// Settled task detail expires after KEPT_MS across engines and producers.
// Live delivery, unpaid witnesses, recent activity and refusals remain protected.
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
  `INSERT INTO questions (id, verb, request, state, due_at, opened_at, canon, fact_engine, fact_at, withdrawn_at)
   VALUES (?, 'simulate', '{}', ?, ?, ?, ?, ?, ?, ?)`, id, state, at, at, state === "fact" ? "c" : null, fact_engine, state === "fact" ? at : null, state === "withdrawn" ? at : null);
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

await L.run("INSERT INTO builds (id, at, record) VALUES ('published', '2026-10-01', '{}')");
await L.run("INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at) VALUES ('published', 'r', 'm', 'verified:e1', 42, 'dps', 1, ?, ?)", OLD, OLD);
const gone = await forget();
check("old facts of either engine and producer, and withdrawn questions expire", gone === 4, `forgot ${gone}`);
check("…the fact with its answers and its served demand",
  !(await has("stale")) && (await count("answers", "stale")) === 0 && (await count("demands", "stale")) === 0);
check("…the withdrawn question with its withdrawn demand", !(await has("withdrawn")) && (await count("demands", "withdrawn")) === 0);
check("current-engine facts and completed non-board demands expire", !(await has("served-engine")) && !(await has("survey")));
for (const [id, why] of [["recent", "a fact inside the window"], ["waited", "a question a live demand waits on"],
  ["refused", "a question its engine refused, which must stay withdrawn"],
  ["open", "an open question"], ["disputed", "a disputed question"]]) {
  check(`stays: ${why}`, await has(id));
}
check("current-engine answer detail expires too", (await count("answers", "served-engine")) === 0);
check("a second call finds nothing more", (await forget()) === 0);

// ---- delivery, credit and recent activity survive the cleanup ----
await question("unpaid", "fact", { fact_engine: "e1" }); await demand("unpaid", "served"); await answer("unpaid", "owed", OLD, "e1");
await L.run("UPDATE answers SET credited = 0 WHERE question = 'unpaid'");
await question("redeliver", "fact", { fact_engine: "e1" }); await demand("redeliver", "live", "chat");
await question("touched", "fact", { fact_engine: "e1" }); await demand("touched", "served", "survey", RECENT);
await question("late-detail", "fact", { fact_engine: "e1" }); await answer("late-detail", "late", RECENT, "e1");
await question("cancelled-now", "open");
await L.run("UPDATE questions SET state = 'withdrawn' WHERE id = 'cancelled-now'");
await question("leased", "fact", { fact_engine: "e1" });
await L.run("UPDATE questions SET lease = 'held', lease_until = ? WHERE id = 'leased'", RECENT);
await question("expired-lease", "fact", { fact_engine: "e1" });
await L.run("UPDATE questions SET lease_prev = 'expired', lease_prev_until = ? WHERE id = 'expired-lease'", OLD);
await question("queued", "fact", { fact_engine: "e1" });
await L.run("INSERT INTO settle_queue (question, engine, peer, joined, queued_at) VALUES ('queued', 'e1', '', 0, ?)", OLD);
await question("spot", "spot", { fact_engine: "e1" });
await question("nondeterministic", "nondeterministic", { fact_engine: "e1" });
await L.run("INSERT INTO verifiers (id, seen, work) VALUES ('owed', ?, 12345)", OLD);
await L.run("INSERT INTO verifier_hours (verifier, hour, work) VALUES ('owed', '2026-10-01T00', 12345)");
await forget();
for (const id of ["unpaid", "redeliver", "touched", "late-detail", "queued", "spot", "nondeterministic", "leased", "cancelled-now"]) check(`protected: ${id}`, await has(id));
check("expired historical leases do not retain completed detail forever", !(await has("expired-lease")));
check("credit aggregates are preserved", (await L.get("SELECT work FROM verifiers WHERE id = 'owed'")).work === 12345
  && (await L.get("SELECT work FROM verifier_hours WHERE verifier = 'owed'")).work === 12345);
await L.run("UPDATE answers SET credited = 1 WHERE question = 'unpaid'");
check("paid task becomes eligible next pass", (await forget()) === 1 && !(await has("unpaid")));

check("published scores and builds survive task expiry", (await L.get("SELECT score FROM scores WHERE identity = 'published'")).score === 42
  && !!(await L.get("SELECT id FROM builds WHERE id = 'published'")));
await question("covered", "fact", { fact_engine: "e1" }); await answer("covered", "paid", OLD, "e1"); await answer("covered", "duplicate", OLD, "e1");
await L.run("UPDATE answers SET credited = 0, owner = 'o-paid' WHERE question = 'covered' AND device = 'duplicate'");
await question("losing", "fact", { fact_engine: "e1" }); await answer("losing", "lost", OLD, "e1");
await L.run("UPDATE answers SET credited = 0, canon = 'different' WHERE question = 'losing'");
await forget();
check("covered and losing answers do not retain tasks forever", !(await has("covered")) && !(await has("losing")));

await question("review-proof", "fact", { fact_engine: "e1" });
await L.run(`INSERT INTO board_reviews (ref, id, round_at, identity, ruler, mode, engine, question, record, state,
  baseline_score, baseline_metric, baseline_cost_seconds, baseline_by, baseline_started_at, baseline_finished_at, created_at)
  VALUES ('published|r|m', 'review', ?, 'published', 'r', 'm', 'e1', 'review-proof', '{}', 'succeeded', 40, 'dps', 1, 'old', ?, ?, ?)`, OLD, OLD, OLD, OLD);
await L.run("UPDATE scores SET review_id = 'review' WHERE identity = 'published'");
await forget();
check("the current score keeps its calibration proof against older writes", await has("review-proof"));
await L.run("UPDATE scores SET review_id = NULL WHERE identity = 'published'");
await forget();
check("superseded calibration proof can expire", !(await has("review-proof")));

// ---- the batch, and what it cannot take ----
for (let i = 0; i < 5; i++) { await question(`b${i}`, "fact", { fact_engine: "e0" }); await demand(`b${i}`, "served"); }
for (let i = 0; i < 3; i++) { await question(`k${i}`, "fact", { fact_engine: "e0" }); await demand(`k${i}`, "live", "chat"); }
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

const oldPacket = { lease: "expired-old-lease", engine: "e0", late: true,
  answers: [{ question: qa, result: { score: 1, metric: "dps" }, work: 100 }] };
const packet = await f.answerPack(env, { device: "owed", owner: "owed" }, oldPacket, NOW);
check("a delayed old packet cannot earn credit after expiry and re-ask", packet.taken.length === 0
  && (await count("answers", qa)) === 0 && (await L.get("SELECT work FROM verifiers WHERE id = 'owed'")).work === 12345);

// ---- a refusal reaches back the window ----
await question("liar-old", "fact", { fact_engine: "e1" }); await demand("liar-old", "served"); await answer("liar-old", "liar", OLD, "e1");
await question("liar-new", "fact", { fact_engine: "e1", at: RECENT }); await demand("liar-new", "served", "sweep", RECENT); await answer("liar-new", "liar", RECENT, "e1");
const out = await f.refuse(env, "liar", NOW);
check("a refusal reopens the fact it witnessed inside the window", (await L.get("SELECT state FROM questions WHERE id = 'liar-new'")).state === "open");
check("…and leaves standing one it witnessed before it", (await L.get("SELECT state FROM questions WHERE id = 'liar-old'")).state === "fact");
check("…and takes back only the work of the window", out.taken_back === 100 && out.reopened === 1, JSON.stringify(out));

console.log(failures ? `\n${failures} failed` : "\nthe library keeps a window, and only what the window and the board still need");
process.exit(failures ? 1 : 0);
