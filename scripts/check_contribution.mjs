// THE CONTRIBUTION RANKING (worker/contribution.js), with no network — docs/BOARD.md
// §"Contribution". A signed-in browser claims its device and the last claim owns
// it; an account's points are its devices' credited work, a refused device's
// counting for nothing; the ranking lists every account with a claimed device,
// most first, by all their points, the last thirty days' or the last seven,
// and each account learns its place on all three; ANONYMOUS until
// the account agrees to show its name; a browser can
// ask what it earned by its own id; nothing works signed out or from another
// site; deleting the account releases its devices.
//   node scripts/check_contribution.mjs
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { contributionRoute } from "../worker/contribution.js";
import { sha256 } from "../worker/accounts.js";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const d1 = (file) => {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec(readFileSync(new URL(`../worker/${file}`, import.meta.url), "utf8"));
  const stmt = (sql, args = []) => ({
    bind: (...a) => stmt(sql, a),
    first: async () => db.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: db.prepare(sql).all(...args) }),
    run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
  });
  return { raw: db, prepare: (sql) => stmt(sql), batch: (stmts) => Promise.all(stmts.map((s) => s.all())) };
};
const accounts = d1("accounts.sql"), library = d1("schema.sql");
// THE PAID HALF, stubbed: it proves a mark for Ann and Cy, and records whom it was asked about.
const askedCloud = [];
const CLOUD = { fetch: async (req) => {
  const { accounts: ids } = await req.json();
  askedCloud.push(...ids);
  const held = { "acct-ann": { tier: "member", titles: [] }, "acct-cy": { tier: "patron", titles: [] } };
  return new Response(JSON.stringify({ ok: true, marks: Object.fromEntries(ids.filter((i) => held[i]).map((i) => [i, held[i]])) }));
} };
const env = { ACCOUNTS: accounts, LIBRARY: library, AUTH_SECRET: "x", CLOUD };

const POINT = 1e9;
const person = async (id, username, display = null) => {
  accounts.raw.prepare("INSERT INTO accounts (id, created_at, username, display_name) VALUES (?, '2026-01-01', ?, ?)")
    .run(id, username, display);
  accounts.raw.prepare("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?, ?, '2999-01-01')")
    .run(await sha256(`token-${id}`), id);
  return `wfsim_session=token-${id}`;
};
const device = (id, work, banned = 0) =>
  library.raw.prepare("INSERT INTO verifiers (id, seen, work, banned) VALUES (?, '2026-01-01', ?, ?)").run(id, work, banned);
const daysAgo = (n) => new Date(Date.now() - n * 86_400_000).toISOString().slice(0, 10);
const credit = (id, ago, work) =>
  library.raw.prepare("INSERT INTO verifier_days (verifier, day, work) VALUES (?, ?, ?)").run(id, daysAgo(ago), work);
const call = async (path, { cookie = "", body, method = body ? "POST" : "GET", origin } = {}) => {
  const headers = { cookie, ...(body ? { "content-type": "application/json" } : {}), ...(origin ? { origin } : {}) };
  const r = await contributionRoute(new Request(`https://wfsim.app${path}`,
    { method, headers, ...(body ? { body: JSON.stringify(body) } : {}) }), env, path.split("?")[0]);
  return r && { status: r.status, ...(await r.json()) };
};
const claim = (cookie, verifier, label) => call("/api/account/devices/claim", { cookie, body: { verifier, label } });
const relabel = (cookie, id, label) => call("/api/account/devices/label", { cookie, body: { id, label } });
const remove = (cookie, id) => call("/api/account/devices/remove", { cookie, body: { id } });
const mine = (cookie) => call("/api/account/devices", { cookie });
const choose = (cookie, named) => call("/api/account/contribution", { cookie, body: { named } });
const ranking = async (cookie = "", q = "") => (await call(`/api/contributors${q}`, { cookie })).contributors;

const ann = await person("acct-ann", "ann", "Ann"), bob = await person("acct-bob", "bob"), cy = await person("acct-cy", "cy");
const X = "x".repeat(24), Y = "y".repeat(24), Z = "z".repeat(24), W = "w".repeat(24);
device(X, 5 * POINT); device(Y, 3 * POINT); device(Z, 9 * POINT); device(W, 50 * POINT, 1);
// X earned 2 today and 3 forty days ago; Y all 3 a week ago; Z 9 long ago; W
// 50 yesterday, and is refused.
credit(X, 0, 2 * POINT); credit(X, 40, 3 * POINT); credit(Y, 7, 3 * POINT); credit(Z, 60, 9 * POINT); credit(W, 1, 50 * POINT);
const points = (verifier) => call("/api/board/points", { body: { verifier } });

