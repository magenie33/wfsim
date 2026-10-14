// A DEVICE'S POINTS LEDGER (worker/contribution.js §"The ledger"), with no
// network — docs/BOARD.md §"Contribution". Every hour a device was credited is
// said with where it went: kept on the device, the reader's own account, or
// another account never named; a span its account ended is said as ended; the
// all-time split adds up; results that earned nothing are counted by why; an
// account sees what came in device by device, its own spans' hours alone; and no
// answer carries a full device id or another account's name.
//   node scripts/check_ledger.mjs
import { contributionRoute } from "../worker/contribution.js";
import { sha256 } from "../worker/accounts.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const accounts = await testDb("accounts"), library = await testDb("library");
const env = { ACCOUNTS: accounts, LIBRARY: library, AUTH_SECRET: "x" };
const POINT = 1e9;

const person = async (id, username, display) => {
  await accounts.run("INSERT INTO accounts (id, created_at, username, display_name) VALUES (?, '2026-01-01', ?, ?)", id, username, display);
  await accounts.run("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, ?, '2999-01-01')", await sha256(`token-${id}`), id);
  return `wfsim_session=token-${id}`;
};
const call = async (path, { cookie = "", body, method = body ? "POST" : "GET" } = {}) => {
  const headers = { cookie, ...(body ? { "content-type": "application/json", origin: "https://wfsim.app" } : {}) };
  const r = await contributionRoute(new Request(`https://wfsim.app${path}`,
    { method, headers, ...(body ? { body: JSON.stringify(body) } : {}) }), env, path.split("?")[0]);
  return r && { status: r.status, ...(await r.json()) };
};

const ann = await person("acct-ann", "annhandle", "Ann Example"), bob = await person("acct-bob", "bobhandle", "Bob Example");
const D = "d".repeat(24), E = "e".repeat(24);
for (const v of [D, E]) await library.run("INSERT INTO verifiers (id, seen, work) VALUES (?, '2026-01-01', 0)", v);
const hour = (v, h, pts, tasks) => library.run("INSERT INTO verifier_hours (verifier, hour, work, tasks) VALUES (?, ?, ?, ?)", v, h, pts * POINT, tasks);
// D: Ann held it from its first hour until the 5th, Bob from then on, and Bob lists it now.
await hour(D, "2026-10-01T10", 4, 2); await hour(D, "2026-10-04T10", 6, 3); await hour(D, "2026-10-06T10", 10, 5);
await accounts.run("INSERT INTO device_spans (verifier, account, from_hour, until_hour) VALUES (?, 'acct-ann', '', '2026-10-05T00')", D);
await accounts.run("INSERT INTO device_spans (verifier, account, from_hour, until_hour) VALUES (?, 'acct-bob', '2026-10-05T00', NULL)", D);
await accounts.run("INSERT INTO devices (verifier, account, claimed_at, label) VALUES (?, 'acct-bob', '2026-10-05', 'Bob PC')", D);
// E: nobody ever claimed it.
await hour(E, "2026-10-06T11", 7, 1);
// D's results that earned nothing: one against an agreed fact, one still open, one set aside — and three that do not count.
const q = (id, state) => library.run("INSERT INTO questions (id, verb, request, state, due_at, opened_at) VALUES (?, 'simulate', '{}', ?, '2026-12-01', '2026-10-01')", id, state);
await q("q-fact", "fact"); await q("q-open", "open"); await q("q-old", "fact");
await q("q-void", "fact"); await q("q-paid", "fact");
const answer = (question, at, { credited = 0, voided = 0, official = 0 } = {}) => library.run(
  `INSERT INTO answers (question, engine, device, owner, official, canon, result, work, at, credited, void)
   VALUES (?, 'e', ?, ?, ?, '{}', '{}', 1, ?, ?, ?)`, question, D, D, official, at, credited, voided);
await answer("q-fact", "2026-10-06T10:00:00.000Z"); await answer("q-open", "2026-10-06T10:01:00.000Z");
await answer("q-void", "2026-10-06T10:02:00.000Z", { voided: 1 }); await answer("q-paid", "2026-10-06T10:03:00.000Z", { credited: 1 });
await answer("q-fact", "2026-10-06T10:04:00.000Z", { official: 1 }); await answer("q-old", "2026-09-01T10:00:00.000Z");

