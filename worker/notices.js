// SPDX-License-Identifier: AGPL-3.0-or-later
// WHAT THE SITE TELLS A READER — docs/ACCOUNTS.md §"Notices". One way: the site
// writes, an account reads; no reader writes to another.
//
//   GET  /api/notices/everyone   what is said to every reader; cached, no cookie read
//   GET  /api/notices            that, what is said to this account, and its state
//   POST /api/notices/read       {at}: everything made before `at` is read
//
// A NOTICE is stored (`notices`): one the owner wrote (`announcement` to all,
// `message` to one) or one a feature wrote when something happened. A STATE is
// never stored: it is read from the account as it is now, so it can never say
// something that stopped being true.
import { sessionAccount, sameSite, json, no } from "./accounts.js";

/// The kinds a writer may store, each worded by the page from its `params`
/// unless it carries its own text. A new kind is a word here and on the page.
export const NOTICE_KINDS = ["announcement", "message", "membership_ending", "membership_ended", "membership_renewal_failed"];
/// What a reader is shown at most; older ones are still in the export.
const NOTICES_SHOWN = 50;
/// A notice to one account is kept this long, then purged (`purgeNotices`).
const NOTICE_KEEP_MS = 180 * 86400_000;
/// The edge keeps what every reader is told this long.
const EVERYONE_CACHE_S = 300;

const iso = (ms) => new Date(ms).toISOString();
const shape = (r) => ({ id: Number(r.id), kind: r.kind, audience: r.account ? "you" : "everyone",
  text: r.text_en || r.text_zh ? { en: r.text_en || r.text_zh, zh: r.text_zh || r.text_en } : null,
  params: r.params ? JSON.parse(r.params) : null, link: r.link || null, created_at: r.created_at, expires_at: r.expires_at || null });

/// WHAT IS SAID TO EVERY READER now: not expired, newest first.
async function everyone(db, now) {
  const { results } = await db.prepare(`SELECT * FROM notices WHERE account IS NULL
      AND created_at > ?1 AND (expires_at IS NULL OR expires_at > ?2) ORDER BY created_at DESC LIMIT ?3`)
    .bind(iso(now - NOTICE_KEEP_MS), iso(now), NOTICES_SHOWN).all();
  return results.map(shape);
}

/// THE ACCOUNT'S STATE a reader is told about while it lasts. A device in its
/// cool-down (docs/BOARD.md §"Contribution") is told with the hour it ends —
/// a ban is not: it is for malice only, and malice is not told how it was seen.
async function stateOf(env, account, now) {
  const out = [];
  const { results: devices } = await env.ACCOUNTS.prepare("SELECT verifier FROM devices WHERE account = ?1").bind(account).all();
  if (devices.length && env.LIBRARY) {
    const ids = devices.map((d) => d.verifier);
    const marks = ids.map((_, i) => `?${i + 2}`).join(", ");
    const { results } = await env.LIBRARY.prepare(`SELECT refused_until FROM verifiers
        WHERE banned = 0 AND refused_until > ?1 AND id IN (${marks})`).bind(iso(now), ...ids).all();
    if (results.length) {
      out.push({ kind: "device_cooling", params: { devices: results.length,
        until: results.map((r) => r.refused_until).sort().pop() } });
    }
  }
  return out;
}

export async function noticesRoute(request, env, path) {
  if (!path.startsWith("/api/notices")) return null;
  if (!env.ACCOUNTS) return no("unavailable", 503);
  const now = Date.now();
  if (path === "/api/notices/everyone") {
    if (request.method !== "GET") return no("method", 405);
    return json({ ok: true, notices: await everyone(env.ACCOUNTS, now) }, 200,
      { "cache-control": `public, max-age=${EVERYONE_CACHE_S}` });
  }
  if (path === "/api/notices") {
    if (request.method !== "GET") return no("method", 405);
    const account = await sessionAccount(env, request);
    if (!account) return no("not_signed_in", 401);
    const { results } = await env.ACCOUNTS.prepare(`SELECT * FROM notices WHERE account = ?1 AND created_at > ?2
        ORDER BY created_at DESC LIMIT ?3`).bind(account, iso(now - NOTICE_KEEP_MS), NOTICES_SHOWN).all();
    const mark = await env.ACCOUNTS.prepare("SELECT read_at FROM notice_marks WHERE account = ?1").bind(account).first();
    const notices = [...results.map(shape), ...(await everyone(env.ACCOUNTS, now))]
      .sort((a, b) => (a.created_at < b.created_at ? 1 : -1)).slice(0, NOTICES_SHOWN);
    return json({ ok: true, notices, read_at: mark ? mark.read_at : null, state: await stateOf(env, account, now) }, 200,
      { "cache-control": "private, no-store" });
  }
  if (path === "/api/notices/read") {
    if (request.method !== "POST") return no("method", 405);
    if (!sameSite(request)) return no("cross_site", 403);
    const account = await sessionAccount(env, request);
    if (!account) return no("not_signed_in", 401);
    let b = {};
    try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
    // NEVER PAST NOW, and never backwards: a clock ahead of the server's reads
    // nothing that has not been made, and a second tab cannot unread.
    const at = iso(Math.min(Date.parse(b.at) || now, now));
    await env.ACCOUNTS.prepare(`INSERT INTO notice_marks (account, read_at) VALUES (?1, ?2)
        ON CONFLICT (account) DO UPDATE SET read_at = GREATEST(notice_marks.read_at, EXCLUDED.read_at)`).bind(account, at).run();
    return json({ ok: true, read_at: at });
  }
  return no("not_found", 404);
}

/// A NOTICE A FEATURE WRITES when something happens to an account. `dedupe_key`
/// is what makes it once: the same key for the same account writes nothing.
export async function noticeWrite(db, { account, kind, params = null, link = null, dedupe_key = null, expires_at = null }) {
  if (!NOTICE_KINDS.includes(kind)) throw new Error(`unknown notice kind ${kind}`);
  await db.prepare(`INSERT INTO notices (account, kind, params, link, dedupe_key, created_at, expires_at)
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT DO NOTHING`)
    .bind(account, kind, params ? JSON.stringify(params) : null, link, dedupe_key, iso(Date.now()), expires_at).run();
}

/// WHAT IS KEPT NO LONGER: a notice to one account past its 180 days, and one to
/// every reader once it has expired or reached the same age.
export async function purgeNotices(env) {
  if (!env.ACCOUNTS) return;
  const now = Date.now();
  await env.ACCOUNTS.prepare("DELETE FROM notices WHERE created_at < ?1 OR (expires_at IS NOT NULL AND expires_at < ?2)")
    .bind(iso(now - NOTICE_KEEP_MS), iso(now)).run();
}

/// What an account's export holds of this: every notice to it, and its mark.
export async function noticesExport(env, account) {
  const { results } = await env.ACCOUNTS.prepare("SELECT kind, text_en, text_zh, params, link, created_at, expires_at FROM notices WHERE account = ?1 ORDER BY created_at")
    .bind(account).all();
  const mark = await env.ACCOUNTS.prepare("SELECT read_at FROM notice_marks WHERE account = ?1").bind(account).first();
  return { notices: results, notices_read_at: mark ? mark.read_at : null };
}
