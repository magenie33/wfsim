// THE WORKER — one script in front of a static site.
//
// wfsim.app is a Cloudflare Worker with STATIC ASSETS (`wrangler.jsonc`), not a
// Pages project, so the Pages conventions do nothing here: an endpoint this
// script does not claim answers with the SPA's HTML and a 200, which is the
// quietest possible failure.
//
// Two things make it work, both in `wrangler.jsonc`: `assets.binding: ASSETS`,
// so this script can hand a request back to the CDN unchanged; and
// `assets.run_worker_first: ["/api/*"]`, so the SPA fallback cannot claim an
// api path before the script sees it.
//
// WHAT THIS ENDPOINT DOES: stores a BUILD. It does NOT score it — the board is
// computed from the builds by `.github/workflows/board.yml`, running the same
// engine that ships to the browser, which is what makes a row reproducible, a
// forged score pointless and this endpoint safe to leave unauthenticated.
//
// KEYED BY IDENTITY, so writes are idempotent: a hundred players arriving at
// the same build produce ONE row, with no dedup pass and no counting.

const MAX_BYTES = 4096;        // a build is a few hundred bytes; this is slack
// AN OUTER BOUND, NOT THE RULE — see below. It is `MAIN_SLOTS + 1`: eight main
// slots and the STANCE, which is the one extra card that rides `mods` (the
// exilus has a key of its own). EXPORTED so `check_board_submit.mjs` can hold
// it against the engine's own constant — this file has no game data and cannot
// derive it, so the only thing keeping the two in step is that assertion.
export const MAX_MODS = 9;
const ID = /^[a-z0-9_]{1,64}$/;

/// WHAT A SUBMISSION CARRIES, declared ONCE. Two things are derived from it —
/// the shape check and the stored record — because two hand-written lists is a
/// defect generator: adding an axis to one and not the other stores an
/// INCOMPLETE record, and what is missing is missing for ever.
///
/// `kind`: `id` is a single slug — `required` ones present and non-empty, the
/// rest optional, and an EMPTY optional axis is not written at all — and `ids`
/// is a list, always written.
///
/// SHAPE ONLY: whether these ids exist, whether the build is legal, and which
/// of them make it a DIFFERENT build are all questions about game data this
/// service has not got. `wfsim-intake` asks the engine all three.
// EXPORTED for `scripts/check_board_submit.mjs`, which asserts this table
// against the keys the PAGE actually sends, so a name added to `boardPayload()`
// and not here fails immediately. `axis` names which of
// `engine::builds::BUILD_AXES` an entry carries — not this worker's list to
// keep, so `scripts/check_build_axes.mjs` asserts every `on_board` axis is
// claimed by a row here.
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
  { key: "benchmark", kind: "id" },
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
  // An OUTER BOUND, not the rule. Admission is the BENCHMARK's business —
  // "full" means every evolution tier and arcane seat THIS weapon has — and
  // this worker has no game data: it cannot know that a Laetum has five tiers
  // and a rifle none, so a fixed count here would be right for one benchmark
  // and silently wrong for the next.
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

/// THE BODY, PARSED AND SIZED, or the refusal to send back. Shared because two
/// endpoints take a build and must not disagree about what one is.
async function body(request) {
  if (!request) return { err: "no request" };
  // Size first, before parsing: the cheapest rejection there is.
  const raw = await request.text();
  if (raw.length > MAX_BYTES) return { err: "payload too large" };
  try { return { b: JSON.parse(raw) }; } catch { return { err: "not json" }; }
}

/// A BUILD AS THIS SERVICE STORES ONE, or the name of the axis that failed.
///
/// SHAPE ONLY, and that is the whole of what this service can check. Whether
/// the build is LEGAL — how many mods a weapon may carry, whether a stance
/// hands capacity back, which evolutions exist — is a question about game data
/// this worker does not have, and `wfsim-intake` asks the engine it instead.
function record(b) {
  const rec = { at: new Date().toISOString().slice(0, 10) };
  for (const a of AXES) {
    const v = b[a.key];
    if (a.kind === "id") {
      if (v !== undefined && typeof v !== "string") return { err: `bad ${a.key}` };
      const s = v || "";
      if (a.required ? !ID.test(s) : s && !ID.test(s)) return { err: `bad ${a.key}` };
      if (s) rec[a.key] = s;
    } else {
      const list = v === undefined ? [] : v;
      if (!Array.isArray(list) || list.length > a.max) return { err: `bad ${a.key}` };
      if (!list.every((s) => typeof s === "string" && ID.test(s))) return { err: `bad ${a.key}` };
      if (list.length) rec[a.key] = list;
    }
  }
  return { rec };
}

