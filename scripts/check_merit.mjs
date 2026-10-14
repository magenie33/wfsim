// MERIT (docs/BOARD.md §"Merit", worker/merit.js): an account first seen is
// granted all its points up to its balance cap; after that a mint adds the
// points earned since, within the day's cap and the balance cap, and a point
// past either never mints later; the day's cap resets at UTC midnight; the caps
// grow with the contribution rank; the toggle made before the first mint still
// leaves it granted; the public totals are sums only; a change is refused from
// another site and from nobody signed in.
//   node scripts/check_merit.mjs
import { mintMerit, meritCaps, meritRoute } from "../worker/merit.js";
import { DIAL_DEFAULTS } from "../worker/dials.js";
import { sha256 } from "../worker/accounts.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const D = DIAL_DEFAULTS;
check("rank 0 mints at most 16,000 a day and holds 160,000; each rank adds 500 and 5,000",
  meritCaps(0, D).daily === 16_000 && meritCaps(0, D).cap === 160_000
  && meritCaps(10_000_000, D).daily === 16_000 + 500 * meritCaps(10_000_000, D).rank
  && meritCaps(10_000_000, D).cap === 10 * meritCaps(10_000_000, D).daily && meritCaps(10_000_000, D).rank > 0);

const L = await testDb("library"), A = await testDb("accounts"), env = { LIBRARY: L, ACCOUNTS: A };
const DAY0 = Date.parse("2026-10-14T20:00:00.000Z"), H = 3_600_000;
const setPoints = async (v, p) => L.run(`INSERT INTO verifiers (id, seen, work) VALUES (?, 'x', ?)
  ON CONFLICT (id) DO UPDATE SET work = excluded.work`, v, p * 1e9);
for (const [acc, v, p] of [["small", "vs", 5_000], ["big", "vb", 1_000_000], ["early", "ve", 3_000]]) {
  await A.run("INSERT INTO accounts (id, created_at, username) VALUES (?, '2026-10-01', ?)", acc, acc);
  await A.run("INSERT INTO device_spans (verifier, account, from_hour, until_hour) VALUES (?, ?, '', NULL)", v, acc);
  await setPoints(v, p);
}
// EARLY TURNED AUTO-PRIORITY ON BEFORE ANY MINT, which made its row.
await A.run("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, 'early', '2099-01-01')", await sha256("t-early"));
const call = (method, path, body, headers = {}) => meritRoute(new Request(`https://wfsim.app${path}`, { method,
  headers: { cookie: "wfsim_session=t-early", "content-type": "application/json", ...headers }, ...(body ? { body: JSON.stringify(body) } : {}) }), env, path).then(async (r) => ({ status: r.status, ...(await r.json()) }));
check("the toggle is taken from the site", (await call("POST", "/api/account/merit", { auto_priority: true })).status === 200);

const bal = async (acc) => Number((await A.get("SELECT balance FROM merit WHERE account = ?", acc))?.balance ?? -1);
await mintMerit(env, DAY0);
const capBig = meritCaps(1_000_000, D).cap;
check("first seen, an account is granted all its points up to its balance cap",
  (await bal("small")) === 5_000 && (await bal("big")) === capBig, JSON.stringify({ small: await bal("small"), big: await bal("big"), capBig }));
check("…a row the toggle made first is granted all the same", (await bal("early")) === 3_000);
check("…and the grant is in its ledger", (await A.all("SELECT kind, amount FROM merit_ledger WHERE account = 'small'")).map((r) => `${r.kind}:${r.amount}`).join() === "grant:5000");

await mintMerit(env, DAY0 + 10 * 60_000);
check("a mint with no new points adds nothing", (await bal("small")) === 5_000
  && (await A.all("SELECT id FROM merit_ledger WHERE account = 'small'")).length === 1);

// SMALL EARNS 20,000 MORE IN ONE DAY: its day's cap mints, the rest never will.
await setPoints("vs", 25_000);
await mintMerit(env, DAY0 + H);
const day1 = meritCaps(25_000, D).daily;
check("a day's mint stops at the day's cap, the cap of the rank the points reach", (await bal("small")) === 5_000 + day1, `${await bal("small")} vs ${5_000 + day1}`);
await setPoints("vs", 26_000);
await mintMerit(env, DAY0 + 2 * H);
check("…a point earned past it mints nothing the same day", (await bal("small")) === 5_000 + day1);
await setPoints("vs", 27_000);
await mintMerit(env, DAY0 + 6 * H);
check("…and the next UTC day mints only what was earned since, not what the cap held back",
  (await bal("small")) === 5_000 + day1 + 1_000, String(await bal("small")));
check("a full balance mints nothing", (await setPoints("vb", 1_100_000), await mintMerit(env, DAY0 + 7 * H), await bal("big")) === capBig);

const mine = await call("GET", "/api/account/merit");
check("an account sees its own balance, caps, toggle and ledger", mine.ok && mine.balance === 3_000 && mine.auto_priority === true
  && mine.daily === 16_000 && mine.cap === 160_000 && mine.ledger.length === 1, JSON.stringify(mine));
const pub = await call("GET", "/api/merit");
check("the public sees sums only", pub.ok && pub.granted === 5_000 + capBig + 3_000 && pub.minted === day1 + 1_000 && pub.accounts === 3
  && !JSON.stringify(pub).includes("small"), JSON.stringify(pub));
check("a change from another site is refused", (await call("POST", "/api/account/merit", { auto_priority: false }, { origin: "https://evil.example" })).status === 403);
check("…and from nobody signed in", (await meritRoute(new Request("https://wfsim.app/api/account/merit"), env, "/api/account/merit")).status === 401);

if (failures) { console.log(`\ncheck_merit: ${failures} failure(s)`); process.exit(1); }
console.log("\ncheck_merit: ok");
