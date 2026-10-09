// SPDX-License-Identifier: AGPL-3.0-or-later
// THE COMPUTE A READER'S MACHINES GIVE THE BOARD, counted under their name —
// docs/BOARD.md §"Contribution". A signed-in page claims the browser it runs
// in (its verifier id, `69-board-work.js`); the work every fact credited to that
// id (`verifiers.work`, worker/verify.js) is then the account's. Only the owner
// is joined to a device, never a submission. Every account with a claimed
// device is on the public ranking, ANONYMOUS until it agrees to show its name
// (`contribution_choice`): publishing a name is the person's choice, asked once.
//
//   GET  /api/account/devices        → { devices: [{ id, label, claimed_at, points, recent, week, last_at, now }], points, recent, week, ranks, named, decided, volunteer }
//   POST /api/account/devices/claim  { verifier, label? } → { ok }
//   POST /api/account/devices/label  { id, label }        → { ok }
//   POST /api/account/devices/remove { id }               → { ok }
//   POST /api/account/contribution   { named }    → { ok }
//   POST /api/board/points           { verifier } → { points, recent, claimed }
//   GET  /api/contributors[?period=recent|week] → { period, contributors: [{ name, points, recent, week, mark?, volunteer?, you? }] }
//        — `name` (the display name, else the username) is null for an account
//        that did not agree, and `mark` is the paid half's, for a named one only.
//
// `recent` is the last `RECENT_DAYS` days and `week` the last `WEEK_DAYS`, so a
// newcomer can lead somewhere; `ranks` is the account's place on each of the
// three, for the showcase a share card draws (`31-share-card.js`).
import { json, no, now, sameSite, sessionAccount } from "./accounts.js";
import { cloudMarks } from "./cloud.js";

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
        `SELECT d.verifier AS id, SUM(d.work) AS work, SUM(CASE WHEN d.day >= ? THEN d.work ELSE 0 END) AS week
          FROM verifier_days d JOIN verifiers v ON v.id = d.verifier
          WHERE v.banned = 0 AND d.day >= ? AND d.verifier IN (${marks}) GROUP BY d.verifier`).bind(week, from, ...part),
    ]);
    for (const r of all.results) out.set(r.id, { work: r.work || 0, recent: 0, week: 0, consent_at: r.consent_at || null });
    for (const r of recent.results) if (out.has(r.id)) Object.assign(out.get(r.id), { recent: r.work || 0, week: r.week || 0 });
  }
  return out;
}
const NONE = { work: 0, recent: 0, week: 0, consent_at: null };

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
        .bind(t, ...part),
      env.LIBRARY.prepare(`SELECT leased_to, weapon, ruler FROM appraisals WHERE lease_until > ? AND leased_to IN (${marks})`)
        .bind(t, ...part),
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
  const ids = results.map((d) => d.verifier);
  const [work, doing] = await Promise.all([workOf(env, ids), activityOf(env, ids)]);
  const choice = await env.ACCOUNTS.prepare("SELECT named FROM contribution_choice WHERE account = ?1").bind(account).first();
  const named = !!(choice && choice.named);
  const everyone = await standings(env);
  const ranks = Object.fromEntries(Object.entries(PERIODS).map(([period, key]) => {
    const at = ordered(everyone, key).findIndex((e) => e.id === account);
    return [period, at < 0 ? null : at + 1];
  }));
  const list = results.map((d) => ({ id: d.verifier.slice(0, 6), label: d.label || null, claimed_at: d.claimed_at,
    ...(doing.get(d.verifier) || { last_at: null, now: null }), ...(work.get(d.verifier) || NONE) }));
  // `shown` is `named` for a page from before the ranking was anonymous.
  return json({ ok: true, named, decided: !!choice, shown: named, volunteer: volunteerSince(list),
    points: points(list.reduce((s, d) => s + d.work, 0)), recent: points(list.reduce((s, d) => s + d.recent, 0)),
    week: points(list.reduce((s, d) => s + d.week, 0)), ranks,
    devices: list.map(({ work: w, recent: r, week: k, consent_at: _c, ...d }) => ({ ...d, points: points(w), recent: points(r), week: points(k) })) });
}

