// SPDX-License-Identifier: AGPL-3.0-or-later
// THE COMPUTE A READER'S MACHINES GIVE THE BOARD, counted under their name —
// docs/BOARD.md §"Contribution". A signed-in page claims the browser it runs
// in (its verifier id, `69-board-work.js`); from then on the work every fact
// credits to that id (`verifier_hours`, worker/verify.js) is the account's, and
// what it earned before anyone claimed it goes to its first claim (§"Spans").
// Only the owner is joined to a device, never a submission. Every account with a claimed
// device is on the public ranking, ANONYMOUS until it agrees to show its name
// (`contribution_choice`): publishing a name is the person's choice, asked once.
//
//   GET  /api/account/devices        → { devices: [{ id, label, claimed_at, points, recent, week, last_at, now }], removed: { count, points }, points, recent, week, ranks, contributor_rank, named, decided, volunteer }
//   POST /api/account/devices/claim  { verifier, label? } → { ok }
//   POST /api/account/devices/label  { id, label }        → { ok }
//   POST /api/account/devices/remove { id }               → { ok }
//   POST /api/account/contribution   { named }    → { ok }
//   POST /api/board/points           { verifier, since? } → { points, recent, week, claimed, today? } — `since` the UTC day's first hour;
//        the points are what no account holds yet, so 0 once it is claimed
//   POST /api/board/ledger           { verifier, from? } → the device's ledger (`ledgerOf`), the signed-in reader's account its `you`
//   POST /api/account/devices/ledger { id, from? }       → the same, for one of the account's devices by its six characters
//   GET  /api/account/ledger[?from=] → { devices: [{ id, label, removed, refused }], hours: [{ hour, id, points }] } — what came in, device by device
//   GET  /api/board/computing        → { computing } — the nav's count, cached a minute at the edge
//   GET  /api/board/demand           → `demandOf`'s body as the settler last wrote it (`DEMAND_KEY`): { v, at, computing, owed: { new_builds, sweeps, rescores, refreshes }, riven_gains,
//        surveys: [{ weapon, ruler, shapes, agreed, started, clear_hours }] (unfinished), per_hour: { volunteers, official }, clear_hours }
//   GET  /api/board/tally            → { totals: { volunteers, official }, per_hour: { volunteers, official }, computing } — the home hero's
//   GET  /api/contributors[?period=recent|week|last_week][&limit=n] → { period, computing, total, contributors: [{ key, name, points, recent, week, last_week, contributor_rank, mark?, volunteer?, you? }], windows: { week, recent, last_week: { from, until } } }
//        — `last_week` is the calendar week before this one, closed: the weekly summary's.
//        — the first `limit` rows (default `PAGE`, at most `MOST`) of `total`; `key` is the row's
//        own for the page to follow it as it moves, and says nothing of the account behind it.
//   GET  /api/contributors?name=<public name>  → { computing, person: { name, points, recent, week, ranks, contributor_rank, mark?, volunteer? } | null }
//        — `computing` is how many browsers answered in the last `COMPUTING_MS`;
//        a person is found by the exact name the ranking shows, never an anonymous one.
//        — `name` (the display name, else the username) is null for an account
//        that did not agree, and `mark` is the paid half's, for a named one only.
//
// `recent` is this calendar month and `week` this calendar week (`periodStarts`),
// so a newcomer can lead somewhere; `ranks` is the account's place on each of the
// three, for the showcase a share card draws (`31-share-card.js`).
import { json, no, now, sameSite, sessionAccount, sha256 } from "./accounts.js";
import { cloudMarks } from "./cloud.js";
import { iso, PACE_HOURS } from "./instant.js";
import { SURVEY_CHANNEL, SURVEY_KEEP_MS } from "./appraise.js";
import { kindOf } from "./factory.js";
import { shownName } from "./names.js";
import { noteFactoryCall } from "./factory_calls.js";
import { meritTotals } from "./merit.js";
import { dialsOf } from "./dials.js";
import { BOARD_KINDS } from "./board_kinds.js";

/// Work is counted in billionths of a point (engine/src/fight/gas.rs prices it).
const WORK_PER_POINT = 1e9;
/// THE SHORT RANKINGS ARE CALENDAR PERIODS IN UTC: the week from Monday 00:00, the
/// month from the 1st, so each opens with everyone at nought and a race to the
/// end of it. A rolling window never restarts, so nobody ever starts level. The
/// page says both ends in the reader's own time (17-account.js `periodWindow`).
/// `at`'s Monday and first of the month, as UTC days.
export function periodStarts(at = Date.now()) {
  const d = new Date(at);
  const monday = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
  return { week: iso(monday).slice(0, 10), month: iso(at).slice(0, 8) + "01", last_week: iso(monday - 7 * 86_400_000).slice(0, 10) };
}
/// …AND EACH ONE'S ENDS as instants, served beside the ranking so the page says
/// them in the reader's time without a second copy of the rule.
export function periodWindows(at = Date.now()) {
  const { week, month } = periodStarts(at), w = Date.parse(week + "T00:00:00Z"), m = new Date(month + "T00:00:00Z");
  return { week: { from: iso(w), until: iso(w + 7 * 86_400_000) },
    recent: { from: iso(m.getTime()), until: iso(Date.UTC(m.getUTCFullYear(), m.getUTCMonth() + 1, 1)) },
    last_week: { from: iso(w - 7 * 86_400_000), until: iso(w) } };
}
/// THE RANKING IS EVERYONE, served `PAGE` rows at a time as the page scrolls,
/// and asked again whole each minute, so `MOST` bounds what one answer carries.
export const PAGE = 100;
export const MOST = 2000;
const VERIFIER_ID = /^[a-z0-9]{16,40}$/;
/// What the owner calls a device: one short line, no control characters.
const LABEL = /^[^\u0000-\u001f\u007f]{1,40}$/u;
const labelOf = (s) => (typeof s === "string" && LABEL.test(s.trim()) ? s.trim() : null);
/// A LIST OF IDS IS BOUND A PART AT A TIME, this many to a statement.
const PER_STATEMENT = 90;

const points = (work) => Math.floor(work / WORK_PER_POINT);

/// THE CONTRIBUTOR RANK, an account's all-time points on Warframe's Mastery
/// Rank curve: rank n is reached at 2,500·n² experience up to 30, and every
/// 147,500 past rank 30's 2,250,000 is one rank more, without end
/// (wiki "Mastery Rank": "2,500 × Rank²"; Legendary ranks "147,500 each").
/// `xp_at_rank` and `xp_at_next` bound the rank the account holds, for its bar.
export const POINTS_PER_XP = 10;
const XP_PAST_30 = 147_500;
const xpAt = (n) => (n <= 30 ? 2500 * n * n : 2_250_000 + (n - 30) * XP_PAST_30);
export function contributorRank(pts) {
  const xp = Math.floor(Math.max(0, pts) / POINTS_PER_XP);
  let rank = xp < xpAt(30) ? Math.floor(Math.sqrt(xp / 2500)) : 30 + Math.floor((xp - xpAt(30)) / XP_PAST_30);
  // A square root a hair under an integer must not hold a rank back a step.
  while (xpAt(rank + 1) <= xp) rank++;
  return { rank, xp, xp_at_rank: xpAt(rank), xp_at_next: xpAt(rank + 1) };
}

/// WHO OWNS EACH OF `ids`, from the devices their owners claimed
/// (`worker/accounts.js` §devices) — an unclaimed one is absent, its own owner.
export async function ownersOf(env, ids) {
  const out = new Map();
  if (!env.ACCOUNTS || !ids.length) return out;
  const { results } = await env.ACCOUNTS.prepare(
    `SELECT verifier, account FROM devices WHERE verifier IN (${ids.map(() => "?").join(", ")})`).bind(...ids).all();
  for (const r of results) out.set(r.verifier, r.account);
  return out;
}


const since = (days) => new Date(Date.now() - (days - 1) * 86_400_000).toISOString().slice(0, 10);

