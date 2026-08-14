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
import worker from "../worker/index.js";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// A KV stub that records what it was asked to store. The worker touches exactly
// `put`, so this is the whole binding it needs.
const store = () => {
  const rows = new Map();
  return { rows, put: async (k, v) => { rows.set(k, JSON.parse(v)); } };
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
};

console.log("the board's submission endpoint\n");

// ---- 1. EVERY KEY SURVIVES ------------------------------------------------
{
  const kv = store();
  const res = await post(PAYLOAD, kv);
  const body = await res.json();
  check("a complete submission is accepted", res.ok && body.ok === true, JSON.stringify(body));
  check("...and exactly one record is written", kv.rows.size === 1, String(kv.rows.size));
  const rec = [...kv.rows.values()][0];
  // DERIVED, not listed: whatever the page sends is what has to arrive.
  const lost = Object.keys(PAYLOAD).filter(
    (k) => JSON.stringify(rec[k]) !== JSON.stringify(PAYLOAD[k]),
  );
  check("every axis the page sends is written down", lost.length === 0,
    `lost: ${lost.join(", ")} — stored ${JSON.stringify(rec)}`);
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

console.log(
  failures
    ? `\n${failures} failed`
    : "\nevery axis a build has survives the hop into storage",
);
process.exit(failures ? 1 : 0);