/// WHAT ONE BROWSER HAS EARNED, asked by the browser itself: its id is a secret
/// only it holds, so this tells nobody else anything. `claimed` says whether
/// an account owns it, and never which.
async function devicePoints(env, b) {
  if (!VERIFIER_ID.test(b.verifier || "")) return no("bad_device");
  const w = (await workOf(env, [b.verifier])).get(b.verifier) || NONE;
  const claimed = !!(env.ACCOUNTS && await env.ACCOUNTS.prepare("SELECT 1 FROM devices WHERE verifier = ?1").bind(b.verifier).first());
  return json({ ok: true, points: points(w.work), recent: points(w.recent), week: points(w.week), claimed });
}

/// A DEVICE BELONGS TO THE LAST ACCOUNT TO CLAIM IT, and its work goes with it:
/// the id is a secret only that browser holds, so whoever sends it is at it.
async function claim(env, account, b) {
  if (!VERIFIER_ID.test(b.verifier || "")) return no("bad_device");
  await env.ACCOUNTS.prepare(
    `INSERT INTO devices (verifier, account, claimed_at, label) VALUES (?1, ?2, ?3, ?4)
     ON CONFLICT (verifier) DO UPDATE SET account = ?2, claimed_at = ?3, label = ?4 WHERE account != ?2`,
  ).bind(b.verifier, account, now().slice(0, 10), labelOf(b.label)).run();
  return json({ ok: true });
}

/// WHAT THE OWNER CALLS IT, or — removed — not theirs any more: its work leaves
/// the account with it, and the browser counts for nobody until it is claimed.
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
     ON CONFLICT (account) DO UPDATE SET named = ?2, chosen_at = ?3`).bind(account, named ? 1 : 0, now().slice(0, 10)).run();
  return json({ ok: true });
}

/// THE THREE RANKINGS, by the column each orders on: all of it, the last
/// `RECENT_DAYS` days, the last `WEEK_DAYS`.
const PERIODS = { all: "points", recent: "recent", week: "week" };

/// EVERY ACCOUNT WITH A CLAIMED DEVICE and what its devices were credited, in
/// points — the one list every ranking and every account's place is cut from.
async function standings(env) {
  const { results } = await env.ACCOUNTS.prepare(
    `SELECT a.id, a.username, a.display_name, c.named, d.verifier FROM devices d JOIN accounts a ON a.id = d.account
       LEFT JOIN contribution_choice c ON c.account = d.account`).all();
  const work = await workOf(env, results.map((r) => r.verifier));
  const by = new Map();
  for (const r of results) {
    const e = by.get(r.id) || { id: r.id, named: !!r.named, name: r.display_name || r.username, work: 0, recent: 0, week: 0, ws: [] };
    const w = work.get(r.verifier) || NONE;
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

/// THE RANKING the page shows, its first `RANKED`. A name and a handle leave
/// here only for an account that agreed; the reader's own row is marked `you`,
/// to them alone.
async function ranking(env, period, me) {
  const key = PERIODS[period] || "points";
  const contributors = ordered(await standings(env), key).slice(0, RANKED)
    .map((e) => ({ id: e.id, name: e.named ? e.name : null, points: e.points, recent: e.recent, week: e.week,
      // THE HONOUR travels with a name only: an anonymous row says nothing more.
      ...(e.named && e.volunteer ? { volunteer: true } : {}) }));
  const marks = await cloudMarks(env, contributors.filter((e) => e.name !== null).map((e) => e.id));
  for (const e of contributors) if (marks[e.id]) e.mark = marks[e.id];
  for (const e of contributors) {
    if (e.id === me) e.you = true;
    delete e.id;
  }
  return json({ ok: true, period: Object.keys(PERIODS).find((p) => PERIODS[p] === key), contributors });
}

/// The response for a contribution path, or null for a path that is not one.
export async function contributionRoute(request, env, path) {
  if (path === "/api/contributors") {
    if (request.method !== "GET") return no("method", 405);
    if (!env.ACCOUNTS) return json({ ok: true, contributors: [] });
    const me = env.AUTH_SECRET ? await sessionAccount(env, request) : null;
    return ranking(env, new URL(request.url).searchParams.get("period"), me);
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
