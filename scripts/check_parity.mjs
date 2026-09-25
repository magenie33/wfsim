// Does every axis the BUILDER shows belong to the weapon, and reach the board?
//
// `weaponAxes()` derives each axis from the weapon it is asked about; this
// asserts that an axis is shown exactly when it has options, that the served
// polarities arrive intact, that an unmodelled evolution is marked where it is
// chosen, and that every axis a build has travels in a board submission.
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
      // THE EXILUS SLOT'S OWN POLARITY, and the STANCE'S. A client that slices
      // one off and pads a null over it shows an empty polarity on a weapon
      // that comes with one: full drain for the mod in it, and a Forma charged
      // for something the weapon already has.
      innate: {
        client: innate.slice(),
        served: (w.innate_polarities || []).slice(),
        stance: w.stance_polarity || null,
      },
    });
  }
  return out; })()`;

// THE PAGE SAYS WHICH WEAPON IT HAS APPLIED, so the check asks instead of
// betting on a duration. A fixed sleep after a navigation lost the same bet
// twice: four weapons of 355 answered with the PREVIOUS page's blocks still up
// and reported a visibility mismatch that no product change could cause or fix. Deterministic, because those four are simply the ones whose
// apply happens to straddle the window on this machine — which is exactly the
// kind of failure that gets read as a real one.
const applied = (id) => `(() => {
  const el = document.getElementById("weapon");
  const v = (x) => { const e = document.getElementById(x); return !!e && !e.hidden; };
  // THE VALUE AND WHAT IS ON SCREEN. The value alone is not enough: it is set
  // synchronously and the blocks below are drawn later, so a weapon that
  // follows one WITH evolutions reads the previous page's block.
  //
  // …AND WAITING FOR QUIESCENCE WAS NOT ENOUGH EITHER. Two identical reads
  // 250 ms apart can both land inside that window, which is where this check's
  // "the builder shows an evolution block for a weapon with none" flake came
  // from — three times over six runs, never the same weapon twice. panelFor
  // is stamped where the blocks are DECIDED, so it is the same fact rather
  // than a longer guess.
  return !!el && el.value === ${JSON.stringify(id)}
    && document.body.dataset.panelFor === ${JSON.stringify(id)}
    ? [v("exilus-block"), v("arcane-block"), v("evo-block")].join(",")
    : null;
})()`;

// …AND WHO THE PAGE THINKS IT IS DRAWING. Carried in the reading rather than
// only in the wait, because this check has a rare mismatch — a builder showing
// an evolution block for a weapon with none, four times over eight runs, never
// the same weapon twice and always one with NO evolutions — that the wait was
// tightened for and did not stop. A failure that names `for` and `sel` says in
// one line whether the page was mid-switch or genuinely wrong, which is the
// difference between a flake and a bug and is not otherwise recoverable from
// the output.
const VISIBLE = `(() => {
  const v = (id) => { const e = document.getElementById(id); return !!e && !e.hidden; };
  return { exilus: v("exilus-block"), arcanes: v("arcane-block"), evolutions: v("evo-block"),
           for: document.body.dataset.panelFor || null,
           sel: (document.getElementById("weapon") || {}).value || null,
           axes: (weaponAxes($("weapon").value).evolutions || []).length };
})()`;
// `node scripts/check_parity.mjs http://host:port` points it at a running
// server instead of the built `site/`.
const app = await openApp({ base: process.argv[2] });

