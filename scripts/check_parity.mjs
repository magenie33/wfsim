// Do the BUILDER and the OPTIMIZER offer the same thing?
//
// They are the same question asked twice — the builder fills a weapon's slots,
// the optimizer searches them — so every axis must present the same options
// and the same visibility on both sides. `weaponAxes()` in app.js exists to
// make that true by construction; this checks that it stayed true.
//
// It has caught, in the two hours it took to write the thing it checks:
//   - the optimizer offering an Exilus and an Arcanes scope on a sentinel
//     weapon, which has neither
//   - the Larkspur being given an exilus slot with no mod that can enter it
//   - the two modules computing the exilus pool from different sources
//     (`poolWithRivens()` vs `currentPool`), agreeing only by coincidence
//
// Usage:
//   node scripts/check_parity.mjs                 serves site/ itself
//   node scripts/check_parity.mjs http://host:port  against a running server
//
// Exits non-zero on any mismatch, so it can gate a push.
import { openApp } from "./cdp.mjs";

// ---- the comparison ------------------------------------------------------
// Both sides are read from the SAME page by calling the functions each module
// actually calls, so this compares behaviour and not a screenshot.
const PROBE = `(async () => {
  const S = (a) => [...new Set(a)].sort();
  const out = [];
  for (const w of META.weapons) {
    switchWeapon(w.id);
    await new Promise((r) => setTimeout(r, 120));
    const AX = weaponAxes(w.id);
    out.push({
      weapon: w.id,
      axes: {
        mods: S(AX.mods.map((m) => m.id)),
        exilus: S(AX.exilus.map((m) => m.id)),
        arcanes: AX.arcanes.map((a) => S(a.options.map((x) => x.id))),
        evolutions: AX.evolutions.map((t) => S(t.options.map((o) => o.id))),
      },
      // THE EXILUS SLOT'S OWN POLARITY. The server sends nine innate slots —
      // eight main plus the exilus — and the client used to slice the ninth
      // off and pad a null over it, so a weapon that comes with an exilus
      // polarity showed an empty one: full drain for the mod in it, and a
      // Forma charged for something the weapon already has.
      innate: { client: innate.slice(), served: (w.innate_polarities || []).slice() },
      // What each module INDEPENDENTLY decides to show.
      shown: {
        builder: { exilus: AX.hasExilus, arcanes: AX.arcanes.length > 0,
                   evolutions: AX.evolutions.length > 0 },
      },
    });
  }
  return out; })()`;

// THE PAGE SAYS WHICH WEAPON IT HAS APPLIED, so the check asks instead of
// betting on a duration. A fixed sleep after a navigation lost the same bet
// twice: four weapons of 355 answered with the PREVIOUS page's blocks still up
// and reported a visibility mismatch that no product change could cause or fix
// (2026-08-21). Deterministic, because those four are simply the ones whose
// apply happens to straddle the window on this machine — which is exactly the
// kind of failure that gets read as a real one.
const applied = (id) => `(() => {
  const el = document.getElementById("weapon");
  const v = (x) => { const e = document.getElementById(x); return !!e && !e.hidden; };
  // THE VALUE AND WHAT IS ON SCREEN. The value alone is not enough: it is set
  // synchronously and the blocks below are drawn once /api/panel answers, so a
  // weapon that follows one WITH evolutions reads the previous page's block.
  return !!el && el.value === ${JSON.stringify(id)}
    ? [v("exilus-block"), v("arcane-block"), v("evo-block"),
       v("opt-exilus-sect"), v("opt-arcanes-sect"), v("opt-evos-sect")].join(",")
    : null;
})()`;

const VISIBLE = `(() => {
  const v = (id) => { const e = document.getElementById(id); return !!e && !e.hidden; };
  return { exilus: v("exilus-block"), arcanes: v("arcane-block"), evolutions: v("evo-block") };
})()`;
const VISIBLE_OPT = `(() => {
  const v = (id) => { const e = document.getElementById(id); return !!e && !e.hidden; };
  return { exilus: v("opt-exilus-sect"), arcanes: v("opt-arcanes-sect"), evolutions: v("opt-evos-sect") };
})()`;