/// THE WORK EACH OF `ids` IS CREDITED — `{ work, recent, week }`, all of it,
/// this month's and this week's (`periodStarts`) — a refused client's
/// counting for nothing.
async function workOf(env, ids) {
  const out = new Map();
  const db = env.LIBRARY;
  if (!db || !ids.length) return out;
  const { month: from, week, last_week } = periodStarts();
  // ONE STATEMENT for every id: the ranking asks for all of them, and ninety
  // at a time was a round trip each while a page waited.
  const { results } = await db.prepare(`SELECT v.id, v.work, v.consent_at,
      (SELECT SUM(d.work) FROM verifier_hours d WHERE d.verifier = v.id AND d.hour >= ?1::text) AS recent,
      (SELECT SUM(d.work) FROM verifier_hours d WHERE d.verifier = v.id AND d.hour >= ?2::text) AS week,
      (SELECT SUM(d.work) FROM verifier_hours d WHERE d.verifier = v.id AND d.hour >= ?4::text AND d.hour < ?2::text) AS last_week
    FROM verifiers v WHERE v.banned = 0 AND v.id IN (SELECT jsonb_array_elements_text((?3::text)::jsonb))`)
    .bind(from, week, JSON.stringify(ids), last_week).all();
  for (const r of results) out.set(r.id, { work: r.work || 0, recent: r.recent || 0, week: r.week || 0, last_week: r.last_week || 0, consent_at: r.consent_at || null });
  return out;
}
const NONE = { work: 0, recent: 0, week: 0, last_week: 0, consent_at: null };

// ---- Spans ---------------------------------------------------------------------
//
// A DEVICE'S WORK IS WHOSE IT WAS WHEN IT WAS EARNED, never whose it is now.
// A claim opens a span (`device_spans`) and the account holds the device's
// `verifier_hours` in it. The first claim's span reaches back to the device's
// first hour: what it earned unclaimed goes to that account once, and the
// browser's own count is empty after. A later claim by another account closes
// the open span and opens its own, and a removal closes it, so points never
// move between accounts: signing in once on someone's computer takes nothing
// they earned, and a machine passed around carries no points to anyone. A span
// ends at the NEXT hour, so a credit already made is never taken back.
const nextHour = () => iso(Date.now() + 3_600_000).slice(0, 13);
const spanAll = (s) => !s.from_hour && s.until_hour == null;

/// THE WORK IN EACH OF `spans` — `{ work, recent, week, consent_at }` in order —
/// a whole device's from `workOf`, a bounded span's summed over its hours.
async function spanWork(env, spans) {
  const whole = await workOf(env, [...new Set(spans.map((s) => s.verifier))]);
  const out = spans.map((s) => ({ ...(whole.get(s.verifier) || NONE) }));
  const cut = spans.map((s, i) => [s, i]).filter(([s]) => !spanAll(s));
  const db = env.LIBRARY;
  if (!db || !cut.length) return out;
  const { month: from, week, last_week } = periodStarts();
  const list = cut.map(([sp], k) => ({ k, v: sp.verifier, f: sp.from_hour, u: sp.until_hour }));
  const { results } = await db.prepare(`SELECT s.k, SUM(h.work) AS work, SUM(CASE WHEN h.hour >= ?1::text THEN h.work ELSE 0 END) AS recent,
      SUM(CASE WHEN h.hour >= ?2::text THEN h.work ELSE 0 END) AS week,
      SUM(CASE WHEN h.hour >= ?4::text AND h.hour < ?2::text THEN h.work ELSE 0 END) AS last_week
    FROM jsonb_to_recordset((?3::text)::jsonb) AS s(k bigint, v text, f text, u text)
    LEFT JOIN verifier_hours h ON h.verifier = s.v COLLATE "C" AND h.hour >= s.f COLLATE "C" AND (s.u IS NULL OR h.hour < s.u COLLATE "C")
    GROUP BY s.k`).bind(from, week, JSON.stringify(list), last_week).all();
  for (const r of results) {
    const [sp, k] = cut[r.k];
    // A REFUSED CLIENT counts for nothing, as `workOf` says by leaving it out.
    out[k] = whole.has(sp.verifier) ? { ...out[k], work: r.work || 0, recent: r.recent || 0, week: r.week || 0, last_week: r.last_week || 0 } : { ...NONE };
  }
  return out;
}

/// WHAT `verifier` EARNED THAT NO ACCOUNT HOLDS: everything after its last
/// span, or all of it when it never had one; nothing while one is open.
async function unheld(env, verifier) {
  const last = env.ACCOUNTS && await env.ACCOUNTS.prepare(
    "SELECT until_hour FROM device_spans WHERE verifier = ?1 ORDER BY from_hour DESC LIMIT 1").bind(verifier).first();
  if (last && last.until_hour == null) return { ...NONE, held: true };
  const [w] = await spanWork(env, [{ verifier, from_hour: last ? last.until_hour : "", until_hour: null }]);
  return { ...w, held: false };
}

/// CLOSE `verifier`'S OPEN SPAN at `at`; one that closes empty is dropped.
const closeSpan = (env, verifier, at) => env.ACCOUNTS.batch([
  env.ACCOUNTS.prepare("UPDATE device_spans SET until_hour = ?1 WHERE verifier = ?2 AND until_hour IS NULL").bind(at, verifier),
  env.ACCOUNTS.prepare("DELETE FROM device_spans WHERE verifier = ?1 AND until_hour = from_hour").bind(verifier),
]);

/// A VOLUNTEER: a device that said yes to computing and has had work credited
/// for it — an honour earned by computing, not by a click, and never sold.
/// `since` is the earliest such yes.
const volunteerSince = (ws) => ws.filter((w) => w.consent_at && w.work > 0).map((w) => w.consent_at).sort()[0] || null;

/// WHAT EACH OF `ids` IS DOING: when it last answered (`last_at`), and the task
/// it holds a live lease on (`now`) — named as a task KIND and its public facts,
/// so the page draws any kind the same way.
async function activityOf(env, ids) {
  const out = new Map();
  const db = env.LIBRARY;
  if (!db || !ids.length) return out;
  const t = Date.now();
  for (let i = 0; i < ids.length; i += PER_STATEMENT) {
    const part = ids.slice(i, i + PER_STATEMENT);
    const marks = part.map(() => "?").join(", ");
    const [seen, held] = await db.batch([
      db.prepare(`SELECT id, last_at FROM verifiers WHERE id IN (${marks})`).bind(...part),
      // A QUESTION OF THE FACT FACTORY, named by the producer that asked it.
      db.prepare(`SELECT q.leased_to, q.request, d.producer, d.payload FROM questions q
          JOIN demands d ON d.question = q.id AND d.state = 'live'
          WHERE q.lease_until > ? AND q.leased_to IN (${marks})`).bind(iso(t), ...part),
    ]);
    for (const r of seen.results) out.set(r.id, { last_at: r.last_at || null, now: null });
    for (const r of held.results) {
      let kind = null;
      try { kind = kindOf(r.producer, JSON.parse(r.request), JSON.parse(r.payload)); } catch (_) { /* a row that is not one names nothing */ }
      if (!kind) continue;
      const e = out.get(r.leased_to) || { last_at: null, now: null };
      e.now = kind;
      out.set(r.leased_to, e);
    }
  }
  return out;
}

/// THE OWNER'S DEVICE BY THE SIX CHARACTERS THE PAGE SEES — only theirs, and
/// only when exactly one matches.
async function ownDevice(env, account, id) {
  if (!/^[a-z0-9]{6}$/.test(id || "")) return null;
  const { results } = await env.ACCOUNTS.prepare(
    "SELECT verifier FROM devices WHERE account = ?1 AND substr(verifier, 1, 6) = ?2").bind(account, id).all();
  return results.length === 1 ? results[0].verifier : null;
}

