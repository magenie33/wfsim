// THE WORKER — one script in front of a static site.
//
// wfsim.app is a Cloudflare Worker with STATIC ASSETS (`wrangler.jsonc`), not a
// Pages project. That distinction cost an evening: this file began life as
// `functions/api/board/submit.js`, which is the Pages convention, and Pages
// conventions do nothing on a Worker — the endpoint returned the SPA's HTML
// with a 200, because `not_found_handling: single-page-application` answers
// every unmatched path with index.html. A 200 carrying the wrong content type
// is the quietest possible failure.
//
// Two things make it work here, and both are in `wrangler.jsonc`:
//   - `assets.binding: ASSETS`, so this script can hand a request back to the
//     CDN unchanged — everything that is not the api is still a static file,
//     served the same way it was before this script existed;
//   - `assets.run_worker_first: ["/api/*"]`, so the SPA fallback cannot claim
//     an api path before the script sees it.
//
// WHAT THIS ENDPOINT DOES: stores a BUILD. Nothing else.
// WHAT IT DOES NOT DO: score it. A submission carries no number and none would
// be believed — the board is computed from the builds by the scheduled job in
// `.github/workflows/board.yml`, running the same engine that ships to the
// browser. That is what makes a row reproducible and a forged score pointless,
// and it is why this endpoint can be public and unauthenticated without being
// a hole: the worst a flood achieves is builds that rank badly.
//
// KEYED BY IDENTITY, so writes are idempotent. A hundred players who arrive at
// the same build write the same key a hundred times and produce ONE row —
// which is the correct semantics for a board of builds, and it means no dedup
// pass and no counting.

const MAX_BYTES = 4096;        // a build is a few hundred bytes; this is slack
const MAX_MODS = 8;            // the exilus slot is out of scope for a benchmark
const ID = /^[a-z0-9_]{1,64}$/;

const bad = (msg, status = 400) =>
  new Response(JSON.stringify({ ok: false, error: msg }), {
    status, headers: { "content-type": "application/json" },
  });

/// The FIGHT this build is, as one stable key — the same shape
/// `engine::builds::identity` produces. Mods are sorted because order does not
/// change the number (measured: the same eight reversed score 0.96478 both
/// ways), so two spellings of one build must not become two rows.
const identity = (b) =>
  [b.benchmark, b.weapon, [...b.mods].sort().join(","),
   [...b.evolutions].sort().join(","), b.arcanes.join(",")].join("|");

async function submit(request, env) {
  if (!env.SUBMISSIONS) return bad("submission storage is not configured", 503);

  // Size first, before parsing: the cheapest rejection there is.
  const raw = await request.text();
  if (raw.length > MAX_BYTES) return bad("payload too large");

  let b;
  try { b = JSON.parse(raw); } catch { return bad("not json"); }

  // SHAPE ONLY. Whether these ids exist, whether the mods are compatible and
  // whether the build fits 60 capacity are questions for the engine, and the
  // engine is not here — `engine::builds::validate` answers them in the
  // scoring job, where the whole data set is available. Anything that fails
  // there is simply never scored and never reaches the board.
  const list = (x) => Array.isArray(x) && x.every((s) => typeof s === "string" && ID.test(s));
  if (!ID.test(b.benchmark || "") || !ID.test(b.weapon || "")) return bad("bad ids");
  if (!list(b.mods) || b.mods.length > MAX_MODS) return bad("bad mods");
  if (!list(b.evolutions) || b.evolutions.length > 8) return bad("bad evolutions");
  if (!list(b.arcanes) || b.arcanes.length > 4) return bad("bad arcanes");

  // NOTHING ABOUT THE SUBMITTER IS STORED. Not the IP, not a token, not a
  // timestamp that could order one person's submissions against another's. The
  // record is the build and the key it hashes to; `at` is the day, which is
  // coarse enough to expire old entries and too coarse to identify anyone.
  const rec = {
    benchmark: b.benchmark,
    weapon: b.weapon,
    mods: [...b.mods].sort(),
    evolutions: b.evolutions,
    arcanes: b.arcanes,
    at: new Date().toISOString().slice(0, 10),
  };
  await env.SUBMISSIONS.put(identity(rec), JSON.stringify(rec), {
    // A build nobody has submitted in a year is not a live answer any more, and
    // the scoring job re-lists everything each run — so expiry is the only
    // cleanup needed.
    expirationTtl: 60 * 60 * 24 * 365,
  });
  return new Response(JSON.stringify({ ok: true }), {
    headers: { "content-type": "application/json" },
  });
}

export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (path === "/api/board/submit") {
      // A GET here is somebody looking for the board itself, which is a STATIC
      // FILE committed to the repo (`data/benchmarks/boards/`) and served from
      // the CDN — no read path goes through a service.
      return request.method === "POST"
        ? submit(request, env)
        : bad("the board is a static file — see /weapons/<name>, or data/benchmarks/boards/ in the repo", 405);
    }
    // EVERYTHING ELSE IS THE SITE, unchanged. Handing the request to the assets
    // binding is what keeps this script from becoming a thing the site depends
    // on: it adds one path and forwards the rest.
    return env.ASSETS.fetch(request);
  },
};
