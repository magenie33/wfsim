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
const MAX_MODS = 9;            // an OUTER BOUND, not the rule — see below
const ID = /^[a-z0-9_]{1,64}$/;

const bad = (msg, status = 400) =>
  new Response(JSON.stringify({ ok: false, error: msg }), {
    status, headers: { "content-type": "application/json" },
  });

/// The FIGHT this build is, as one stable key — the same shape
/// `engine::builds::identity` produces, plus the MODE, which is how the scoring
/// job keys its own rows (`identity(build)#mode`).
///
/// THE MODE HAS TO BE IN HERE, and leaving it out is what made the board look
/// the way it did: a base-form Torid and a cycled one are the same weapon, the
/// same mods and the same evolutions, so they hashed to ONE key and the second
/// write replaced the first. Worse, `mode` was not even stored — so the scorer
/// saw a submission with no mode, took its migration fallback ("the cycle where
/// there is one"), and every Incarnon weapon on the board said `cycle` whatever
/// its submitter had chosen. 306 cycle rows, 158 base ones, and not one weapon
/// with both: that shape was this line, not anybody's playstyle (2026-08-09).
///
/// MODS ARE NOT SORTED. They combine ELEMENTS in the order they are listed, so
/// the same four elementals in two orders are two different weapons: on the
/// Torid, Heat/Cold/Toxin/Electric pairs to Blast + Corrosive and Heat/Toxin/
/// Cold/Electric to Gas + Magnetic — 12,424 DPS against 46,583 (measured
/// 2026-08-04). This sorted for a day on the strength of one measurement that
/// happened to reorder mods whose pairing did not change, and the board scored
/// whichever pairing the sort produced.
///
/// Evolutions ARE a set: one per tier, and the tier decides where each sits.
const identity = (b) =>
  [b.benchmark, b.weapon, b.mode || "", b.mods.join(","),
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
  // AN OUTER BOUND, NOT THE RULE. Admission became the BENCHMARK's business on
  // 2026-08-05 — "full" now means every evolution tier and arcane seat THIS
  // WEAPON has, and this worker has no game data: it cannot know that a Laetum
  // has five tiers and a rifle none. It briefly hardcoded "exactly 8", which
  // was right for one benchmark and would silently be wrong for the second.
  //
  // So the engine is the only place that decides, and this only keeps the
  // obviously malformed out of storage. The cost is a little junk in KV that
  // the scorer then refuses; the alternative is two rules that drift, and a
  // worker confidently rejecting builds a benchmark would have accepted.
  // HOW IT WAS PLAYED. Shape only, like everything else here — whether THIS
  // weapon has a `base` or an `alternate` is a question for the engine, and the
  // scorer refuses a mode a weapon does not have ("refused torid: it has no
  // `alternate` mode"). Optional, because records written before this exist and
  // the scorer's fallback is what reads them.
  if (b.mode !== undefined && !ID.test(b.mode || "")) return bad("bad mode");
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
    // HALF THE ENTRANT'S IDENTITY, and it used to be dropped on this line — the
    // page has sent it since 2026-08-07 and nothing here ever wrote it down.
    ...(b.mode ? { mode: b.mode } : {}),
    // As submitted. The order IS the build (see `identity`).
    mods: b.mods,
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