async function devices(request, env, account) {
  // ITS PLACES ARE READ OFF THE RANKING'S OWN MINUTE (`kept`), found by its row
  // key: worked out here, every open account page re-read every claimed device.
  const [{ results }, { results: spans }, choice, { everyone, day }] = await Promise.all([
    env.ACCOUNTS.prepare("SELECT verifier, claimed_at, label FROM devices WHERE account = ?1 ORDER BY claimed_at").bind(account).all(),
    env.ACCOUNTS.prepare("SELECT verifier, from_hour, until_hour FROM device_spans WHERE account = ?1").bind(account).all(),
    env.ACCOUNTS.prepare("SELECT named FROM contribution_choice WHERE account = ?1").bind(account).first(),
    kept(request, env),
  ]);
  const ids = results.map((d) => d.verifier);
  const [held, doing, mine] = await Promise.all([spanWork(env, spans), activityOf(env, ids), rowKey(env, account, day)]);
  // EACH DEVICE'S SHARE IS WHAT ITS SPANS OF THIS ACCOUNT HOLD; a removed one's still counts.
  const work = new Map();
  spans.forEach((s, i) => {
    const e = work.get(s.verifier) || NONE, w = held[i];
    work.set(s.verifier, { work: e.work + w.work, recent: e.recent + w.recent, week: e.week + w.week, consent_at: w.consent_at });
  });
  const current = new Set(ids);
  const gone = [...work].filter(([v]) => !current.has(v)).map(([, w]) => w);
  const named = !!(choice && choice.named);
  const ranks = Object.fromEntries(Object.entries(PERIODS).map(([period, key]) => {
    const at = ordered(everyone, key).findIndex((e) => e.key === mine);
    return [period, at < 0 ? null : at + 1];
  }));
  const list = results.map((d) => ({ id: d.verifier.slice(0, 6), label: d.label || null, claimed_at: d.claimed_at,
    ...(doing.get(d.verifier) || { last_at: null, now: null }), ...(work.get(d.verifier) || NONE) }));
  const all = [...work.values()];
  const sum = (k) => all.reduce((t, w) => t + w[k], 0);
  // `shown` is `named` for a page from before the ranking was anonymous.
  return json({ ok: true, named, decided: !!choice, shown: named, volunteer: volunteerSince(all),
    points: points(sum("work")), recent: points(sum("recent")), week: points(sum("week")), ranks,
    contributor_rank: contributorRank(points(sum("work"))),
    removed: { count: gone.length, points: points(gone.reduce((t, w) => t + w.work, 0)) },
    devices: list.map(({ work: w, recent: r, week: k, last_week: _l, consent_at: _c, ...d }) => ({ ...d, points: points(w), recent: points(r), week: points(k) })) });
}

/// WHAT ONE BROWSER HOLDS, asked by the browser itself: its id is a secret only
/// it holds, so this tells nobody else anything. Its points are what no account
/// holds yet (`unheld`) — all it earned until its first claim, nothing while an
/// account holds it. `claimed` says whether one does, and never which.
async function devicePoints(env, b) {
  if (!VERIFIER_ID.test(b.verifier || "")) return no("bad_device");
  return json({ ok: true, ...await devicePointsOf(env, b.verifier, b.since) });
}
/// …AS A BODY, also carried by `/api/board/answers` (worker/verify.js) to the
/// page that sent results, so it need not ask again.
export async function devicePointsOf(env, verifier, since) {
  const w = await unheld(env, verifier);
  const claimed = w.held;
  // ITS OWN "TODAY": the whole UTC hours from the one the page names as its
  // midnight — results sent, what they took, and the points credited in them.
  let today = null;
  if (/^\d{4}-\d\d-\d\dT\d\d$/.test(since || "") && env.LIBRARY) {
    const t = await env.LIBRARY.prepare("SELECT SUM(tasks) AS tasks, SUM(ms) AS ms, SUM(work) AS work FROM verifier_hours WHERE verifier = ? AND hour >= ?")
      .bind(verifier, since).first();
    today = { tasks: (t && t.tasks) || 0, ms: (t && t.ms) || 0, points: points((t && t.work) || 0) };
  }
  return { points: points(w.work), recent: points(w.recent), week: points(w.week), claimed, ...(today ? { today } : {}) };
}

// ---- The ledger ----------------------------------------------------------------
//
// WHERE A DEVICE'S POINTS WENT, hour by hour, as transparent as privacy allows
// (docs/BOARD.md §"Contribution"). Each hour is its span's holder's (§"Spans"):
// `device` while no account held it, `you` for the reader's own account, `other`
// for any other — whose name never leaves here — and `closed` once that span
// ended. A full device id is never sent; only its own browser can ask by it.
const HOUR = /^\d{4}-\d\d-\d\dT\d\d$/;
const LEDGER_DAYS = 30;
/// The first hour a ledger lists: the one asked, "" for all of it, else the last `LEDGER_DAYS`.
const ledgerFrom = (f) => (f === "" || HOUR.test(f || "") ? f : since(LEDGER_DAYS) + "T00");

/// `verifier`'s ledger as `me` reads it: every hour from `from`, the all-time
/// split by holder, and its results that earned nothing in the window — those
/// that disagreed with the agreed result, those still waiting for another
/// computer, and those set aside with a refusal.
async function ledgerOf(env, verifier, me, from) {
  const db = env.LIBRARY;
  const [spans, hours, v, u, adj] = await Promise.all([
    env.ACCOUNTS ? env.ACCOUNTS.prepare("SELECT account, from_hour, until_hour FROM device_spans WHERE verifier = ?1 ORDER BY from_hour")
      .bind(verifier).all().then((r) => r.results) : [],
    db.prepare("SELECT hour, work, tasks FROM verifier_hours WHERE verifier = ? ORDER BY hour").bind(verifier).all().then((r) => r.results),
    db.prepare("SELECT banned FROM verifiers WHERE id = ?").bind(verifier).first(),
    db.prepare(`SELECT SUM(CASE WHEN a.void = 1 THEN 1 ELSE 0 END) AS refused,
        SUM(CASE WHEN a.void = 0 AND q.state = 'fact' THEN 1 ELSE 0 END) AS disagreed,
        SUM(CASE WHEN a.void = 0 AND q.state IN ('open', 'disputed', 'spot', 'nondeterministic') THEN 1 ELSE 0 END) AS waiting
        FROM answers a JOIN questions q ON q.id = a.question
        WHERE a.device = ? AND a.at >= ? AND a.credited = 0 AND a.official = 0`).bind(verifier, from).first(),
    // WHAT OF AN HOUR WAS GIVEN BY HAND (`adjustments`), already in its work and
    // named apart: compensation for an outage reads as that, not as computing.
    db.prepare("SELECT hour, reason, SUM(work) AS work FROM adjustments WHERE verifier = ? AND hour >= ? GROUP BY hour, reason")
      .bind(verifier, from).all().then((r) => r.results),
  ]);
  const given = new Map();
  for (const a of adj) given.set(a.hour, [...(given.get(a.hour) || []), { reason: a.reason, points: points(a.work || 0) }]);
  const at = (h) => {
    const s = spans.find((x) => (!x.from_hour || h >= x.from_hour) && (x.until_hour == null || h < x.until_hour));
    return s ? { holder: me && s.account === me ? "you" : "other", closed: s.until_hour != null } : { holder: "device", closed: false };
  };
  const sum = { tasks: 0, device: 0, you: 0, other: 0 }, rows = [];
  for (const r of hours) {
    const w = at(r.hour);
    sum.tasks += r.tasks || 0;
    sum[w.holder] += r.work || 0;
    if (r.hour >= from) rows.push({ hour: r.hour, tasks: r.tasks || 0, points: points(r.work || 0), ...w,
      ...(given.has(r.hour) ? { given: given.get(r.hour) } : {}) });
  }
  return { ok: true, from, earlier: !!hours.length && hours[0].hour < from, refused: !!(v && v.banned),
    summary: { tasks: sum.tasks, points: points(sum.device + sum.you + sum.other), device: points(sum.device), you: points(sum.you), other: points(sum.other) },
    unpaid: { disagreed: (u && u.disagreed) || 0, waiting: (u && u.waiting) || 0, refused: (u && u.refused) || 0 }, hours: rows };
}

