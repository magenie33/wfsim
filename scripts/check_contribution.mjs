// THE CONTRIBUTION RANKING (worker/contribution.js), with no network — docs/BOARD.md
// §"Contribution". A signed-in browser claims its device and the first claim owns
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
import { contributionRoute, contributorRank } from "../worker/contribution.js";
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
  library.raw.prepare("INSERT INTO verifier_hours (verifier, hour, work) VALUES (?, ?, ?)").run(id, daysAgo(ago) + "T00", work);
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
  a.named === false && a.decided === false && JSON.stringify(await ranking()) === JSON.stringify([{ name: null, points: 8, recent: 5, week: 2,
    contributor_rank: { rank: 0, xp: 0, xp_at_rank: 0, xp_at_next: 2500 } }]),
  JSON.stringify(await ranking()));

check("a device claimed takes what it earned unclaimed, and the browser's own count is empty after",
  (await points(Y)).points === 0 && (await points(Y)).claimed === true);
const taken = await claim(bob, Y);
const annAfter = await mine(ann), bobAfter = await mine(bob);
check("signed in on someone's device, another account takes nothing it earned: the points stay where they were earned",
  taken.ok && annAfter.points === 8 && bobAfter.points === 0 && bobAfter.devices.length === 1
  && annAfter.devices.length === 1 && annAfter.removed.count === 1 && annAfter.removed.points === 3,
  JSON.stringify({ ann: annAfter.points, bob: bobAfter.points, removed: annAfter.removed }));
const nextHour = new Date(Date.now() + 3_600_000).toISOString().slice(0, 13);
library.raw.prepare("INSERT INTO verifier_hours (verifier, hour, work) VALUES (?, ?, ?)").run(Y, nextHour, 2 * POINT);
library.raw.prepare("UPDATE verifiers SET work = work + ? WHERE id = ?").run(2 * POINT, Y);
check("...and what the device earns from the next hour is the new account's",
  (await mine(bob)).points === 2 && (await mine(ann)).points === 8, `${(await mine(bob)).points} ${(await mine(ann)).points}`);
library.raw.prepare("DELETE FROM verifier_hours WHERE verifier = ? AND hour = ?").run(Y, nextHour);
library.raw.prepare("UPDATE verifiers SET work = work - ? WHERE id = ?").run(2 * POINT, Y);
// FROM HERE Y IS BOB'S OWN BROWSER, as though bob had claimed it first.
accounts.raw.prepare("DELETE FROM device_spans WHERE verifier = ?").run(Y);
accounts.raw.prepare("INSERT INTO device_spans (verifier, account, from_hour) VALUES (?, 'acct-bob', '')").run(Y);

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
accounts.raw.prepare("UPDATE accounts SET display_name = 'wf.jiuuvd.cn' WHERE id = 'acct-ann'").run();
check("a display name set before the rule that refuses it is shown as the username, and kept",
  (await ranking()).some((e) => e.name === "ann") && !(await ranking()).some((e) => e.name === "wf.jiuuvd.cn")
  && accounts.raw.prepare("SELECT display_name FROM accounts WHERE id = 'acct-ann'").get().display_name === "wf.jiuuvd.cn",
  JSON.stringify(await ranking()));
accounts.raw.prepare("UPDATE accounts SET display_name = 'Ann' WHERE id = 'acct-ann'").run();
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

const who = (name) => call(`/api/contributors?name=${encodeURIComponent(name)}`);
const found = await who("bob");
check("a named person is found by the name the ranking shows, with their place on all three",
  JSON.stringify(found.person) === JSON.stringify({ name: "bob", points: 3, recent: 3, week: 0, ranks: { all: 3, recent: 1, week: null },
    contributor_rank: { rank: 0, xp: 0, xp_at_rank: 0, xp_at_next: 2500 } }),
  JSON.stringify(found));
// THE CONTRIBUTOR RANK: Mastery Rank's curve on all-time points, ten to one experience.
const crAt = (pts) => { const c = contributorRank(pts); return `${c.rank}:${c.xp}:${c.xp_at_rank}:${c.xp_at_next}`; };
check("a contributor rank is 2,500·n² experience to 30, ten points to one experience",
  crAt(24_999) === "0:2499:0:2500" && crAt(25_000) === "1:2500:2500:10000" && crAt(124_353) === "2:12435:10000:22500"
  && crAt(22_499_999) === "29:2249999:2102500:2250000" && crAt(22_500_000) === "30:2250000:2250000:2397500",
  [24_999, 25_000, 124_353, 22_499_999, 22_500_000].map(crAt).join(" "));
check("...and 147,500 a rank past 30, without end", crAt(23_975_000) === "31:2397500:2397500:2545000"
  && crAt(27_000_000) === "33:2700000:2692500:2840000" && contributorRank(1e9).rank === 692, [23_975_000, 27_000_000, 1e9].map(crAt).join(" "));
