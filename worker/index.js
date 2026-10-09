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

import { decodeShare, useShareHost } from "./share_codec.js";
import { accountRoute } from "./accounts.js";
import { cloudPath, cloudRoute } from "./cloud.js";
import { agentRoute } from "./agents.js";
import { BOT_AUTH_DIRECTORY, botAuthDirectory } from "./bot_auth.js";
import { rollupUsage } from "./usage_days.js";
import { EXT_DOCUMENTS, extDocument } from "./ext_documents.js";
import { ocrSample } from "./ocr_samples.js";
import { popularity } from "./popularity.js";
import { qqRoute } from "./qq.js";
import { worldRoute } from "./world.js";
import { appraiseRoute } from "./appraise.js";
import { verifyRoute } from "./verify.js";
import { contributionRoute } from "./contribution.js";
import { liveKey, serveLive, pushLive } from "./live_board.js";

const MAX_BYTES = 4096;        // a build is a few hundred bytes; this is slack
// AN OUTER BOUND, NOT THE RULE — see below. It is `MAIN_SLOTS + 1`: eight main
// slots and the STANCE, which is the one extra card that rides `mods` (the
// exilus has a key of its own). EXPORTED so `check_board_submit.mjs` can hold
// it against the engine's own constant — this file has no game data and cannot
// derive it, so the only thing keeping the two in step is that assertion.
export const MAX_MODS = 9;
const ID_PLAIN = /^[a-z0-9_]{1,64}$/;
// A MOD BELOW ITS MAX RANK is `<card>@<rank>` (`engine::data::mods::RANK_MARK`),
// and only the axes marked `ranked` may carry one.
const RANKED_ID = /^[a-z0-9_]{1,64}(@[0-9]{1,2})?$/;

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
// `engine::board::builds::BUILD_AXES` an entry carries — not this worker's list to
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
  { key: "mods", kind: "ids", max: MAX_MODS, axis: "mods", ranked: true },
  { key: "evolutions", kind: "ids", max: 8, set: true, axis: "evolutions" },
  { key: "arcanes", kind: "ids", max: 4, axis: "arcanes" },
  // WHO HELD IT, and the only row here that is not flat. An Exalted weapon's
  // numbers are its Warframe's ability's, so the frame — and the Operator in
  // the same payload — is part of the BUILD rather than a term of the fight.
  // Which weapons those are is game data this service does not have, so it
  // takes the object from any submission, bounded, and the engine's door drops
  // it wherever a ruler supplies the frame instead.
  { key: "wielder", kind: "nested", max: 4096, axis: "wielder" },
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
  { key: "exilus", kind: "id", axis: "mods", ranked: true },
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
    const ID = a.ranked ? RANKED_ID : ID_PLAIN;
    if (a.kind === "id") {
      if (v !== undefined && typeof v !== "string") return { err: `bad ${a.key}` };
      const s = v || "";
      if (a.required ? !ID.test(s) : s && !ID.test(s)) return { err: `bad ${a.key}` };
      if (s) rec[a.key] = s;
    } else if (a.kind === "nested") {
      // SHAPE AND SIZE, which is all a service with no game data can say about
      // an object. `JSON.stringify` is also the bound: a record is stored
      // verbatim, so what is measured is what is kept.
      if (v === undefined) continue;
      if (v === null || typeof v !== "object" || Array.isArray(v)) return { err: `bad ${a.key}` };
      const text = JSON.stringify(v);
      if (text.length > a.max) return { err: `bad ${a.key}` };
      rec[a.key] = JSON.parse(text);
    } else {
      const list = v === undefined ? [] : v;
      if (!Array.isArray(list) || list.length > a.max) return { err: `bad ${a.key}` };
      if (!list.every((s) => typeof s === "string" && ID.test(s))) return { err: `bad ${a.key}` };
      if (list.length) rec[a.key] = list;
    }
  }
  return { rec };
}