async function deviceLedger(request, env, b) {
  if (!VERIFIER_ID.test(b.verifier || "")) return no("bad_device");
  if (!env.LIBRARY) return no("unavailable", 503);
  const me = env.AUTH_SECRET && env.ACCOUNTS ? await sessionAccount(env, request) : null;
  return json(await ledgerOf(env, b.verifier, me, ledgerFrom(b.from)));
}
async function ownLedger(env, account, b) {
  const v = await ownDevice(env, account, b.id);
  if (!v) return no("not_your_device", 404);
  if (!env.LIBRARY) return no("unavailable", 503);
  return json(await ledgerOf(env, v, account, ledgerFrom(b.from)));
}

/// WHAT CAME IN TO THE ACCOUNT, hour by hour and device by device — its spans'
/// hours alone, a removed device's included, each named by its six characters.
async function accountLedger(env, account, from) {
  const [{ results: spans }, { results: listed }] = await Promise.all([
    env.ACCOUNTS.prepare("SELECT verifier, from_hour, until_hour FROM device_spans WHERE account = ?1").bind(account).all(),
    env.ACCOUNTS.prepare("SELECT verifier, label FROM devices WHERE account = ?1").bind(account).all(),
  ]);
  const hours = [];
  // ONE STATEMENT for every span (`spanWork`'s reason).
  if (spans.length) {
    const list = spans.map((sp, k) => ({ k, v: sp.verifier, f: sp.from_hour, u: sp.until_hour }));
    const { results } = await env.LIBRARY.prepare(`SELECT h.hour, s.v, h.work
        FROM jsonb_to_recordset((?1::text)::jsonb) AS s(k bigint, v text, f text, u text)
        JOIN verifier_hours h ON h.verifier = s.v COLLATE "C" AND h.hour >= s.f COLLATE "C" AND h.hour >= ?2::text
          AND (s.u IS NULL OR h.hour < s.u COLLATE "C") ORDER BY s.k, h.hour`).bind(JSON.stringify(list), from).all();
    for (const r of results) hours.push({ hour: r.hour, id: r.v.slice(0, 6), points: points(r.work || 0) });
  }
  const ids = [...new Set(spans.map((s) => s.verifier))];
  const banned = new Set();
  for (let i = 0; i < ids.length; i += PER_STATEMENT) {
    const part = ids.slice(i, i + PER_STATEMENT);
    const { results } = await env.LIBRARY.prepare(`SELECT id FROM verifiers WHERE banned = 1 AND id IN (${part.map(() => "?").join(", ")})`)
      .bind(...part).all();
    for (const r of results) banned.add(r.id);
  }
  const label = new Map(listed.map((d) => [d.verifier, d.label]));
  const devices = ids.map((v) => ({ id: v.slice(0, 6), label: label.get(v) || null, removed: !label.has(v), refused: banned.has(v) }));
  return json({ ok: true, from, devices, hours: hours.sort((x, y) => (x.hour < y.hour ? -1 : x.hour > y.hour ? 1 : 0)) });
}

/// A DEVICE WORKS FOR THE ACCOUNT SIGNED IN ON IT, from the next hour on
/// (§"Spans"), and what it earned before anyone held it comes with the first
/// claim. ONLY THAT BROWSER CAN CLAIM IT: the full id is a secret it alone
/// keeps, and no endpoint sends it back (an owner's list shows six characters).
async function claim(env, account, b) {
  if (!VERIFIER_ID.test(b.verifier || "")) return no("bad_device");
  const v = b.verifier, db = env.ACCOUNTS;
  const owner = await db.prepare("SELECT account FROM devices WHERE verifier = ?1").bind(v).first();
  if (owner && owner.account === account) return json({ ok: true });
  await closeSpan(env, v, nextHour());
  const last = await db.prepare("SELECT until_hour FROM device_spans WHERE verifier = ?1 ORDER BY from_hour DESC LIMIT 1").bind(v).first();
  await db.batch([
    db.prepare("INSERT INTO device_spans (verifier, account, from_hour) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING").bind(v, account, last ? last.until_hour : ""),
    db.prepare(`INSERT INTO devices (verifier, account, claimed_at, label) VALUES (?1, ?2, ?3, ?4)
      ON CONFLICT (verifier) DO UPDATE SET account = ?2, claimed_at = ?3, label = ?4`).bind(v, account, now(), labelOf(b.label)),
  ]);
  return json({ ok: true });
}

/// WHAT THE OWNER CALLS IT, or — removed — off their list: its span closes and
/// what it earned in it stays theirs, so a browser whose cookies were cleared
/// is removed without losing a point. Signed in there again, it is claimed again.
async function relabel(env, account, b) {
  const v = await ownDevice(env, account, b.id);
  if (!v) return no("not_your_device", 404);
  const label = labelOf(b.label);
  if (!label) return no("bad_label");
  await env.ACCOUNTS.prepare("UPDATE devices SET label = ?1 WHERE verifier = ?2").bind(label, v).run();
  return json({ ok: true });
}
async function release(env, account, b) {
  const v = await ownDevice(env, account, b.id);
  if (!v) return no("not_your_device", 404);
  await closeSpan(env, v, nextHour());
  await env.ACCOUNTS.prepare("DELETE FROM devices WHERE verifier = ?1").bind(v).run();
  return json({ ok: true });
}

/// THE READER'S ANSWER, either way, so they are asked once. A page from before
/// the ranking was anonymous sends `shown`, which meant the same.
async function choose(env, account, b) {
  const named = typeof b.named === "boolean" ? b.named : b.shown;
  if (typeof named !== "boolean") return no("bad_choice");
  await env.ACCOUNTS.prepare(
    `INSERT INTO contribution_choice (account, named, chosen_at) VALUES (?1, ?2, ?3)
     ON CONFLICT (account) DO UPDATE SET named = ?2, chosen_at = ?3`).bind(account, named ? 1 : 0, now()).run();
  return json({ ok: true });
}

/// THE THREE RANKINGS, by the column each orders on: all of it, this month,
/// this week (`periodStarts`).
const PERIODS = { all: "points", recent: "recent", week: "week" };

/// EVERY ACCOUNT WITH A CLAIMED DEVICE and what its devices were credited, in
/// points — the one list every ranking and every account's place is cut from.
export async function standings(env, day = keyDay()) {
  const { results } = await env.ACCOUNTS.prepare(
    `SELECT a.id, a.username, a.display_name, h.display_name AS held_name, h.reason AS held_reason, c.named, s.verifier, s.from_hour, s.until_hour
       FROM device_spans s JOIN accounts a ON a.id = s.account LEFT JOIN contribution_choice c ON c.account = s.account
       LEFT JOIN display_name_holds h ON h.account = s.account`).all();
  const work = await spanWork(env, results);
  const by = new Map();
  for (const [i, r] of results.entries()) {
    const e = by.get(r.id) || { id: r.id, named: !!r.named, name: shownName(r.display_name, r.username, r.held_name ? { display_name: r.held_name, reason: r.held_reason } : null), work: 0, recent: 0, week: 0, last_week: 0, ws: [] };
    const w = work[i];
    e.work += w.work;
    e.recent += w.recent;
    e.week += w.week;
    e.last_week += w.last_week;
    e.ws.push(w);
    by.set(r.id, e);
  }
  return Promise.all([...by.values()].map(async (e) => ({ id: e.id, key: await rowKey(env, e.id, day), named: e.named, name: e.name,
    points: points(e.work), recent: points(e.recent), week: points(e.week), last_week: points(e.last_week), volunteer: !!volunteerSince(e.ws) })));
}
/// ONE RANKING: everyone with any of `key`, most first, a tie by the row key
/// so every answer and every place agree.
const ordered = (list, key) => list.filter((e) => e[key] > 0)
  .sort((x, y) => y[key] - x[key] || y.points - x.points || (x.key < y.key ? -1 : 1));