const ledger = (cookie, verifier, from = "2026-10-01T00") => call("/api/board/ledger", { cookie, body: { verifier, from } });
const where = (l) => l.hours.map((h) => `${h.hour}:${h.holder}${h.closed ? "/closed" : ""}:${h.points}`).join(" ");

const asBob = await ledger(bob, D);
check("each hour is its span's holder's, the reader's own account said as theirs",
  where(asBob) === "2026-10-01T10:other/closed:4 2026-10-04T10:other/closed:6 2026-10-06T10:you:10", where(asBob));
const asAnn = await ledger(ann, D);
check("...and read by the other holder, the same hours the other way round, an ended span said as ended",
  where(asAnn) === "2026-10-01T10:you/closed:4 2026-10-04T10:you/closed:6 2026-10-06T10:other:10", where(asAnn));
check("...and signed out, every held hour is another account's",
  (await ledger("", D)).hours.every((h) => h.holder === "other"));
const free = await ledger("", E);
check("a device nobody claimed keeps every point", where(free) === "2026-10-06T11:device:7" && free.summary.device === 7, where(free));
check("the all-time split adds up, by holder", asBob.summary.points === 20 && asBob.summary.you === 10 && asBob.summary.other === 10
  && asBob.summary.device === 0 && asBob.summary.tasks === 10, JSON.stringify(asBob.summary));
const late = await ledger(bob, D, "2026-10-05T00");
check("the window lists its hours alone, says there are earlier ones, and keeps the all-time split",
  late.hours.length === 1 && late.earlier === true && late.summary.points === 20, JSON.stringify(late));
check("results that earned nothing are counted by why, in the window, a volunteer's alone",
  JSON.stringify(asBob.unpaid) === JSON.stringify({ disagreed: 1, waiting: 1, refused: 1 }), JSON.stringify(asBob.unpaid));
check("a malformed device is refused, and the ledger is asked by POST alone",
  (await ledger("", "nope")).reason === "bad_device" && (await call("/api/board/ledger")).status === 405);

const own = await call("/api/account/devices/ledger", { cookie: bob, body: { id: D.slice(0, 6), from: "2026-10-01T00" } });
check("an owner reads a listed device's ledger by its six characters", own.ok && where(own) === where(asBob), JSON.stringify(own));
check("...and nobody else can", (await call("/api/account/devices/ledger", { cookie: ann, body: { id: D.slice(0, 6) } })).status === 404
  && (await call("/api/account/devices/ledger", { body: { id: D.slice(0, 6) } })).status === 401);
const bobIn = await call("/api/account/ledger?from=2026-10-01T00", { cookie: bob });
check("an account sees what came in, device by device, its own spans' hours alone",
  bobIn.hours.map((h) => `${h.hour}:${h.id}:${h.points}`).join(" ") === `2026-10-06T10:${D.slice(0, 6)}:10`
  && bobIn.devices.length === 1 && bobIn.devices[0].label === "Bob PC" && !bobIn.devices[0].removed, JSON.stringify(bobIn));
const annIn = await call("/api/account/ledger?from=2026-10-01T00", { cookie: ann });
check("...a device another account took since still counted, as removed",
  annIn.hours.map((h) => h.points).join(",") === "4,6" && annIn.devices[0].removed === true && annIn.devices[0].label === null, JSON.stringify(annIn));
check("...and signed out, nothing", (await call("/api/account/ledger")).status === 401);

const leaks = JSON.stringify([asBob, asAnn, free, own, bobIn, annIn]);
check("no answer carries a full device id or an account's id or name",
  ![D, E, "acct-ann", "acct-bob", "Ann Example", "Bob Example", "annhandle", "bobhandle"].some((s) => leaks.includes(s)));

console.log(failures ? `\n${failures} failed` : "\na device's ledger says where every point went, and names nobody else");
process.exitCode = failures ? 1 : 0;
