// SPDX-License-Identifier: AGPL-3.0-or-later
// The vote date is authoritative: today's votes are pending, earlier votes are settled.
import { sessionAccount, sameSite, json, no } from "./accounts.js";
import { agentAccount } from "./agents.js";

export const RECOMMENDATION_BATCH_MAX = 200;
const ID = /^[a-f0-9]{32}$/;
const SLUG = /^[a-z0-9_]{1,100}$/;
const CORS = { "access-control-allow-origin": "*", "access-control-allow-methods": "GET, POST, OPTIONS",
  "access-control-allow-headers": "authorization, content-type", "access-control-max-age": "3600" };
const validId = value => typeof value === "string" && ID.test(value);
const clock = async db => {
  const day = db ? (await db.prepare("SELECT recommendation_day()::text AS day").first()).day : new Date().toISOString().slice(0, 10);
  return { day, next_settlement: new Date(Date.parse(day) + 86400000).toISOString() };
};
const result = (body, status = 200) => json(body, status, { "cache-control": "no-store" });
const limited = async (env, key) => env.RECOMMENDATION_LIMIT && !(await env.RECOMMENDATION_LIMIT.limit({ key })).success;
const throttled = () => json({ ok: false, reason: "rate_limited" }, 429, { "retry-after": "60", "cache-control": "no-store" });
const heatMaxAge = time => Math.min(30, Math.max(0, Math.floor((Date.parse(time.next_settlement) - Date.now()) / 1000)));
const points = async (db, account, day) => Number((await db.prepare("SELECT count(*) AS n FROM record_recommendation_votes WHERE account = ? AND day < ?::date").bind(account, day).first()).n);

export async function recommendationExport(env, account) {
  const { results } = await env.ACCOUNTS.prepare(`SELECT r.identity, r.day::text AS day, t.weapon, t.record, t.ruler, t.mode
    FROM record_recommendation_votes r JOIN state_recommendation_target_snapshots t USING (identity) WHERE r.account = ? ORDER BY r.day, r.identity`).bind(account).all();
  return { recommendations: results.map(row => ({ ...row, record: JSON.parse(row.record) })) };
}

export async function recommendationsRoute(request, env, path, ctx) {
  const response = await recommendationRoute(request, env, path, ctx);
  if (!response) return null;
  const headers = new Headers(response.headers);
  for (const [key, value] of Object.entries(CORS)) headers.set(key, value);
  return new Response(response.body, { status: response.status, headers });
}

