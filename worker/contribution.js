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
//   POST /api/board/points           { verifier, since? } → { points, recent, week, claimed, today? } — `since` the page's midnight as a UTC hour;
//        the points are what no account holds yet, so 0 once it is claimed
//   GET  /api/board/computing        → { computing } — the nav's count, cached a minute at the edge
//   GET  /api/board/demand           → { computing, owed: { new_builds, sweeps, rescores }, riven_gains, surveys: [{ weapon, ruler, shapes, agreed, started }], per_hour: { volunteers, official } }
//   GET  /api/board/tally            → { totals: { volunteers, official }, per_hour: { volunteers, official }, computing } — the home hero's
//   GET  /api/contributors[?period=recent|week] → { period, computing, contributors: [{ name, points, recent, week, contributor_rank, mark?, volunteer?, you? }] }
//   GET  /api/contributors?name=<public name>  → { computing, person: { name, points, recent, week, ranks, contributor_rank, mark?, volunteer? } | null }
//        — `computing` is how many browsers answered in the last `COMPUTING_MS`;
//        a person is found by the exact name the ranking shows, never an anonymous one.
//        — `name` (the display name, else the username) is null for an account
//        that did not agree, and `mark` is the paid half's, for a named one only.
//
// `recent` is the last `RECENT_DAYS` days and `week` the last `WEEK_DAYS`, so a
// newcomer can lead somewhere; `ranks` is the account's place on each of the
// three, for the showcase a share card draws (`31-share-card.js`).
import { json, no, now, sameSite, sessionAccount } from "./accounts.js";
import { cloudMarks } from "./cloud.js";
import { iso } from "./instant.js";
import { SURVEY_CHANNEL, SURVEY_KEEP_MS } from "./appraise.js";

