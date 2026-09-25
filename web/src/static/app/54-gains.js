// ---- MARGINAL GAIN: what is THIS SLOT worth as something else? ------------
//
// Asked OF A SLOT — "if slot N became this mod, what happens to the kill
// rate?" — which is correct rather than convenient: elements combine by MOD
// ORDER (MECHANICS §3) and the payload's list is slot-ordered. Answered by
// SIMULATING each candidate, since the builder has no second damage model; the
// metric is KILL PROGRESS, falling back to DPS when the baseline cannot kill.
// PAIRED randomness is what makes a tenth of the run count usable.
/// ONE MEASUREMENT, and how far it can be from the truth: `{ v, se }`.
///
/// `score`/`dps` are MEANS over the runs, the statistic every surface ranks.
/// `se` is the server's own spread over those runs (sigma / sqrt(runs)).
/// Estimating it by re-running the reference at another seed takes ONE sample
/// of a distribution, which on identical inputs answers anywhere from 0.7% to
/// 11.2% — one draw deciding whether EVERY chip is suppressed or none is.
const readGain = (r, useKills) => {
  if (!r || !r.ok) return null;
  const runs = (useKills ? r.score_runs : r.dps_runs) || [];
  return useKills
    ? { v: r.score ?? 0, se: r.score_se ?? 0, runs }
    : { v: r.dps ?? 0, se: r.dps_se ?? 0, runs };
};

/// What `cand` is worth against `ref`, with the uncertainty of the COMPARISON.
///
/// PAIRED, because the runs are: `monte_carlo` advances its master rng once per
/// run whatever the run does, so run `i` of each build is drawn from the same
/// luck and the comparison's spread is the spread of the DIFFERENCES.
///
/// The standard ratio-estimator error: with `d_i = c_i − ratio·b_i`,
/// `SE(ratio) = sd(d) / (√n · mean(b))`. It says two things quadrature — the
/// formula for INDEPENDENT samples — cannot:
///
///  · a candidate that scales this fight PROPORTIONALLY gives `d_i = 0` on
///    every run, so the band is exactly zero and the gain is a fact (the
///    Serration / Amalgam Serration pair, 0.9623 = 2.55/2.65 at any run count);
///  · one that changes WHICH fight happens gives a real band, however close
///    its mean lands.
///
/// Quadrature overstates a paired band AND leaves "exact" undecidable. With no
/// series to pair it falls back to quadrature, the honest answer when the
/// pairing cannot be seen.
const gainOver = (cand, ref) => {
  const ratio = cand.v / ref.v;
  const a = cand.runs || [], b = ref.runs || [];
  const n = Math.min(a.length, b.length);
  if (n < 2 || !ref.v) {
    return { pct: ratio - 1, se: ratio * Math.hypot(cand.se / cand.v || 0, ref.se / ref.v || 0) };
  }
  let ss = 0;
  for (let i = 0; i < n; i++) {
    const d = a[i] - ratio * b[i];
    ss += d * d;
  }
  // Σd is zero by construction when both means are taken over the same n, so
  // the sum of squares IS the variance's numerator.
  const se = Math.sqrt(ss / (n - 1) / n) / ref.v;
  // FLOATING-POINT RESIDUE IS NOT A BAND. A candidate that scales this fight
  // proportionally leaves `d_i` at the last bits of a double rather than at
  // zero — measured 4e-15 on the Serration pair — and a band of 4e-15 prints
  // as "±0.0%", which is the "≈0%" this whole display was written to stop.
  return { pct: ratio - 1, se: se < 1e-9 * Math.abs(ratio) ? 0 : se };
};

// ONE seed for the whole scan: the reference and every candidate are measured
// under the same luck, so a candidate that does not perturb the fight compares
// against the reference exactly (see `gainBand`).
//
// There is no second seed. A scan's resolution is a property of THIS scope in
// THIS fight — a status mod perturbs it and a damage mod barely does — so it
// has to be measured rather than assumed; but the runs already paid for measure
// it, and one extra run at another seed only draws a single sample of it.
const GAIN_SEED = 0x5EED;
// TEN, and it is both the floor and the default. Below it a status mod's chip
// is a coin flip — M24: one run swings a status mod +-39 points — so a number
// under ten is not a cheaper answer, it is a wrong one.
const GAIN_RUNS_MIN = 10;
const GAIN_RUNS_MAX = 2000;
const gainRuns = () =>
  Math.max(GAIN_RUNS_MIN, Math.min(GAIN_RUNS_MAX, Math.round(Number(gainPrefs.runs)) || GAIN_RUNS_MIN));
// A gain READS with its sign — "12.3%" and "+12.3%" are different claims.
const gainPct = (x) => (x >= 0 ? "+" : "−") + sig2(Math.abs(x) * 100) + "%";

