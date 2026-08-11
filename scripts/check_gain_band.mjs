// THE TWENTY-SIXTH: a quick-calc chip says how well it knows its own number.
//
// "≈0%" was one string for two different findings — a mod that does nothing,
// and a mod nobody measured hard enough — and only the difference between them
// is actionable: the first says pick something else, the second says raise the
// runs (owner, 2026-08-12: "就不要出现约等于0的情况"). So a chip either states a
// number it is sure of, or states the number AND its width.
//
// It also asserts the two properties that make the width trustworthy, because
// both were wrong at once and either one alone brings the symptom back:
//
//   1. The scan reads the MEAN of the runs it paid for, not the median run.
//      `score`/`dps` are one engagement however many were run — at 10 runs the
//      median moved 9.8% between seeds where the mean moved 5.9%.
//   2. The width comes from the SERVER's spread over those runs, not from
//      running the reference a second time at another seed. That second run was
//      a single sample of the spread, and on identical inputs it answered
//      anywhere from 0.7% to 11.2% — so the same scan censored every chip or
//      none of them, at random.
//
// The NEGATIVE CONTROL is the pair the bug was reported on: Serration and
// Amalgam Serration differ only in base damage, so neither re-rolls the fight,
// both compare against the reference exactly, and both must print a bare
// number in the order their cards state. A band on those two would mean the
// pairing was lost, which is how a 3.8% difference gets printed upside down.
import { openApp } from "./cdp.mjs";

let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? " ok " : "FAIL"}  ${name}${detail ? `  — ${detail}` : ""}`);
  if (!ok) failed++;
};

const app = await openApp({ boot: 12000 });

const r = await app.evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Latron_Prime'); route(); await sleep(3500);

  // THE SERVER'S OWN ANSWER first, so the rest can be read against it.
  const sim = (mods, runs) => api('/api/simulate', {
    weapon: 'latron_prime', mods, evolutions: [], arcane: [], rivens: [],
    enemy: 'thrax_centurion', level: 9999, steel_path: true,
    aiming: false, headshot_pct: 0, duration: 60, runs, seed: 0x5EED });
  const REST = ['split_chamber','point_strike','vital_sense','galvanized_aptitude',
    'primed_cryo_rounds','malignant_force','hellfire'];
  const [b10, b100] = await Promise.all([sim(REST, 10), sim(REST, 100)]);
  const ser = await sim([...REST, 'serration'], 10);
  const ama = await sim([...REST, 'amalgam_serration'], 10);

  // …then a REAL scan, driven the way a player drives it: open a mod slot.
  const slot = document.querySelector('.slot');
  if (slot) slot.click();
  await sleep(600);
  for (let i = 0; i < 90 && (!gainScan.key || gainScan.running); i++) await sleep(500);

  // THE SCAN'S OWN FIGHT, asked of the server directly: the scan runs the
  // active scenario against the build on screen, which is neither the fight nor
  // the build the fixture above uses. Comparing its floor against anything else
  // compares two different measurements.
  const own = await api('/api/simulate',
    { ...buildPayload(), ...fightPayload(gainScenario().scenario) });

  const chips = [...document.querySelectorAll('.gainchip')].map((e) => ({
    text: e.textContent.trim(), title: e.getAttribute('title') || '', cls: e.className }));
  return {
    b10, b100, ser, ama,
    floor: gainScan.floor, base: gainScan.base, running: gainScan.running,
    own: { mean: own && own.score_mean, se: own && own.score_se, dps: own && own.dps_mean },
    n: Object.keys(gainScan.by).length,
    // Every gain the scan produced, with the width it claims for it.
    gains: Object.entries(gainScan.by).map(([id, g]) =>
      ({ id, pct: g.pct, se: g.se, diverged: g.diverged })),
    chips,
  };
})()`);

// ---- 1. the server reports the mean and its spread ----------------------
check("the sim reports the MEAN, not only the median run",
  typeof r.b10.score_mean === "number" && r.b10.score_mean > 0,
  `score ${r.b10.score} score_mean ${r.b10.score_mean}`);
check("...and the spread it measured over those runs",
  typeof r.b10.score_se === "number" && r.b10.score_se > 0
  && typeof r.b10.dps_se === "number" && r.b10.dps_se > 0,
  `score_se ${r.b10.score_se} dps_se ${r.b10.dps_se}`);
