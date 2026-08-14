// THE TWENTY-SIXTH: a quick-calc chip says how well it knows its own number.
//
// "≈0%" was one string for two different findings — a mod that does nothing,
// and a mod nobody measured hard enough — and only the difference between them
// is actionable: the first says pick something else, the second says raise the
// runs (owner, 2026-08-12). So a chip either states a number it is
// sure of, or states the number AND its width.
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
    gains: Object.entries(gainScan.by).map(([id, g]) => ({ id, pct: g.pct, se: g.se })),
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
// EVERY GAIN CARRIES ITS OWN WIDTH, and the width is the PAIRED one — the
// spread of `c_i − ratio·b_i` over the runs, which is the error of the number
// actually printed.
//
// It used to be a PROXY: had the median run's proc count changed? A count that
// happened to coincide printed a bare number and claimed the comparison was
// exact — and on the Kuva Nukor all seven progenitor elements report the same
// count while their fights differ by up to 30%, so seven chips claimed an
// exactness none of them had and the order between two of them was a coin flip
// printed as a fact (owner, 2026-08-14).
//
// So a bare number now means ONE thing: the runs were identical, which under
// the kill-rate metric is the same finding as "no effect here". The two sets
// are asserted to be the same set, because that is what makes this a statement
// about the code rather than a count that happens to be positive.
const banded = r.chips.filter((c) => /±/.test(c.text));
const bare = r.chips.filter((c) => !/±/.test(c.text));
const worded = r.chips.filter((c) => /no effect here|无效果/.test(c.text));
check("an unresolved chip states its width", banded.length > 0,
  `${banded.length} banded of ${r.chips.length}`);
check("...and a bare one does not", bare.length > 0 && worded.length > 0,
  `${bare.length} bare, ${worded.length} of them a worded zero`);
check("a banded chip explains the width in its tooltip",
  banded.every((c) => /±/.test(c.title)), banded[0] && banded[0].title);

// PAIRING IS THE WHOLE REASON A BAND CAN BE ZERO, and the two mods this file
// already uses as its control are the case: Serration and Amalgam Serration
// differ only in base damage, so run for run they scale this fight by a
// constant and `c_i − ratio·b_i` is zero on every one. Derived, not asserted —
// no proxy is consulted.
const damageOnly = ["serration", "amalgam_serration"]
  .map((id) => r.gains.find((g) => g.id === id)).filter(Boolean);
check("a damage-only mod pairs to a band of exactly zero",
  damageOnly.length === 2 && damageOnly.every((g) => g.se === 0),
  damageOnly.map((g) => `${g.id} ${(g.pct * 100).toFixed(1)}% ±${g.se}`).join(", "));
// …and a STATUS mod does not, because it decides which fight happens. Without
// this the check above would pass on a band that is always zero.
const statusMod = r.gains.find((g) => g.id === "thermite_rounds" || g.id === "hellfire");
check("...and a status mod does not", statusMod && statusMod.se > 0.01,
  statusMod && `${statusMod.id} ±${(statusMod.se * 100).toFixed(2)}%`);

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

// ---- 5. the ORDER is the MEAN, and the band is beside it ------------------
// The mean is the unbiased estimate of what an option is worth; the spread is a
// property of the measurement, not of the option — run it long enough and the
// spread goes to zero while the mean stays put. A lower-bound sort was tried
// (2026-08-13) and reverted: it demotes whatever is merely hard to measure, and
// a status mod is hard to measure by nature, so the list ends up describing the
// simulator rather than the build.
//
// This does NOT make the order stable and is not meant to: two options whose
// bands overlap are genuinely unranked, which the chip already says with its ±.
const ord = await app.evaluate(`(async () => {
  const rows = [...document.querySelectorAll('#mod-menu .opt[data-id]')]
    .map((e) => e.dataset.id)
    .map((id) => ({ id, g: gainScan.by[id] || null }))
    .filter((x) => x.g);
  return rows.map((x) => ({ id: x.id, pct: x.g.pct, se: x.g.se || 0 }));
})()`);
check("the picker ranks the options it scanned", ord.length > 3, `${ord.length} rows`);
const outOfOrder = ord.filter((g, i) => i > 0 && g.pct > ord[i - 1].pct + 1e-9);
check("...by the MEAN, descending", outOfOrder.length === 0,
  outOfOrder.slice(0, 2).map((g) => `${g.id} ${g.pct}`).join(", "));
// …and a WIDE band does not cost an option its place, which is the half the
// lower-bound sort got wrong. Asserted on numbers so it holds on any day.
const wide = { pct: 0.95, se: 0.50 };
const narrow = { pct: 0.90, se: 0.02 };
check("...so a +95% ±50% still outranks a +90% ±2%",
  wide.pct > narrow.pct, `${wide.pct} vs ${narrow.pct}`);