/// A BUILD INTO THE BOARD'S DOOR from inside the worker — an agreed riven gain
/// (appraise.js) — checked and stored exactly as a reader's submission is.
export async function submitRecord(env, build) {
  const built = record(build || {});
  if (built.err || !env.LIBRARY) return false;
  await env.LIBRARY.prepare("INSERT INTO inbox (id, at, record) VALUES (?, ?, ?)")
    .bind(crypto.randomUUID(), built.rec.at, JSON.stringify(built.rec)).run();
  return true;
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

// ---- SHORT SHARE LINKS -----------------------------------------------------
//
// A share code is the build, spelled out (`web/src/static/app/30-share.js`), and
// a posted link carrying it reads as a long random string — which is what
// phishing heuristics look for. So the code is STORED and the link names it by
// a short id: `/weapons/<Wiki_Name>/s/<id>`.
//
// CONTENT-ADDRESSED: the id is a hash of what is stored, computed HERE. The
// same build is always the same id, a write is idempotent (`INSERT OR
// IGNORE`), and a client cannot choose an id and so cannot choose which stored
// link to overwrite. It is a build and never a redirect: nothing stored here
// can send a reader anywhere but the weapon it names.

/// What a share code may contain: the version character, then the alphabet
/// every form travels in — the compact forms' own and base64url's. EXPORTED so
/// `check_share_short.mjs` can hold it against the page's `SHARE_TEXT_OK`.
export const SHARE_CODE = /^[0-4][A-Za-z0-9~.:;,!_%-]{1,1500}$/;
/// A weapon's URL slug (`urlSlug` on the page): every one in the roster is
/// letters, digits, `_` and `-`, so a slash has nowhere to hide.
const SHARE_WEAPON = /^[A-Za-z0-9_-]{1,80}$/;
/// Ten base62 characters is 59 bits: nobody guesses one and no two builds meet.
export const SHARE_ID = /^[0-9A-Za-z]{10}$/;
const B62 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The id of a (weapon, code) pair. The WEAPON is in the hash because the row
/// stores it, and a row whose weapon a first writer could pick would be a page
/// that opens under the wrong name.
export async function shareId(weapon, code, claim) {
  // A CLAIM IS PART OF WHAT IS STORED, so it is part of the id; without one the
  // hash is what it always was, and every id already posted still resolves.
  const text = `${weapon}\n${code}` + (claim ? `\n${JSON.stringify(claim)}` : "");
  const bytes = new Uint8Array(await crypto.subtle.digest(
    "SHA-256", new TextEncoder().encode(text)));
  let n = 0n;
  for (const b of bytes.slice(0, 8)) n = (n << 8n) | BigInt(b);
  let out = "";
  for (let i = 0; i < 10; i++) { out = B62[Number(n % 62n)] + out; n /= 62n; }
  return out;
}

/// Any origin may ask: the desktop client and the dev server both make links
/// that point here, and nothing here depends on who is asking.
const CORS = {
  "access-control-allow-origin": "*",
  "access-control-allow-methods": "GET, POST, OPTIONS",
  "access-control-allow-headers": "content-type",
};
const shareJson = (obj, status = 200, extra = {}) =>
  new Response(JSON.stringify(obj), {
    status, headers: { "content-type": "application/json", ...CORS, ...extra },
  });

async function shareStore(request, env) {
  if (!env.LIBRARY) return shareJson({ ok: false, error: "not configured" }, 503);
  const { b, err } = await body(request);
  if (err) return shareJson({ ok: false, error: err }, 400);
  const weapon = b && b.w, code = b && b.c;
  if (typeof weapon !== "string" || !SHARE_WEAPON.test(weapon)
      || typeof code !== "string" || !SHARE_CODE.test(code)) {
    return shareJson({ ok: false, error: "not a share code" }, 400);
  }
  let claim = null;
  if (b.m !== undefined) {
    claim = shareClaim(b.m, await shareNamesOf(env, request.url));
    if (!claim) return shareJson({ ok: false, error: "not a measurement" }, 400);
  }
  const id = await shareId(weapon, code, claim);
  try {
    // THE DAY, and nothing finer — the same promise every table here makes.
    await env.LIBRARY.prepare(
      "INSERT OR IGNORE INTO shares (id, weapon, code, at, claim) VALUES (?, ?, ?, ?, ?)",
    ).bind(id, weapon, code, new Date().toISOString().slice(0, 10), claim && JSON.stringify(claim)).run();
  } catch (e) {
    console.log("share write failed:", (e && e.message) || String(e));
    return shareJson({ ok: false, error: "could not be stored" }, 503);
  }
  return shareJson({ ok: true, id, path: `/weapons/${weapon}/s/${id}` });
}

// ---- SHARE PREVIEW ---------------------------------------------------------
//
// A PASTED SHORT LINK PREVIEWS AS THE BUILD IT CARRIES. A chat reads the page's
// head without running it, and the weapon's own head describes the BOARD's best
// build — so a link to somebody's build previewed as somebody else's. The code
// is decoded by the page's own codec (`share_codec.js`, generated) against
// `site/share-names.json`, and nothing a sharer typed reaches the preview.

let shareNames = null;
async function shareNamesOf(env, url) {
  if (shareNames) return shareNames;
  const r = await env.ASSETS.fetch(new Request(new URL("/share-names.json", url)));
  if (!r.ok) return null;
  shareNames = await r.json();
  return shareNames;
}

/// THE SHARER'S MEASUREMENT, rebuilt field by field from what arrived, or null.
/// NOTHING TYPED: a scenario and an enemy are ids the names table knows, the
/// metric an id, the value the page's own spelling of a number (`fmtScore`), the
/// rest small integers. A forger can misstate the number and nothing else.
///   s   an official scenario's id — the whole fight, reproducible by anyone
///   k   metric id      v  the value as the page printed it
///   e   enemy id (absent for a target the sharer built)
///   l   level          sp 1 on the Steel Path      d  duration, seconds
export function shareClaim(m, table) {
  if (!m || typeof m !== "object" || Array.isArray(m) || !table) return null;
  const int = (x, lo, hi) => Number.isInteger(x) && x >= lo && x <= hi;
  if (!ID_PLAIN.test(String(m.k)) || String(m.k).length > 12) return null;
  if (typeof m.v !== "string" || !/^\d{1,12}(\.\d{1,12})?$/.test(m.v)) return null;
  const out = { k: m.k, v: m.v };
  if (m.s !== undefined) {
    if (!(table.scenarios || {})[m.s]) return null;
    return { s: m.s, ...out };
  }
  if (m.e !== undefined) {
    if (!(table.enemies || {})[m.e]) return null;
    out.e = m.e;
  }
  if (!int(m.l, 1, 9999) || !int(m.d, 1, 3600) || ![0, 1, undefined].includes(m.sp)) return null;
  out.l = m.l;
  if (m.sp) out.sp = 1;
  out.d = m.d;
  return out;
}

/// The claim as a sentence, or "" — and the number alone, for the title.
/// A metric is named by its id, uppercased: the engine's labels are exactly
/// that (`engine::rules::metrics`), and the id is what the claim carries.
export function claimText(c, table) {
  if (!c) return { line: "", headline: "", where: "" };
  const headline = `${c.v} ${String(c.k).toUpperCase()}`;
  if (c.s) {
    const where = `${table.scenarios[c.s]} benchmark`;
    return { headline, where, line: `${headline} in the ${where}.` };
  }
  const who = c.e ? table.enemies[c.e] : "a custom target";
  const where = `vs ${who} Lv ${c.l}${c.sp ? " SP" : ""}, ${c.d} s — the sharer's own fight`;
  return { headline, where, line: `${headline} ${where}.` };
}

/// What the codec asks of its host, from the names table: the frozen order, and
/// which weapons' evolution ids carry the weapon's own prefix.
export function shareHostOf(table) {
  const to = new Map(), from = new Map();
  table.order.forEach((id, i) => { to.set(id, i); from.set(i, id); });
  const prefixed = new Set(table.evolution_prefixed || []);
  return {
    index: { to, from },
    evoPrefixFor: (w) => (prefixed.has(w) ? w + "_" : ""),
    // A riven's local name is never shown here; its stats are.
    rivenName: () => "",
  };
}

/// A decoded build as the two lines a chat shows. ENGLISH, because the preview
/// is read by whoever the link is posted to and English is the one both markets
/// read; names only from the table, so nothing typed reaches it.
/// A decoded build as the names it shows — one reading for the text and the card.
export function shareBuildNames(d, table, host) {
  const nm = (id) => table.names[id] || String(id).replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  const slots = (d.slots || []).map((s) => s && s.mod).filter(Boolean);
  const pre = host.evoPrefixFor(d.w);
  return {
    weapon: nm(d.w),
    mods: slots.filter((m) => !String(m).startsWith("~")).map(nm),
    rivenSlots: slots.filter((m) => String(m).startsWith("~")).length,
    arcanes: (d.arcane || []).filter((a) => a && a !== "none").map(nm),
    evolutions: (d.evos || []).filter(Boolean).map((e) => nm(pre + e)),
    rivens: (d.rivens || []).map((r) => [
      ...(r.s.bonuses || []).map((b) => "+" + nm(b.id)),
      ...(r.s.malus ? ["−" + nm(r.s.malus.id)] : []),
    ].join(" ")),
  };
}

export function sharePreviewText(d, table, host, claim) {
  const { weapon, mods, arcanes, evolutions: evos, rivens } = shareBuildNames(d, table, host);
  const lines = [
    mods.length ? `Mods: ${mods.join(" · ")}` : "",
    rivens.length ? `Riven: ${rivens.join("; ")}` : "",
    arcanes.length ? `Arcane: ${arcanes.join(" · ")}` : "",
    evos.length ? `Evolutions: ${evos.join(" · ")}` : "",
  ].filter(Boolean);
  const said = claimText(claim, table);
  const build = lines.length ? lines.join(". ") + "." : `A ${weapon} build shared from WFSim.`;
  return {
    title: said.headline ? `${weapon} build — ${said.headline} | WFSim` : `${weapon} build | WFSim`,
    description: said.line ? `${said.line} ${build}` : build,
  };
}

const attr = (s) => String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
/// The weapon page's head, restated for one build: title, description, the OG
/// and Twitter pair, `og:url` naming the link itself — and `noindex`, because a
/// thousand builds of one weapon are one page to a search engine (the canonical
/// still names the weapon).
export function rewriteShareHead(html, preview, url, image) {
  const meta = (key, name, value) => {
    const tag = `<meta ${key}="${name}" content="${attr(value)}" />`;
    const re = new RegExp(`<meta ${key}="${name}" content="[^"]*" />`);
    return (h) => (re.test(h) ? h.replace(re, tag) : h.replace("</head>", `${tag}\n</head>`));
  };
  return [
    (h) => h.replace(/<title>[^<]*<\/title>/, `<title>${attr(preview.title)}</title>`),
    meta("name", "description", preview.description),
    meta("property", "og:title", preview.title),
    meta("property", "og:description", preview.description),
    meta("property", "og:url", url),
    meta("name", "twitter:title", preview.title),
    meta("name", "twitter:description", preview.description),
    meta("name", "robots", "noindex"),
    ...(image ? [
      meta("property", "og:image", image),
      meta("property", "og:image:width", "1200"),
      meta("property", "og:image:height", "630"),
      meta("name", "twitter:card", "summary_large_image"),
      meta("name", "twitter:image", image),
    ] : []),
  ].reduce((h, f) => f(h), html);
}

/// THE CARD'S VERSION, in its address: a card is a pure function of the row and
/// this code, so a URL that names both may be cached for ever. Bump it when
/// `share_card.js` changes what it draws.
const SHARE_CARD_V = "1";

/// The build a short id names, decoded, with its table and claim — or null.
async function shareRowOf(id, env, url) {
  const row = await env.LIBRARY.prepare("SELECT weapon, code, claim FROM shares WHERE id = ?").bind(id).first();
  const table = row && await shareNamesOf(env, url);
  if (!table) return null;
  const host = shareHostOf(table);
  useShareHost(host);
  const d = await decodeShare(row.code);
  return d && d.w ? { d, table, host, claim: row.claim ? JSON.parse(row.claim) : null } : null;
}

/// `/og/s/<id>.png`: the card for one short link, drawn on first ask and kept
/// in the edge cache under its versioned address. A 404 for anything that is
/// not a link, so a chat falls back to no image rather than a broken one.
async function shareCardResponse(id, request, env, ctx) {
  const cache = typeof caches !== "undefined" ? caches.default : null;
  const hit = cache && await cache.match(request);
  if (hit) return hit;
  let got = null;
  try { got = env.LIBRARY && await shareRowOf(id, env, request.url); } catch (e) {
    console.log("share card failed:", (e && e.message) || String(e));
  }
  if (!got) return new Response("not found", { status: 404, headers: { "content-type": "text/plain" } });
  const names = shareBuildNames(got.d, got.table, got.host);
  const { shareCardSvg } = await import("./share_card.js");
  const { sharePng } = await import("./share_png.js");
  const png = await sharePng(shareCardSvg({
    weapon: names.weapon, mods: names.mods, rivenSlots: names.rivenSlots,
    riven: names.rivens.join("; "), arcanes: names.arcanes, evolutions: names.evolutions,
    claim: got.claim ? (({ headline, where }) => ({ headline, line: where }))(claimText(got.claim, got.table)) : null,
  }));
  const res = new Response(png, { headers: {
    "content-type": "image/png",
    "cache-control": "public, max-age=31536000, immutable",
  } });
  if (cache && ctx) ctx.waitUntil(cache.put(request, res.clone()));
  return res;
}

/// The build a short id names, as a preview, or null — no row, an unreadable
/// code, a names table that is not there. Null serves the weapon page unchanged.
async function sharePreviewFor(id, env, url) {
  if (!env.LIBRARY) return null;
  try {
    const got = await shareRowOf(id, env, url);
    return got ? sharePreviewText(got.d, got.table, got.host, got.claim) : null;
  } catch (e) {
    console.log("share preview failed:", (e && e.message) || String(e));
    return null;
  }
}

async function shareFetch(id, env) {
  if (!SHARE_ID.test(id)) return shareJson({ ok: false, error: "not a share id" }, 404);
  if (!env.LIBRARY) return shareJson({ ok: false, error: "not configured" }, 503);
  const row = await env.LIBRARY.prepare("SELECT weapon, code, claim FROM shares WHERE id = ?")
    .bind(id).first();
  if (!row) return shareJson({ ok: false, error: "no such link" }, 404);
  // FOREVER: the id is a hash of the row, so the row under it can never change.
  return shareJson({ ok: true, w: row.weapon, c: row.code, ...(row.claim ? { m: JSON.parse(row.claim) } : {}) }, 200,
    { "cache-control": "public, max-age=31536000, immutable" });
}

// ---- USAGE -----------------------------------------------------------------
//
// ONE DATA POINT PER THING A READER DID, into Workers Analytics Engine
// (`env.USAGE`). The page computes everything on the device, so without this the
// edge cannot tell a bounce from an hour-long search. docs/ANALYTICS.md.

/// THE VOCABULARY, declared once and named by DOMAIN, never by a control: a
/// renamed event is a broken time series, and history cannot be backfilled.
/// EXPORTED so `check_usage_events.mjs` holds it against what the page sends.
export const USAGE_EVENTS = [
  "app.boot", "app.view", "app.error", "engine.fail",
  "builder.weapon", "builder.warframe", "builder.operator", "builder.riven",
  "builder.companion", "builder.enemy",
  "simulator.start", "simulator.run", "optimizer.start", "optimizer.run",
  "share.create", "share.entry", "share.open", "board.open", "board.submit", "desktop.download",
  "presets.saved", "door.seen", "door.open",
  "nona.open", "nona.ask", "nona.concise",
];
/// The wire's schema, written into every point so a later change stays readable.
export const USAGE_SCHEMA = 1;
const USAGE_CID = /^[0-9a-f]{32}$/;
const USAGE_ROUTE = /^[a-z0-9_-]{1,32}$/;
const USAGE_LANG = /^[a-z]{2}(-[a-z0-9]{2,8})?$/;
const USAGE_RELEASE = /^[0-9A-Za-z._-]{1,40}$/;
const USAGE_SHELLS = ["web", "desktop"];
export const USAGE_CRAWLER = /(bot|crawler|spider|slurp)[/;\s)]|headlesschrome|lighthouse/i;