async function recommendationRoute(request, env, path, ctx) {
  const heat = path.match(/^\/api\/recommendations\/heat\/([a-z0-9_]{1,100})$/);
  if (!heat && !["/api/recommendations", "/api/recommendations/state", "/api/account/recommendations", "/api/account/recommendations/votes"].includes(path)) return null;
  if (request.method === "OPTIONS") return new Response(null, { status: 204 });
  if (!env.ACCOUNTS) return no("unavailable", 503);
  let time = await clock();
  const db = env.ACCOUNTS;
  if (heat) {
    if (request.method !== "GET") return no("method", 405);
    const cache = typeof caches !== "undefined" ? caches.default : null;
    const key = new Request(new URL(path + "?day=" + time.day, request.url));
    const cached = cache && await cache.match(key);
    if (cached) {
      const headers = new Headers(cached.headers);
      headers.set("cache-control", "public, max-age=" + heatMaxAge(time));
      return new Response(cached.body, { status: cached.status, headers });
    }
    if (await limited(env, "heat:" + (request.headers.get("cf-connecting-ip") || "unknown"))) return throttled();
    time = await clock(db);
    const { results } = await db.prepare(`SELECT r.identity, sum(11 - (?::date - r.day)) AS tenths
      FROM state_recommendation_target_snapshots t JOIN record_recommendation_votes r USING (identity)
      WHERE t.weapon = ? AND r.day >= ?::date - 10 AND r.day < ?::date GROUP BY r.identity`)
      .bind(time.day, heat[1], time.day, time.day).all();
    const maxAge = heatMaxAge(time);
    const response = json({ ok: true, ...time, heat: Object.fromEntries(results.map(row => [row.identity, Number(row.tenths) / 10])) }, 200,
      { "cache-control": "public, max-age=" + maxAge });
    if (cache && ctx && maxAge > 0) ctx.waitUntil(cache.put(key, response.clone()));
    return response;
  }
  if (await limited(env, "address:" + (request.headers.get("cf-connecting-ip") || "unknown"))) return throttled();
  const bearer = !!request.headers.get("authorization");
  if (request.method === "POST" && !bearer && !sameSite(request)) return no("cross_site", 403);
  const account = bearer ? await agentAccount(env, request) : await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  if (await limited(env, "account:" + account)) return throttled();
  time = await clock(db);
  if (path === "/api/account/recommendations/votes") {
    if (request.method !== "GET") return no("method", 405);
    const query = new URL(request.url).searchParams, identity = query.get("identity"), before = query.get("before") || "";
    const limit = Number(query.get("limit") || 100);
    if (!Number.isInteger(limit) || limit < 1 || limit > 100 || !validId(identity) || before && (before < "0001-01-01" || !/^\d{4}-\d\d-\d\d$/.test(before) || !Number.isFinite(Date.parse(before)) || new Date(before).toISOString().slice(0, 10) !== before)) return no("bad_query");
    const { results } = await db.prepare(`SELECT day::text AS day, greatest(0, 11 - (?2::date - day)) AS tenths
      FROM record_recommendation_votes WHERE account = ?1 AND identity = ?3 AND (?4::text = '' OR day < NULLIF(?4::text, '')::date)
      ORDER BY day DESC LIMIT ?5::bigint`).bind(account, time.day, identity, before, limit).all();
    return result({ ok: true, ...time, votes: results.map(row => ({ day: row.day, weight: row.day === time.day ? null : Number(row.tenths) / 10 })),
      before: results.length === limit ? results.at(-1).day : null });
  }
  if (path === "/api/account/recommendations") {
    if (request.method !== "GET") return no("method", 405);
    const query = new URL(request.url).searchParams;
    const filter = query.get("filter") || "available", weapon = query.get("weapon") || "";
    const offset = Number(query.get("offset") || 0);
    const limit = Number(query.get("limit") || 50);
    if (!Number.isInteger(limit) || limit < 1 || limit > 50 || !["all", "available", "pending"].includes(filter) || weapon && !SLUG.test(weapon) || !Number.isSafeInteger(offset) || offset < 0 || offset > 1000000) return no("bad_query");
    const grouped = `SELECT identity, max(day) AS last_day, count(*) FILTER (WHERE day < ?2::date) AS points,
      bool_or(day = ?2::date) AS pending FROM record_recommendation_votes WHERE account = ?1 GROUP BY identity`;
    const [totals, page, count] = await Promise.all([
      db.prepare(`WITH own AS (${grouped}) SELECT count(*) AS total, count(*) FILTER (WHERE pending) AS pending,
        COALESCE(sum(points), 0) AS points, string_agg(DISTINCT t.weapon, ',') AS weapons
        FROM own JOIN state_recommendation_target_snapshots t USING (identity)`).bind(account, time.day).first(),
      db.prepare(`WITH own AS (${grouped}) SELECT t.*, own.last_day::text, own.points, own.pending
        FROM own JOIN state_recommendation_target_snapshots t USING (identity)
        WHERE (?3::text = '' OR t.weapon = ?3::text) AND (?4::text = 'all' OR own.pending = (?4::text = 'pending'))
        ORDER BY own.pending, own.last_day DESC, own.identity LIMIT ?6::bigint OFFSET ?5`)
        .bind(account, time.day, weapon, filter, offset, limit).all(),
      db.prepare(`WITH own AS (${grouped}) SELECT count(*) AS n FROM own JOIN state_recommendation_target_snapshots t USING (identity)
        WHERE (?3::text = '' OR t.weapon = ?3::text) AND (?4::text = 'all' OR own.pending = (?4::text = 'pending'))`)
        .bind(account, time.day, weapon, filter).first(),
    ]);
    const ids = page.results.map(row => row.identity);
    const measured = ids.length && env.LIBRARY ? (await env.LIBRARY.prepare(`SELECT s.identity, s.ruler, s.mode, s.score, s.metric, s.finished_at
      FROM board_facts s JOIN jsonb_to_recordset((?::text)::jsonb) AS r(identity text, ruler text, mode text) USING (identity, ruler, mode)
      WHERE s.retired_at IS NULL`)
      .bind(JSON.stringify(page.results.map(({ identity, ruler, mode }) => ({ identity, ruler, mode })))).all()).results : [];
    const recent = ids.length ? (await db.prepare(`SELECT identity, day::text AS day, greatest(0, 11 - (?2::date - day)) AS tenths
      FROM record_recommendation_votes WHERE account = ?1 AND identity = ANY((?3::text)::text[]) AND day >= ?2::date - 10 ORDER BY day DESC`)
      .bind(account, time.day, "{" + ids.join(",") + "}").all()).results : [];
    return result({ ok: true, ...time, points: Number(totals.points), weapons: (totals.weapons || "").split(",").filter(Boolean), total: Number(totals.total), pending: Number(totals.pending),
      available: Number(totals.total) - Number(totals.pending), count: Number(count.n), next: offset + page.results.length < Number(count.n) ? offset + page.results.length : null,
      builds: page.results.map(row => ({ ...row, record: JSON.parse(row.record), points: Number(row.points),
        measurement: measured.find(score => score.identity === row.identity && score.ruler === row.ruler && score.mode === row.mode) || null,
        votes: recent.filter(vote => vote.identity === row.identity).map(vote => ({ day: vote.day, weight: vote.day === time.day ? null : Number(vote.tenths) / 10 })) })) });
  }
  if (request.method !== "POST") return no("method", 405);
  if (Number(request.headers.get("content-length") || 0) > 10000) return no("too_large", 413);
  const reader = request.body?.getReader();
  let raw = "", bytes = 0;
  const decoder = new TextDecoder();
  if (reader) for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    bytes += value.length;
    if (bytes > 10000) { await reader.cancel(); return no("too_large", 413); }
    raw += decoder.decode(value, { stream: true });
  }
  raw += decoder.decode();
  let body;
  try { body = JSON.parse(raw); } catch { return no("not_json"); }
  if (!body || typeof body !== "object" || Array.isArray(body)) return no("bad_vote");
  if (path === "/api/recommendations/state") {
    if (!Array.isArray(body.identities) || body.identities.length > RECOMMENDATION_BATCH_MAX || !body.identities.every(validId)) return no("bad_identities");
    const { results } = await db.prepare("SELECT identity FROM record_recommendation_votes WHERE account = ? AND day = ?::date AND identity = ANY((?::text)::text[])")
      .bind(account, time.day, "{" + body.identities.join(",") + "}").all();
    return result({ ok: true, ...time, points: await points(db, account, time.day), pending: results.map(row => row.identity) });
  }
  if (!validId(body.identity) || !["recommend", "withdraw"].includes(body.action)) return no("bad_vote");
  if (body.day !== time.day) return result({ ok: false, reason: "day_changed", ...time }, 409);
  if (body.action === "withdraw") {
    await db.prepare("DELETE FROM record_recommendation_votes WHERE account = ? AND identity = ? AND day = ?::date AND day = recommendation_day()")
      .bind(account, body.identity, time.day).run();
  } else {
    let target = await db.prepare("SELECT identity FROM state_recommendation_target_snapshots WHERE identity = ?").bind(body.identity).first();
    if (!target) {
      if (!env.LIBRARY || typeof body.ruler !== "string" || typeof body.mode !== "string" || !SLUG.test(body.ruler) || !SLUG.test(body.mode)) return no("bad_target");
      const build = await env.LIBRARY.prepare(`SELECT b.record, b.weapon FROM entity_weapon_builds b WHERE b.id = ?
        AND EXISTS (SELECT 1 FROM board_facts s WHERE s.identity = b.id AND s.ruler = ? AND s.mode = ? AND s.retired_at IS NULL)`)
        .bind(body.identity, body.ruler, body.mode).first();
      if (!build) return no("unknown_build", 404);
      await db.prepare(`INSERT INTO state_recommendation_target_snapshots (identity, weapon, record, ruler, mode) VALUES (?, ?, ?, ?, ?)
        ON CONFLICT DO NOTHING`).bind(body.identity, build.weapon, build.record, body.ruler, body.mode).run();
    }
    await db.prepare("INSERT INTO record_recommendation_votes (account, identity, day) SELECT ?, ?, ?::date WHERE ?::date = recommendation_day() ON CONFLICT DO NOTHING")
      .bind(account, body.identity, time.day, time.day).run();
  }
  const after = await clock(db);
  if (after.day !== time.day) return result({ ok: false, reason: "day_changed", ...after }, 409);
  return result({ ok: true, ...time, identity: body.identity, pending: body.action === "recommend" });
}