/// HOW MANY BROWSERS ARE COMPUTING NOW: those that answered an order within
/// this long, refused ones aside — a count, never who.
export const COMPUTING_MS = 10 * 60_000;
async function computingNow(env) {
  if (!env.LIBRARY) return 0;
  const from = iso(Date.now() - COMPUTING_MS);
  const r = await env.LIBRARY.prepare("SELECT COUNT(*) AS n FROM verifiers WHERE banned = 0 AND last_at >= ?").bind(from).first();
  return (r && r.n) || 0;
}

/// ONE PERSON, by the exact name the ranking shows them under — an account
/// that agreed to be named, the most points first where two share one — with
/// their points and place on each ranking.
async function person(request, env, name) {
  // READ OFF THE RANKING'S OWN MINUTE (`kept`), where only a named row carries a name.
  const { everyone, computing } = await kept(request, env);
  const hit = everyone.filter((e) => e.name === name).sort((x, y) => y.points - x.points)[0];
  if (!hit) return json({ ok: true, computing, person: null });
  const ranks = Object.fromEntries(Object.entries(PERIODS).map(([period, key]) => {
    const at = ordered(everyone, key).findIndex((e) => e.key === hit.key);
    return [period, at < 0 ? null : at + 1];
  }));
  return json({ ok: true, computing, person: { name: hit.name, points: hit.points,
    recent: hit.recent, week: hit.week, ranks, contributor_rank: hit.contributor_rank, ...(hit.volunteer ? { volunteer: true } : {}),
    ...(hit.mark ? { mark: hit.mark } : {}) } });
}

/// THE RANKING the page shows, its first `limit`. A name and a handle leave
/// here only for an account that agreed; the reader's own row is marked `you`,
/// to them alone.
/// THE OFFICIAL POOL, in points and by where it came from (`points_pool`).
async function poolOf(env) {
  if (!env.LIBRARY) return null;
  const { results } = await env.LIBRARY.prepare("SELECT source, SUM(work) AS work FROM points_pool GROUP BY source").all();
  const by = Object.fromEntries(results.map((r) => [r.source, points(r.work || 0)]));
  return { points: points(results.reduce((t, r) => t + (r.work || 0), 0)),
    deleted_account: by.deleted_account || 0, refused: by.refused || 0 };
}

/// …and the week before this one, closed, which no page ranks: the weekly
/// summary posted elsewhere reads it, so its numbers are the page's.
const RANKED = { ...PERIODS, last_week: "last_week" };

async function ranking(request, env, period, limit, me) {
  const key = RANKED[period] || "points";
  const { everyone, computing, day, pool, merit } = await kept(request, env);
  const mine = me && await rowKey(env, me, day);
  const all = ordered(everyone, key);
  const contributors = all.slice(0, limit).map((e) => ({ ...e, ...(e.key === mine ? { you: true } : {}) }));
  return json({ ok: true, period: Object.keys(RANKED).find((p) => RANKED[p] === key), computing, total: all.length, contributors, pool: pool || null, merit: merit || null,
    windows: periodWindows() });
}

/// A ROW'S KEY, for the page to follow it as it moves: its account's id and
/// the UTC day hashed with the server's secret. A new day is a new key, so a
/// row is followed within a day and never linked across days by its key.
const keyDay = () => new Date().toISOString().slice(0, 10);
const rowKey = async (env, id, day) => (await sha256(`${env.AUTH_SECRET || ""}:contributor:${day}:${id}`)).slice(0, 16);

/// EVERY ROW AS IT MAY BE SERVED, and the count, kept a minute at the edge:
/// every open ranking asks again each minute and `standings` reads every
/// claimed device. Only what an answer may carry is kept — no account id, and
/// an anonymous row's name never.
const KEPT_SECONDS = 60;
/// …AND ONCE AN ISOLATE, a worked-out ranking and never one being worked out:
/// a request's I/O is its own, so a promise another request began dies with that
/// request and every page that waited on it hangs until its own timeout.
let keptHere = null;
async function kept(request, env) {
  // NO FRESHER THAN THE EDGE'S MINUTE, so only where there is an edge.
  if (typeof caches === "undefined") return keptWorkedOut(request, env);
  if (keptHere && Date.now() - keptHere.at < KEPT_SECONDS * 1000) return keptHere.body;
  const body = await keptWorkedOut(request, env);
  keptHere = { at: Date.now(), body };
  return body;
}
/// THE NAMED ROWS' MARKS, asked of the paid half every `MARKS_KEEP_MS` an
/// isolate: a mark is a tier or a title, which moves in days, and the asking is
/// seconds. A reader's ask waits `MARKS_WAIT_MS` at most and falls back to the
/// last marks drawn; the published ranking waits for the answer (`wait`) and is
/// null without one, since a badge gone for ten minutes reads as a refund.
const MARKS_KEEP_MS = 10 * 60_000, MARKS_WAIT_MS = 4_000;
let marksHere = null;
async function marksOfNamed(env, named, wait = false) {
  if (!wait && marksHere && Date.now() - marksHere.at < MARKS_KEEP_MS && named.every((id) => marksHere.asked.has(id))) return marksHere.marks;
  const parts = [];
  for (let i = 0; i < named.length; i += PER_STATEMENT) parts.push(cloudMarks(env, named.slice(i, i + PER_STATEMENT)));
  const asking = Promise.all(parts).then((got) => {
    if (got.some((g) => g === null)) return null;
    marksHere = { at: Date.now(), asked: new Set(named), marks: Object.assign({}, ...got) };
    return marksHere.marks;
  });
  if (wait) return asking;
  const late = new Promise((res) => setTimeout(() => res(null), MARKS_WAIT_MS));
  return (await Promise.race([asking, late])) || (marksHere ? marksHere.marks : {});
}
async function keptWorkedOut(request, env) {
  const cache = typeof caches !== "undefined" ? caches.default : null;
  const at = new Request(new URL("/api/contributors/kept", request.url).toString());
  const hit = cache && await cache.match(at);
  if (hit) return hit.json();
  // THE PUBLISHED RANKING when it is current: it is this same body, worked out once.
  const published = await publishedContributors(env);
  const body = published ? { everyone: published.rankings.all, day: published.day, computing: published.computing, pool: published.pool,
    merit: published.merit || null }
    : await everyoneOf(env);
  if (cache) await cache.put(at, new Response(JSON.stringify(body),
    { headers: { "content-type": "application/json", "cache-control": `public, max-age=${KEPT_SECONDS}` } }));
  return body;
}
/// EVERY ROW AS IT MAY BE SERVED: no account id, and a name, a mark or the
/// honour only on a row whose account agreed to be named. Null when `wait`
/// and the paid half did not answer for the marks.
async function everyoneOf(env, wait = false) {
  const day = keyDay(), rows = await standings(env, day);
  // THE PAID HALF answers a hundred accounts an ask, so the named rows go in parts.
  const marks = await marksOfNamed(env, rows.filter((e) => e.named).map((e) => e.id), wait);
  if (!marks) return null;
  const everyone = rows.map((e) => ({ key: e.key, name: e.named ? e.name : null,
    points: e.points, recent: e.recent, week: e.week, last_week: e.last_week, contributor_rank: contributorRank(e.points),
    // THE HONOUR travels with a name only: an anonymous row says nothing more.
    ...(e.named && e.volunteer ? { volunteer: true } : {}), ...(e.named && marks[e.id] ? { mark: marks[e.id] } : {}) }));
  return { everyone, day, computing: await computingNow(env), pool: await poolOf(env), merit: await meritTotals(env).catch(() => null) };
}