/// The point as it is written, or null for anything that is not one. SHAPE ONLY,
/// like `record`: whether a subject exists is game data this worker has not got.
export function usagePoint(b, country) {
  if (!b || typeof b !== "object" || b.v !== USAGE_SCHEMA) return null;
  const s = (x) => (typeof x === "string" ? x : "");
  const subject = s(b.subject), n = b.n === undefined ? 0 : b.n;
  if (!USAGE_EVENTS.includes(b.e) || !USAGE_CID.test(s(b.cid))
      || !USAGE_ROUTE.test(s(b.route)) || !USAGE_LANG.test(s(b.lang))
      || !USAGE_RELEASE.test(s(b.release)) || !USAGE_SHELLS.includes(b.shell)
      || (subject && !ID_PLAIN.test(subject))
      || typeof n !== "number" || !Number.isFinite(n) || n < 0) return null;
  // THE VISITOR IS THE INDEX, so when the engine samples it keeps or drops a
  // visitor whole — a return rate computed over half-sampled visitors is wrong.
  // NOTHING ELSE ABOUT THEM: no IP, no user agent; the country is the edge's.
  return {
    indexes: [b.cid],
    blobs: [b.e, b.cid, subject, b.route, b.lang, b.shell, b.release,
      /^[A-Z]{2}$/.test(s(country)) ? country : ""],
    doubles: [USAGE_SCHEMA, n],
  };
}