// `node scripts/check_parity.mjs http://host:port` points it at a running
// server instead of the built `site/`.
const app = await openApp({ base: process.argv[2] });
const { send, evaluate, sleep } = app;
const url = app.BASE;
let bad = 0;
// Poll for the applied weapon, with the old fixed wait as the CEILING rather
// than the answer. A timeout is reported through the assertion it breaks — the
// check must not paper over a page that never settles.
const settled = async (id) => {
  // QUIESCENCE, not a guess and not the answer: the same flags twice in a row.
  // Waiting for the page to stop changing cannot mask a real disagreement the
  // way waiting for an expected value would.
  let last = null;
  for (let i = 0; i < 60; i++) {
    const now = await evaluate(applied(id));
    if (now !== null && now === last) return true;
    last = now;
    await sleep(250);
  }
  return false;
};
{
  const rows = await evaluate(PROBE);
  for (const r of rows) {
    const notes = [];
    for (const [k, v] of Object.entries(r.axes)) {
      const n = Array.isArray(v[0]) ? v.map((x) => x.length).join(",") : v.length;
      notes.push(`${k} ${n}`);
    }
    // The two modules render their own visibility; read BOTH pages for real.
    await send("Page.navigate", { url: `${url}/weapons/${r.weapon}` });
    await settled(r.weapon);
    const shownBuilder = await evaluate(VISIBLE);
    await send("Page.navigate", { url: `${url}/weapons/${r.weapon}/optimizer` });
    await settled(r.weapon);
    const shownOpt = await evaluate(VISIBLE_OPT);
    const diffs = Object.keys(shownBuilder)
      .filter((k) => shownBuilder[k] !== shownOpt[k])
      .map((k) => `${k}: builder ${shownBuilder[k]} vs optimizer ${shownOpt[k]}`);
    // An axis that is SHOWN must have options, and one with options must show.
    for (const k of ["exilus", "arcanes", "evolutions"]) {
      const has = k === "exilus" ? r.axes.exilus.length > 0 : r.axes[k].length > 0;
      if (has !== shownBuilder[k]) diffs.push(`${k}: has options ${has} but builder shows ${shownBuilder[k]}`);
    }
    // Nothing the server said about polarities may be lost on the way in.
    const { client, served } = r.innate;
    if (JSON.stringify(client) !== JSON.stringify(served)) {
      diffs.push(`innate polarities: client ${JSON.stringify(client)} vs served ${JSON.stringify(served)}`);
    }
    notes.push(`exilus pol ${client[8] || "—"}`);
    console.log(`${r.weapon.padEnd(20)} ${notes.join("  ").padEnd(66)} ${diffs.length ? "MISMATCH" : "ok"}`);
    diffs.forEach((d) => console.log("    " + d));
    bad += diffs.length;
  }
}
// ---- ...AND THE SAME VISIBILITY, which is the other half of the charter ----
//
// An axis can offer the same options on both sides and still say different
// things about them. The one that mattered: `fully_unmodeled` — a perk whose
// every effect the engine has no rule for — was marked in the OPTIMIZER's
// evolution list and not on the BUILDER's tile, so the surface where the choice
// is actually made showed it as an ordinary pick. Invisible until the roster
// grew: eleven Incarnon weapons landed on 2026-08-08 carrying 31 such perks
// (owner).
//
// Asserted on the SCREEN rather than on the data, because the data was right
// the whole time — both modules read the same `unmodeled` array off `/api/meta`
// and only one of them drew it.
const VIS = await app.evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  const out = { checked: 0, missing: [] };
  for (const w of META.weapons) {
    const info = META.weapons.find(x => x.id === w.id);
    const flagged = [];
    for (const t of (info.evolutions || [])) {
      for (const o of t.options) if ((o.unmodeled || []).length) flagged.push(o.id);
    }
    if (!flagged.length) continue;
    history.pushState({}, '', weaponPath(w.id)); route(); await s(900);
    for (const id of flagged) {
      const card = document.querySelector('#evo-rows .evopick[data-id="' + id + '"]');
      out.checked++;
      if (!card || !card.querySelector('.exchip.unmod')) out.missing.push(w.id + ' / ' + id);
    }
  }
  return out;
})()`);
app.check(`every unmodelled evolution is marked on the BUILDER's tile too (${VIS.checked} of them)`,
  VIS.missing.length === 0, VIS.missing.slice(0, 8).join(", "));

// ---- …AND EVERY AXIS THE BUILD HAS REACHES THE BOARD --------------------
//
// The third place the same axis has to exist. An axis that reaches the BUILDER
// and not the SUBMISSION is the failure this file is about, one surface over:
// the page offers a choice, the player makes it, and the board never hears it.
//
// It cost three submissions. The Kuva Nukor's progenitor element reached the
// builder at 19:13 on 2026-08-13 and `boardPayload` at 22:25 — three hours in
// which wfsim.app let you pick an element and dropped it on the way out, so
// every Nukor build tested in that window is stored with no element and refused
// on every scoring run since (owner, 2026-08-14: "我都选了元素的").
//
// DERIVED FROM THE BUILD, not from a list of axes somebody remembered to
// update: `snapshotState()` IS the build, so every key in it must either travel
// or be named here as something a board row is not. That list is short and
// stating it is the point — a row carries the build, not the layout you reached
// it through.
// WHERE EACH ONE GOES. A map rather than a name match, because the payload is a
// PROJECTION of the build and not a copy of it — `slots` becomes `mods` with
// the exilus one dropped, `evoSel` becomes a list. Adding an axis therefore has
// to say where it travels, which is the whole point: the Nukor's element was
// added to the build and to nothing else.
const TRAVELS_AS = {
  weapon: "weapon",
  mode: "mode",
  slots: "mods",
  evoSel: "evolutions",
  arcane: "arcanes",
  valence: "valence",
};
// …and what a board row deliberately is NOT. Short, and stating it is the
// point: a row carries the build, never the layout you reached it through.
const NOT_A_ROW = [
  "arcaneRank",   // every arcane scores at max rank
];
const AXES = await app.evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  // A weapon that HAS every axis, so nothing is missing for want of a subject.
  history.pushState({}, '', weaponPath('kuva_nukor')); route(); await s(2500);
  const sc = builtinScenarios().find(x => x.builtin === 'single_target');
  pickPreset(scenarioBarCfg(), presetId(sc)); await s(900);
  return { build: Object.keys(snapshotState()), payload: Object.keys(boardPayload() || {}) };
})()`);
const missing = AXES.build.filter((k) =>
  !NOT_A_ROW.includes(k) && !AXES.payload.includes(TRAVELS_AS[k]));
app.check("every axis a build has reaches the board's submission",
  missing.length === 0,
  missing.length ? `dropped on the way out: ${missing.join(", ")}`
    : `${AXES.build.length} axes, ${AXES.payload.length} fields`);
// …and neither list is a way to SILENCE this: every name in both must still be
// a thing the build has.
const stale = [...Object.keys(TRAVELS_AS), ...NOT_A_ROW].filter((k) => !AXES.build.includes(k));
app.check("...and nothing is named here that the build no longer has",
  stale.length === 0, stale.join(", "));

// The table above already names each mismatch; `finish` only has to carry the
// verdict and the exit code.
app.failures += bad;
await app.finish("builder and optimizer agree on every axis");
