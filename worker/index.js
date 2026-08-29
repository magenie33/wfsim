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

/// WHAT A BUILD IS, declared ONCE.
///
/// Three things are derived from this table — the shape check, the stored
/// record, and the identity key — and they used to be three hand-written lists.
/// That is the defect generator: adding an axis meant remembering all three,
/// and TWICE an axis was added to some of them and not the others. `mode` was
/// validated and neither stored nor keyed, so the scorer took its migration
/// fallback and every Incarnon weapon's row said `cycle`. Then
/// `valence` was neither stored nor keyed, so seven Kuva Nukor submissions were
/// refused on every scoring run — "has no Valence element" — while the panel
/// had told each submitter "sent".
///
/// Neither was a hard bug to fix and both were invisible for weeks, because a
/// dropped field does not throw: it produces a record that is merely INCOMPLETE
/// and a scorer that quietly refuses it. So the answer is not to be more
/// careful with three lists, it is to have one.
///
/// `kind`:
///   - `id`  — a single slug. `required` ones must be present and non-empty;
///             the rest may be empty, which is what an ordinary weapon sends
///             for `valence` and what a weapon with one mode sends for `mode`.
///             An EMPTY optional axis is not written to the record at all.
///   - `ids` — a list of slugs, always written even when empty.
/// `set`: the ORDER does not matter for this axis, so the key sorts it.
/// Evolutions are a set (one per tier, the tier decides where each sits). Mods
/// are NOT: they combine elements in the order they are listed, and on the
/// Torid Heat/Cold/Toxin/Electric against Heat/Toxin/Cold/Electric is 12,424
/// DPS against 46,583.
///
/// SHAPE ONLY, throughout. Whether these ids exist, whether the mods are
/// compatible, whether the build fits 60 capacity and whether THIS weapon has
/// the mode or the element named are questions for the engine, and the engine
/// is not here — `engine::builds::validate_for_board` answers them in the
/// scoring job, where the whole data set is available. Anything that fails
/// there is never scored and never reaches the board. The cost is a little junk
/// in KV; the alternative is two rules that drift, and a worker confidently
/// rejecting builds a benchmark would have accepted.
// EXPORTED for `scripts/check_board_submit.mjs`, which asserts this table
// against the keys the PAGE actually sends. That assertion is the one that
// would have caught both losses on the day they happened — a name added to
// `boardPayload()` and not to this table fails it immediately, without anyone
// having to notice a board row that never appeared.
//
// `axis` names which of `engine::builds::BUILD_AXES` an entry carries, where
// there is one. It is not this worker's list to keep — the engine declares what
// a build consists of and serves it at `/api/meta.build_axes`, and
// `scripts/check_build_axes.mjs` asserts every axis marked `on_board` is
// claimed by a row here. A worker with no game data cannot look that up at
// request time, so the check does it once, on the ground.
export const AXES = [
  // THE RULER IS NOT PART OF WHAT THIS IS. A submission has
  // never carried a score — it carries a BUILD, and the number is produced by
  // the scorer. So the ruler a build happened to be measured under was never a
  // property of the record; it was a GATE, and the gate was expensive: of 914
  // distinct builds players had submitted, only 46 had ever been scored on more
  // than one board.
  //
  // THE STORE IS A LIBRARY OF BUILDS and every ruler crosses the whole of it, so
  // `benchmark` stays only as PROVENANCE — where the submitter happened to be —
  // and is `identity: false`, which is what lets the same build arriving from
  // two different fights be one record instead of two. It is optional because a
  // build uploaded from a scenario of the player's own has no ruler to name.
  { key: "benchmark", kind: "id", identity: false },
  // Not a build axis either, but it IS the record's identity: a build is a
  // statement about one weapon.
  { key: "weapon", kind: "id", required: true },
  // HOW IT WAS PLAYED — half the entrant's identity. Optional because records
  // written before the dimension existed are still in KV, and the scorer's
  // migration fallback is what reads those.
  { key: "mode", kind: "id", axis: "mode" },
  // THE PROGENITOR ELEMENT of an adversary weapon. The ELEMENT only: the ruler
  // scores every row at the roll's maximum, so the percentage is not a row's to
  // state. Empty on everything that is not out of a Lich.
  { key: "valence", kind: "id", axis: "valence" },
  // An OUTER BOUND, not the rule. Admission became the BENCHMARK's business on
  // 2026-08-05 — "full" means every evolution tier and arcane seat THIS weapon
  // has — and this worker has no game data: it cannot know that a Laetum has
  // five tiers and a rifle none. It briefly hardcoded "exactly 8", which was
  // right for one benchmark and would silently be wrong for the second.
  { key: "mods", kind: "ids", max: MAX_MODS, axis: "mods" },
  { key: "evolutions", kind: "ids", max: 8, set: true, axis: "evolutions" },
  { key: "arcanes", kind: "ids", max: 4, axis: "arcanes" },
  // A MODULAR WEAPON'S PARTS, as TWO FLAT IDS rather than as the object the
  // simulate request carries. Spellings are per-protocol and always have been
  // (`arcane` on a request, `arcanes` here); what is shared is the axis, which
  // both rows name. Flat because this worker's identity key is a join of
  // strings and its validation is `id`/`ids` — an object would need a third
  // kind, in the one file with no game data to check it against. The CHAMBER is
  // not here: it is the weapon, and `weapon` already carries it.
  // THE EXILUS SLOT'S MOD, its own key rather than a ninth entry in `mods`:
  // an exilus-eligible mod is legal in a MAIN slot too, so a flat list cannot
  // say which one came out of the exilus slot, and only the page knows. It
  // joined on 2026-08-25, when the rulers stopped excluding the slot — beam
  // range is exilus, and beam range is how many bodies a beam reaches.
  { key: "exilus", kind: "id", axis: "mods" },
  { key: "grip", kind: "id", axis: "assembly" },
  { key: "loader", kind: "id", axis: "assembly" },
  // A RIVEN, AS A SHAPE — which stats it rolled and which is the malus. The
  // ROLLS are not here and never will be: a row states a shape and the scorer
  // finds that shape's own best corner for the ruler's fight, the same way
  // every row is scored at full Forma and at the valence roll's ceiling. Two
  // players who rolled the same stats submitted the same build.
  //
  // A SET, because a riven's stats do not combine with each other — two people
  // listing them in different orders described one riven. WHERE it sits is in
  // `mods`, which carries the bare `riven` at its own position: an elemental
  // riven pairs with the build's other elementals, so position is the build.
  { key: "riven_pos", kind: "ids", max: 3, set: true, axis: "rivens" },
  { key: "riven_neg", kind: "id", axis: "rivens" },
];

