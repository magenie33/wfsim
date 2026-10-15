// WHO ASKED (docs/BOARD.md §"Who asked"): a demand carries its initiator and a
// question its earliest-due live demand's; an initiator past its share of the
// live leases is passed over (dial `initiator_share_pct`, worker/pg/sync.sql
// `factory_capped`) and nobody else is; a code is erased once its ask is over
// (`factory_forget_codes`); a board row's initiator follows its batch, and a
// code is a keyed hash, the same for the same asker; a submission signed in
// keeps its account's code beside it and one signed out `anonymous`, and an
// arrival is asked as the one its queue row names, the fight it named waited on.
//   node scripts/check_initiator.mjs
import fs from "node:fs";
import { facts } from "../worker/factory.js";
import { initiatorOf, boardTasksRoute } from "../worker/board_tasks.js";
import { initiatorCode, sha256 } from "../worker/accounts.js";
import worker from "../worker/index.js";
import { DIAL_DEFAULTS } from "../worker/dials.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const L = await testDb("library");
const env = { LIBRARY: L, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) }, AUTH_SECRET: "s" };
const NOW = Date.parse("2026-10-14T12:00:00.000Z"), iso = (ms) => new Date(ms).toISOString(), at = iso(NOW);
const f = facts();
const ask = (ref, initiator, due_in_ms = 3_600_000, producer = "sweep") => ({ producer, ref, verb: "simulate",
  request: { record: { weapon: "braton_prime", mods: [ref] }, ruler: "standard_single_target", mode: "base" }, due_in_ms, payload: {}, initiator });

// ---- carried ----
const [q1] = await f.demandAll(env, [ask("r1", "user:aaaa")], "e1", NOW);
check("a demand and its question carry who asked",
  (await L.get("SELECT initiator FROM demands WHERE ref = 'r1'")).initiator === "user:aaaa"
  && (await L.get("SELECT initiator FROM questions WHERE id = ?", q1)).initiator === "user:aaaa");
// THE SAME QUESTION ASKED SOONER BY ANOTHER: the question is the earlier ask's.
const shared = { record: { weapon: "braton_prime", mods: ["r1"] }, ruler: "standard_single_target", mode: "base" };
await f.demandAll(env, [{ ...ask("r1b", "system", 60_000, "new_build"), request: shared }], "e1", NOW);
check("a question asked again sooner takes the sooner ask's initiator, as it takes its producer",
  (await L.get("SELECT initiator, producer FROM questions WHERE id = ?", q1)).initiator === "system");
await f.withdraw(env, "new_build", "r1b", NOW);
check("…and once that ask is withdrawn, the earliest-due live demand's again",
  (await L.get("SELECT initiator FROM questions WHERE id = ?", q1)).initiator === "user:aaaa");

// ---- the share ----
// TEN ROWS OF ONE PERSON, TEN OF THE SYSTEM; the person holds six of eight leases.
const rows = [];
for (let i = 0; i < 10; i++) rows.push(ask(`u${i}`, "user:heavy"), ask(`s${i}`, "system"));
const ids = await f.demandAll(env, rows, "e1", NOW);
const userIds = ids.filter((_, k) => k % 2 === 0), sysIds = ids.filter((_, k) => k % 2 === 1);
const lease = async (id, until = iso(NOW + 600_000)) =>
  L.run("UPDATE questions SET lease = 'l', leased_to = 'dev', leased_at = ?, lease_until = ? WHERE id = ?", at, until, id);
for (const id of userIds.slice(0, 6)) await lease(id);
for (const id of sysIds.slice(0, 2)) await lease(id);
const eligible = async () => (await L.all(
  "SELECT initiator FROM factory_eligible(?::text, 0, 1, 'e1', '[\"x\"]', 'x', '', NULL, 7200, 120) e WHERE e.state = 'open'", at)).map((r) => r.initiator);
const setDial = (name, value) => L.run(`INSERT INTO factory_dials (name, value, set_by, set_at) VALUES (?, ?, 'check', ?)
  ON CONFLICT (name) DO UPDATE SET value = excluded.value`, name, value, at);
