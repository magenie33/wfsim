// THE HOP NOBODY WAS WATCHING — `worker/index.js`, the one link in the chain
// that is neither the engine nor the page.
//
// A build reaches the board through three things: the PAGE builds a payload,
// the WORKER writes it to KV, the SCORER reads it back and validates it. Two of
// those are covered — `check_parity.mjs` asserts every axis a build has reaches
// the page's payload, and `engine::builds::validate_for_board` is unit-tested
// and is what `/api/board/check` calls before sending. The middle one had no
// test at all, and that is where builds were being lost.
//
// TWICE, the same way. `mode` was sent by the page and never written down, so
// the scorer took its migration fallback and every Incarnon weapon's row said
// `cycle` (2026-08-09). Then `valence`: seven Kuva Nukor submissions refused on
// every scoring run since they arrived — "Kuva Nukor has no Valence element" —
// while the panel had told each submitter "sent", because the field was dropped
// AFTER `/api/board/check` had approved the payload carrying it (owner,
// 2026-08-14). The page could not see it and the engine could not see it.
//
// So this asserts the property rather than the two fields: EVERY KEY THE PAGE
// SENDS SURVIVES INTO STORAGE, and the key a record hashes to tells two builds
// apart whenever any of those keys differs. A third axis added tomorrow fails
// this without anyone remembering to come back.
//
//   node scripts/check_board_submit.mjs
import worker, { AXES } from "../worker/index.js";
import fs from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// A KV stub that records what it was asked to store. The worker touches exactly
// `put`, so this is the whole binding it needs.
const store = () => {
  const rows = new Map();
  return {
    rows,
    put: async (k, v) => { rows.set(k, JSON.parse(v)); },
    // KV LISTS IN PAGES and the endpoint pages through them; the stub answers
    // in one page, which is the shape a store this size really has.
    list: async () => ({ keys: [...rows.keys()].map((name) => ({ name })), list_complete: true }),
  };
};