async function usage(request, env) {
  const { b, err } = await body(request);
  if (err) return new Response(null, { status: 400, headers: CORS });
  const point = usagePoint(b, request.cf && request.cf.country);
  if (!point) return new Response(null, { status: 400, headers: CORS });
  if (!env.USAGE) return new Response(null, { status: 503, headers: CORS });
  // A CRAWLER THAT RUNS THE PAGE (Applebot, Googlebot) boots the app like a
  // reader and would be counted as one. Answered, never written.
  if (USAGE_CRAWLER.test(request.headers.get("user-agent") || "")) return new Response(null, { status: 204, headers: CORS });
  // Fire and forget on this side too: `writeDataPoint` queues, it does not wait.
  env.USAGE.writeDataPoint(point);
  return new Response(null, { status: 204, headers: CORS });
}

// ---- THE SITE AS AN AGENT READS IT -----------------------------------------
//
// A page with a markdown twin (`build_site_app.py` `ship_agent_files`) serves
// the twin to `Accept: text/markdown`, and names it and the discovery documents
// in a `Link` header either way — docs/AGENT.md §Machine-readable.

/// The twin of a page that has one, or null.
function markdownTwin(path) {
  if (path === "/") return "/index.md";
  if (path === "/weapons") return "/weapons.md";
  const m = path.match(/^\/weapons\/([^/.]+)$/);
  return m ? `/weapons/${m[1]}.md` : null;
}