check("a path that is not one is left to the next route", (await contributionRoute(
  new Request("https://wfsim.app/api/account"), env, "/api/account")) === null);
check("signed out, nothing is claimed", (await claim("", X)).status === 401);
check("a claim from another site is refused", (await call("/api/account/devices/claim",
  { cookie: ann, body: { verifier: X }, origin: "https://evil.example" })).status === 403);
check("a malformed device is refused", (await claim(ann, "nope")).reason === "bad_device");

await claim(ann, X, "Mac · Chrome"); await claim(ann, Y);
const a = await mine(ann);
check("an account's points are its devices' credited work", a.points === 8 && a.devices.length === 2, JSON.stringify(a));
check("...and its last thirty days, the days before them left out", a.recent === 5, JSON.stringify(a));
check("...and its page never sees a device's whole id", a.devices.every((d) => d.id.length === 6));
check("an account is on the ranking once it claims a device, ANONYMOUS and not yet asked",
  a.named === false && a.decided === false && JSON.stringify(await ranking()) === JSON.stringify([{ name: null, points: 8, recent: 5, week: 2 }]),
  JSON.stringify(await ranking()));

await claim(bob, Y);
check("the last claim owns a device, and its work goes with it", (await mine(ann)).points === 5 && (await mine(bob)).points === 3);

await claim(cy, Z); await claim(cy, W);
const r = await ranking();
check("the ranking is most first, a refused device counting for nothing",
  r.map((e) => e.points).join(" ") === "9 5 3", JSON.stringify(r));
check("...and an anonymous row carries no name, handle or mark, and the paid half is not asked about it",
  r.every((e) => e.name === null && !("username" in e) && !("mark" in e)) && askedCloud.length === 0,
  `${JSON.stringify(r)} ${askedCloud}`);
check("the reader's own row is marked to them, and to nobody else",
  JSON.stringify((await ranking(ann)).map((e) => !!e.you)) === "[false,true,false]" && !(await ranking()).some((e) => e.you));

await choose(ann, true); await choose(bob, true); await choose(cy, false);
const named = await ranking();
check("agreed, a row shows the display name alone, or the username when there is none",
  JSON.stringify(named.map((e) => e.name)) === JSON.stringify([null, "Ann", "bob"]) && named.every((e) => !("username" in e)),
  JSON.stringify(named));
check("...a named row carries the mark the paid half proves, and only named rows were asked about",
  JSON.stringify(named[1].mark) === JSON.stringify({ tier: "member", titles: [] }) && !("mark" in named[0]) && !("mark" in named[2])
  && !askedCloud.includes("acct-cy"), `${JSON.stringify(named)} ${askedCloud}`);
check("...and a no is kept, so it is not asked again", (await mine(cy)).decided === true && (await mine(cy)).named === false);
await choose(ann, false);
check("taken back, the name leaves the ranking", !(await ranking()).some((e) => e.name === "Ann") && (await mine(ann)).named === false);
check("an answer that is not one is refused", (await call("/api/account/contribution", { cookie: ann, body: { named: "yes" } })).reason === "bad_choice");

const recent = await ranking("", "?period=recent");
check("the last thirty days rank by those days, and an account with none there is not on it",
  recent.map((e) => `${e.name}:${e.recent}`).join(" ") === "bob:3 null:2", JSON.stringify(recent));
const week = await call("/api/contributors?period=week");
check("the last seven days rank by those days, a week ago already outside them",
  week.period === "week" && week.contributors.map((e) => `${e.name}:${e.week}`).join(" ") === "null:2", JSON.stringify(week));
check("an account learns its place on each ranking, and none where it has nothing",
  JSON.stringify((await mine(ann)).ranks) === JSON.stringify({ all: 2, recent: 2, week: 1 })
  && JSON.stringify((await mine(bob)).ranks) === JSON.stringify({ all: 3, recent: 1, week: null }),
  `${JSON.stringify((await mine(ann)).ranks)} ${JSON.stringify((await mine(bob)).ranks)}`);

check("a browser asks what it earned by its own id, and is told whether it is claimed",
  JSON.stringify(await points(X)) === JSON.stringify({ status: 200, ok: true, points: 5, recent: 2, week: 2, claimed: true }),
  JSON.stringify(await points(X)));
check("...an id nobody claimed or credited earns nothing", (await points("q".repeat(24))).points === 0
  && (await points("q".repeat(24))).claimed === false);