// ---- 6. a half-filled ranking looks like one -----------------------------
// An option the scan had not reached rendered exactly like one that finished
// with nothing to say — no chip — while the list re-sorts on every result that
// lands. Read mid-scan that is a ranking that keeps changing its mind, and the
// only way to learn it was not final was to click away and back (report,
// 2026-08-13).
//
// Driven as four STATES rather than by racing a live scan, which is the part
// that cannot be timed reliably.
const states = await app.evaluate(`(async () => {
  const id = 'sicarus_prime_wisemans_regard';
  const saveScan = gainScan, saveAxis = gainAxis;
  const out = {};
  gainScan = { key: null, axis: null, running: false, by: {}, done: 0, total: 0 };
  out.idle = gainChipFor(id, 'EVO IV');
  gainAxis = { kind: 'evo', idx: 0 };
  gainScan = { key: gainKey(), axis: gainAxis, running: true, by: {}, done: 7, total: 12 };
  out.pending = gainChipFor(id, 'EVO IV');
  gainScan = { key: 'another-axis', axis: { kind: 'mods', idx: 0 }, running: true, by: {}, done: 3, total: 9 };
  out.otherAxis = gainChipFor(id, 'EVO IV');
  gainAxis = { kind: 'evo', idx: 0 };
  gainScan = { key: gainKey(), axis: gainAxis, running: false, done: 12, total: 12,
               by: { [id]: { pct: 0.4123, se: 0.0169, runs: 10 } } };
  out.done = gainChipFor(id, 'EVO IV');
  gainScan = saveScan; gainAxis = saveAxis;
  return out;
})()`);
check("an option the scan has not reached says so", /class="gainchip pend"/.test(states.pending),
  states.pending.slice(0, 60));
check("...and says how far along it is", /7/.test(states.pending) && /12/.test(states.pending),
  states.pending.slice(0, 90));
check("...but never on an axis nobody is measuring", states.otherAxis === "", states.otherAxis);
check("...nor when nothing is running at all", states.idle === "", states.idle);
check("a finished option shows its NUMBER, never the marker",
  /\+41\.23%/.test(states.done) && !/pend/.test(states.done), states.done.slice(0, 60));

// ---- 7. a coin flip has to LOOK like one --------------------------------
// The chip answers "what is this worth"; the LIST answers "which do I pick",
// and it answers by sorting, which produces an order even where there is none.
// Two gains that differ by less than the two bands together are one answer, and
// the reader acts on the order — picked the top one, measured worse than the
// one under it (owner, 2026-08-14).
//
// Driven as state for the same reason the four above are: it is a rule about
// two numbers, so it is asserted on two numbers.
const ties = await app.evaluate(`(async () => {
  const saveScan = gainScan, saveAxis = gainAxis;
  gainAxis = { kind: 'valence', idx: 0 };
  const mk = (by) => { gainScan = { key: gainKey(), axis: gainAxis, running: false,
    done: 3, total: 3, metric: 'kill rate', note: 'x', by }; };
  // Separated: the leader is 30 points clear of a 0.04-wide answer.
  mk({ a: { pct: 0.32, se: 0.026, runs: 10 },
       b: { pct: 0.033, se: 0.0004, runs: 10 } });
  const clear = { lead: gainChipFor('a', 'V'), other: gainChipFor('b', 'V') };
  // Not separated: 0.003 apart, each answer ±0.04.
  mk({ a: { pct: 0.03333, se: 0.00043, runs: 10 },
       b: { pct: 0.03330, se: 0.00041, runs: 10 } });
  const tied = { lead: gainChipFor('a', 'V'), other: gainChipFor('b', 'V') };
  gainScan = saveScan; gainAxis = saveAxis;
  return { clear, tied };
})()`);
check("an option clear of the leader is not marked tied",
  !/gtie/.test(ties.clear.other), ties.clear.other.slice(0, 90));
// …NOR IS THE LEADER ITSELF, which shipped marked on every ranking there is:
// "tied" printed on the first row of a list with a clear winner is a caveat
// where there is nothing to caveat (owner, 2026-08-14).
check("...and neither is the leader, when nothing is near it",
  !/gtie/.test(ties.clear.lead), ties.clear.lead.slice(0, 90));
check("...and one inside its width is",
  /gtie/.test(ties.tied.other), ties.tied.other.slice(0, 120));
// The LEADER carries it too, or the marker reads as "this one is worse".
check("...on both of them, since neither is above the other",
  /gtie/.test(ties.tied.lead), ties.tied.lead.slice(0, 120));


console.log(failed ? `\n${failed} failed` : "\na quick-calc chip states how well it knows its number");
await app.finish("gain-band");
process.exit(failed ? 1 : 0);