// The one property that makes it a standard error rather than a number: ten
// times the runs must narrow it by about sqrt(10). Loose bounds — the runs
// inside one call are mildly correlated — but a se that does NOT fall with the
// run count is not measuring what it claims to.
const ratio = (r.b10.score_se / r.b10.score_mean) / (r.b100.score_se / r.b100.score_mean);
check("...and it narrows with the run count, as a standard error must",
  ratio > 2.2 && ratio < 4.5, `10 runs vs 100 runs: ${ratio.toFixed(2)}x (sqrt(10) = 3.16)`);

// ---- 2. the scan uses it, and burns no second seed ----------------------
check("the scan ran and knows its own resolution",
  !r.running && r.n > 0 && r.floor > 0, `${r.n} candidates, floor ${r.floor}`);
// Same fight, same build, same run count — so the scan's resolution IS the
// server's, to the width of one re-measurement. A second-seed guess would land
// anywhere; this must land on the number.
const served = r.own.se / r.own.mean;
check("the resolution is the server's, not a second-seed guess",
  Math.abs(r.floor - served) < 0.25 * served, `scan ${r.floor} vs server ${served}`);
check("every gain carries a width", r.gains.length > 0 && r.gains.every((g) => typeof g.se === "number"),
  `${r.gains.filter((g) => typeof g.se !== "number").length} without one`);

// ---- 3. no chip collapses to "about nothing" ---------------------------
check("some chips were drawn", r.chips.length > 0, `${r.chips.length}`);
// NEITHER shape of nothing: not the censored "≈0%" that hid an unmeasured
// gain, and not the bare "+0.00%" that hid a mod this fight cannot use.
const zeroish = (c) => /^≈?[+−-]?0(\.0+)?%$/.test(c.text);
check("NO chip reads ≈0% or +0.00%", !r.chips.some(zeroish),
  r.chips.filter(zeroish).map((c) => c.text).join(", "));
const noEffect = r.chips.filter((c) => /no effect here|无效果/.test(c.text));
check("...a measured zero says so in words instead", noEffect.length > 0,
  `${noEffect.length} of ${r.chips.length}`);
// An unresolved chip states its width; a resolved one does not pretend to have
// been re-rolled. Both shapes must actually occur, or one of the two branches
// is dead and this check is asserting nothing.
const banded = r.chips.filter((c) => /±/.test(c.text));
const bare = r.chips.filter((c) => !/±/.test(c.text));
check("an unresolved chip states its width", banded.length > 0 || r.gains.every((g) => !g.diverged),
  `${banded.length} banded of ${r.chips.length}`);
check("...and an exactly-paired one does not", bare.length > 0, `${bare.length} bare of ${r.chips.length}`);
check("a banded chip explains the width in its tooltip",
  banded.every((c) => /±/.test(c.title)), banded[0] && banded[0].title);

// ---- 4. the negative control: the pair the bug was reported on ----------
// Same fight, same statuses — so the comparison is paired and exact, and the
// order is the one the two cards state (+165% against +155%).
const gain = (x) => x.score_mean / r.b10.score_mean - 1;
check("Serration and Amalgam Serration do not re-roll the fight",
  r.ser.procs === r.b10.procs && r.ama.procs === r.b10.procs,
  `ref ${r.b10.procs} · serration ${r.ser.procs} · amalgam ${r.ama.procs}`);
check("...so the 165%/155% order is measured, not a coin flip",
  gain(r.ser) > gain(r.ama),
  `serration ${(gain(r.ser) * 100).toFixed(2)}% vs amalgam ${(gain(r.ama) * 100).toFixed(2)}%`);
// The gap is 3.8% of the reference. At 10 runs the raw spread is ~13%, so this
// ordering is ONLY right because the two are paired against the same luck —
// which is the thing that must not silently regress.
check("...and the gap is exactly what the two cards differ by",
  Math.abs((1 + gain(r.ama)) / (1 + gain(r.ser)) - 2.55 / 2.65) < 0.001,
  `ratio ${((1 + gain(r.ama)) / (1 + gain(r.ser))).toFixed(4)}, cards say ${(2.55 / 2.65).toFixed(4)}`);

console.log(failed ? `\n${failed} failed` : "\na quick-calc chip states how well it knows its number");
await app.finish("gain-band");
process.exit(failed ? 1 : 0);
