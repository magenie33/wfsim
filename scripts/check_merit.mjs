// MERIT (docs/BOARD.md §"Merit", worker/merit.js): an account first seen is
// granted all its points up to its balance cap; after that a mint adds the
// points earned since, within the day's cap and the balance cap, and a point
// past either never mints later; the day's cap resets at UTC midnight; the caps
// grow with the contribution rank; the toggle made before the first mint still
// leaves it granted; the public totals are sums only; a change is refused from
// another site and from nobody signed in.
//   node scripts/check_merit.mjs
import { mintMerit, meritCaps, meritRoute, settleMerit, holdPriority } from "../worker/merit.js";
import { DIAL_DEFAULTS, forgetDials } from "../worker/dials.js";
import { sha256, initiatorCode } from "../worker/accounts.js";
import { boardTasksRoute } from "../worker/board_tasks.js";
import { BOARD_KINDS } from "../worker/board_kinds.js";
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
const setPoints = async (v, p) => L.run(`INSERT INTO entity_compute_devices (id, seen, work) VALUES (?, 'x', ?)
  ON CONFLICT (id) DO UPDATE SET work = excluded.work`, v, p * 1e9);
for (const [acc, v, p] of [["small", "vs", 5_000], ["big", "vb", 1_000_000], ["early", "ve", 3_000]]) {
  await A.run("INSERT INTO entity_accounts (id, created_at, username) VALUES (?, '2026-10-01', ?)", acc, acc);
  await A.run("INSERT INTO record_compute_device_ownership_intervals (verifier, account, from_hour, until_hour) VALUES (?, ?, '', NULL)", v, acc);
  await setPoints(v, p);
}
// EARLY TURNED AUTO-PRIORITY ON BEFORE ANY MINT, which made its row.
await A.run("INSERT INTO state_login_sessions (token_hash, account, expires_at) VALUES (?, 'early', '2099-01-01')", await sha256("t-early"));
const call = (method, path, body, headers = {}) => meritRoute(new Request(`https://wfsim.app${path}`, { method,
  headers: { cookie: "wfsim_session=t-early", "content-type": "application/json", ...headers }, ...(body ? { body: JSON.stringify(body) } : {}) }), env, path).then(async (r) => ({ status: r.status, ...(await r.json()) }));
check("the toggle is taken from the site", (await call("POST", "/api/account/merit", { auto_priority: true })).status === 200);

const bal = async (acc) => Number((await A.get("SELECT balance FROM state_account_priority_credit WHERE account = ?", acc))?.balance ?? -1);
await mintMerit(env, DAY0);
const capBig = meritCaps(1_000_000, D).cap;
check("first seen, an account is granted all its points up to its balance cap",
  (await bal("small")) === 5_000 && (await bal("big")) === capBig, JSON.stringify({ small: await bal("small"), big: await bal("big"), capBig }));
check("…a row the toggle made first is granted all the same", (await bal("early")) === 3_000);
check("…and the grant is in its ledger", (await A.all("SELECT kind, amount FROM record_priority_credit_entries WHERE account = 'small'")).map((r) => `${r.kind}:${r.amount}`).join() === "grant:5000");

await mintMerit(env, DAY0 + 10 * 60_000);
check("a mint with no new points adds nothing", (await bal("small")) === 5_000
  && (await A.all("SELECT id FROM record_priority_credit_entries WHERE account = 'small'")).length === 1);

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