const post = async (body, kv) =>
  worker.fetch(
    new Request("https://wfsim.app/api/board/submit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    }),
    { SUBMISSIONS: kv, ASSETS: { fetch: async () => new Response("site") } },
  );

// THE PAYLOAD THE PAGE ACTUALLY SENDS, field for field — `boardPayload()` in
// web/src/static/app.js. An adversary weapon, because that is the case with the
// most axes on it.
const PAYLOAD = {
  benchmark: "single_target_v1",
  weapon: "kuva_nukor",
  mode: "base",
  mods: ["hornet_strike", "barrel_diffusion", "primed_target_cracker",
         "primed_heated_charge", "convulsion", "pathogen_rounds",
         "galvanized_diffusion", "galvanized_shot"],
  evolutions: [],
  arcanes: ["secondary_deadhead"],
  valence: "magnetic",
  // STATED AND EMPTY, exactly as the page sends them on a build with no riven.
  // The rule the endpoint applies to both is `valence`'s: an empty optional
  // axis is ABSENT rather than empty, so neither reaches storage here.
  riven_pos: [],
  riven_neg: "",
};

console.log("the board's submission endpoint\n");

// ---- 0. THE WORKER KNOWS EVERY AXIS THE PAGE SENDS ------------------------
//
// THE ASSERTION THAT WOULD HAVE CAUGHT BOTH LOSSES ON THE DAY THEY HAPPENED.
// Everything below tests the worker against a payload written HERE, which is
// only ever as current as this file — and this file did not exist when either
// axis was added. So the first thing checked is the two lists against each
// other: `boardPayload()` in the page is the definition of what a submission
// is, `AXES` is what the endpoint knows how to keep, and a name in one and not
// the other is the bug, before any request is made.
{
  const src = fs.readFileSync("web/src/static/app.js", "utf8");
  const body = src.slice(src.indexOf("function boardPayload()"));
  const ret = body.slice(body.indexOf("return {"), body.indexOf("\n  };"));
  // Keys of the returned literal sit at four spaces, as `name:` or bare
  // `name,`. Anything deeper belongs to a value expression, and a `//` line is
  // a comment — this file has more comment than code.
  const sent = [...ret.matchAll(/^ {4}([A-Za-z_$][\w$]*)\s*[:,]/gm)].map((m) => m[1]);
  const known = AXES.map((a) => a.key);
  const missing = sent.filter((k) => !known.includes(k));
  const extra = known.filter((k) => !sent.includes(k));
  check("the page sends a payload this file could read at all", sent.length > 3, sent.join(","));
  check("every axis the PAGE sends is an axis the WORKER knows",
    missing.length === 0, `worker has no: ${missing.join(", ")}   (page sends ${sent.join(", ")})`);
  check("...and the worker knows no axis the page never sends",
    extra.length === 0, `page never sends: ${extra.join(", ")}`);
}

// ---- 1. EVERY KEY SURVIVES ------------------------------------------------
{
  const kv = store();
  const res = await post(PAYLOAD, kv);
  const body = await res.json();
  check("a complete submission is accepted", res.ok && body.ok === true, JSON.stringify(body));
  check("...and exactly one record is written", kv.rows.size === 1, String(kv.rows.size));
  const rec = [...kv.rows.values()][0];
  // DERIVED, not listed: whatever the page sends is what has to arrive.
  //
  // AN EMPTY AXIS IS EXPECTED TO BE ABSENT, which is the endpoint's own rule
  // and not a concession — a record should no more carry an empty riven than
  // an empty valence, and the assertion below is the other half of it.
  const said = (v) => (Array.isArray(v) ? v.length > 0 : v !== "" && v != null);
  const lost = Object.keys(PAYLOAD).filter(
    (k) => said(PAYLOAD[k]) && JSON.stringify(rec[k]) !== JSON.stringify(PAYLOAD[k]),
  );
  check("every axis the page sends is written down", lost.length === 0,
    `lost: ${lost.join(", ")} — stored ${JSON.stringify(rec)}`);
  const kept_empty = Object.keys(PAYLOAD).filter((k) => !said(PAYLOAD[k]) && k in rec);
  check("...and an axis it has nothing for is absent, not empty",
    kept_empty.length === 0, `stored empty: ${kept_empty.join(", ")}`);
}

// ---- 2. THE KEY TELLS TWO BUILDS APART ------------------------------------
//
// One per axis, walked rather than spelled out. Two submissions differing in
// ANY axis must land on two keys — a shared key is a silent overwrite, and the
// build that loses is the one the player submitted second.
{
  const variants = {
    weapon: "kuva_bramma",
    mode: "cycle",
    valence: "toxin",
    arcanes: ["secondary_encumber"],
    evolutions: ["laetum_devastating_attrition"],
    // The ORDER is the build: the same mods in two orders combine to two
    // different elements, so this must key differently too.
    mods: [PAYLOAD.mods[1], PAYLOAD.mods[0], ...PAYLOAD.mods.slice(2)],
    // A RIVEN'S SHAPE. Two players who rolled different stats did not submit
    // the same build, and a key that could not tell them apart would file the
    // second under the first's number — the failure this file exists for.
    riven_pos: ["critical_damage", "multishot"],
    riven_neg: "recoil",
  };
  for (const [axis, value] of Object.entries(variants)) {
    const kv = store();
    await post(PAYLOAD, kv);
    await post({ ...PAYLOAD, [axis]: value }, kv);
    check(`two builds differing only in \`${axis}\` are two records`,
      kv.rows.size === 2, `${kv.rows.size} record(s): ${[...kv.rows.keys()].join("  |  ")}`);
  }
  // ...and the control: the SAME build twice is one record, or the board would
  // fill with duplicates of whoever pressed the button twice.
  const kv = store();
  await post(PAYLOAD, kv);
  await post(PAYLOAD, kv);
  check("the same build twice is one record", kv.rows.size === 1, String(kv.rows.size));
}

// ---- 3. AN ORDINARY WEAPON IS UNAFFECTED ----------------------------------
//
// Most of the roster has no valence, and it sends the field EMPTY rather than
// omitting it. A shape guard written as `!== undefined` would have turned the
// whole roster away — which is the failure mode of fixing this in a hurry.
{
  const kv = store();
  const plain = { ...PAYLOAD, weapon: "torid", valence: "" };
  const res = await post(plain, kv);
  check("a weapon with no valence still submits", res.ok, String(res.status));
  const rec = [...kv.rows.values()][0];
  check("...and carries no empty valence into storage", rec && !("valence" in rec),
    JSON.stringify(rec));
}

// ---- 4. MALFORMED INPUT IS STILL TURNED AWAY ------------------------------
{
  const kv = store();
  const res = await post({ ...PAYLOAD, valence: "not an id!" }, kv);
  check("a malformed valence is rejected", !res.ok && kv.rows.size === 0, String(res.status));
}

// ---- 5. A BENCHMARK THE WORKER HAS NEVER HEARD OF -------------------------
//
// THERE WILL BE MANY RULERS. `single_target` was alone for months, then a
// companion, then `group_clear` (2026-08-17) — and the point of this block is
// that adding the FOURTH costs nothing here. The worker validates `benchmark`
// as an ID and holds no LIST of them, which is what makes a ruler a data file
// rather than a deploy; this asserts it stays that way.
//
// A NAME NOBODY HAS SEEN, deliberately: submitting to an existing ruler would
// prove only that the existing rulers work.
{
  const kv = store();
  const fresh = { ...PAYLOAD, benchmark: "a_ruler_invented_by_this_check_v3" };
  const res = await post(fresh, kv);
  check("a benchmark the worker has never heard of is accepted", res.ok, String(res.status));
  const rec = [...kv.rows.values()][0];
  check("...and reaches storage under its own name",
    rec && rec.benchmark === fresh.benchmark, JSON.stringify(rec && rec.benchmark));

  // …AND ONE BUILD IS ONE RECORD, whichever ruler it arrived from (owner,
  // 2026-08-25). This assertion used to be its opposite — two rulers scoring one
  // build were two records, because the identity key carried the benchmark — and
  // it was right while a submission was bound to the fight it was measured
  // under. It is not any more: the store is a LIBRARY OF BUILDS and every ruler
  // crosses the whole of it, so the ruler is provenance (`identity: false`) and
  // the same build arriving from two fights is the same build.
  //
  // LAST WRITE WINS, which is what "the same build always produces the same
  // key" has always meant here — a resubmission overwrites the record rather
  // than adding one. That is the right way round for the two fields that are
  // not the build: `at` is the day, so a build somebody is still submitting
  // stays current, and `benchmark` is only where the last submitter happened to
  // be standing. Neither is ranked.
  await post(PAYLOAD, kv);
  check("...and the same build from another ruler is the SAME record",
    kv.rows.size === 1, `${kv.rows.size} records`);
  const only = [...kv.rows.values()][0];
  check("...still carrying a provenance, and the same build",
    !!only && !!only.benchmark && only.weapon === PAYLOAD.weapon,
    JSON.stringify(only && { benchmark: only.benchmark, weapon: only.weapon }));

  // THE NEGATIVE CONTROL FOR THAT COLLAPSE: dropping the ruler must not make
  // every build one record. A build with no ruler at all is legal now — that is
  // what an upload from a scenario of the player's own is — and it is still a
  // record of its own build.
  const kv2 = store();
  const nameless = { ...PAYLOAD };
  delete nameless.benchmark;
  const res2 = await post(nameless, kv2);
  check("a build uploaded from no ruler at all is accepted", res2.ok, String(res2.status));
  await post({ ...nameless, weapon: "braton_prime" }, kv2);
  check("...and two different builds are still two records",
    kv2.rows.size === 2, `${kv2.rows.size} records`);
}

// ---- and the library can say how big it is -----------------------------------
//
// THE BOARD IS A STATIC FILE and always will be — committed to the repo, served
// from the CDN, unblockable and free. What it cannot carry is how far behind it
// is, so the page asks the library for the one number that says: how many
// builds it holds. A COUNT and nothing else, which is also the most the store
// can honestly report, since it keeps nothing about a submitter to report.
{
  const kv = store();
  await post(PAYLOAD, kv);
  await post({ ...PAYLOAD, weapon: "braton_prime" }, kv);
  const res = await worker.fetch(
    new Request("https://wfsim.app/api/board/pending"),
    { SUBMISSIONS: kv, ASSETS: { fetch: async () => new Response("site") } },
  );
  const body = res.ok ? await res.json() : null;
  check("the library reports its own size", !!body && body.ok === true && body.count === 2,
    JSON.stringify(body));
  check("...and reports nothing else about it",
    !!body && Object.keys(body).sort().join(",") === "capped,count,ok",
    JSON.stringify(body && Object.keys(body)));
  const post_ = await worker.fetch(
    new Request("https://wfsim.app/api/board/pending", { method: "POST" }),
    { SUBMISSIONS: kv, ASSETS: { fetch: async () => new Response("site") } },
  );
  check("...and it is a READ, so a POST is refused", post_.status === 405,
    String(post_.status));
}

console.log(
  failures
    ? `\n${failures} failed`
    : "\nevery axis a build has survives the hop into storage",
);
process.exit(failures ? 1 : 0);