check("with the share dial off, nobody is passed over", (await eligible()).includes("user:heavy"));
await setDial("initiator_share_pct", 50);
await setDial("initiator_floor_rows", 1);
let open = await eligible();
check("an initiator holding 6 of 8 live leases, past a 50% share, is passed over", !open.includes("user:heavy"), JSON.stringify(open));
check("…and the system's rows are still handed out", open.includes("system"));
await setDial("initiator_floor_rows", 6);
check("nobody is passed over while holding no more than the floor", (await eligible()).includes("user:heavy"));
await setDial("initiator_floor_rows", 1);
for (const id of userIds.slice(0, 4)) await lease(id, iso(NOW - 1));
check("a lease run out no longer counts against the share", (await eligible()).includes("user:heavy"));
check("the SQL's unset floor is worker/dials.js's default",
  /COALESCE\(\(SELECT value FROM factory_dials WHERE name = 'initiator_floor_rows'\), (\d+)\)/.exec(
    (await L.get("SELECT prosrc FROM pg_proc WHERE proname = 'factory_capped'")).prosrc)?.[1] === String(DIAL_DEFAULTS.initiator_floor_rows)
  && DIAL_DEFAULTS.initiator_share_pct === 0);

// ---- erased ----
await f.demandAll(env, [ask("b1", "bot:cccc")], "e1", NOW);
await L.run("UPDATE demands SET state = 'served' WHERE ref IN ('u0', 'b1')");
await L.run("UPDATE questions SET state = 'fact' WHERE id IN (SELECT question FROM demands WHERE ref IN ('u0', 'b1'))");
await L.run("UPDATE demands SET state = 'withdrawn' WHERE ref = 's0'");
const n = (await L.get("SELECT factory_forget_codes() AS n")).n;
check("an ask that is over loses its user or bot code, demand and question alike", n === 4
  && (await L.all("SELECT initiator FROM demands WHERE ref IN ('u0', 'b1')")).every((r) => r.initiator === null), `erased ${n}`);
check("…a live one keeps it, and the system's is never a code to erase",
  (await L.get("SELECT initiator FROM demands WHERE ref = 'u1'")).initiator === "user:heavy"
  && (await L.get("SELECT initiator FROM demands WHERE ref = 's0'")).initiator === "system");

// ---- where it comes from ----
check("a player's arrival is whoever its row names, every other batch WFSim's — a backfill's too",
  initiatorOf("arrivals-2026-10-14", "user:q") === "user:q" && initiatorOf("arrivals-2026-10-14") === null
  && initiatorOf("backfill-2026-10-14", "user:q") === "system" && initiatorOf("refresh-2026-10-14") === "system");

// ---- a submission ----
const A = await testDb("accounts");
await A.run("INSERT INTO accounts (id, created_at, username) VALUES ('acc1', ?, 'p')", at);
await A.run("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, 'acc1', '2099-01-01T00:00:00.000Z')", await sha256("tok"));
const PAYLOAD = { benchmark: "standard_single_target_v1", weapon: "kuva_nukor", mode: "base",
  mods: ["hornet_strike", "barrel_diffusion", "primed_target_cracker", "primed_heated_charge", "convulsion", "pathogen_rounds",
    "galvanized_diffusion", "galvanized_shot"], evolutions: [], arcanes: ["secondary_deadhead"], valence: "magnetic" };
const submit = (cookie) => worker.fetch(new Request("https://wfsim.app/api/board/submit", { method: "POST",
  headers: { "content-type": "application/json", ...(cookie ? { cookie } : {}) }, body: JSON.stringify(PAYLOAD) }),
  { ...env, ACCOUNTS: A, ASSETS: { fetch: async () => new Response("site") } });
const okIn = (await submit("wfsim_session=tok")).ok, okOut = (await submit(null)).ok;
const inbox = (await L.all("SELECT initiator FROM inbox ORDER BY initiator")).map((r) => r.initiator);
check("a submission signed in keeps its account's code, one signed out `anonymous`, and nothing else about either",
  okIn && okOut && inbox.length === 2 && inbox[0] === "anonymous" && inbox[1] === await initiatorCode(env, "user", "acc1")
  && (await L.all("SELECT * FROM inbox")).every((r) => Object.keys(r).sort().join() === "at,id,initiator,record"), JSON.stringify(inbox));

// ---- an arrival asked ----
await L.run("INSERT INTO batches (id, at, why, total) VALUES ('arrivals-2026-10-14', 'a', 't', 1), ('backfill-2026-10-14', 'b', 't', 1)");
await L.run("INSERT INTO builds (id, at, record) VALUES ('bx', '2026-10-14', ?), ('by', '2026-10-14', ?)",
  JSON.stringify({ weapon: "braton_prime", mods: ["serration"] }), JSON.stringify({ weapon: "braton_prime", mods: ["split_chamber"] }));