check("...carried on every ranking row and on the account's own answer",
  (await ranking()).every((e) => e.contributor_rank && e.contributor_rank.rank === 0) && (await mine(bob)).contributor_rank.xp_at_next === 2500);
check("...never an anonymous one, by name or by handle", (await who("cy")).person === null && (await who("Ann")).person === null);
library.raw.prepare("UPDATE verifiers SET last_at = ? WHERE id = ?").run(new Date().toISOString(), X);
library.raw.prepare("UPDATE verifiers SET last_at = '2020-01-01T00:00:00.000Z' WHERE id = ?").run(Y);
// THE DEMAND: owed rows by why — a new build's, a sweep's, a rescore's, one row
// owed by two batches counted once — riven gains waiting, and the last hour's
// scores by who measured them.
{
  const L = library.raw, now = Date.now(), ago = (ms) => new Date(now - ms).toISOString();
  L.prepare("INSERT INTO batches (id, at, why, total) VALUES ('arrivals-2026-03-01', 'x', 'x', 3), ('rescore-x', 'x', 'x', 2)").run();
  for (const [b, id] of [["arrivals-2026-03-01", "n1"], ["arrivals-2026-03-01", "s1"], ["arrivals-2026-03-01", "r1"], ["rescore-x", "r1"], ["rescore-x", "r2"]]) {
    L.prepare("INSERT INTO queue (batch, build_id, ruler, mode) VALUES (?, ?, 'heavy_gunner', 'base')").run(b, id);
  }
  L.prepare("INSERT INTO orders (identity, ruler, mode, record, state, slot, at, priority) VALUES ('n1', 'heavy_gunner', 'base', '{}', 'todo', 1, '1970-01-01T00:00:00.000Z', 0)").run();
  L.prepare(`INSERT INTO scores (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at) VALUES
    ('a', 'r', 'base', 'verified:e1', 1, 'kpm', 0, 'T', ?), ('b', 'r', 'base', 'abc123', 1, 'kpm', 0, 'T', ?),
    ('c', 'r', 'base', 'abc123', 1, 'kpm', 0, 'T', ?), ('d', 'r', 'base', 'verified:e1', 1, 'kpm', 0, 'T', ?)`)
    .run(ago(60_000), ago(120_000), ago(180_000), ago(7_200_000));
  L.prepare(`INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at, request) VALUES
    ('A1', 'qq', 'c', 'a', 'r', 'torid', 'x', '{}', ?, '{}'), ('A2', 'qq', 'c', 'a', 'r', 'torid', 'x', '{}', ?, '{}')`).run(ago(60_000), ago(2 * 86_400_000));
  L.prepare(`INSERT INTO appraisals (code, channel, chat, asker, room, weapon, ruler, riven, at, request, started_at, agreed_at) VALUES
    ('S1', 'survey', '{}', 'wfsim', '', 'furis', 'x', '{}', ?, '{}', ?, ?), ('S2', 'survey', '{}', 'wfsim', '', 'furis', 'x', '{}', ?, '{}', ?, NULL),
    ('S3', 'survey', '{}', 'wfsim', '', 'furis', 'x', '{}', ?, '{}', NULL, NULL), ('S4', 'survey', '{}', 'wfsim', '', 'furis', 'x', '{}', ?, '{}', NULL, NULL)`)
    .run(ago(60_000), ago(0), ago(0), ago(60_000), ago(0), ago(60_000), ago(8 * 86_400_000));
  const d = await call("/api/board/demand");
  check("each survey is a goal: its shapes, how many are agreed and begun, a lapsed one gone, none counted as a chat's riven gain",
    JSON.stringify(d.surveys) === JSON.stringify([{ weapon: "furis", ruler: "x", shapes: 3, agreed: 1, started: 2 }]), JSON.stringify(d.surveys));
  check("the demand counts owed rows by why, one row once, the last hour's scores by who, and riven gains of the last day",
    JSON.stringify([d.owed, d.per_hour, d.riven_gains]) === JSON.stringify([{ new_builds: 1, rescores: 2, sweeps: 1 }, { volunteers: 1, official: 2 }, 1]),
    JSON.stringify(d));
}
check("the nav asks the same count alone", (await call("/api/board/computing")).computing === 1);
check("both say how many browsers are computing now, an old answer not counted",
  found.computing === 0 && (await who("bob")).computing === 1 && (await call("/api/contributors")).computing === 1);

const N = "n".repeat(24);
device(N, 6 * POINT); credit(N, 0, 6 * POINT);
check("a browser nobody claimed asks what it earned by its own id: its whole notebook, unclaimed",
  JSON.stringify(await points(N)) === JSON.stringify({ status: 200, ok: true, points: 6, recent: 6, week: 6, claimed: false }),
  JSON.stringify(await points(N)));
