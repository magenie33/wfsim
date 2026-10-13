// SPDX-License-Identifier: AGPL-3.0-or-later
// THE BOARD THE SITE READS — docs/BOARD.md §"The live board". The bot server
// writes the projection of `scores` (verified facts only) here the moment it
// moves, into the public bucket (`PUBLIC`, `wfsim-public`) under its public
// paths, and `cdn.wfsim.app` serves that bucket through the CDN: the page
// reads it there and never through this worker. `/board/<file>` and
// `/board.meta.json` on wfsim.app are answered here for the readers that name
// that address — a desktop shell in the field, the bots, the MCP server, the
// relay. The copy committed under `site/board/` is a daily snapshot for people,
// read here only until the first live board lands (`isOpen`).
//
//   GET /board/[<scene>/]<weapon|index>.json, /board.meta.json     the live board
//   PUT /api/board/live/<name>.json   (bearer BOARD_PUSH_TOKEN)   the server's write

/// A BASE BOARD FILE, or one under a scene's directory (`Benchmark::cell_dir`).
const NAME = /^(?:[a-z0-9_]{1,80}\/)?[a-z0-9_]{1,80}\.json$/;
/// Long enough to spare R2 a read per page view, short enough that a row the
/// server just published is on the page within the minute. Stored on each
/// object, so the CDN in front of `cdn.wfsim.app` keeps the same promise.
const FRESH_SECONDS = 30;
const CACHE_CONTROL = `public, max-age=${FRESH_SECONDS}`;
const MAX_BYTES = 32 * 1024 * 1024;
/// THE STAMP'S KEY: its public path, like every other file's.
const STAMP = "board.meta.json";

const reply = (body, status) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

/// The bucket key a public path names — the path itself, without its slash —
/// or null for a path that is not a board file.
export function liveKey(path) {
  if (path === "/board.meta.json") return STAMP;
  const m = path.match(/^\/board\/((?:[^/]+\/)?[^/]+)$/);
  return m && NAME.test(m[1]) && m[1] !== "meta.json" ? `board/${m[1]}` : null;
}
/// …and the key a pushed name lands under: `meta.json` is the stamp.
const pushedKey = (name) => (name === "meta.json" ? STAMP : `board/${name}`);

/// THE LIVE BOARD IS OPEN once its stamp is in the bucket — the server pushes it
/// last, after every file it names. Until the first push the committed board
/// answers; after it, never again. Remembered per isolate, since it only ever opens.
let liveOpen = false;
async function isOpen(env) {
  if (!liveOpen) liveOpen = !!(await env.PUBLIC.head(STAMP).catch(() => null));
  return liveOpen;
}

export async function serveLive(request, env, ctx, key) {
  if (request.method !== "GET" && request.method !== "HEAD") return reply({ ok: false, error: "GET only" }, 405);
  if (!env.PUBLIC || !(await isOpen(env))) return env.ASSETS.fetch(request);
  const cache = caches.default;
  const hit = await cache.match(request);
  if (hit) return hit;
  let obj;
  try { obj = await env.PUBLIC.get(key); } catch (_) { return reply({ ok: false, error: "the live board is unreachable" }, 503); }
  // A WEAPON NOBODY HAS RANKED is an empty board, never an error: the page
  // reads a 404 as "no rows" (`fetchJsonPatient`).
  if (!obj) return reply({ ok: false, error: "not on the board" }, 404);
  const res = new Response(obj.body, {
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": CACHE_CONTROL,
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
  if (!env.PUBLIC) return reply({ ok: false, error: "no public bucket" }, 503);
  const body = await request.arrayBuffer();
  if (body.byteLength > MAX_BYTES) return reply({ ok: false, error: "too large" }, 413);
  try { JSON.parse(new TextDecoder().decode(body)); } catch (_) { return reply({ ok: false, error: "not json" }, 400); }
  await env.PUBLIC.put(pushedKey(name), body,
    { httpMetadata: { contentType: "application/json; charset=utf-8", cacheControl: CACHE_CONTROL } });
  return reply({ ok: true }, 200);
}
