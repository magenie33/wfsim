// WHO ASKED (docs/BOARD.md §"Who asked"): a demand carries its initiator and a
// question its earliest-due live demand's; an initiator past its share of the
// live leases is passed over (dial `initiator_share_pct`, worker/pg/sync.sql
// `factory_capped`) and nobody else is; a code is erased once its ask is over
// (`factory_forget_codes`); a board row's initiator follows its batch, and a
// code is a keyed hash, the same for the same asker.
//   node scripts/check_initiator.mjs
import { facts } from "../worker/factory.js";
import { initiatorOf } from "../worker/board_tasks.js";
import { initiatorCode } from "../worker/accounts.js";
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
check("a player's arrival is not recorded, every other batch is WFSim's",
  initiatorOf("arrivals-2026-10-14") === null && initiatorOf("backfill-2026-10-14") === "system" && initiatorOf("refresh-2026-10-14") === "system");
const c1 = await initiatorCode(env, "bot", "qq:1"), c2 = await initiatorCode(env, "bot", "qq:1"), c3 = await initiatorCode({ AUTH_SECRET: "t" }, "bot", "qq:1");
check("a code is its kind and a keyed hash: the same asker the same code, another key another", /^bot:[\w-]{16}$/.test(c1) && c1 === c2 && c1 !== c3, c1);

if (failures) { console.log(`\ncheck_initiator: ${failures} failure(s)`); process.exit(1); }
console.log("\ncheck_initiator: ok");
