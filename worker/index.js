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

/// WHAT A BUILD IS, declared ONCE. Three things are derived from it — the shape
/// check, the stored record and the identity key — because three hand-written
/// lists is a defect generator: adding an axis to some and not the others
/// produces an INCOMPLETE record and a scorer that quietly refuses it.
///
/// `kind`: `id` is a single slug — `required` ones present and non-empty, the
/// rest optional, and an EMPTY optional axis is not written at all — and `ids`
/// is a list, always written. `set` means the ORDER does not matter, so the key
/// sorts it: evolutions are a set, mods are not.
///
/// SHAPE ONLY: whether these ids exist and whether the build is legal are the
/// engine's questions, answered by `engine::builds::validate_for_board` in the
/// scoring job. The cost is a little junk in KV; the alternative is two rules
/// that drift.
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

/// The BUILD this is, as one stable key — every axis of it, in `AXES` order.
///
/// EVERY axis, which is the whole point of deriving it: a key that cannot tell
/// two builds apart files the second under the first's number, silently, and
/// the build that loses is the one submitted second. Writes stay idempotent
/// because the same build always produces the same key.
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
/// Extracted so `/submit` and `/disagree` are held to ONE definition. A report
/// about a row the scorer would never file under that key is a report nobody
/// can act on, and two copies of this loop is how the two would drift apart.
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

  // ONE ROW, AND NOTHING ELSE TO KEEP IN STEP. The key is the build, so a
  // resubmission is the same row — `INSERT OR REPLACE`, with no read to ask
  // whether it is new and no counter to bump: "how many builds are there" is
  // a COUNT, which is most of why the library lives in a database at all
  // (docs/BOARD.md §"One database").
  //
  // A FAILURE HERE REACHES THE SUBMITTER, and must. This is the
  // authoritative write, and telling somebody "sent" when it was not is the
  // one answer a submission endpoint may never give.
  try {
    await env.LIBRARY.prepare(
      "INSERT OR REPLACE INTO builds (identity, at, record) VALUES (?, ?, ?)",
    ).bind(identity(rec), rec.at, JSON.stringify(rec)).run();
  } catch (e) {
    console.log("library write failed:", (e && e.message) || String(e));
    return bad("the library could not be written", 503);
  }
  return new Response(JSON.stringify({ ok: true }), {
    headers: { "content-type": "application/json" },
  });
}
/// TWO MEASUREMENTS OF ONE ROW DISAGREE, and the board should look again.
///
/// A HASH CANNOT ASK THIS. What a row READS is enumerable from the row, so a
/// data change is caught exactly; what the code DOES to it is not, and a
/// change that moves a number leaves every fingerprint agreeing. The audit
/// re-fights published rows to find those and crosses the board in days —
/// while a player runs the one build they care about and finds it at once.
///
/// NOTHING HERE IS TRUSTED AS A SCORE. The numbers are a REPORT that two
/// measurements differ; the board answers by measuring again, and only its
/// own measurement can move a row. The worst a forged report buys is one
/// wasted rescore, which is why this needs no authentication and never will.
///
/// THE ONE EVENT IN THIS SYSTEM. Nobody can derive it from anything else, so
/// it gets a table of its own — keyed by the ROW, so a thousand players
/// finding one disagreement leave one report.
async function disagree(request, env) {
  if (!env.LIBRARY) return bad("the library is not configured", 503);
  const { b, err } = await body(request);
  if (err) return bad(err);

  const num = (x) => (typeof x === "number" && Number.isFinite(x) && x > 0 ? x : null);
  const client = num(b.client);
  const board = num(b.board);
  if (client === null || board === null) return bad("client and board must be positive numbers");
  // THE RULER IS REQUIRED HERE, unlike on a submission. A build carries no
  // ruler because every ruler crosses the library; a DISAGREEMENT is about
  // one row, and a row is a build under one ruler.
  if (typeof b.benchmark !== "string" || !ID.test(b.benchmark)) return bad("bad benchmark");

  const built = record(b);
  if (built.err) return bad(built.err);
  try {
    await env.LIBRARY.prepare(
      "INSERT OR REPLACE INTO disagreements (ruler, identity, at, client, board, record)"
      + " VALUES (?, ?, ?, ?, ?, ?)",
    ).bind(
      b.benchmark, identity(built.rec), built.rec.at, client, board,
      JSON.stringify(built.rec),
    ).run();
  } catch (e) {
    console.log("disagreement write failed:", (e && e.message) || String(e));
    return bad("the report could not be written", 503);
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
  try {
    const r = await env.LIBRARY.prepare("SELECT COUNT(*) AS n FROM builds").first();
    const n = Number(r && r.n);
    if (Number.isFinite(n)) count = n;
  } catch (e) {
    console.log("library count failed:", (e && e.message) || String(e));
  }
  return new Response(JSON.stringify({ ok: true, count, capped: false }), {
    headers: {
      "content-type": "application/json",
      // A MINUTE. The board moves in hours, so a count up to a minute old is
      // exact enough for the sentence it is in.
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
/// THE TABLE CANNOT HOLD MORE. One row per Ko-fi message id and a DAY: no
/// amount, no name, no email, no message. Asked for a total, this worker
/// could not produce one.
async function supporters(env) {
  if (!env.LIBRARY) return bad("the library is not configured", 503);
  let count = null;
  try {
    const r = await env.LIBRARY.prepare("SELECT COUNT(*) AS n FROM supporters").first();
    const n = Number(r && r.n);
    if (Number.isFinite(n)) count = n;
  } catch (e) {
    console.log("supporter count failed:", (e && e.message) || String(e));
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
/// carrying json, and the json carries a `verification_token` only the account
/// owner can read off their own dashboard. That token is the whole of the
/// authentication and it is a SECRET (`wrangler secret put KOFI_TOKEN`) —
/// without it configured this refuses everything, because an endpoint that
/// counts anonymous POSTs is a counter anybody can drive.
///
/// IDEMPOTENT ON THE MESSAGE ID. Ko-fi retries a delivery it did not see
/// acknowledged, and a retry must not be a second supporter — the id is the
/// key, so a replay writes the same row and the count does not move.
///
/// WHAT IS DROPPED, before anything is written: the amount, the supporter's
/// name and email, the message they typed, and the timestamp's time. What is
/// kept is that a payment happened, on a day.
async function kofi(request, env) {
  if (!env.LIBRARY) return bad("the library is not configured", 503);
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
  try {
    await env.LIBRARY.prepare(
      "INSERT OR REPLACE INTO supporters (message_id, at) VALUES (?, ?)",
    ).bind(id, new Date().toISOString().slice(0, 10)).run();
  } catch (e) {
    console.log("supporter write failed:", (e && e.message) || String(e));
    return bad("the supporter could not be recorded", 503);
  }
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
    // A DISAGREEMENT IS NOT A SCORE. What it stores is a REPORT that two
    // measurements of one row differ, which the board answers by measuring
    // again — the number a browser sends never reaches a ranking.
    if (path === "/api/board/disagree") {
      return request.method === "POST"
        ? disagree(request, env)
        : bad("POST only", 405);
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