check("...and a claimed one holds nothing of its own, every point its account's",
  JSON.stringify(await points(X)) === JSON.stringify({ status: 200, ok: true, points: 0, recent: 0, week: 0, claimed: true }),
  JSON.stringify(await points(X)));
check("...an id nobody claimed or credited earns nothing", (await points("q".repeat(24))).points === 0
  && (await points("q".repeat(24))).claimed === false);
check("...a refused one, nothing either", (await points(W)).points === 0 && (await points(W)).recent === 0);
library.raw.prepare("INSERT INTO verifier_hours (verifier, hour, work, tasks, ms) VALUES (?, '2999-01-01T05', ?, 4, 6000)").run(X, 3 * POINT);
const day = await call("/api/board/points", { body: { verifier: X, since: "2999-01-01T00" } });
check("a browser asks its own today from the hour its midnight falls in: results, time and points credited",
  JSON.stringify(day.today) === JSON.stringify({ tasks: 4, ms: 6000, points: 3 }), JSON.stringify(day));
check("...and a malformed id is refused", (await points("nope")).reason === "bad_device");

accounts.raw.prepare("DELETE FROM accounts WHERE id = 'acct-cy'").run();
check("deleting an account releases its devices, its place and its answer",
  !accounts.raw.prepare("SELECT 1 FROM devices WHERE account = 'acct-cy'").get()
  && !accounts.raw.prepare("SELECT 1 FROM contribution_choice WHERE account = 'acct-cy'").get()
  && !(await ranking()).some((e) => e.points === 9));

// AN OWNER'S DEVICES, each with its name, what it last did and what it holds now.
const ed = await person("acct-ed", "ed"), fay = await person("acct-fay", "fay");
const P = "p".repeat(24), Q = "q2".repeat(12);
device(P, 4 * POINT); device(Q, 1 * POINT); credit(P, 60, 4 * POINT); credit(Q, 60, 1 * POINT);
await claim(ed, P, "Mac · Chrome"); await claim(ed, Q, "\u0007bell");
library.raw.prepare("UPDATE verifiers SET last_at = '2026-10-08T06:00:00.000Z' WHERE id = ?").run(P);
library.raw.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, lease, lease_until, leased_to, at)
  VALUES ('b1', 'standard_single_target', 'base', '{"weapon":"torid","mods":["x"]}', 'todo', 1, 'l', ?, ?, '1970-01-01T00:00:00.000Z')`).run(new Date(Date.now() + 60000).toISOString(), P);
library.raw.prepare(`INSERT INTO orders (identity, ruler, mode, record, state, slot, lease, lease_until, leased_to, at)
  VALUES ('b2', 'standard_single_target', 'base', '{"weapon":"furis"}', 'todo', 2, 'm', ?, ?, '1970-01-01T00:00:00.000Z')`).run(new Date(Date.now() - 60000).toISOString(), Q);
const eds = (await mine(ed)).devices;
const p6 = P.slice(0, 6), q6 = Q.slice(0, 6);
const dp = eds.find((d) => d.id === p6), dq = eds.find((d) => d.id === q6);
check("an owner sees each device by the name it was claimed with, a name that is not one left empty",
  dp.label === "Mac · Chrome" && dq.label === null, JSON.stringify(eds));
check("...when each last answered, and the task each holds a live lease on, by kind and its public facts alone",
  dp.last_at === "2026-10-08T06:00:00.000Z" && JSON.stringify(dp.now) === JSON.stringify({ kind: "board", weapon: "torid", ruler: "standard_single_target", mode: "base" })
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

const before = await mine(ed);
const gone = (await remove(ed, q6)).ok && await mine(ed);
check("removed, a device leaves the list and its points stay the account's, said as removed",
  gone && gone.devices.length === 1 && gone.points === before.points && gone.removed.count === 1 && gone.removed.points === 1,
  JSON.stringify(gone && { devices: gone.devices.length, points: gone.points, before: before.points, removed: gone.removed }));
check("...and still on the ranking", (await ranking(ed)).find((e) => e.you).points === before.points);
check("...and signed in there again it is claimed again, its points not counted twice",
  (await claim(ed, Q)).ok && (await mine(ed)).devices.length === 2 && (await mine(ed)).points === before.points,
  JSON.stringify(await mine(ed)));
const leaks = JSON.stringify([await mine(ed), await ranking(ed), await call("/api/contributors?name=ed")]);
check("no answer carries a device's full id, the one thing a claim needs", ![P, Q, X, Y, Z].some((v) => leaks.includes(v)));

console.log(failures ? `\n${failures} failed` : "\nan account's points are the work its devices were credited");
process.exitCode = failures ? 1 : 0;