// ---- priority ----
{
  const L = await testDb("library"), A = await testDb("accounts");
  const env = { LIBRARY: L, ACCOUNTS: A, AUTH_SECRET: "s", FACTORY_TOKEN: "ft",
    ASSETS: { fetch: async () => new Response(JSON.stringify({ engine: "e1", release: "r1" })) } };
  const NOW = Date.now(), at = new Date(NOW).toISOString();
  const dial = (name, value) => L.run(`INSERT INTO state_factory_parameters (name, value, set_by, set_at) VALUES (?, ?, 'check', ?)
    ON CONFLICT (name) DO UPDATE SET value = excluded.value`, name, value, at);
  await dial("merit_priority", 1);
  forgetDials();
  await A.run("INSERT INTO entity_accounts (id, created_at, username) VALUES ('payer', '2026-10-01', 'payer')");
  await A.run("INSERT INTO state_login_sessions (token_hash, account, expires_at) VALUES (?, 'payer', '2099-01-01')", await sha256("tp"));
  const me = (method, body) => meritRoute(new Request("https://wfsim.app/api/account/merit", { method,
    headers: { cookie: "wfsim_session=tp", "content-type": "application/json" }, ...(body ? { body: JSON.stringify(body) } : {}) }), env, "/api/account/merit")
    .then((r) => r.json());
  await me("POST", { auto_priority: true });
  await A.run("UPDATE state_account_priority_credit SET balance = 1000, granted_at = ? WHERE account = 'payer'", at);
  const code = await initiatorCode(env, "user", "payer");
  check("turning priority on keeps whose code is whose, and the panel offers it", (await A.get("SELECT account FROM relation_account_priority_codes WHERE code = ?", code))?.account === "payer"
    && (await me("GET")).priority === true);
  const batch = "arrivals-2026-10-14";
  await L.run("INSERT INTO entity_compute_batches (id, at, why, total) VALUES (?, 'a', 't', 9)", batch);
  const rows = ["p1", "p2", "p3", "p4", "free"];
  for (const id of rows) {
    await L.run("INSERT INTO entity_weapon_builds (id, at, record) VALUES (?, ?, ?)", id, at.slice(0, 10), JSON.stringify({ weapon: "braton_prime", mods: [id] }));
    await L.run("INSERT INTO state_board_pending_work (batch, build_id, ruler, mode, initiator) VALUES (?, ?, 'standard_single_target', 'base', ?)", batch, id, id === "free" ? "anonymous" : code);
  }
  const ask = (ids) => boardTasksRoute(new Request("https://x/api/board/ask", { method: "POST", headers: { authorization: "Bearer ft" },
    body: JSON.stringify({ batch, rows: ids.map((identity) => ({ identity, ruler: "standard_single_target", mode: "base" })) }) }), env, "/api/board/ask");
  await dial("merit_rows_per_hour", 3); await dial("merit_share_pct", 100);
  forgetDials();
  await ask(rows);
  check("queue admission creates no priority holds or Merit charge", !(await L.get("SELECT 1 FROM state_compute_priority_holds"))
    && Number((await A.get("SELECT balance FROM state_account_priority_credit WHERE account='payer'")).balance) === 1000);
  const admitted = await L.all(`SELECT d.*,q.request FROM relation_task_consumers d JOIN state_compute_work q ON q.id=d.question ORDER BY d.ref`);
  const ordinaryDue = admitted.map(row => Date.parse(row.due_at) - Date.parse(row.asked_at));
  check("queue admission retains equal deadlines across account and anonymous sources", ordinaryDue.every(value => value === ordinaryDue[0]));
  // Retained priority holds still own their charge and refund decisions.
  const heldAt=Date.now();
  await holdPriority(env,batch,admitted.map(row=>({producer:row.producer,ref:row.ref,request:JSON.parse(row.request),
    due_in_ms:Date.parse(row.due_at)-heldAt,initiator:row.initiator,waits:Number(row.waits)>0})),"("+BOARD_KINDS.map(kind=>"'"+kind+"'").join(",")+")",heldAt);
  for (const hold of await L.all("SELECT producer,ref,boosted_due FROM state_compute_priority_holds")) {
    await L.run("UPDATE relation_task_consumers SET due_at=? WHERE producer=? AND ref=?",hold.boosted_due,hold.producer,hold.ref);
    await L.run("UPDATE state_compute_work SET due_at=? WHERE id=(SELECT question FROM relation_task_consumers WHERE producer=? AND ref=?)",hold.boosted_due,hold.producer,hold.ref);
  }
  const due = Object.fromEntries((await L.all("SELECT payload::jsonb ->> 'identity' AS id, due_at, asked_at FROM relation_task_consumers")).map((r) => [r.id, Date.parse(r.due_at) - Date.parse(r.asked_at)]));
  const holds = await L.all("SELECT ref, price, state, code FROM state_compute_priority_holds ORDER BY id");
  const bal = async () => Number((await A.get("SELECT balance FROM state_account_priority_credit WHERE account = 'payer'")).balance);
  check("retained holds keep their price, hourly cap and balance charge",
    holds.length === 3 && holds.every((h) => Number(h.price) === DIAL_DEFAULTS.merit_price_default) && (await bal()) === 1000 - 3 * DIAL_DEFAULTS.merit_price_default,
    JSON.stringify({ holds, bal: await bal() }));
  check("…each held row is due 30% sooner, an anonymous one and one past the cap as they would be",
    Math.abs(due.p1 / due.free - 0.7) < 0.01 && Math.abs(due.p4 / due.free - 1) < 0.01, JSON.stringify(due));
  // THE SHARE: nothing more is held once the holds are this much of the live rows.
  await dial("merit_rows_per_hour", 100); await dial("merit_share_pct", 1);
  forgetDials();
  await L.run("DELETE FROM relation_task_consumers WHERE payload::jsonb ->> 'identity' = 'p4'");
  await ask(["p4"]);
  check("another queue admission leaves retained holds unchanged", (await L.all("SELECT id FROM state_compute_priority_holds")).length === 3);
  await dial("merit_share_pct", 20);
  forgetDials();
  // SETTLED: p1 taken while an unpaid row was due between its two deadlines; p2 taken past nobody; p3's ask withdrawn.
  const qOf = async (id) => (await L.get("SELECT question FROM relation_task_consumers WHERE payload::jsonb ->> 'identity' = ?", id)).question;
  const [q1, q2, qf] = [await qOf("p1"), await qOf("p2"), await qOf("free")];
  const h1 = await L.get("SELECT boosted_due, natural_due FROM state_compute_priority_holds WHERE ref LIKE 'p1|%'");
  const between = new Date((Date.parse(h1.boosted_due) + Date.parse(h1.natural_due)) / 2).toISOString();
  await L.run("UPDATE state_compute_work SET due_at = ?, producer = (SELECT producer FROM state_compute_work WHERE id = ?) WHERE id = ?", between, q1, qf);
  await L.run("UPDATE state_compute_work SET leased_at = ? WHERE id IN (?, ?)", new Date(heldAt+1000).toISOString(), q1, q2);
  const h2 = await L.get("SELECT boosted_due, natural_due FROM state_compute_priority_holds WHERE ref LIKE 'p2|%'");
  if (Date.parse(between) > Date.parse(h2.boosted_due) && Date.parse(between) < Date.parse(h2.natural_due)) {
    await L.run("UPDATE state_compute_work SET producer = 'other' WHERE id = ?", q2);
  }
  await L.run("UPDATE relation_task_consumers SET state = 'withdrawn' WHERE payload::jsonb ->> 'identity' = 'p3'");
  await settleMerit(env, NOW + 60_000);
  const st = Object.fromEntries((await L.all("SELECT ref, state, code FROM state_compute_priority_holds")).map((h) => [h.ref.split("|")[0], h]));
  check("a row that went ahead of someone is charged; one that went ahead of nobody, or was withdrawn, is refunded",
    st.p1.state === "charged" && st.p2.state === "refunded" && st.p3.state === "refunded" && (await bal()) === 1000 - DIAL_DEFAULTS.merit_price_default,
    JSON.stringify({ st, bal: await bal() }));
  check("…and a settled hold keeps no code", Object.values(st).every((h) => h.code === ""));
  await L.run("UPDATE state_compute_priority_holds SET state = 'held', code = ? WHERE ref LIKE 'p2|%'", code);
  await settleMerit(env, NOW + 120_000);
  check("a refund is made once, however often the settling runs", (await bal()) === 1000 - DIAL_DEFAULTS.merit_price_default
    && (await A.all("SELECT id FROM record_priority_credit_entries WHERE kind = 'refund'")).length === 2);
  // A WAITED-ON ROW goes first already, and is never held.
  const before = (await L.all("SELECT id FROM state_compute_priority_holds")).length;
  const waited = await holdPriority(env, batch, [{ producer: "new_build", ref: "w|standard_single_target|base", verb: "simulate",
    request: { ruler: "standard_single_target", mode: "base" }, due_in_ms: 1000, initiator: code, waits: true }], "('new_build')", NOW);
  check("a row someone waits on is never held", (await L.all("SELECT id FROM state_compute_priority_holds")).length === before && waited[0].due_in_ms === 1000);
  await me("POST", { auto_priority: false });
  check("turning priority off deletes whose code is whose", !(await A.get("SELECT code FROM relation_account_priority_codes WHERE account = 'payer'")));
}

if (failures) { console.log(`\ncheck_merit: ${failures} failure(s)`); process.exit(1); }
console.log("\ncheck_merit: ok");