const bad = (msg, status = 400) =>
  new Response(JSON.stringify({ ok: false, error: msg }), {
    status, headers: { "content-type": "application/json" },
  });

/// The BUILD this is, as one stable key — every axis of it, in `AXES` order.
///
/// EVERY axis, which is the whole point of deriving it: a key that cannot tell
/// two builds apart files the second under the first's number, silently, and
/// the build that loses is the one submitted second. It has happened twice, and
/// both times the missing axis was also the one that was not being stored —
/// see the note on `AXES`. Writes stay idempotent because the same build always
/// produces the same key.
const identity = (b) =>
  // `identity: false` marks an axis the record CARRIES and is not IDENTIFIED by
  // — provenance rather than a choice inside the build. Read off the same table
  // as everything else, so an axis cannot be identity-bearing here and absent
  // from storage, which is how a build was lost twice.
  AXES.filter((a) => a.identity !== false).map((a) => {
    const v = b[a.key];
    if (a.kind === "id") return v || "";
    const list = v || [];
    return (a.set ? [...list].sort() : list).join(",");
  }).join("|");

async function submit(request, env) {
  if (!env.SUBMISSIONS) return bad("submission storage is not configured", 503);

  // Size first, before parsing: the cheapest rejection there is.
  const raw = await request.text();
  if (raw.length > MAX_BYTES) return bad("payload too large");

  let b;
  try { b = JSON.parse(raw); } catch { return bad("not json"); }

  // ONE PASS OVER `AXES` for both jobs — check the shape, and copy what
  // survives it. There is no second list to fall out of step with, which is
  // the only reason this endpoint can be trusted to store a build it was never
  // taught about by name.
  //
  // NOTHING ABOUT THE SUBMITTER IS STORED. Not the IP, not a token, not a
  // timestamp that could order one person's submissions against another's. The
  // record is the build and the key it hashes to; `at` is the day, which is
  // coarse enough to expire old entries and too coarse to identify anyone.
  const rec = { at: new Date().toISOString().slice(0, 10) };
  for (const a of AXES) {
    const v = b[a.key];
    if (a.kind === "id") {
      if (v !== undefined && typeof v !== "string") return bad(`bad ${a.key}`);
      const s = v || "";
      if (a.required ? !ID.test(s) : s && !ID.test(s)) return bad(`bad ${a.key}`);
      // An empty optional axis is absent rather than empty — an ordinary
      // weapon's record has no `valence` key, exactly as before this table.
      if (s) rec[a.key] = s;
    } else {
      // AN EMPTY LIST IS AN ABSENT AXIS, the same rule an `id` axis has three
      // lines up. A build with no riven sends `riven_pos: []` — the page states
      // every axis rather than omitting the ones it has nothing for — and a
      // record should no more carry an empty riven than it carries an empty
      // valence. `undefined` means the same thing, so a caller written before
      // an axis existed is not a rejection.
      const list = v === undefined ? [] : v;
      if (!Array.isArray(list) || list.length > a.max) return bad(`bad ${a.key}`);
      if (!list.every((s) => typeof s === "string" && ID.test(s))) return bad(`bad ${a.key}`);
      // As submitted, never sorted here: sorting would store a build the
      // player never made. The KEY sorts the axes that are sets.
      if (list.length) rec[a.key] = list;
    }
  }
  const key = identity(rec);
  await env.SUBMISSIONS.put(key, JSON.stringify(rec), {
    // A build nobody has submitted in a year is not a live answer any more, and
    // the scoring job re-lists everything each run — so expiry is the only
    // cleanup needed.
    expirationTtl: 60 * 60 * 24 * 365,
  });
  await mirror(env, key, rec);
  return new Response(JSON.stringify({ ok: true }), {
    headers: { "content-type": "application/json" },
  });
}