// Quick calc's TWO settings: which saved scenario, and how many runs.
//
// A run count is the one knob whose right value nobody but the reader knows: it
// depends on how close the answers turn out to be and on how long you are
// willing to wait. Ten is the floor and the default, where a status mod's
// answer stops being a coin flip (M24); above it you are buying resolution the
// scan cannot invent. What a run is MEASURED BY still belongs to the scenario,
// and there is no "current" scenario either — a scan is only worth reading
// against something with a name that can be returned to.
// ON by default: the ranking is the reason the picker is
// worth opening. Off, nothing simulates, no chip is drawn, and is not
// offered as an order — a sort key with no values behind it is a trap.
// NO `scenario` HERE ANY MORE: the fight is the simulator's, and a stale key
// left in a reader's localStorage is inert because nothing reads it. See
// `gainScenario`.
let gainPrefs = { on: true, runs: GAIN_RUNS_MIN };
/// Is the every-rank editor open? Not persisted: it is a place you go to.
let everyRankOpen = false;
try { const s = JSON.parse(localStorage.getItem("wfsim-gain")); if (s) gainPrefs = { ...gainPrefs, ...s }; } catch (_) {}
const saveGainPrefs = () => localStorage.setItem("wfsim-gain", JSON.stringify(gainPrefs));

let gainScan = { key: null, running: false, base: 0, floor: 0, by: {}, done: 0, total: 0,
  ids: new Set(), note: "", metric: "" };

/// THE FIGHT A SCAN RUNS UNDER, and there is only one: THE ONE YOU ARE IN.
///
/// A picker of its own — `gainPrefs.scenario`, persisted, and therefore STICKY
/// across weapons, scenarios and sessions — is two controls for one fact, which
/// is how one silently undoes the other: build a nine-body Ocucor fight, switch
/// the simulator to it, and the quick calc keeps
/// ranking every slot under whichever scenario that popover was last left on —
/// an official single-target ruler, most likely, since that is where the app
/// lands a first-time visitor. The mods that only pay in a crowd read as worth
/// nothing, and nothing on screen said the numbers came from another fight.
///
/// So it follows the simulator, and the rule is the arena's own: a fight is
/// EDITED IN ONE PLACE. Ranking a slot under some other scenario is still one
/// click — switch to it, and the simulator shows it too, so the numbers and
/// the fight on screen can no longer disagree.
function gainScenario() {
  const ps = scenarioList();
  // The NAME only — what is measured is `theFight()`. Resolved by ID, not by
  // label: an official ruler's name is a translated sentence while its identity
  // is `single_target_no_aim`.
  const p = ps.find((x) => presetId(x) === activeScenario);
  // ONE PASS, at the reader's own count. It was one run over the field and then
  // the leaders again — two numbers with two precisions, and the cheap one
  // printed a minus sign in front of mods worth +40% (M24: a status mod swings
  // ±39 points on a single run). Ten is where that stops being a coin flip; it
  // is not where it stops moving, which is why every tooltip still says how
  // many runs its number came from.
  //
  // THE COUNT IS THE ONLY THING THIS SCAN OWNS. It is the reader's precision,
  // not an edit to the fight — which is what makes "the quick calc is the
  // simulator run many times" true rather than aspirational.
  return { name: p ? p.name : "—", refine: 0,
    // THE RUNS THEMSELVES, because this scan PAIRS with them. See `gainOver`.
    scenario: theFight({ runs: gainRuns(), seed: GAIN_SEED, run_series: true }) };
}

// A scan belongs to ONE AXIS POSITION of one build under one scenario.
let gainAxis = { kind: "mods", idx: 0 };
// The key is the AXIS, the BUILD and the FIGHT THIS SCAN WILL ACTUALLY RUN —
// `gainScenario()`'s own output, not a hand-listed copy of some of `sim`.
//
// Naming the scenario fields one by one gives a list that drifts: a missing
// `buffs` means raising a buff's starting stacks changes what the scan would
// measure without changing the key, and the old ranking stays on screen looking
// current. Any
// hand-maintained list of "the fields that matter" grows a hole the moment a
// field is added; deriving the key from the payload cannot.
const gainKey = () => JSON.stringify([gainAxis, buildPayload(), gainPrefs.on,
  gainScenario().scenario, everyRank()]);

/// Every candidate for an axis position, as `{ id, payload }` — the payload
/// being what to OVERRIDE on `buildPayload()` to try it.
///
/// THE SERVER'S LIST (`/api/candidates`), written once for the quick calc and
/// the optimizer alike, so a rule added there reaches both. The request is the
/// build as the page holds it — SLOTS rather than the wire's flat mod list,
/// because which slot is the exilus is a fact a flat list cannot carry.
async function gainCandidates(axis) {
  const r = await api("/api/candidates", {
    weapon: $("weapon").value,
    slots: slots.map(slotModId),
    evo_sel: evoSel,
    arcane: arcanes.slice(),
    arcane_rank: arcaneRanks.slice(),
    mode,
    valence_element: valence.element,
    valence_bonus: valence.bonus,
    assembly: assembly ? { ...assembly } : null,
    rivens: rivenPayload(),
    every_rank: everyRank(),
    axis,
  });
  return (r && r.candidates) || [];
}

// How many of the leaders AUTO looks at twice. Small on purpose: the second
// pass exists to settle an ORDER, and an order is decided at the top.
const GAIN_REFINE_TOP = 12;