check("...a refused one, nothing either", (await points(W)).points === 0 && (await points(W)).recent === 0);
check("...and a malformed id is refused", (await points("nope")).reason === "bad_device");

accounts.raw.prepare("DELETE FROM accounts WHERE id = 'acct-cy'").run();
check("deleting an account releases its devices, its place and its answer",
  !accounts.raw.prepare("SELECT 1 FROM devices WHERE account = 'acct-cy'").get()
  && !accounts.raw.prepare("SELECT 1 FROM contribution_choice WHERE account = 'acct-cy'").get()
  && !(await ranking()).some((e) => e.points === 9));

// AN OWNER'S DEVICES, each with its name, what it last did and what it holds now.
const ed = await person("acct-ed", "ed"), fay = await person("acct-fay", "fay");
const P = "p".repeat(24), Q = "q2".repeat(12);
device(P, 4 * POINT); device(Q, 1 * POINT);
await claim(ed, P, "Mac · Chrome"); await claim(ed, Q, "\u0007bell");
library.raw.prepare("UPDATE verifiers SET last_at = '2026-10-08T06:00:00Z' WHERE id = ?").run(P);
library.raw.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, lease, lease_until, leased_to, at)
  VALUES ('b1', 'standard_single_target', 'base', '{"weapon":"torid","mods":["x"]}', 'todo', 1, 'l', ?, ?, 0)`).run(Date.now() + 60000, P);
library.raw.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, lease, lease_until, leased_to, at)
  VALUES ('b2', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 2, 'm', ?, ?, 0)`).run(Date.now() - 60000, Q);
const eds = (await mine(ed)).devices;
const p6 = P.slice(0, 6), q6 = Q.slice(0, 6);
const dp = eds.find((d) => d.id === p6), dq = eds.find((d) => d.id === q6);
check("an owner sees each device by the name it was claimed with, a name that is not one left empty",
  dp.label === "Mac · Chrome" && dq.label === null, JSON.stringify(eds));
check("...when each last answered, and the task each holds a live lease on, by kind and its public facts alone",
  dp.last_at === "2026-10-08T06:00:00Z" && JSON.stringify(dp.now) === JSON.stringify({ kind: "board", weapon: "torid", ruler: "standard_single_target", mode: "base" })
  && dq.now === null && dq.last_at === null, JSON.stringify(eds));
check("the owner renames a device", (await relabel(ed, p6, "书房 PC")).ok && (await mine(ed)).devices.find((d) => d.id === p6).label === "书房 PC");
check("...and a name that is not one is refused", (await relabel(ed, p6, "x".repeat(41))).reason === "bad_label"
  && (await relabel(ed, p6, "  ")).reason === "bad_label");
check("nobody renames or removes another's device", (await relabel(fay, p6, "mine")).status === 404 && (await remove(fay, p6)).status === 404
  && (await mine(ed)).devices.length === 2);
// THE HONOUR: a device that said yes and has been credited makes its account a volunteer.
check("no yes on record, no honour, however much was credited", (await mine(ed)).volunteer === null);
library.raw.prepare("UPDATE verifiers SET consent_v = 1, consent_at = '2026-10-08T08:00:00.000Z' WHERE id = ?").run(P);
library.raw.prepare("UPDATE verifiers SET consent_v = 1, consent_at = '2026-10-07T08:00:00.000Z' WHERE id = ?").run(Q);
library.raw.prepare("UPDATE verifiers SET work = 0 WHERE id = ?").run(Q);
check("a yes and work credited is the honour, since that yes — a yes with nothing computed is not",
  (await mine(ed)).volunteer === "2026-10-08T08:00:00.000Z", JSON.stringify((await mine(ed)).volunteer));
library.raw.prepare("UPDATE verifiers SET work = ? WHERE id = ?").run(1 * POINT, Q);
await choose(ed, true);
const withEd = await ranking();
check("...shown beside a name on the ranking", withEd.some((e) => e.name === "ed" && e.volunteer === true), JSON.stringify(withEd));
await choose(ed, false);
check("...and never on an anonymous row", (await ranking()).every((e) => e.name !== null || !("volunteer" in e)));

check("removed, a device and its work leave the account", (await remove(ed, q6)).ok
  && (await mine(ed)).devices.length === 1 && (await mine(ed)).points === 4);

console.log(failures ? `\n${failures} failed` : "\nan account's points are the work its devices were credited");
process.exitCode = failures ? 1 : 0;