/// THE SAME RECORD, INTO D1 — the first step of moving the library out of KV,
/// and deliberately the only step that can be taken without changing anything
/// that reads it.
///
/// KV IS STILL THE AUTHORITY. Everything downstream — the scorer, the pending
/// count — reads KV and keeps reading it; this writes a second copy and nothing
/// looks at it yet. That is what makes the step reversible: delete the binding
/// and the system is exactly what it was.
///
/// WHY D1 AT ALL. The library is the one irreplaceable thing here and KV cannot
/// be asked a question about it: no queries, no transactions, no bulk read, and
/// listing is the only index. "Which weapons are under-covered", "how much did
/// the library grow this month", "which facts are stale" are the questions
/// running a board consists of, and none of them can be put to a key-value
/// store. D1 is SQLite — it answers them, and it dumps whole, which is a hard
/// requirement for an asset with no other copy (docs/BOARD.md, 2026-08-26).
///
/// A FAILURE HERE MAY NOT REACH THE SUBMITTER. The submission already succeeded
/// — the authoritative write is above — so a broken mirror must be invisible:
/// telling a player "sent" and then failing on a copy they do not know exists is
/// the worst of both. It is caught and dropped, and the next submission of the
/// same build writes the row again (`INSERT OR REPLACE` on the identity).
async function mirror(env, key, rec) {
  if (!env.LIBRARY) return;
  try {
    await env.LIBRARY.prepare(
      "INSERT OR REPLACE INTO builds (identity, at, record) VALUES (?, ?, ?)",
    ).bind(key, rec.at, JSON.stringify(rec)).run();
  } catch (e) {
    // Nothing to tell the caller and nowhere useful to put it: the request is
    // already answered. It shows up as a row that is in KV and not in D1, which
    // is what the two-way count in the backup job is for.
    console.log("library mirror failed:", (e && e.message) || String(e));
  }
}

/// HOW MANY BUILDS THE LIBRARY HOLDS — the one number that says whether the
/// board on screen is current.
///
/// THE BOARD IS A STATIC FILE, committed to the repo and served from the CDN,
/// which is what makes it fast and free — and what makes it only as fresh as
/// the last scoring run. Rather than moving the board behind a service to make
/// it live, the page keeps reading the file and asks THIS for the one fact the
/// file cannot carry: how many builds have arrived since. The strip it draws is
/// "N builds submitted since this board was scored", which answers both "did
/// mine arrive" and "is this list current" without pretending to rank anything.
///
/// A COUNT, and nothing else. No build, no weapon, no day — the store already
/// holds nothing about the submitter and this endpoint hands back less than the
/// store holds, not more.
///
/// KV LISTS IN PAGES, at most 1000 keys each, and the loop is bounded: at the
/// library's present size (about a thousand) this is one call, and at twenty
/// thousand it stops counting and says so rather than walking KV on every board
/// view. A capped count still answers the question the strip asks.
async function pending(env) {
  if (!env.SUBMISSIONS) return bad("submission storage is not configured", 503);
  let count = 0;
  let cursor;
  let capped = true;
  for (let page = 0; page < 20; page++) {
    const r = await env.SUBMISSIONS.list({ limit: 1000, cursor });
    count += (r.keys || []).length;
    if (r.list_complete) { capped = false; break; }
    cursor = r.cursor;
  }
  return new Response(JSON.stringify({ ok: true, count, capped }), {
    headers: {
      "content-type": "application/json",
      // A MINUTE. The board itself moves every twenty, so a count that is up to
      // a minute old is exact enough for the sentence it is in — and it keeps a
      // busy board page from listing KV once per reader.
      "cache-control": "public, max-age=60",
    },
  });
}