/// THE RANKING AS A FILE (docs/BOARD.md §"The public stats"): the worker's
/// ten-minute cron writes it to the public bucket, and a signed-out page reads
/// it from the CDN instead of asking. Each period is ordered here, by the same
/// `ordered` every answer uses, so the page only slices. The reader's own row
/// (`you`) needs who is asking, so a signed-in page still asks `/api/contributors`,
/// which reads this file back while it is current.
export const CONTRIBUTORS_KEY = "stats/contributors.json";
/// Read back while younger than this many publish periods.
const CONTRIBUTORS_FRESH_PERIODS = 2;
export function contributorsFile(body, at = iso(Date.now())) {
  const rankings = Object.fromEntries(Object.entries(PERIODS).map(([period, key]) => [period, ordered(body.everyone, key)]));
  return { ok: true, v: 1, at, day: body.day, computing: body.computing, pool: body.pool || null, merit: body.merit || null, rankings,
    windows: periodWindows(Date.parse(at)) };
}
export async function publishContributors(env, now = Date.now()) {
  if (!env.PUBLIC || !env.ACCOUNTS) return false;
  const every = (await dialsOf(env, now)).stats_contributors_ms;
  const head = await env.PUBLIC.head(CONTRIBUTORS_KEY).catch(() => null);
  // A CRON A FEW SECONDS EARLY still writes: the dial is a period, not a deadline.
  if (head && head.uploaded && now - new Date(head.uploaded).getTime() < every - 60_000) return false;
  // A RANKING WITHOUT ITS MARKS IS NOT PUBLISHED: the last file stays served while fresh.
  const body = await everyoneOf(env, true);
  if (!body) { console.log(JSON.stringify({ event: "contributors.unmarked" })); return false; }
  const file = contributorsFile(body, iso(now));
  await env.PUBLIC.put(CONTRIBUTORS_KEY, JSON.stringify(file), { httpMetadata: { contentType: "application/json",
    cacheControl: `public, max-age=${Math.round(Math.min(every, 600_000) / 2000)}` } });
  return true;
}
async function publishedContributors(env, now = Date.now()) {
  const obj = env.PUBLIC && await env.PUBLIC.get(CONTRIBUTORS_KEY).catch(() => null);
  if (!obj) return null;
  let file;
  try { file = JSON.parse(await obj.text()); } catch { return null; }
  const every = (await dialsOf(env, now)).stats_contributors_ms;
  // A FILE OF ANOTHER DAY keys its rows by another day: never read across midnight.
  if (!file || file.v !== 1 || file.day !== keyDay() || now - Date.parse(file.at) > every * CONTRIBUTORS_FRESH_PERIODS) return null;
  return file;
}

/// WHAT THE BOARD ASKS FOR AND WHAT IS ANSWERING IT, for the compute page's
/// "Demand and compute" (73-compute.js) — docs/BOARD.md §"Demand". ONE COPY FOR
/// EVERY READER: the settler's daemon works it out each minute and writes it to
/// the public bucket as `DEMAND_KEY` (scripts/factoryd.mjs); the page reads that
/// object and `/api/board/demand` serves the same bytes. Worked out per request,
/// each isolate and each edge would keep its own minute, and readers disagree.
export const DEMAND_KEY = "demand.json", DEMAND_CACHE = "public, max-age=30";
/// …AND WHAT A FACT COSTS, over a day: every witness's fight, the ones that
/// agreed and the ones that did not, over the facts formed. Shorter, a few
/// hours of one bad release price every row at what that release wasted.
export const DEMAND_COST_MS = 86_400_000;
/// THE DAY'S ANSWERS ARE FOUND by reading back at most this many by id, the
/// one index the table keeps in time order.
export const DEMAND_COST_SCAN = 1_500_000;
/// …AND PRICED AGAIN THIS OFTEN, kept per `env` between: a day's price moves
/// slowly, and reading the day back is seconds of the database every time. The
/// owed rows and the open questions are counted on the same clock: each reads
/// a table that does not fit in memory, and once a minute it pushed the
/// volunteers' own rows out of the cache until their syncs timed out on locks.
export const DEMAND_COST_EVERY_MS = 10 * 60_000;
/// …EACH OF ITS STATEMENTS GIVEN THIS LONG, past the daemon's thirty seconds: on
/// a board of a million and a half open questions the day's join and counts read
/// for tens of seconds, read-only, once in `DEMAND_COST_EVERY_MS`.
export const DEMAND_COST_TIMEOUT_MS = 120_000;
const demandCosts = new WeakMap();
/// THE BOARD'S KINDS OF ORDER, as `orders` lists them, each by the `k` the
/// owed rows are counted under. A kind is added here and named on the page
/// (73-compute.js `ORDER_KIND_NAMES`); a page that does not know it yet shows it by its id.
export const ORDER_KINDS = [["rescore", 1], ["sweep", 2], ["new_build", 0], ["refresh", 3]];
/// THE FLEET THIS MOMENT (`now`): who asked, what they hold and what they
/// answered over the last `NOW_WINDOW_MS`. Every speed above is a day's, and
/// a day's mean hides an afternoon of nobody handed anything.
export const NOW_WINDOW_MS = 10 * 60_000;
/// STARVED: rows wait on the server, enough of them for every asking device to
/// take a pack, while those devices answer under this many each in the window.
/// A healthy hour's fleet answers 450 to 1600 a device; the afternoon the
/// pick was planned on a drained queue's statistics it answered 8 to 38.
export const STARVED_ANSWERS_PER_DEVICE = 15, STARVED_WAITING_PER_DEVICE = 10, STARVED_MIN_ASKING = 3;
export function starvedOf(reading, orders) {
  const waiting = orders.filter((o) => o.kind !== "riven_gain").reduce((n, o) => n + o.total - o.dispatched, 0);
  return reading.asking >= STARVED_MIN_ASKING && waiting >= STARVED_WAITING_PER_DEVICE * reading.asking
    && reading.answered < STARVED_ANSWERS_PER_DEVICE * reading.asking;
}
/// THE READING: three index reads, cheap enough each minute. A lease is read
/// through `questions_holder`: through the open questions it read every one.
async function nowOf(db, now) {
  const since = iso(now - NOW_WINDOW_MS);
  const [asking, held, answered] = await db.batch([
    db.prepare("SELECT COUNT(*) AS n FROM verifiers WHERE asked_at > ? AND banned = 0").bind(since),
    db.prepare("SELECT COUNT(*) AS n FROM questions WHERE leased_to IS NOT NULL AND lease_until > ?").bind(iso(now)),
    db.prepare("SELECT COUNT(*) AS n FROM answers WHERE at > ? AND official = 0").bind(since),
  ]);
  const n = (r) => Number((r.results[0] || {}).n || 0);
  return { window_seconds: NOW_WINDOW_MS / 1000, asking: n(asking), held: n(held), answered: n(answered) };
}

