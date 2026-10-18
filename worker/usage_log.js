// SPDX-License-Identifier: AGPL-3.0-or-later
// ---- THE USAGE LOG -------------------------------------------------------------
//
// docs/ANALYTICS.md §"The log". Every usage point, unsampled, into
// `usage_points` (worker/pg/library.sql): the only place a point is kept, and
// the source every visitor count is read from.

/// HOW LONG A POINT IS KEPT. After it only `usage_days`' totals remain; the
/// rollup reads the seven days before a day, so this stays well past that.
export const USAGE_LOG_DAYS = 90;
const DAY_MS = 86400000;

/// THE VISITOR AS STORED: the SHA-256 of the browser's id, its first 128 bits.
/// The id is 128 random bits, so the hash cannot be walked back to it; only the
/// browser holding the id can name its rows, which is what lets it delete them.
export async function usageVisitor(cid) {
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(cid));
  return [...new Uint8Array(bytes).slice(0, 16)].map((x) => x.toString(16).padStart(2, "0")).join("");
}

/// WRITE ONE POINT. Never throws: a point that cannot be written is a point
/// missing, and the reader is answered either way.
export async function logPoint(env, at, visitor, p) {
  if (!env.LIBRARY) return;
  try {
    await env.LIBRARY.prepare(`INSERT INTO usage_points
      (at, day, visitor, event, subject, route, lang, shell, release, country, n)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
      .bind(at, at.slice(0, 10), visitor, p.e, p.subject, p.route, p.lang, p.shell, p.release, p.country, p.n).run();
  } catch (e) {
    console.error("usage log", e && e.message);
  }
}

/// EVERYTHING KEPT UNDER ONE BROWSER'S ID, deleted at its own request. The
/// totals already kept in `usage_days` name nobody and stay.
export async function forgetVisitor(env, cid) {
  const { meta } = await env.LIBRARY.prepare("DELETE FROM usage_points WHERE visitor = ?")
    .bind(await usageVisitor(cid)).run();
  return meta.changes;
}

/// DROP WHAT IS PAST ITS TIME. Returns the rows deleted.
export async function purgeUsageLog(env, now = Date.now()) {
  if (!env.LIBRARY) return 0;
  const before = new Date(now - USAGE_LOG_DAYS * DAY_MS).toISOString().slice(0, 10);
  const { meta } = await env.LIBRARY.prepare("DELETE FROM usage_points WHERE day < ?").bind(before).run();
  return meta.changes;
}