/// HOW MANY PEOPLE HAVE CHIPPED IN — a COUNT, and nothing else.
///
/// Social proof is the one lever on `/support` with a replicated experiment
/// behind it: a request that legitimises a small gift performs best beside a
/// statement that others have given (Cialdini & Schroeder 1976, and its 2007
/// replication). It is also the only figure about this project's funding that
/// can be published without publishing the author's finances.
///
/// SO THE STORE HOLDS A COUNT AND CANNOT HOLD MORE. One key per Ko-fi message
/// id, an empty value, and the DAY as metadata — no amount, no name, no email,
/// no message. That is a property of the schema rather than a promise about the
/// endpoint: asked for a total, this worker could not produce one.
///
/// A NAMESPACE OF ITS OWN, not a prefix inside `SUBMISSIONS`: the pending count
/// above is `SUBMISSIONS.list()` — every key in it — so a supporter stored
/// there would be counted as a build waiting to be scored.
///
/// SILENT UNTIL IT IS CONFIGURED. Without the binding this answers 503 and the
/// page draws no line at all, which is the same rule a channel with no url
/// follows: an option that does not work yet is worse than one not offered.
const SUPPORTER = "kofi:";

async function supporters(env) {
  if (!env.SUPPORT) return bad("supporter storage is not configured", 503);
  let count = 0;
  let cursor;
  for (let page = 0; page < 20; page++) {
    const r = await env.SUPPORT.list({ prefix: SUPPORTER, limit: 1000, cursor });
    count += (r.keys || []).length;
    if (r.list_complete) break;
    cursor = r.cursor;
  }
  return new Response(JSON.stringify({ ok: true, count }), {
    headers: {
      "content-type": "application/json",
      // An hour. This changes a handful of times a week at best, and the line
      // it feeds is a footnote beside the channels rather than a live figure.
      "cache-control": "public, max-age=3600",
    },
  });
}

/// KO-FI'S WEBHOOK, which is what makes the count above automatic.
///
/// Ko-fi POSTs `application/x-www-form-urlencoded` with a single `data` field
/// carrying json, and the json carries a `verification_token` that only the
/// account owner can read off their own dashboard. That token is the whole of
/// the authentication and it is a SECRET (`wrangler secret put KOFI_TOKEN`) —
/// without it configured this refuses everything, because an endpoint that
/// counts anonymous POSTs is a counter anybody can drive.
///
/// IDEMPOTENT ON THE MESSAGE ID. Ko-fi retries a delivery it did not see
/// acknowledged, and a retry must not be a second supporter — the id is the
/// key, so a replay writes the same key again and the count does not move.
///
/// WHAT IS DROPPED, before anything is written: the amount, the supporter's
/// name and email, the message they typed, and the timestamp's time. What is
/// kept is that a payment happened, on a day.
async function kofi(request, env) {
  if (!env.SUPPORT) return bad("supporter storage is not configured", 503);
  if (!env.KOFI_TOKEN) return bad("supporter webhook is not configured", 503);
  let msg;
  try {
    const form = await request.formData();
    msg = JSON.parse(form.get("data") || "null");
  } catch (_) {
    return bad("not a Ko-fi payload");
  }
  if (!msg || typeof msg !== "object") return bad("not a Ko-fi payload");
  if (msg.verification_token !== env.KOFI_TOKEN) return bad("bad token", 403);
  const id = String(msg.message_id || "");
  // A plain id, because it becomes a key: Ko-fi sends a uuid, and anything
  // else is a payload this was not written for.
  if (!/^[A-Za-z0-9-]{8,64}$/.test(id)) return bad("bad message id");
  await env.SUPPORT.put(SUPPORTER + id, "", {
    metadata: { at: new Date().toISOString().slice(0, 10) },
  });
  return new Response(JSON.stringify({ ok: true }), {
    headers: { "content-type": "application/json" },
  });
}

export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (path === "/api/support/count") {
      return request.method === "GET" ? supporters(env) : bad("GET only", 405);
    }
    if (path === "/api/support/kofi") {
      return request.method === "POST" ? kofi(request, env) : bad("POST only", 405);
    }
    if (path === "/api/board/pending") {
      return request.method === "GET" ? pending(env) : bad("GET only", 405);
    }
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