async function submit(request, env) {
  if (!env.LIBRARY) return bad("the library is not configured", 503);
  const { b, err } = await body(request);
  if (err) return bad(err);

  // NOTHING ABOUT THE SUBMITTER IS STORED. Not the IP, not a token, not a
  // timestamp that could order one person's submissions against another's.
  // The row is the build and the key it hashes to; `at` is the DAY, which is
  // coarse enough to say when and too coarse to identify anyone.
  const built = record(b);
  if (built.err) return bad(built.err);
  const rec = built.rec;

  // INTO THE INBOX, VERBATIM, under an id that means nothing.
  //
  // WHAT A BUILD IS CANNOT BE DECIDED HERE. Telling two apart needs the mod
  // POOL — an elemental card enters the element sequence and a plain one does
  // not, and the sequence decides the pairing (Torid, six mods: 12,424 DPS
  // against 46,583) — and this service has no game data. A key derived without
  // it would be a SECOND answer to the question that must have one, computed by
  // the half with no evidence. So the record is stored as it arrived and
  // `wfsim-intake` derives the key with the engine.
  //
  // AND THE CLIENT MAY NOT DERIVE IT EITHER, though it has the engine: an id
  // that arrives over the wire is an id an attacker chooses, and choosing one
  // is choosing which stored build to overwrite.
  //
  // A RESUBMISSION IS A SECOND ROW HERE and one row in `builds`. This is a
  // queue: two rows costing one intake is cheaper than a read to find out.
  //
  // A FAILURE HERE REACHES THE SUBMITTER, and must. This is the authoritative
  // write, and telling somebody "sent" when it was not is the one answer a
  // submission endpoint may never give.
  try {
    await env.LIBRARY.prepare(
      "INSERT INTO inbox (id, at, record) VALUES (?, ?, ?)",
    ).bind(crypto.randomUUID(), rec.at, JSON.stringify(rec)).run();
  } catch (e) {
    console.log("inbox write failed:", (e && e.message) || String(e));
    return bad("the library could not be written", 503);
  }
  return new Response(JSON.stringify({ ok: true }), {
    headers: { "content-type": "application/json" },
  });
}
/// HOW MANY BUILDS THE LIBRARY HOLDS — the count the static board cannot
/// carry, so the page can say "N builds have arrived since this board was
/// scored".
///
/// ONE QUERY. It was a walk of a key namespace metered at a thousand
/// operations a DAY — seven per reader, which took the board down at a
/// hundred and forty-three of them — and then a counter key with an hourly
/// corrector to avoid the walk. `COUNT(*)` replaces both.
///
/// A COUNT, AND NOTHING ELSE. No build, no weapon, no day: the library holds
/// nothing about a submitter, and this hands back less than it holds.
async function pending(env) {
  if (!env.LIBRARY) return bad("the library is not configured", 503);
  let count = null;
  let owed = null;
  try {
    // BOTH TABLES. A submission that has arrived but has not been through
    // intake yet is in the library as far as the submitter is concerned, and
    // the sentence this feeds is "N have arrived since this board was scored".
    const r = await env.LIBRARY.prepare(
      "SELECT (SELECT COUNT(*) FROM builds) + (SELECT COUNT(*) FROM inbox) AS n",
    ).first();
    const n = Number(r && r.n);
    if (Number.isFinite(n)) count = n;
  } catch (e) {
    console.log("library count failed:", (e && e.message) || String(e));
  }
  try {
    // …AND WHAT IS STILL OWED, PER RULER. The count above answers "did my
    // build arrive"; this answers "is this board finished", which is a
    // different question and the one a reader of the board has. They part
    // company exactly when a person asks for rows to be measured again: nothing
    // has arrived, and the board is about to change anyway.
    //
    // BY RULER, because the page shows one at a time and the three differ
    // sixfold in what they have left.
    const q = await env.LIBRARY.prepare(
      "SELECT ruler, COUNT(*) AS n FROM queue GROUP BY ruler",
    ).all();
    owed = {};
    for (const row of (q && q.results) || []) owed[String(row.ruler)] = Number(row.n);
  } catch (e) {
    // AN ABSENT QUEUE IS NOT ZERO OWED, it is "this cannot be answered" — so
    // `null` travels and the page says nothing rather than "all done".
    console.log("queue count failed:", (e && e.message) || String(e));
    owed = null;
  }
  return new Response(JSON.stringify({ ok: true, count, owed, capped: false }), {
    headers: {
      "content-type": "application/json",
      // A MINUTE. The board moves in hours, so a count up to a minute old is
      // exact enough for the sentence it is in.
      "cache-control": "public, max-age=60",
    },
  });
}

export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
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
    // A HASHED FILE THAT IS GONE IS A 404, AND NOT THE APP.
    //
    // `not_found_handling: single-page-application` answers every unmatched
    // path with index.html and a 200, which is right for a route and wrong for
    // a content-addressed file: an edge holding a page from the previous build
    // asks for `/asset/app.<old>.js`, gets HTML, runs it as JavaScript, and the
    // app never boots — every surface on the site simply gone, with nothing in
    // the console but a syntax error in a file that looks like it loaded.
    //
    // `build_site_app.py` keeps the previous generation so this is rare; this
    // is what makes it LOUD when it happens anyway. Nothing under these two
    // prefixes is ever html, so html here is the fallback and never the file.
    const asset = await env.ASSETS.fetch(request);
    if ((path.startsWith("/asset/") || path.startsWith("/pkg/"))
        && (asset.headers.get("content-type") || "").includes("text/html")) {
      return new Response("not found", { status: 404, headers: { "content-type": "text/plain" } });
    }
    // EVERYTHING ELSE IS THE SITE, unchanged. Handing the request to the assets
    // binding is what keeps this script from becoming a thing the site depends
    // on: it adds one path and forwards the rest.
    return asset;
  },
};