/// The page a twin is the twin of, or null — the inverse of `markdownTwin`.
function twinOf(path) {
  if (path === "/index.md") return "/";
  if (path === "/weapons.md") return "/weapons";
  const m = path.match(/^\/weapons\/([^/.]+)\.md$/);
  return m ? `/weapons/${m[1]}` : null;
}

/// A twin fetched by its own address names its page as canonical: without it
/// a search engine finds two copies of every weapon and picks neither.
async function twinFile(request, env, page) {
  const md = await env.ASSETS.fetch(request);
  if (!md.ok || (md.headers.get("content-type") || "").includes("text/html")) return md;
  const headers = new Headers(md.headers);
  headers.set("link", `<https://wfsim.app${page}>; rel="canonical"`);
  return new Response(md.body, { status: md.status, headers });
}

/// Whether the client ranks markdown at least as high as html (RFC 9110 q).
function prefersMarkdown(request) {
  const q = {};
  for (const part of (request.headers.get("accept") || "").split(",")) {
    const [type, ...params] = part.trim().toLowerCase().split(";");
    const w = params.map((p) => p.trim()).find((p) => p.startsWith("q="));
    q[type] = w ? Number(w.slice(2)) || 0 : 1;
  }
  const md = q["text/markdown"] ?? 0;
  return md > 0 && md >= (q["text/html"] ?? 0);
}

