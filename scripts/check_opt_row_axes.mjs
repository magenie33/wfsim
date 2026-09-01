// A RESULT ROW STATES EVERY AXIS THE SEARCH VARIED, AND THE ELEMENT IS ONE.
//
// `engine::builds::BUILD_AXES` declares what a build consists of — mods,
// evolutions, arcanes, arcane ranks, mode, assembly, valence, riven — and a row
// that omits one is a ranking nobody can reproduce. The element is the case
// that made this worth a check: an adversary weapon's progenitor is part of
// what the build IS, two Kuva Nukors differing only in it are two builds with
// two scores, and the rows drew everything except that. A search ranging over
// three elements printed three rows that read identically.
//
// ASSERTED ON THE RENDERER, not on a search. `buildContentsHtml` is the one
// describer both the optimizer's rows and the simulator's "open now" line use,
// so a missing axis is missing in both places at once — and driving a real
// search here would spend minutes to exercise a pure function.
//
//   node scripts/check_opt_row_axes.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

// ENGLISH, PINNED. Every name on this line is translated, so a check whose
// needles are English reads a Chinese page as eight missing axes — which is the
// same picture a genuinely missing axis makes. LANG is read at BOOT, so the
// switch is a reload rather than a setting: the pattern `check_disclosure` uses.
await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')`);
await send("Page.navigate", { url: BASE });
await sleep(12000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  const out = { axes: (META.build_axes || []).map((a) => a.id) };

  // ONE BUILD VARYING ON EVERY AXIS AT ONCE. Not a realistic build — a
  // realistic one leaves axes empty, and an empty axis and an unrendered axis
  // look the same, which is the failure this is written to catch.
  out.full = buildContentsHtml({
    mods: ['hornet_strike', 'riven'],
    exilus: 'pistol_amp',
    arcanes: ['secondary_deadhead'],
    arcaneRanks: [5],
    evolutions: ['kuva_nukor_evo1_incarnon_form'],
    modeLabel: 'Incarnon cycle',
    valence: 'magnetic',
    assembly: { grip: 'lovetap', loader: 'flutter' },
    riven: true,
  });

  // …AND ONE VARYING ON NONE OF THEM. What it must NOT do is invent an answer:
  // a line saying "without riven" on a weapon that has none is a claim.
  out.bare = buildContentsHtml({ mods: ['hornet_strike'], arcanes: [] });
  return out;
})()`);

const has = (s) => (needle) => s.toLowerCase().includes(needle.toLowerCase());
const inFull = has(r.full);

check(
  "the engine declares the axes this row has to state",
  Array.isArray(r.axes) && r.axes.length >= 8,
  JSON.stringify(r.axes),
);

// EACH AXIS BY SOMETHING ONLY IT PUTS ON THE LINE. Matching on the rendered
// text rather than on a class keeps this honest about what a READER sees.
for (const [axis, needle] of [
  ["mods", "hornet strike"],
  ["mods (exilus)", "exilus"],
  ["arcanes", "deadhead"],
  ["arcane_ranks", "r5"],
  ["mode", "incarnon cycle"],
  ["assembly", "lovetap"],
  ["valence", "magnetic"],
  ["rivens", "riven"],
]) {
  check(`the row states ${axis}`, inFull(needle), r.full.slice(0, 240));
}

check(
  "…and a build with none of them invents nothing",
  !has(r.bare)("magnetic") && !has(r.bare)("riven") && !has(r.bare)("lovetap"),
  r.bare,
);
check(
  "…while still saying there is no arcane, which is an answer",
  has(r.bare)("arcane"),
  r.bare,
);

process.exit(0);
