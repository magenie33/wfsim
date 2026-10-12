// SPDX-License-Identifier: AGPL-3.0-or-later
// THE POPULARITY RANKING — docs/ANALYTICS.md §"Shown publicly". Per weapon, how
// many people finished a simulation or a search on it, each once a day whichever
// they ran (`tested` in `usage_days`), added up over the last 30 finished days.
// Totals the cron already keeps; nothing new is collected, and no visitor is in it.

const DAYS = 30;
const EVENTS = ["tested"];

const reply = (body, status, maxAge) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8",
    "cache-control": maxAge ? `public, max-age=${maxAge}` : "no-store" } });

export async function popularity(request, env, ctx) {
  if (request.method !== "GET") return reply({ ok: false, error: "GET only" }, 405);
  if (!env.LIBRARY) return reply({ ok: false, error: "not offered here" }, 501);
  // ONE ANSWER AN HOUR at each edge: the totals move once a day.
  const key = new Request(new URL("/api/popularity", request.url).toString());
  const hit = await caches.default.match(key);
  if (hit) return hit;
  // THE WINDOW ENDS AT THE NEWEST FINISHED DAY, not today: a day is written
  // after midnight UTC, so "today" would always be an empty day in the sum.
  // The window's first day is reckoned here, so the one query reads alike in
  // SQLite and in Postgres.
  const last = (await env.LIBRARY.prepare("SELECT MAX(day) AS d FROM usage_days WHERE event = 'visitors' AND subject = 'all'").first() || {}).d;
  const after = last ? new Date(Date.parse(`${last}T00:00:00Z`) - DAYS * 86_400_000).toISOString().slice(0, 10) : null;
  const { results } = after ? await env.LIBRARY.prepare(
    `SELECT subject, SUM(visitors) AS n, MIN(day) AS first FROM usage_days
     WHERE event IN (${EVENTS.map(() => "?").join(",")}) AND day > ? GROUP BY subject`).bind(...EVENTS, after).all() : { results: [] };
  const scores = Object.fromEntries(results.filter((r) => r.n > 0).map((r) => [r.subject, r.n]));
  const days = results.map((r) => r.first).sort();
  const res = reply({ ok: true, window_days: DAYS, from: days[0] || null, to: results.length ? last : null,
    events: EVENTS, scores }, 200, 3600);
  ctx.waitUntil(caches.default.put(key, res.clone()));
  return res;
}
