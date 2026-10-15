// ---- USAGE DAYS ---------------------------------------------------------------
//
// docs/ANALYTICS.md §Retention. The usage log keeps 90 days, so once a day the
// worker's cron reads each finished UTC day it has not yet kept and writes that
// day's TOTALS into `usage_days` (worker/pg/library.sql). Totals only: a visitor
// never leaves the log. A day before `USAGE_LOG_FROM` is read from the Analytics
// Engine instead, sampled, and kept with `sampled` set.
//
// IT FILLS EVERY MISSING DAY STILL IN THE DATASET, not just yesterday, so the
// first runs keep the history there is and a missed run is made up by the next.
// A day is written once and never revised: its numbers are final when it ends.

import { usageVisitor, purgeUsageLog } from "./usage_log.js";

/// A RESULT, as opposed to a page opened. `scripts/usage.py` holds the same list
/// and `check_usage_events` holds the two equal: a day kept under another
/// definition is a trend that moves for no reason.
export const USAGE_RESULTS = ["builder.weapon", "builder.warframe", "builder.operator", "builder.riven",
  "simulator.run", "optimizer.run"];
/// A WEAPON TESTED: a simulation or a search that finished on it. Kept per
/// weapon as one visitor count, so a reader who did both on one day is ONE —
/// the popularity ranking reads it (`worker/popularity.js`).
export const USAGE_TESTED = ["simulator.run", "optimizer.run"];
const DATASET = "wfsim";
/// THE FIRST DAY READ FROM THE LOG: the log began on 2026-10-16, and a day's
/// `returning` reads the seven days before it, so the first day whose seven are
/// all in the log is the first it can count whole.
export const USAGE_LOG_FROM = "2026-10-23";
const CHECK_RELEASE = "deploy-check";
/// The dataset keeps ~92 days; a day older than this may be cut short.
const KEEP_BACK_DAYS = 85;
/// "Returning" is a visitor seen on any of the seven days before.
const RETURN_DAYS = 7;
/// A RUN'S SHARE OF THE BACKLOG: two queries a day keeps a run far inside the
/// worker's subrequest limit, and the next run takes the rest.
const DAYS_PER_RUN = 10;
/// ONE STATEMENT A BATCH: each statement is its own round trip, and a day sent
/// a row at a time outlived the cron.
const BATCH_ROWS = 500;

const DAY_MS = 86400000;
const dayOf = (ms) => new Date(ms).toISOString().slice(0, 10);
const market = (country) => (country === "CN" ? "china" : "overseas");

/// A DAY'S ROWS FROM THE LOG — every point, so `n` is a count and nothing is sampled.
async function logRows(env, from, next) {
  const { results } = await env.LIBRARY.prepare(`
    SELECT day, visitor AS cid, event AS e, subject, country, 1 AS si, COUNT(*) AS n
    FROM usage_points WHERE day >= ? AND day < ? AND release != ?
    GROUP BY day, visitor, event, subject, country`).bind(from, next, CHECK_RELEASE).all();
  return results;
}

/// A DAY'S ROWS FROM THE ANALYTICS ENGINE. A schema 1 point carried the
/// browser's id itself; it is hashed here, so a visitor is one visitor across
/// the change and across the two sources.
async function datasetRows(env, from, next) {
  const rows = await usageSql(env, `
      SELECT toStartOfInterval(timestamp, INTERVAL '1' DAY) AS day, blob2 AS cid, blob1 AS e,
             blob3 AS subject, blob8 AS country, double1 AS schema, max(_sample_interval) AS si, SUM(_sample_interval) AS n
      FROM ${DATASET} WHERE timestamp >= toDateTime('${from} 00:00:00')
        AND timestamp < toDateTime('${next} 00:00:00') AND blob7 != '${CHECK_RELEASE}'
      GROUP BY day, cid, e, subject, country, schema LIMIT 1000000`);
  const hashed = new Map();
  for (const r of rows) {
    if (Number(r.schema) !== 1) continue;
    if (!hashed.has(r.cid)) hashed.set(r.cid, await usageVisitor(r.cid));
    r.cid = hashed.get(r.cid);
  }
  return rows;
}

async function usageSql(env, query) {
  const r = await fetch(
    `https://api.cloudflare.com/client/v4/accounts/${env.USAGE_ACCOUNT}/analytics_engine/sql`,
    { method: "POST", body: `${query} FORMAT JSON`, headers: { authorization: `Bearer ${env.USAGE_READ_TOKEN}` } });
  if (!r.ok) throw new Error(`analytics sql ${r.status}: ${(await r.text()).slice(0, 300)}`);
  return (await r.json()).data;
}

/// ONE DAY'S ROWS, as `usage_days` holds them: per (event, subject, market) the
/// visitors and the points; and the derived visitor counts under event
/// `visitors` — `all`, `result`, `returning` — `visitors.country`, and per
/// weapon `tested`.
export function usageDayRows(day, rows) {
  const before = new Set(), today = new Map(), sampled = { any: false };
  const out = new Map();
  const add = (event, subject, mkt, cid, points) => {
    const k = `${event}\n${subject}\n${mkt}`;
    const o = out.get(k) || { event, subject, market: mkt, cids: new Set(), points: 0 };
    o.cids.add(cid);
    o.points += points;
    out.set(k, o);
  };
  for (const r of rows) {
    if (Number(r.si) > 1) sampled.any = true;
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
    visitors: o.cids.size, points: Math.round(o.points), sampled: sampled.any ? 1 : 0 }));
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
  const dataset = !!(env.USAGE_ACCOUNT && env.USAGE_READ_TOKEN);
  // A DAY IS KEPT ONCE ITS `visitors`/`all` ROW IS: written last, so a run that
  // stopped part way leaves the day to be written again whole.
  const kept = new Set((await env.LIBRARY.prepare(
    "SELECT day FROM usage_days WHERE event = 'visitors' AND subject = 'all'").all()).results.map((r) => r.day));
  // ONLY DAYS A SOURCE HAS: one before it began is not a zero, and asking for
  // each of them would spend the run's subrequests on nothing.
  const logged = (await env.LIBRARY.prepare("SELECT DISTINCT day FROM usage_points WHERE day >= ?")
    .bind(USAGE_LOG_FROM).all()).results.map((r) => r.day);
  const held = !dataset ? [] : (await usageSql(env, `
    SELECT toStartOfInterval(timestamp, INTERVAL '1' DAY) AS day FROM ${DATASET}
    WHERE timestamp > NOW() - INTERVAL '${KEEP_BACK_DAYS}' DAY GROUP BY day`))
    .map((r) => r.day.slice(0, 10)).filter((d) => d < USAGE_LOG_FROM);
  const days = [...new Set([...held, ...logged])].filter((d) => d < today && !kept.has(d)).sort().slice(0, DAYS_PER_RUN);
  const written = [];
  for (const day of days) {
    const from = dayOf(Date.parse(day) - RETURN_DAYS * DAY_MS);
    const next = dayOf(Date.parse(day) + DAY_MS);
    const rows = await (day >= USAGE_LOG_FROM ? logRows : datasetRows)(env, from, next);
    const out = usageDayRows(day, rows);
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