await L.run(`INSERT INTO queue (batch, build_id, ruler, mode, initiator) VALUES ('arrivals-2026-10-14', 'bx', 'standard_single_target', 'base', 'user:named'),
  ('backfill-2026-10-14', 'by', 'standard_single_target', 'base', NULL)`);
const askRows = (batch, identity, extra = {}) => boardTasksRoute(new Request("https://x/api/board/ask", { method: "POST",
  headers: { authorization: "Bearer ft" }, body: JSON.stringify({ batch, rows: [{ identity, ruler: "standard_single_target", mode: "base", ...extra }] }) }),
  { ...env, FACTORY_TOKEN: "ft" }, "/api/board/ask");
await askRows("arrivals-2026-10-14", "bx", { initiator: "user:forged" });
await askRows("backfill-2026-10-14", "by");
const who = Object.fromEntries((await L.all("SELECT payload::jsonb ->> 'identity' AS id, initiator FROM demands WHERE payload::jsonb ->> 'identity' IN ('bx', 'by')")).map((r) => [r.id, r.initiator]));
check("an arrival is asked as its queue row names, never as the runner says; a backfill is the system's",
  who.bx === "user:named" && who.by === "system", JSON.stringify(who));
// THE FIGHT AN ARRIVAL NAMED is waited on; the build's other rows, and a backfill's, are not.
await L.run("INSERT INTO builds (id, at, record) VALUES ('bn', '2026-10-14', ?), ('bo', '2026-10-14', ?), ('bb', '2026-10-14', ?)",
  JSON.stringify({ weapon: "braton_prime", mods: ["hornet_strike"] }), JSON.stringify({ weapon: "braton_prime", mods: ["vital_sense"] }),
  JSON.stringify({ weapon: "braton_prime", mods: ["point_strike"] }));
await L.run(`INSERT INTO queue (batch, build_id, ruler, mode, initiator) VALUES ('arrivals-2026-10-14', 'bn', 'standard_single_target', 'base', 'anonymous'),
  ('arrivals-2026-10-14', 'bo', 'standard_single_target', 'base', 'anonymous'), ('arrivals-2026-10-14', 'bs', 'standard_single_target', 'base', 'system')`);
await L.run("INSERT INTO builds (id, at, record) VALUES ('bs', '2026-10-14', ?)", JSON.stringify({ weapon: "braton_prime", mods: ["heavy_caliber"] }));
await askRows("arrivals-2026-10-14", "bn", { named: true });
await askRows("arrivals-2026-10-14", "bo");
await askRows("backfill-2026-10-14", "bb", { named: true });
await askRows("arrivals-2026-10-14", "bs", { named: true });
const waits = Object.fromEntries((await L.all(`SELECT d.payload::jsonb ->> 'identity' AS id, q.waits FROM demands d JOIN questions q ON q.id = d.question
    WHERE d.payload::jsonb ->> 'identity' IN ('bn', 'bo', 'bb', 'bs')`)).map((r) => [r.id, Number(r.waits)]));
check("the fight a player's arrival named is asked as waited on; the build's other rows, a backfill's and a survey's are not",
  waits.bn === 1 && waits.bo === 0 && waits.bb === 0 && waits.bs === 0, JSON.stringify(waits));
const c1 = await initiatorCode(env, "bot", "qq:1"), c2 = await initiatorCode(env, "bot", "qq:1"), c3 = await initiatorCode({ AUTH_SECRET: "t" }, "bot", "qq:1");
check("a code is its kind and a keyed hash: the same asker the same code, another key another", /^bot:[\w-]{16}$/.test(c1) && c1 === c2 && c1 !== c3, c1);


// ---- every intake reads it ----
// A PIPELINE THAT READS THE INBOX WITHOUT IT hands intake a record with no
// initiator, and every row it ships is unrecorded.
const readers = ["scripts/fetch_inbox.sh", "scripts/live_board.sh"].map((p) => [p, fs.readFileSync(new URL(`../${p}`, import.meta.url), "utf8")]);
const blind = readers.flatMap(([p, src]) => [...src.matchAll(/SELECT ([^"]*?) FROM inbox/g)].filter((m) => !/\binitiator\b/.test(m[1])).map((m) => `${p}: ${m[0]}`));
check("every intake's read of the inbox takes who asked", readers.every(([, src]) => /FROM inbox/.test(src)) && !blind.length, blind.join("; "));

if (failures) { console.log(`\ncheck_initiator: ${failures} failure(s)`); process.exit(1); }
console.log("\ncheck_initiator: ok");
