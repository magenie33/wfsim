// ---- USAGE DAYS ---------------------------------------------------------------
//
// docs/ANALYTICS.md §Retention. The usage log keeps 90 days, so once a day the
// worker's cron reads each finished UTC day it has not yet kept and writes that
// day's TOTALS into `usage_days` (worker/pg/library.sql). Totals only: a visitor
// never leaves the log. THE LOG IS THE ONLY SOURCE: a day it does not hold whole
// is never written, since a day kept short is a dip that never happened.
//
// IT FILLS EVERY MISSING DAY STILL IN THE LOG, not just yesterday, so a missed
// run is made up by the next. A day is written once and never revised: its
// numbers are final when it ends.

import { purgeUsageLog } from "./usage_log.js";

/// A RESULT, as opposed to a page opened. `scripts/usage.py` holds the same list
/// and `check_usage_events` holds the two equal: a day kept under another
/// definition is a trend that moves for no reason.
export const USAGE_RESULTS = ["builder.weapon", "builder.warframe", "builder.operator", "builder.riven",
  "simulator.run", "optimizer.run"];
/// A WEAPON TESTED: a simulation or a search that finished on it. Kept per
/// weapon as one visitor count, so a reader who did both on one day is ONE —
/// the popularity ranking reads it (`worker/popularity.js`).
export const USAGE_TESTED = ["simulator.run", "optimizer.run"];
/// THE LOG'S FIRST WHOLE DAY: its first point is 2026-10-15T06:05Z, so that day
/// holds only its last eighteen hours.
export const USAGE_LOG_FROM = "2026-10-16";
const CHECK_RELEASE = "deploy-check";
/// "Returning" is a visitor seen on any of the seven days before.
const RETURN_DAYS = 7;
/// A RUN'S SHARE OF THE BACKLOG: a run after a long outage stays short, and the
/// next run takes the rest.
const DAYS_PER_RUN = 10;
/// ONE STATEMENT A BATCH: each statement is its own round trip, and a day sent
/// a row at a time outlived the cron.
const BATCH_ROWS = 500;

const DAY_MS = 86400000;
const dayOf = (ms) => new Date(ms).toISOString().slice(0, 10);
const market = (country) => (country === "CN" ? "china" : "overseas");

/// A DAY'S ROWS FROM THE LOG — every point, so `n` is a count.
async function logRows(env, from, next) {
  const { results } = await env.LIBRARY.prepare(`
    SELECT day, visitor AS cid, event AS e, subject, country, COUNT(*) AS n
    FROM usage_points WHERE day >= ? AND day < ? AND release != ?
    GROUP BY day, visitor, event, subject, country`).bind(from, next, CHECK_RELEASE).all();
  return results;
}

/// ONE DAY'S ROWS, as `usage_days` holds them: per (event, subject, market) the
/// visitors and the points; and the derived visitor counts under event
/// `visitors` — `all`, `result`, `returning` — `visitors.country`, and per
/// weapon `tested`.
export function usageDayRows(day, rows) {
  const before = new Set(), today = new Map();
  const out = new Map();
  const add = (event, subject, mkt, cid, points) => {
    const k = `${event}\n${subject}\n${mkt}`;
    const o = out.get(k) || { event, subject, market: mkt, cids: new Set(), points: 0 };
    o.cids.add(cid);
    o.points += points;
    out.set(k, o);
  };
  for (const r of rows) {
    if (r.day.slice(0, 10) !== day) { before.add(r.cid); continue; }
    const mkt = market(r.country);
    add(r.e, r.subject || "", mkt, r.cid, Number(r.n));
    if (USAGE_TESTED.includes(r.e) && r.subject) add("tested", r.subject, mkt, r.cid, Number(r.n));
    const v = today.get(r.cid) || { mkt, country: r.country || "", result: false };
    if (USAGE_RESULTS.includes(r.e)) v.result = true;
    today.set(r.cid, v);
  }
  for (const [cid, v] of today) {
    add("visitors", "all", v.mkt, cid, 0);
    if (v.result) add("visitors", "result", v.mkt, cid, 0);
    if (before.has(cid)) add("visitors", "returning", v.mkt, cid, 0);
    add("visitors.country", v.country, v.mkt, cid, 0);
  }
  const last = (o) => (o.event === "visitors" && o.subject === "all" ? 1 : 0);
  return [...out.values()].sort((a, b) => last(a) - last(b)).map((o) => ({ day, event: o.event, subject: o.subject, market: o.market,
    visitors: o.cids.size, points: Math.round(o.points), sampled: 0 }));
}

/// KEEP EVERY FINISHED DAY THE TABLE LACKS, then drop the log past its time.
/// Returns the days written.
export async function rollupUsage(env, now = Date.now()) {
  if (!env.LIBRARY) return [];
  const written = await keepDays(env, now);
  await purgeUsageLog(env, now);
  return written;
}

async function keepDays(env, now) {
  const today = dayOf(now);
  // A DAY IS KEPT ONCE ITS `visitors`/`all` ROW IS: written last, so a run that
  // stopped part way leaves the day to be written again whole.
  const kept = new Set((await env.LIBRARY.prepare(
    "SELECT day FROM usage_days WHERE event = 'visitors' AND subject = 'all'").all()).results.map((r) => r.day));
  const logged = (await env.LIBRARY.prepare("SELECT DISTINCT day FROM usage_points WHERE day >= ?")
    .bind(USAGE_LOG_FROM).all()).results.map((r) => r.day);
  const days = logged.filter((d) => d < today && !kept.has(d)).sort().slice(0, DAYS_PER_RUN);
  const written = [];
  for (const day of days) {
    const from = dayOf(Date.parse(day) - RETURN_DAYS * DAY_MS);
    const next = dayOf(Date.parse(day) + DAY_MS);
    const out = usageDayRows(day, await logRows(env, from, next));
    if (!out.length) continue;
    const put = (part) => env.LIBRARY.prepare(`INSERT INTO usage_days
      (day, event, subject, market, visitors, points, sampled) VALUES ${part.map(() => "(?, ?, ?, ?, ?, ?, ?)").join(", ")}
      ON CONFLICT (day, event, subject, market) DO UPDATE SET
        visitors = excluded.visitors, points = excluded.points, sampled = excluded.sampled`)
      .bind(...part.flatMap((o) => [o.day, o.event, o.subject, o.market, o.visitors, o.points, o.sampled]));
    for (let i = 0; i < out.length; i += BATCH_ROWS) await put(out.slice(i, i + BATCH_ROWS)).run();
    written.push(day);
  }
  return written;
}