// ---- AN AXIS DESCRIBES THE WEAPON IT WAS ASKED ABOUT ----------------------
//
// `weaponAxes(id)` derives every axis from the `id` it is handed. An axis that
// reads the live `#weapon` select instead can return one weapon's mods and
// another's evolution tiers, and the caller cannot see it:
// `show("evo-block", AX.evolutions.length > 0)` then draws the block for a
// weapon with none.
//
// THAT IS THIS CHECK'S OWN INTERMITTENT FAILURE, demonstrated rather than
// guessed at: "the builder shows an evolution block for a weapon with none",
// four times over eight runs, never the same weapon twice, always a weapon
// with NO evolutions. Reproducing it by re-running the 219-weapon
// sweep is a coin flip that costs seven minutes; this states the PROPERTY, so
// it is deterministic and costs nothing.
const CROSSED = `(() => {
  const withEvo = META.weapons.find((w) => (w.evolutions || []).length);
  const none = META.weapons.find((w) => !(w.evolutions || []).length);
  const was = $("weapon").value;
  $("weapon").value = withEvo.id;                 // the select says one weapon…
  const asked = weaponAxes(none.id).evolutions.length;   // …ask about another
  $("weapon").value = was;
  return { a: withEvo.id, b: none.id, asked };
})()`;
{
  const x = await app.evaluate(CROSSED);
  app.check("an axis describes the weapon it was ASKED about, not the one on screen",
    x.asked === 0,
    `#weapon said ${x.a} and weaponAxes(${x.b}) answered with ${x.asked} evolution tiers`);
}

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
    await send("Page.navigate", { url: `${url}/weapons/${r.weapon}` });
    await settled(r.weapon);
    const shownBuilder = await evaluate(VISIBLE);
    const diffs = [];
    // An axis that is SHOWN must have options, and one with options must show.
    // `for`/`sel`/`axes` in the reading say whether the page was mid-switch.
    for (const k of ["exilus", "arcanes", "evolutions"]) {
      const has = k === "exilus" ? r.axes.exilus.length > 0 : r.axes[k].length > 0;
      if (has !== shownBuilder[k]) {
        diffs.push(`${k}: has options ${has} but builder shows ${shownBuilder[k]}`
          + ` [saw ${shownBuilder.for}/${shownBuilder.sel}/${shownBuilder.axes}]`);
      }
    }
    // Nothing the server said about polarities may be lost on the way in.
    //
    // PER INDEX, NOT WHOLE ARRAYS. The client's is fixed at ten — eight main,
    // the exilus, and the STANCE, whose polarity is a field of its own rather
    // than an entry in `innate_polarities` — while the server's LENGTH is the
    // weapon's slot count. Comparing the two whole would assert how the client
    // pads, which is not a property, and it reported every sentinel as a
    // mismatch for having eight served against ten held.
    const { client, served, stance } = r.innate;
    served.forEach((p, i) => {
      if ((client[i] ?? null) !== (p ?? null)) {
        diffs.push(`innate polarity ${i}: client ${client[i] ?? "—"} vs served ${p ?? "—"}`);
      }
    });
    if ((client[9] ?? null) !== (stance ?? null)) {
      diffs.push(`stance polarity: client ${client[9] ?? "—"} vs served ${stance ?? "—"}`);
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
// grew: eleven Incarnon weapons landed on 2026-08-08 carrying 31 such perks.
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
      // THE ROW YOU CHOOSE FROM. A tier is a dropdown, so what to read is a
      // list row — rendered with ddRender, the same function the popover
      // calls, so this is real markup rather than the registry.
      let card = null;
      for (const b of document.querySelectorAll('[data-slot^="dd-evo-"]')) {
        ddRender(b.dataset.slot);
        card = document.querySelector('#dd-menu .opt[data-v="' + id + '"]');
        if (card) break;
      }
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
// It cost three submissions: the Kuva Nukor's progenitor element reached the
// builder three hours before `boardPayload`, so every Nukor build tested in
// that window is stored with no element and refused on every scoring run since.
//
// DERIVED FROM THE BUILD rather than from a list somebody remembered to update:
// `snapshotState()` IS the build, so every key in it must either travel or be
// named here as something a board row is not.
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
  // ONE STATE KEY, TWO PAYLOAD FIELDS. A modular weapon's parts are one fact on
  // the page and two flat ids in a board record — the worker validates `id` and
  // `ids` and has no game data to check an object against — so the mapping is a
  // LIST here. Spellings are per-protocol and always have been; what is shared
  // is the axis, which both fields name.
  assembly: ["grip", "loader"],
};
// …and what a board row deliberately is NOT. Short, and stating it is the
// point: a row carries the build, never the layout you reached it through.
const NOT_A_ROW = [
  "arcaneRank",   // every arcane scores at max rank
  "wielder",      // every ruler scores in the Prototype's hands
];
const AXES = await app.evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  // A weapon that HAS every axis, so nothing is missing for want of a subject.
  history.pushState({}, '', weaponPath('kuva_nukor')); route(); await s(2500);
  const sc = builtinScenarios().find(x => x.builtin === 'standard_single_target');
  pickPreset(scenarioBarCfg(), presetId(sc)); await s(900);
  return { build: Object.keys(snapshotState()), payload: Object.keys(boardPayload() || {}) };
})()`);
const asList = (v) => (Array.isArray(v) ? v : [v]);
const missing = AXES.build.filter((k) =>
  !NOT_A_ROW.includes(k) && !asList(TRAVELS_AS[k]).every((f) => AXES.payload.includes(f)));
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
await app.finish("every axis the builder shows is the weapon's, and reaches the board");