const DISCOVERY_LINKS = [
  '</llms.txt>; rel="describedby"; type="text/plain"',
  '</.well-known/api-catalog>; rel="api-catalog"',
  '</.well-known/agent-skills/index.json>; rel="describedby"; type="application/json"',
  '</.well-known/mcp/server-card.json>; rel="service-desc"; type="application/json"',
];

// A `.well-known` document with no extension gets no media type from the asset
// layer, and a scanner that reads the header takes it for something else.
const WELL_KNOWN_TYPES = { "/.well-known/api-catalog": "application/linkset+json" };
// …and the two a registry reads from another origin, which ARD requires to allow.
const WELL_KNOWN_OPEN = new Set(["/.well-known/ai-catalog.json", "/.well-known/agent-card.json"]);

async function agentPage(request, env, twin) {
  const link = [`<${twin}>; rel="alternate"; type="text/markdown"`, ...DISCOVERY_LINKS].join(", ");
  if ((request.method === "GET" || request.method === "HEAD") && prefersMarkdown(request)) {
    const url = new URL(request.url);
    url.pathname = twin;
    const md = await env.ASSETS.fetch(new Request(url.toString(), request));
    // The SPA fallback answers a missing twin with html: serve the page instead.
    if (md.ok && !(md.headers.get("content-type") || "").includes("text/html")) {
      const headers = new Headers(md.headers);
      headers.set("content-type", "text/markdown; charset=utf-8");
      headers.set("vary", "Accept");
      headers.set("link", link);
      return new Response(md.body, { status: 200, headers });
    }
  }
  const page = await env.ASSETS.fetch(request);
  const headers = new Headers(page.headers);
  headers.append("vary", "Accept");
  headers.set("link", link);
  return new Response(page.body, { status: page.status, headers });
}

