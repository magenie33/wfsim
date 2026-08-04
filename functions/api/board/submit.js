// THE SUBMISSION ENDPOINT — a Cloudflare Pages Function on the SITE'S OWN
// ORIGIN (`wfsim.app/api/board/submit`).
//
// Same origin is not a convenience. A separate api domain is a second DNS name
// and a second thing that can be blocked, which is the exact failure the art
// rule was written about: the CDN used to 301 to raw.githubusercontent.com,
// "unreliable to blocked from mainland China, i.e. precisely where the players
// are". A board nobody in China can submit to is not a community board.
//
// WHAT THIS DOES: stores a BUILD. Nothing else.
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
//
// SETUP (once, by the repo owner):
//   1. Pages project → Settings → Bindings → KV namespace:
//      variable name `SUBMISSIONS`, bound to a namespace you create.
//
// NAMED FOR WHAT IT HOLDS, which is not the board. The board is the generated
// YAML in `data/benchmarks/boards/` — this namespace holds the BUILDS people
// sent, waiting to be scored. An earlier version called the binding `BOARD`
// and that is a debugging trap: "the board is empty, but the BOARD binding
// looks fine" is a sentence you can waste an afternoon on (user, 2026-08-04,
// asking for standard names precisely so this would not happen).
//   2. Nothing else. The Function is deployed with the site by the same push.

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

export async function onRequestPost({ request, env }) {
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

// A GET here is somebody looking for the board itself, which is a STATIC FILE
// committed to the repo (`data/benchmarks/boards/`) and served from the CDN —
// no read path goes through a service.
export const onRequestGet = () =>
  bad("the board is a static file — see /weapons/<name>, or data/benchmarks/boards/ in the repo", 405);
