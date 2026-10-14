// THE FACTORY'S DIALS (worker/dials.js): a value outside its bounds is never
// set and never used, a dial never set is its default, a change is in force
// within DIALS_FRESH_MS without a deploy, every change is logged, and a client
// is told the dials it reads with every hand-out.
//   node scripts/check_dials.mjs
import { dialsOf, setDial, forgetDials, DIALS, DIAL_DEFAULTS, DIALS_FRESH_MS, CLIENT_DIALS } from "../worker/dials.js";
import { verifyRoute, PROTOCOL } from "../worker/verify.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const L = await testDb("library"), A = await testDb("accounts");
const env = { LIBRARY: L, ACCOUNTS: A, ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) } };
const T0 = Date.parse("2026-10-14T00:00:00.000Z");

console.log("a dial never set is its default");
forgetDials();
const d0 = await dialsOf(env, T0);
check("every dial reads its default from an empty table", Object.keys(DIALS).every((k) => d0[k] === DIAL_DEFAULTS[k]), JSON.stringify(d0));
check("every default is inside its own bounds", Object.entries(DIALS).every(([, x]) => x.default >= x.min && x.default <= x.max));
check("CLIENTS_PER_FACT is not a dial", !("clients_per_fact" in DIALS));
forgetDials();
check("no table at all is the defaults too", (await dialsOf({ LIBRARY: { prepare: () => ({ all: async () => { throw new Error("no table"); } }) } }, T0)).pack_ms === DIAL_DEFAULTS.pack_ms);

console.log("\na value outside its bounds is refused, and nothing is written");
for (const [name, value] of [["pack_ms", DIALS.pack_ms.max + 1], ["packs_held", DIALS.packs_held.min - 1], ["pack_max", 2.5], ["nope", 1]]) {
  const r = await setDial(env, name, value, "check", T0);
  check(`${name} = ${value} is refused`, r.ok === false, JSON.stringify(r));
}
check("...and no row or log line was written", !(await L.get("SELECT 1 FROM factory_dials")) && !(await L.get("SELECT 1 FROM factory_dial_log")));
await L.run("INSERT INTO factory_dials (name, value, set_by, set_at) VALUES ('pack_max', 100000, 'hand', 'x')");
forgetDials();
check("a row put out of bounds by hand is not used: the default is", (await dialsOf(env, T0)).pack_max === DIAL_DEFAULTS.pack_max);
await L.run("DELETE FROM factory_dials");

console.log("\na change is in force within DIALS_FRESH_MS, and logged");
forgetDials();
await dialsOf(env, T0);
await L.run("INSERT INTO factory_dials (name, value, set_by, set_at) VALUES ('pack_ms', 600000, 'hand', 'x')");
check("an isolate keeps what it read until DIALS_FRESH_MS has passed", (await dialsOf(env, T0 + DIALS_FRESH_MS - 1)).pack_ms === DIAL_DEFAULTS.pack_ms);
check("...and reads the table after it", (await dialsOf(env, T0 + DIALS_FRESH_MS)).pack_ms === 600_000);
await L.run("DELETE FROM factory_dials");
forgetDials();
const set1 = await setDial(env, "pack_ms", 120_000, "owner", T0);
const set2 = await setDial(env, "pack_ms", 300_000, "owner", T0 + 1000);
check("a set in bounds is taken", set1.ok && set2.ok);
check("...and in force at once in the isolate that set it", (await dialsOf(env, T0 + 1000)).pack_ms === 300_000);
const log = await L.all("SELECT value_before, value_after, set_by FROM factory_dial_log WHERE name = 'pack_ms' ORDER BY id");
check("every change is logged with the value before and after, and who set it",
  log.length === 2 && log[0].value_before === null && log[0].value_after === 120_000 && log[1].value_before === 120_000
  && log[1].value_after === 300_000 && log.every((x) => x.set_by === "owner"), JSON.stringify(log));

console.log("\na client is told the dials it reads");
await setDial(env, "packs_held", 3, "owner", Date.now());
const r = await (await verifyRoute(new Request("https://x/api/board/work", { method: "POST",
  body: JSON.stringify({ verifier: "a".repeat(24), engine: "e1", protocol: PROTOCOL, consent: { v: 1, at: "2026-10-08T08:00:00.000Z" }, lanes: 1 }) }), env, "/api/board/work")).json();
check("a hand-out carries every client dial, as in force", r.dials && CLIENT_DIALS.every((k) => k in r.dials) && r.dials.packs_held === 3 && r.dials.pack_ms === 300_000,
  JSON.stringify(r));
check("...and nothing a client does not read", r.dials && Object.keys(r.dials).length === CLIENT_DIALS.length);

console.log(failures ? `\n${failures} failed` : "\nthe dials are set within bounds, logged, and told");
process.exit(failures ? 1 : 0);
