// SPDX-License-Identifier: AGPL-3.0-or-later
// THE BOARD THE SITE READS — docs/BOARD.md §"The live board". `/board/<file>`
// and `/board.meta.json` are answered from R2, where the bot server writes the
// projection of `scores` (verified facts only) the moment it moves. The copy
// committed under `site/board/` is a daily snapshot for people, read here only
// until the first live board lands (`isOpen`).
//
//   GET /board/[<scene>/]<weapon|index>.json, /board.meta.json     the live board
//   PUT /api/board/live/<name>.json   (bearer BOARD_PUSH_TOKEN)   the server's write

const PREFIX = "board/live/";
/// A BASE BOARD FILE, or one under a scene's directory (`Benchmark::cell_dir`).
const NAME = /^(?:[a-z0-9_]{1,80}\/)?[a-z0-9_]{1,80}\.json$/;
/// Long enough to spare R2 a read per page view, short enough that a row the
/// server just published is on the page within the minute.
const FRESH_SECONDS = 30;
const MAX_BYTES = 32 * 1024 * 1024;

const reply = (body, status) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

/// The R2 key a public path names, or null for a path that is not a board file.
export function liveKey(path) {
  if (path === "/board.meta.json") return `${PREFIX}meta.json`;
  const m = path.match(/^\/board\/((?:[^/]+\/)?[^/]+)$/);
  return m && NAME.test(m[1]) && m[1] !== "meta.json" ? PREFIX + m[1] : null;
}

/// THE LIVE BOARD IS OPEN once its stamp is in R2 — the server pushes it last,
/// after every file it names. Until the first push the committed board answers;
/// after it, never again. Remembered per isolate, since it only ever opens.
let liveOpen = false;
async function isOpen(env) {
  if (!liveOpen) liveOpen = !!(await env.UPLOADS.head(`${PREFIX}meta.json`).catch(() => null));
  return liveOpen;
}

export async function serveLive(request, env, ctx, key) {
  if (request.method !== "GET" && request.method !== "HEAD") return reply({ ok: false, error: "GET only" }, 405);
  if (!env.UPLOADS || !(await isOpen(env))) return env.ASSETS.fetch(request);
  const cache = caches.default;
  const hit = await cache.match(request);
  if (hit) return hit;
  let obj;
  try { obj = await env.UPLOADS.get(key); } catch (_) { return reply({ ok: false, error: "the live board is unreachable" }, 503); }
  // A WEAPON NOBODY HAS RANKED is an empty board, never an error: the page
  // reads a 404 as "no rows" (`fetchJsonPatient`).
  if (!obj) return reply({ ok: false, error: "not on the board" }, 404);
  const res = new Response(obj.body, {
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": `public, max-age=${FRESH_SECONDS}`,
      etag: obj.httpEtag,
      "access-control-allow-origin": "*",
    },
  });
  if (ctx && ctx.waitUntil) ctx.waitUntil(cache.put(request, res.clone()));
  return res;
}

export async function pushLive(request, env, name) {
  if (request.method !== "PUT") return reply({ ok: false, error: "PUT only" }, 405);
  const auth = request.headers.get("authorization") || "";
  if (!env.BOARD_PUSH_TOKEN || auth !== `Bearer ${env.BOARD_PUSH_TOKEN}`) return reply({ ok: false, error: "unauthorized" }, 401);
  if (!NAME.test(name)) return reply({ ok: false, error: "bad name" }, 400);
  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_BYTES) return reply({ ok: false, error: "too large" }, 413);
  try { JSON.parse(new TextDecoder().decode(body)); } catch (_) { return reply({ ok: false, error: "not json" }, 400); }
  await env.UPLOADS.put(PREFIX + name, body, { httpMetadata: { contentType: "application/json" } });
  return reply({ ok: true }, 200);
}