/// `WORK_WEIGHTS` counts in billionths of a point.
const WORK_PER_POINT = 1e9;
/// The ranking's other column: the days counted back from today, today included.
export const RECENT_DAYS = 30;
export const WEEK_DAYS = 7;
/// How many names the ranking shows.
export const RANKED = 100;
const VERIFIER_ID = /^[a-z0-9]{16,40}$/;
/// What the owner calls a device: one short line, no control characters.
const LABEL = /^[^\u0000-\u001f\u007f]{1,40}$/u;
const labelOf = (s) => (typeof s === "string" && LABEL.test(s.trim()) ? s.trim() : null);
/// D1 binds at most a hundred parameters a statement.
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
/// the last `RECENT_DAYS` days and the last `WEEK_DAYS` — a refused client's
/// counting for nothing.
async function workOf(env, ids) {
  const out = new Map();
  if (!env.LIBRARY) return out;
  const from = since(RECENT_DAYS), week = since(WEEK_DAYS);
  for (let i = 0; i < ids.length; i += PER_STATEMENT) {
    const part = ids.slice(i, i + PER_STATEMENT);
    const marks = part.map(() => "?").join(", ");
    const [all, recent] = await env.LIBRARY.batch([
      env.LIBRARY.prepare(`SELECT id, work, consent_at FROM verifiers WHERE banned = 0 AND id IN (${marks})`).bind(...part),
      env.LIBRARY.prepare(
        `SELECT d.verifier AS id, SUM(d.work) AS work, SUM(CASE WHEN d.hour >= ? THEN d.work ELSE 0 END) AS week
          FROM verifier_hours d JOIN verifiers v ON v.id = d.verifier
          WHERE v.banned = 0 AND d.hour >= ? AND d.verifier IN (${marks}) GROUP BY d.verifier`).bind(week, from, ...part),
    ]);
    for (const r of all.results) out.set(r.id, { work: r.work || 0, recent: 0, week: 0, consent_at: r.consent_at || null });
    for (const r of recent.results) if (out.has(r.id)) Object.assign(out.get(r.id), { recent: r.work || 0, week: r.week || 0 });
  }
  return out;
}
const NONE = { work: 0, recent: 0, week: 0, consent_at: null };

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
  if (!env.LIBRARY || !cut.length) return out;
  const from = since(RECENT_DAYS), week = since(WEEK_DAYS);
  for (let i = 0; i < cut.length; i += PER_STATEMENT) {
    const part = cut.slice(i, i + PER_STATEMENT);
    const rows = await env.LIBRARY.batch(part.map(([s]) => env.LIBRARY.prepare(
      `SELECT SUM(work) AS work, SUM(CASE WHEN hour >= ? THEN work ELSE 0 END) AS recent, SUM(CASE WHEN hour >= ? THEN work ELSE 0 END) AS week
         FROM verifier_hours WHERE verifier = ? AND hour >= ? AND (? IS NULL OR hour < ?)`)
      .bind(from, week, s.verifier, s.from_hour, s.until_hour, s.until_hour)));
    part.forEach(([s, k], j) => {
      const r = rows[j].results[0] || {};
      // A REFUSED CLIENT counts for nothing, as `workOf` says by leaving it out.
      out[k] = whole.has(s.verifier) ? { ...out[k], work: r.work || 0, recent: r.recent || 0, week: r.week || 0 } : { ...NONE };
    });
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
  if (!env.LIBRARY || !ids.length) return out;
  const t = Date.now();
  for (let i = 0; i < ids.length; i += PER_STATEMENT) {
    const part = ids.slice(i, i + PER_STATEMENT);
    const marks = part.map(() => "?").join(", ");
    const [seen, held, gains] = await env.LIBRARY.batch([
      env.LIBRARY.prepare(`SELECT id, last_at FROM verifiers WHERE id IN (${marks})`).bind(...part),
      env.LIBRARY.prepare(`SELECT leased_to, record, ruler, mode FROM orders WHERE lease_until > ? AND leased_to IN (${marks})`)
        .bind(iso(t), ...part),
      env.LIBRARY.prepare(`SELECT leased_to, weapon, ruler FROM appraisals WHERE lease_until > ? AND leased_to IN (${marks})`)
        .bind(iso(t), ...part),
    ]);
    for (const r of seen.results) out.set(r.id, { last_at: r.last_at || null, now: null });
    for (const r of held.results) {
      let weapon = null;
      try { weapon = JSON.parse(r.record).weapon || null; } catch (_) { /* a record that is not one names nothing */ }
      const e = out.get(r.leased_to) || { last_at: null, now: null };
      e.now = { kind: "board", weapon, ruler: r.ruler, mode: r.mode };
      out.set(r.leased_to, e);
    }
    for (const r of gains.results) {
      const e = out.get(r.leased_to) || { last_at: null, now: null };
      e.now = { kind: "riven_gain", weapon: r.weapon, ruler: r.ruler };
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

async function devices(env, account) {
  const { results } = await env.ACCOUNTS.prepare(
    "SELECT verifier, claimed_at, label FROM devices WHERE account = ?1 ORDER BY claimed_at").bind(account).all();
  const { results: spans } = await env.ACCOUNTS.prepare(
    "SELECT verifier, from_hour, until_hour FROM device_spans WHERE account = ?1").bind(account).all();
  const ids = results.map((d) => d.verifier);
  const [held, doing] = await Promise.all([spanWork(env, spans), activityOf(env, ids)]);
  // EACH DEVICE'S SHARE IS WHAT ITS SPANS OF THIS ACCOUNT HOLD; a removed one's still counts.
  const work = new Map();
  spans.forEach((s, i) => {
    const e = work.get(s.verifier) || NONE, w = held[i];
    work.set(s.verifier, { work: e.work + w.work, recent: e.recent + w.recent, week: e.week + w.week, consent_at: w.consent_at });
  });
  const current = new Set(ids);
  const gone = [...work].filter(([v]) => !current.has(v)).map(([, w]) => w);
  const choice = await env.ACCOUNTS.prepare("SELECT named FROM contribution_choice WHERE account = ?1").bind(account).first();
  const named = !!(choice && choice.named);
  const everyone = await standings(env);
  const ranks = Object.fromEntries(Object.entries(PERIODS).map(([period, key]) => {
    const at = ordered(everyone, key).findIndex((e) => e.id === account);
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
    devices: list.map(({ work: w, recent: r, week: k, consent_at: _c, ...d }) => ({ ...d, points: points(w), recent: points(r), week: points(k) })) });
}

/// WHAT ONE BROWSER HOLDS, asked by the browser itself: its id is a secret only
/// it holds, so this tells nobody else anything. Its points are what no account
/// holds yet (`unheld`) — all it earned until its first claim, nothing while an
/// account holds it. `claimed` says whether one does, and never which.
async function devicePoints(env, b) {
  if (!VERIFIER_ID.test(b.verifier || "")) return no("bad_device");
  const w = await unheld(env, b.verifier);
  const claimed = w.held;
  // ITS OWN "TODAY": the whole UTC hours from the one the page names as its
  // midnight — results sent, what they took, and the points credited in them.
  let today = null;
  if (/^\d{4}-\d\d-\d\dT\d\d$/.test(b.since || "") && env.LIBRARY) {
    const t = await env.LIBRARY.prepare("SELECT SUM(tasks) AS tasks, SUM(ms) AS ms, SUM(work) AS work FROM verifier_hours WHERE verifier = ? AND hour >= ?")
      .bind(b.verifier, b.since).first();
    today = { tasks: (t && t.tasks) || 0, ms: (t && t.ms) || 0, points: points((t && t.work) || 0) };
  }
  return json({ ok: true, points: points(w.work), recent: points(w.recent), week: points(w.week), claimed, ...(today ? { today } : {}) });
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
    db.prepare("INSERT OR IGNORE INTO device_spans (verifier, account, from_hour) VALUES (?1, ?2, ?3)").bind(v, account, last ? last.until_hour : ""),
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

/// THE THREE RANKINGS, by the column each orders on: all of it, the last
/// `RECENT_DAYS` days, the last `WEEK_DAYS`.
const PERIODS = { all: "points", recent: "recent", week: "week" };

/// EVERY ACCOUNT WITH A CLAIMED DEVICE and what its devices were credited, in
/// points — the one list every ranking and every account's place is cut from.
async function standings(env) {
  const { results } = await env.ACCOUNTS.prepare(
    `SELECT a.id, a.username, a.display_name, c.named, s.verifier, s.from_hour, s.until_hour FROM device_spans s
       JOIN accounts a ON a.id = s.account LEFT JOIN contribution_choice c ON c.account = s.account`).all();
  const work = await spanWork(env, results);
  const by = new Map();
  for (const [i, r] of results.entries()) {
    const e = by.get(r.id) || { id: r.id, named: !!r.named, name: r.display_name || r.username, work: 0, recent: 0, week: 0, ws: [] };
    const w = work[i];
    e.work += w.work;
    e.recent += w.recent;
    e.week += w.week;
    e.ws.push(w);
    by.set(r.id, e);
  }
  return [...by.values()].map((e) => ({ id: e.id, named: e.named, name: e.name, points: points(e.work),
    recent: points(e.recent), week: points(e.week), volunteer: !!volunteerSince(e.ws) }));
}
/// ONE RANKING: everyone with any of `key`, most first.
const ordered = (list, key) => list.filter((e) => e[key] > 0)
  .sort((x, y) => y[key] - x[key] || y.points - x.points || (x.id < y.id ? -1 : 1));

/// HOW MANY BROWSERS ARE COMPUTING NOW: those that answered an order within
/// this long, refused ones aside — a count, never who.
const COMPUTING_MS = 10 * 60_000;
async function computingNow(env) {
  if (!env.LIBRARY) return 0;
  const from = iso(Date.now() - COMPUTING_MS);
  const r = await env.LIBRARY.prepare("SELECT COUNT(*) AS n FROM verifiers WHERE banned = 0 AND last_at >= ?").bind(from).first();
  return (r && r.n) || 0;
}

/// ONE PERSON, by the exact name the ranking shows them under — an account
/// that agreed to be named, the most points first where two share one — with
/// their points and place on each ranking.
async function person(env, name) {
  const everyone = await standings(env);
  const hit = everyone.filter((e) => e.named && e.name === name).sort((x, y) => y.points - x.points)[0];
  if (!hit) return json({ ok: true, computing: await computingNow(env), person: null });
  const ranks = Object.fromEntries(Object.entries(PERIODS).map(([period, key]) => {
    const at = ordered(everyone, key).findIndex((e) => e.id === hit.id);
    return [period, at < 0 ? null : at + 1];
  }));
  const marks = await cloudMarks(env, [hit.id]);
  return json({ ok: true, computing: await computingNow(env), person: { name: hit.name, points: hit.points,
    recent: hit.recent, week: hit.week, ranks, contributor_rank: contributorRank(hit.points), ...(hit.volunteer ? { volunteer: true } : {}),
    ...(marks[hit.id] ? { mark: marks[hit.id] } : {}) } });
}

/// THE RANKING the page shows, its first `RANKED`. A name and a handle leave
/// here only for an account that agreed; the reader's own row is marked `you`,
/// to them alone.
async function ranking(env, period, me) {
  const key = PERIODS[period] || "points";
  const contributors = ordered(await standings(env), key).slice(0, RANKED)
    .map((e) => ({ id: e.id, name: e.named ? e.name : null, points: e.points, recent: e.recent, week: e.week,
      contributor_rank: contributorRank(e.points),
      // THE HONOUR travels with a name only: an anonymous row says nothing more.
      ...(e.named && e.volunteer ? { volunteer: true } : {}) }));
  const marks = await cloudMarks(env, contributors.filter((e) => e.name !== null).map((e) => e.id));
  for (const e of contributors) if (marks[e.id]) e.mark = marks[e.id];
  for (const e of contributors) {
    if (e.id === me) e.you = true;
    delete e.id;
  }
  return json({ ok: true, period: Object.keys(PERIODS).find((p) => PERIODS[p] === key), computing: await computingNow(env), contributors });
}

/// WHAT THE BOARD ASKS FOR AND WHAT IS ANSWERING IT, for the compute page's
/// "Demand and compute" (73-compute.js): owed rows by why, a row asked twice
/// counted as the first of a new build's, a rescore's, an old build's sweep for
/// a ruler it lacks — the riven gains handed
/// to the community in the last day and not done, the scores written in the last
/// hour by the volunteers and by the official machines, and how many browsers
/// are computing. Totals only, kept a minute per isolate.
const DEMAND_KEEP_MS = 60_000;
let demandKept = null;
async function demand(env) {
  if (demandKept && Date.now() - demandKept.at < DEMAND_KEEP_MS) return json(demandKept.body);
  if (!env.LIBRARY) return json({ ok: true, computing: 0, owed: { new_builds: 0, sweeps: 0, rescores: 0 }, riven_gains: 0, surveys: [], per_hour: { volunteers: 0, official: 0 } });
  const t = Date.now(), hour = iso(t - 3_600_000);
  const db = env.LIBRARY;
  const [owed, done, gains, surveys] = await db.batch([
    db.prepare(`SELECT k, COUNT(*) AS n FROM (SELECT MIN(CASE WHEN o.priority = 0 THEN 0 WHEN q.batch LIKE 'rescore%' THEN 1 ELSE 2 END) AS k
                FROM queue q JOIN batches b ON b.id = q.batch
                LEFT JOIN orders o ON o.identity = q.build_id AND o.ruler = q.ruler AND o.mode = q.mode
                GROUP BY q.build_id, q.ruler, q.mode) GROUP BY k`),
    db.prepare(`SELECT SUM(measured_by LIKE 'verified:%') AS volunteers, SUM(measured_by NOT LIKE 'verified:%') AS official
                FROM scores WHERE finished_at >= ?`).bind(hour),
    db.prepare("SELECT COUNT(*) AS n FROM appraisals WHERE done_at IS NULL AND request IS NOT NULL AND at > ? AND channel != ?")
      .bind(iso(t - 86_400_000), SURVEY_CHANNEL),
    // EACH SURVEY AS A GOAL: every riven shape of one weapon on one ruler, how
    // many two owners have agreed on and how many anyone has begun.
    db.prepare(`SELECT weapon, ruler, COUNT(*) AS shapes, SUM(agreed_at IS NOT NULL) AS agreed, SUM(started_at IS NOT NULL) AS started
                FROM appraisals WHERE channel = ? AND request IS NOT NULL AND at > ? GROUP BY weapon, ruler ORDER BY MIN(at), weapon`)
      .bind(SURVEY_CHANNEL, iso(t - SURVEY_KEEP_MS)),
  ]);
  const by = Object.fromEntries(owed.results.map((r) => [r.k, r.n]));
  const h = done.results[0] || {};
  const body = { ok: true, computing: await computingNow(env), owed: { new_builds: by[0] || 0, rescores: by[1] || 0, sweeps: by[2] || 0 },
    riven_gains: (gains.results[0] || {}).n || 0, surveys: surveys.results, per_hour: { volunteers: h.volunteers || 0, official: h.official || 0 } };
  demandKept = { at: t, body };
  return json(body);
}

/// THE HOME HERO'S TALLY: every score the board holds, by who computed it, and
/// the last hour's. Counting the whole table costs a scan, so that count is a
/// BASE kept `TALLY_BASE_MS` at the edge and each answer adds the rows finished
/// since it, read on `scores_oldest`. A row replaced since the base is counted
/// again until the next base; the volunteers' count stays exact either way.
const TALLY_BASE_MS = 10 * 60_000;
const SPLIT = `SUM(measured_by LIKE 'verified:%') AS volunteers, SUM(measured_by NOT LIKE 'verified:%') AS official`;
async function tally(request, env) {
  const cache = typeof caches !== "undefined" ? caches.default : null;
  const key = new Request(new URL("/api/board/tally", request.url).toString());
  const hit = cache && await cache.match(key);
  if (hit) return hit;
  const db = env.LIBRARY;
  const split = (r) => ({ volunteers: (r && r.volunteers) || 0, official: (r && r.official) || 0 });
  const baseKey = new Request(new URL("/api/board/tally/base", request.url).toString());
  let base = cache && await cache.match(baseKey).then((r) => (r ? r.json() : null));
  if (!base && db) {
    const at = iso(Date.now());
    base = { at, ...split(await db.prepare(`SELECT ${SPLIT} FROM scores`).first()) };
    if (cache) await cache.put(baseKey, new Response(JSON.stringify(base),
      { headers: { "content-type": "application/json", "cache-control": `public, max-age=${TALLY_BASE_MS / 1000}` } }));
  }
  let totals = { volunteers: 0, official: 0 }, perHour = { volunteers: 0, official: 0 };
  if (db) {
    const [since, hour] = await db.batch([
      db.prepare(`SELECT ${SPLIT} FROM scores WHERE finished_at > ?`).bind(base.at),
      db.prepare(`SELECT ${SPLIT} FROM scores WHERE finished_at >= ?`).bind(iso(Date.now() - 3_600_000)),
    ]);
    const s = split(since.results[0]);
    totals = { volunteers: base.volunteers + s.volunteers, official: base.official + s.official };
    perHour = split(hour.results[0]);
  }
  const r = new Response(JSON.stringify({ ok: true, totals, per_hour: perHour, computing: await computingNow(env) }),
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
    if (name) return person(env, name);
    return ranking(env, q.get("period"), me);
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
    let b = {};
    try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
    return devicePoints(env, b);
  }
  const posts = { "/api/account/devices/claim": claim, "/api/account/devices/label": relabel,
    "/api/account/devices/remove": release, "/api/account/contribution": choose };
  if (path !== "/api/account/devices" && !posts[path]) return null;
  if (!env.ACCOUNTS || !env.AUTH_SECRET) return no("unavailable", 503);
  const get = path === "/api/account/devices";
  if (request.method !== (get ? "GET" : "POST")) return no("method", 405);
  if (!get && !sameSite(request)) return no("cross_site", 403);
  const account = await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  if (get) return devices(env, account);
  let b = {};
  try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
  return posts[path](env, account, b);
}