/// THE WHOLE PICTURE, totals only, never a device or a person (docs/BOARD.md
/// §"Demand" says each field).
/// HOURS TO CLEAR ARE COMPUTE, NOT ROWS: one riven search costs as much as
/// dozens of board rows, so rows per hour cannot say when the rest is done.
/// Each verb's open work is priced at what one of its facts cost over the day
/// and divided by the compute the volunteers answered per hour: every order
/// listed, so the countdown is when the total the page shows is done. Null
/// where a verb with work formed no fact to price it by.
/// THE ORDERS ARE ONE LIST, a kind a row (`ORDER_KINDS`): what each kind owes,
/// and how much of it a device has taken — the rest waits on the server.
export async function demandOf(env, now = Date.now(), scan = DEMAND_COST_SCAN, options = {}) {
  const db = env.LIBRARY, at = iso(now);
  const kept = demandCosts.get(env);
  const counts = kept && now >= kept.countsAt && now - kept.countsAt < DEMAND_COST_EVERY_MS ? kept : null;
  const costs = kept && now >= kept.costsAt && now - kept.costsAt < (options.costEveryMs || DEMAND_COST_EVERY_MS) ? kept : null;
  const began = performance.now();
  const reads = await db.batch([
    db.prepare(`SET LOCAL statement_timeout = ${costs ? "DEFAULT" : DEMAND_COST_TIMEOUT_MS}`),
    ...(options.pace ? [] : [db.prepare(`SELECT ${SPLIT} FROM scores WHERE finished_at >= ?`).bind(iso(now - PACE_HOURS * 3_600_000))]),
    db.prepare(`SELECT COUNT(*) AS n, SUM(CASE WHEN started_at IS NULL THEN 0 ELSE 1 END) AS started
                FROM appraisals WHERE done_at IS NULL AND request IS NOT NULL AND at > ? AND channel != ?`)
      .bind(iso(now - 86_400_000), SURVEY_CHANNEL),
    // EACH SURVEY AS A GOAL: every riven shape of one weapon on one ruler, how
    // many two owners have agreed on and how many anyone has begun. A survey
    // whose every shape is agreed is reached, and no longer a goal.
    db.prepare(`SELECT weapon, ruler, COUNT(*) AS shapes, SUM(CASE WHEN agreed_at IS NULL THEN 0 ELSE 1 END) AS agreed, SUM(CASE WHEN started_at IS NULL THEN 0 ELSE 1 END) AS started,
                SUM(CASE WHEN agreed_at >= ?3::text THEN 1 ELSE 0 END) AS lately
                FROM appraisals WHERE channel = ?1::text AND request IS NOT NULL AND at > ?2::text GROUP BY weapon, ruler
                HAVING SUM(CASE WHEN agreed_at IS NULL THEN 0 ELSE 1 END) < COUNT(*) ORDER BY MIN(at), weapon`)
      .bind(SURVEY_CHANNEL, iso(now - SURVEY_KEEP_MS), iso(now - PACE_HOURS * 3_600_000)),
    // THE LAST WHOLE HOURS, the one under way left out.
    db.prepare("SELECT COALESCE(SUM(ms), 0) AS ms FROM verifier_hours WHERE hour >= ?1::text AND hour < ?2::text")
      .bind(iso(now - PACE_HOURS * 3_600_000).slice(0, 13), iso(now).slice(0, 13)),
    ...(counts ? [] : [
      // A ROW A NEW BUILD'S DEMAND IS LIVE FOR counts as a new build's. Those
      // demands are read once and joined whole: looked up a queue row at a time,
      // every row cost an index walk for the few that have one.
      db.prepare(`WITH d AS MATERIALIZED (SELECT ref FROM demands WHERE state = 'live' AND producer = 'new_build')
                  SELECT k, COUNT(*) AS n FROM (SELECT MIN(CASE WHEN d.ref IS NOT NULL THEN 0 WHEN q.batch LIKE 'rescore%' THEN 1 WHEN q.batch LIKE 'refresh-%' THEN 3 ELSE 2 END) AS k
                  FROM queue q JOIN batches b ON b.id = q.batch
                  LEFT JOIN d ON d.ref = q.build_id || '|' || q.ruler || '|' || q.mode
                  GROUP BY q.build_id, q.ruler, q.mode) x GROUP BY k`),
      db.prepare("SELECT verb, COUNT(*) AS n FROM questions WHERE state = 'open' GROUP BY verb"),
    ]),
    ...(costs ? [] : [
      db.prepare(`WITH s AS MATERIALIZED (SELECT question, compute_ms FROM answers
                    WHERE id > (SELECT COALESCE(MAX(id), 0) FROM answers) - ?2::bigint AND at >= ?1::text AND official = 0)
                  SELECT q.verb, SUM(COALESCE(s.compute_ms, 0)) AS ms FROM s JOIN questions q ON q.id = s.question GROUP BY q.verb`)
        .bind(iso(now - DEMAND_COST_MS), scan),
      // …OVER THE FACTS FORMED SINCE THE FIRST ANSWER THAT READ: a busy day has
      // more answers than the read reaches, and its facts over the whole day
      // priced every row at the fraction of the day the answers covered.
      db.prepare(`SELECT verb, COUNT(*) AS n FROM questions WHERE fact_at >= GREATEST(?1::text,
                    (SELECT at FROM answers WHERE id > (SELECT COALESCE(MAX(id), 0) FROM answers) - ?2::bigint ORDER BY id LIMIT 1))
                  GROUP BY verb`).bind(iso(now - DEMAND_COST_MS), scan),
    ]),
    ...(counts ? [] : [
      // A ROW A DEVICE HAS TAKEN — leased, by a volunteer or an official
      // machine, now or before — is dispatched, by the kind `owed` counts it as.
      db.prepare(`SELECT k, COUNT(*) AS n FROM (SELECT x.id, MIN(CASE WHEN d.producer = 'new_build' THEN 0
                      WHEN d.payload::jsonb ->> 'batch' LIKE 'rescore%' THEN 1 WHEN d.producer = 'refresh' THEN 3 ELSE 2 END) AS k
                    FROM questions x JOIN demands d ON d.question = x.id AND d.state = 'live' AND d.producer IN (SELECT value FROM jsonb_array_elements_text((?::text)::jsonb))
                   WHERE x.state = 'open' AND x.verb = 'simulate' AND (x.leased_at IS NOT NULL OR x.lease IS NOT NULL)
                   GROUP BY x.id) y GROUP BY k`).bind(JSON.stringify(BOARD_KINDS)),
    ]),
  ]);
  const [, done, gains, surveys, answered, ...rest] = options.pace
    ? [reads[0], { results: [options.pace] }, ...reads.slice(1)] : reads;
  let cursor = 0;
  const owed = counts ? null : rest[cursor++], open = counts ? null : rest[cursor++];
  const spent = costs ? null : rest[cursor++], facts = costs ? null : rest[cursor++], taken = counts ? null : rest[cursor++];
  const counted = counts || { by: Object.fromEntries(owed.results.map((r) => [r.k, Number(r.n)])),
    takenBy: Object.fromEntries(taken.results.map((r) => [r.k, Number(r.n)])),
    openOf: open.results.map((r) => ({ verb: r.verb, n: Number(r.n) })) };
  const by = counted.by;
  const h = done.results[0] || {};
  const perHour = (x) => Math.round(Number(x || 0) / PACE_HOURS);
  const { msOf, factsOf } = costs || { msOf: Object.fromEntries(spent.results.map((r) => [r.verb, Number(r.ms)])),
    factsOf: Object.fromEntries(facts.results.map((r) => [r.verb, Number(r.n)])) };
  if (!costs || !counts) {
    demandCosts.set(env, { countsAt: counts ? counts.countsAt : now, costsAt: costs ? costs.costsAt : now,
      msOf, factsOf, by, takenBy: counted.takenBy, openOf: counted.openOf });
    if (!costs) options.onCosts?.(performance.now() - began);
  }
  const msPerHour = Number((answered.results[0] || {}).ms || 0) / PACE_HOURS;
  // THE BOARD'S ROWS are the queue's; every other verb's open questions are its work.
  const left = { simulate: (by[0] || 0) + (by[1] || 0) + (by[2] || 0) + (by[3] || 0) };
  for (const r of counted.openOf) if (r.verb !== "simulate") left[r.verb] = r.n;
  let ms = 0;
  for (const [verb, n] of Object.entries(left)) {
    if (!n) continue;
    if (!factsOf[verb]) { ms = null; break; }
    ms += n * (msOf[verb] || 0) / factsOf[verb];
  }
  const clearHours = ms !== null && msPerHour > 0 ? ms / msPerHour : null;
  const orders = [...ORDER_KINDS.map(([kind, k]) => ({ kind, total: by[k] || 0, dispatched: Math.min(by[k] || 0, counted.takenBy[k] || 0) })),
    { kind: "riven_gain", total: Number((gains.results[0] || {}).n || 0), dispatched: Number((gains.results[0] || {}).started || 0) }]
    .filter((o) => o.total > 0);
  const reading = await nowOf(db, now);
  return { ok: true, v: 1, at, computing: await computingNow(env),
    owed: { new_builds: by[0] || 0, rescores: by[1] || 0, sweeps: by[2] || 0, refreshes: by[3] || 0 },
    riven_gains: Number((gains.results[0] || {}).n || 0),
    orders,
    surveys: surveys.results.map(({ lately, ...g }) => {
      const s = { weapon: g.weapon, ruler: g.ruler, shapes: Number(g.shapes), agreed: Number(g.agreed), started: Number(g.started) };
      const pace = Number(lately || 0) / (PACE_HOURS);
      return { ...s, clear_hours: pace > 0 ? (s.shapes - s.agreed) / pace : null };
    }),
    per_hour: { volunteers: perHour(h.volunteers), official: perHour(h.official) }, clear_hours: clearHours,
    now: { ...reading, starved: starvedOf(reading, orders) } };
}