export default {
  async scheduled(_event, env, ctx) {
    ctx.waitUntil(rollupUsage(env));
  },
  async fetch(request, env, ctx) {
    const path = new URL(request.url).pathname;
    const twin = markdownTwin(path);
    if (twin) return agentPage(request, env, twin);
    const page = twinOf(path);
    if (page) return twinFile(request, env, page);
    // THE AGENT DOCUMENTS AND ENDPOINTS, drawn by the worker from one set of
    // constants (worker/agents.js) rather than served as files.
    const agent = await agentRoute(request, env, path);
    if (agent) return agent;
    if (path === BOT_AUTH_DIRECTORY) return botAuthDirectory(request, env);
    // A `.well-known` path is a document or nothing: html here is the SPA
    // fallback claiming a path a client will parse.
    if (EXT_DOCUMENTS.has(path)) return extDocument(request, env, path);
    if (path.startsWith("/.well-known/")) {
      const doc = await env.ASSETS.fetch(request);
      if ((doc.headers.get("content-type") || "").includes("text/html")) {
        return new Response("not found", { status: 404, headers: { "content-type": "text/plain" } });
      }
      if (!WELL_KNOWN_TYPES[path] && !WELL_KNOWN_OPEN.has(path)) return doc;
      const headers = new Headers(doc.headers);
      if (WELL_KNOWN_TYPES[path]) headers.set("content-type", WELL_KNOWN_TYPES[path]);
      if (WELL_KNOWN_OPEN.has(path)) headers.set("access-control-allow-origin", "*");
      return new Response(doc.body, { status: doc.status, headers });
    }
    {
      const r = await contributionRoute(request, env, path);
      if (r) return r;
    }
    if (path.startsWith("/api/auth/") || path === "/api/account" || path.startsWith("/api/account/")) {
      const r = await accountRoute(request, env, path);
      if (r) return r;
    }
    if (cloudPath(path)) return cloudRoute(request, env, path);
    const card = path.match(/^\/og\/s\/([0-9A-Za-z]{10})\.png$/);
    if (card) return shareCardResponse(card[1], request, env, ctx);
    if (path === "/api/e") {
      if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
      return request.method === "POST" ? usage(request, env) : new Response(null, { status: 405, headers: CORS });
    }
    if (path === "/api/s" || path.startsWith("/api/s/")) {
      if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
      if (path === "/api/s") {
        return request.method === "POST" ? shareStore(request, env) : shareJson({ ok: false, error: "POST only" }, 405);
      }
      return request.method === "GET"
        ? shareFetch(path.slice("/api/s/".length), env)
        : shareJson({ ok: false, error: "GET only" }, 405);
    }
    // A SHORT LINK OPENS THE WEAPON'S OWN PAGE — its prerendered title and
    // preview, which is what a chat shows when the link is pasted. The page
    // itself reads the id and asks `/api/s/<id>` for the build.
    const short = path.match(/^\/weapons\/([^/]+)\/s\/([0-9A-Za-z]{10})(?:\/[0-9A-Za-z]{8})?\/?$/);
    if (short) {
      const page = new URL(request.url);
      page.pathname = `/weapons/${short[1]}`;
      const res = await env.ASSETS.fetch(new Request(page.toString(), request));
      const preview = res.ok && await sharePreviewFor(short[2], env, request.url);
      if (!preview) return new Response(res.body, { status: res.status, headers: res.headers });
      const headers = new Headers(res.headers);
      headers.delete("etag");
      headers.delete("content-length");
      const own = new URL(request.url);
      const image = `${own.origin}/og/s/${short[2]}.png?v=${SHARE_CARD_V}`;
      return new Response(rewriteShareHead(await res.text(), preview, own.origin + own.pathname, image),
        { status: res.status, headers });
    }
    if (path === "/api/qq" || path.startsWith("/api/qq/")) return qqRoute(request, env, path);
    if (path === "/api/world" || path.startsWith("/api/world/")) return worldRoute(request, env, ctx, path);
    if (path.startsWith("/api/appraise/")) return appraiseRoute(request, env, path);
    if (["/api/board/work", "/api/board/verify", "/api/board/release", "/api/board/mine"].includes(path)) return verifyRoute(request, env, path);
    if (path.startsWith("/api/board/live/")) return pushLive(request, env, path.slice("/api/board/live/".length));
    {
      const key = liveKey(path);
      if (key) return serveLive(request, env, ctx, key);
    }
    if (path === "/api/ocr/sample") return ocrSample(request, env);
    if (path === "/api/popularity") return popularity(request, env, ctx);
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
    if ((path.startsWith("/asset/") || path.startsWith("/pkg/") || path.startsWith("/ocr/"))
        && (asset.headers.get("content-type") || "").includes("text/html")) {
      return new Response("not found", { status: 404, headers: { "content-type": "text/plain" } });
    }
    // EVERYTHING ELSE IS THE SITE, unchanged. Handing the request to the assets
    // binding is what keeps this script from becoming a thing the site depends
    // on: it adds one path and forwards the rest.
    return asset;
  },
};