/// THE PAGE'S FALLBACK, where the bucket's own address is not reached: the
/// daemon's object as it stands, never worked out here.
async function demand(env) {
  const obj = env.PUBLIC && await env.PUBLIC.get(DEMAND_KEY).catch(() => null);
  if (!obj) return json({ ok: false, error: "no_demand" }, 503);
  return new Response(obj.body, { headers: { "content-type": "application/json", "cache-control": DEMAND_CACHE } });
}

/// THE HOME HERO'S TALLY: every score the board holds, by who computed it, and
/// how many an hour over the pace window (`PACE_HOURS`). Counting the whole table costs a scan, so that count is a
/// BASE kept `TALLY_BASE_MS` at the edge and each answer adds the rows finished
/// since it, read on `scores_oldest`. A row replaced since the base is counted
/// again until the next base; the volunteers' count stays exact either way.
export const TALLY_BASE_MS = 60 * 60_000;
const SPLIT = `SUM(CASE WHEN measured_by LIKE 'verified:%' THEN 1 ELSE 0 END) AS volunteers, SUM(CASE WHEN measured_by LIKE 'verified:%' THEN 0 ELSE 1 END) AS official`;
const split = (r) => ({ volunteers: (r && r.volunteers) || 0, official: (r && r.official) || 0 });
export async function scorePace(env, now = Date.now()) {
  return split(await env.LIBRARY.prepare(`SELECT ${SPLIT} FROM scores WHERE finished_at >= ?`)
    .bind(iso(now - PACE_HOURS * 3_600_000)).first());
}
/// THE BASE: every score by who computed it, as of `at`.
export async function tallyBase(db, now = Date.now()) {
  return { at: iso(now), ...split(await db.prepare(`SELECT ${SPLIT} FROM scores`).first()) };
}
/// THE TALLY'S BODY from a base: the worker's answer and the settler's file
/// (`TALLY_KEY`) are this one function.
export async function tallyOf(env, base, now = Date.now(), pace = null) {
  const db = env.LIBRARY;
  let totals = { volunteers: 0, official: 0 }, perHour = { volunteers: 0, official: 0 };
  if (db && base) {
    const reads = await db.batch([
      db.prepare(`SELECT ${SPLIT} FROM scores WHERE finished_at > ?`).bind(base.at),
      ...(pace ? [] : [db.prepare(`SELECT ${SPLIT} FROM scores WHERE finished_at >= ?`).bind(iso(now - PACE_HOURS * 3_600_000))]),
    ]);
    const [since, hour] = pace ? [reads[0], { results: [pace] }] : reads;
    const s = split(since.results[0]);
    totals = { volunteers: base.volunteers + s.volunteers, official: base.official + s.official };
    const h = split(hour.results[0]);
    perHour = { volunteers: Math.round(h.volunteers / PACE_HOURS), official: Math.round(h.official / PACE_HOURS) };
  }
  return { ok: true, v: 1, at: iso(now), totals, per_hour: perHour, computing: await computingNow(env) };
}
/// …AS A FILE, written by the settler every `stats_tally_ms` (scripts/factoryd.mjs)
/// and read by the home page from the CDN; this answer is its fallback.
export const TALLY_KEY = "stats/tally.json", TALLY_CACHE = "public, max-age=30";
async function tally(request, env) {
  const cache = typeof caches !== "undefined" ? caches.default : null;
  const key = new Request(new URL("/api/board/tally", request.url).toString());
  const hit = cache && await cache.match(key);
  if (hit) return hit;
  const db = env.LIBRARY;
  const baseKey = new Request(new URL("/api/board/tally/base", request.url).toString());
  let base = cache && await cache.match(baseKey).then((r) => (r ? r.json() : null));
  if (!base && db) {
    base = await tallyBase(db);
    if (cache) await cache.put(baseKey, new Response(JSON.stringify(base),
      { headers: { "content-type": "application/json", "cache-control": `public, max-age=${TALLY_BASE_MS / 1000}` } }));
  }
  const r = new Response(JSON.stringify(await tallyOf(env, base)),
    { headers: { "content-type": "application/json", "cache-control": "public, max-age=60" } });
  if (cache) await cache.put(key, r.clone());
  return r;
}

/// THE NAV'S COUNT, asked by every page a reader opens, so held a minute in the
/// edge cache and computed by one cheap count.
async function computingCount(request, env) {
  const cache = typeof caches !== "undefined" ? caches.default : null;
  const key = new Request(new URL("/api/board/computing", request.url).toString());
  const hit = cache && await cache.match(key);
  if (hit) return hit;
  const r = new Response(JSON.stringify({ ok: true, computing: await computingNow(env) }),
    { headers: { "content-type": "application/json", "cache-control": "public, max-age=60" } });
  if (cache) await cache.put(key, r.clone());
  return r;
}

/// The response for a contribution path, or null for a path that is not one.
export async function contributionRoute(request, env, path) {
  if (path === "/api/contributors") {
    if (request.method !== "GET") return no("method", 405);
    if (!env.ACCOUNTS) return json({ ok: true, contributors: [] });
    const me = env.AUTH_SECRET ? await sessionAccount(env, request) : null;
    const q = new URL(request.url).searchParams;
    const name = (q.get("name") || "").trim();
    if (name) return person(request, env, name);
    const limit = Math.min(MOST, Math.max(1, parseInt(q.get("limit"), 10) || PAGE));
    return ranking(request, env, q.get("period"), limit, me);
  }
  if (path === "/api/board/computing") {
    if (request.method !== "GET") return no("method", 405);
    return computingCount(request, env);
  }
  if (path === "/api/board/demand") {
    if (request.method !== "GET") return no("method", 405);
    return demand(env);
  }
  if (path === "/api/board/tally") {
    if (request.method !== "GET") return no("method", 405);
    return tally(request, env);
  }
  if (path === "/api/board/points") {
    if (request.method !== "POST") return no("method", 405);
    return noteFactoryCall(request, env, path, async () => {
      let b = {};
      try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
      return devicePoints(env, b);
    });
  }
  if (path === "/api/board/ledger") {
    if (request.method !== "POST") return no("method", 405);
    let b = {};
    try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
    return deviceLedger(request, env, b);
  }
  const posts = { "/api/account/devices/claim": claim, "/api/account/devices/label": relabel,
    "/api/account/devices/remove": release, "/api/account/contribution": choose, "/api/account/devices/ledger": ownLedger };
  const gets = ["/api/account/devices", "/api/account/ledger"];
  if (!gets.includes(path) && !posts[path]) return null;
  if (!env.ACCOUNTS || !env.AUTH_SECRET) return no("unavailable", 503);
  const get = gets.includes(path);
  if (request.method !== (get ? "GET" : "POST")) return no("method", 405);
  if (!get && !sameSite(request)) return no("cross_site", 403);
  const account = await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  if (path === "/api/account/ledger") {
    if (!env.LIBRARY) return no("unavailable", 503);
    return accountLedger(env, account, ledgerFrom(new URL(request.url).searchParams.get("from") ?? undefined));
  }
  if (get) return devices(request, env, account);
  let b = {};
  try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
  return posts[path](env, account, b);
}
