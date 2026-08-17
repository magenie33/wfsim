// A RANKED ROW IS A BUILD YOU CAN RE-RUN, AND IT COMES BACK WITH ITS OWN
// NUMBER.
//
// The THIRTIETH check, and the only one written so that it cannot go stale.
//
// Every other check about a build NAMES the axes it is about. This one asserts
// the ANSWER. "The simulator is the truth and the optimizer obeys it" was a
// statement about the ENGINE and it held — `parse_fight` sees to it, and a
// winner replayed under the fight it was scored in matches to 0.1%. What it
// never covered was the PAGE, which kept its own hand-written translation of a
// ranked row into a build and dropped an axis out of it four times: `mode` from
// the board submission (2026-08-09), `valence` from the worker's table
// (2026-08-14), both from the share tuple (2026-08-15), and `valence` from the
// optimizer's "+ add" (2026-08-16) — the one a player measured, reporting 26
// KPM on the ranking and 15 in the simulator for what he had been told was the
// same build.
//
// Four patches, one shape, and the reason it kept coming back is that every
// guard was a LIST of axes, and a list has to be maintained by whoever adds the
// fifth. This one holds no list: it runs a real search, applies the winner the
// way the button does, runs the simulator, and asserts the two numbers agree. A
// fifth axis is covered on the day it is added, by nobody.
//
// ON CONTROLS. Deleting an axis from the request must be OBSERVED — otherwise
// the whole check might be comparing a thing with itself. But "every axis
// changes the answer" is a claim about the GAME and it is false: the Kuva Nukor
// has one firing mode, so dropping `mode` is legitimately free. So the rotation
// is discovered from the payload's own keys, the pipe is proven by requiring
// that SOME axis moves, and the sharp control is the last one — a build
// assembled from a replay with a live axis removed must fail the very assertion
// that would otherwise pass, which is what proves the assertion can fail at all.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

// TWO WEAPONS, because no single one has every axis live. The Kuva Nukor is
// where the bug was — seven progenitor elements, and the page opens on the
// first of them, so a winner scored on another is the case that broke. The
// Torid brings the two the Nukor cannot: a second firing mode and four
// evolution tiers.
const WEAPONS = [
  { id: "kuva_nukor", page: "Kuva_Nukor",
    mods: ["hornet_strike", "barrel_diffusion", "lethal_torrent",
           "pathogen_rounds", "primed_heated_charge"],
    extra: { valence: { magnetic: "fixed" }, valence_element: "impact", valence_bonus: 0.55 } },
  { id: "torid", page: "Torid",
    mods: ["serration", "split_chamber", "point_strike", "vital_sense", "hellfire"],
    extra: { modes: { base: "search", cycle: "search" } } },
];

const runFor = (w) => evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const W = ${JSON.stringify(w)};
  localStorage.clear();
  history.pushState({}, '', '/weapons/' + W.page + '/optimizer'); route(); await sleep(4000);

  // A SCOPE WHOSE WINNER IS NOT THE DEFAULT ANYTHING. A ranking whose every
  // axis sits where the page already sits would pass this check with the bug
  // still in — that is exactly how "+ add" hid a dropped element for a week.
  const evos = {};
  (weaponInfo(W.id).evolutions || []).forEach(t => {
    evos[t.tier] = t.options.map(o => o.id);
  });
  const arc = (arcanePool(0) || []).map(a => a.id).filter(a => a !== 'none')[0];
  const body = {
    weapon: W.id,
    mods: Object.fromEntries(W.mods.map(m => [m, 'search'])),
    build_size: 3, build_min: 3,
    arcanes: arc ? { [arc]: 'fixed' } : {},
    evolutions: evos, modes: {}, exilus: {},
    ...W.extra,
    ...theFight(),
    duration: 30, runs: 12, final_runs: 12, finalists: 3, threads: 1, buffs: {},
  };
  const r0 = await postJson('/api/optimize', body);
  let s = r0;
  for (let i = 0; i < 800 && (!s || !s.done); i++) {
    await sleep(300);
    s = await postJson('/api/optimize/status', {});
  }
  const res = (s && s.result) || s;
  const row = ((res || {}).results || [])[0];
  if (!row) return { weapon: W.id, err: (s && s.error) || 'no ranking' };
  if (!row.replay) return { weapon: W.id, err: 'no replay on the row' };

  // ---- 1. THE ROW'S OWN REQUEST REPRODUCES THE ROW ----------------------
  // POST it and nothing else. If it needs assembly, it is not the contract.
  const direct = await postJson('/api/simulate', row.replay);

  // ---- 2. …AND SO DOES THE BUILD THE BUTTON MAKES OF IT ------------------
  // Through the page's own path: "+ add", open the preset, then let the
  // BUILDER assemble its own request the way Run Sim does. This is the hop
  // that broke, and the only way to test a hop is to walk it.
  addResult(row);
  const ps = loadPresetList(BUILDS);
  const saved = ps[ps.length - 1];
  history.pushState({}, '', '/weapons/' + W.page); route(); await sleep(2500);
  pickPreset(buildBarCfg(), presetId(saved)); await sleep(1200);
  const asBuilt = async () => api('/api/simulate', {
    ...buildPayload(), ...theFight(),
    duration: row.replay.duration, runs: row.replay.runs,
  });
  const viaPage = await asBuilt();

  // ---- 3. THE PAIR IS A PAIR --------------------------------------------
  // \`buildPayload(stateFromBuild(p)) === p\` over the axes a payload states.
  // The property that replaces counting fields: it fails for any axis EITHER
  // side forgets, and needs no list of them to say so.
  const roundTrip = (payload) => {
    const back = stateFromBuild(payload, W.id, row.exilus);
    const live = [slots, evoSel, arcanes, arcaneRanks, mode, valence];
    slots = back.slots; evoSel = back.evoSel; arcanes = back.arcane;
    arcaneRanks = back.arcaneRank;
    if (back.mode) mode = back.mode;
    if (back.valence) valence = back.valence;
    const out = buildPayload();
    [slots, evoSel, arcanes, arcaneRanks, mode, valence] = live;
    return out;
  };
  const round = roundTrip(row.replay);
  const tripFails = ['mods', 'evolutions', 'arcane', 'mode', 'valence_element', 'valence_bonus']
    .filter(k => k in row.replay
                 && JSON.stringify(round[k]) !== JSON.stringify(row.replay[k]));

  // ---- 4. IS THE PIPE ALIVE? --------------------------------------------
  // Drop each axis the payload actually carries and see which ones the engine
  // notices. The rotation comes from the payload's OWN keys, so an axis added
  // tomorrow joins it without anybody editing this file. A degenerate axis —
  // one firing mode, an arcane inert in this fight — is REPORTED, not failed:
  // that is a fact about the weapon, not about the wiring.
  const AXES = Object.keys(row.replay).filter(k =>
    ['mods', 'evolutions', 'arcane', 'mode', 'valence_element'].indexOf(k) >= 0
    && row.replay[k] != null
    && (Array.isArray(row.replay[k]) ? row.replay[k].length : String(row.replay[k]).length));
  const dropped = [];
  for (const k of AXES) {
    const broken = { ...row.replay };
    delete broken[k];
    const out = await postJson('/api/simulate', broken);
    dropped.push({ axis: k, refused: !(out && out.ok),
                   score: out && out.ok ? out.score_mean : null });
  }

  // ---- 5. THE SHARP CONTROL ---------------------------------------------
  // Assertion 2 above is the one that matters, so prove it CAN fail. Take an
  // axis the engine demonstrably notices, build a state from a replay missing
  // it, and re-run through the page's own payload: the number must now be
  // wrong. Without this, "the page reproduces the row" might mean nothing.
  let control = null;
  const liveAxis = dropped.find(d => d.refused || d.score === null
    || Math.abs(d.score - row.kill_progress) > 0.05 * Math.abs(row.kill_progress));
  if (liveAxis && !liveAxis.refused) {
    const maimed = { ...row.replay };
    delete maimed[liveAxis.axis];
    const back = stateFromBuild(maimed, W.id, row.exilus);
    // Applied the way a preset is, so the control walks the SAME hop.
    whileApplying(() => restoreState(back, W.id));
    await sleep(900);
    const out = await asBuilt();
    control = { axis: liveAxis.axis, score: out && out.ok ? out.score_mean : null };
  }

  return { weapon: W.id, err: null,
           rowKpm: row.kill_progress, rowSe: row.kill_progress_se,
           mode: row.mode, valence: row.valence, evolutions: row.evolutions,
           direct: direct && direct.ok ? direct.score_mean : null,
           directSe: direct && direct.ok ? direct.score_se : null,
           directErr: direct && direct.ok === false ? direct.error : null,
           viaPage: viaPage && viaPage.ok ? viaPage.score_mean : null,
           viaErr: viaPage && viaPage.ok === false ? viaPage.error : null,
           tripFails, dropped, control,
           replayKeys: Object.keys(row.replay).sort() };
})()`);

// FOUR SIGMA OF THE TWO COMBINED, which is the same arithmetic the page shows
// the reader. Not a flat percentage: at 12 runs one would have to be loose
// enough to hide the bug this exists for, and at 1000 it would cry wolf.
const band = (a, b, sa, sb) => {
  const se = Math.hypot(sa || 0, sb || 0);
  return { off: Math.abs(a - b) > Math.max(4 * se, 0.01 * Math.abs(b)), se };
};
const n4 = (x) => (x == null ? "—" : Number(x).toFixed(4));

let anyLive = false;
for (const w of WEAPONS) {
  const r = await runFor(w);
  const tag = `[${w.id}]`;
  check(`${tag} a ranked row carries a request that reproduces it`,
    r.err === null, String(r.err));
  if (r.err) continue;

  const d = band(r.direct, r.rowKpm, r.directSe, r.rowSe);
  check(`${tag} ...and POSTing it, alone, gives the row's own number`,
    r.direct !== null && !d.off,
    r.directErr || `row ${n4(r.rowKpm)} ± ${n4(r.rowSe)} vs replay ${n4(r.direct)} (4σ = ${n4(4 * d.se)})`);

  // THE HOP THAT BROKE. Everything above can pass with the page still wrong.
  const p = band(r.viaPage, r.rowKpm, r.directSe, r.rowSe);
  check(`${tag} the build '+ add' makes of it re-runs at the row's number`,
    r.viaPage !== null && !p.off,
    r.viaErr || `row ${n4(r.rowKpm)} vs the page's own build ${n4(r.viaPage)} (4σ = ${n4(4 * p.se)})`);

  check(`${tag} the payload/state pair round-trips on every axis it states`,
    r.tripFails.length === 0, `dropped or changed: ${JSON.stringify(r.tripFails)}`);

  // THE PIPE. Some axis must matter, or every agreement above is vacuous.
  const moved = r.dropped.filter((x) => x.refused || x.score === null
    || band(x.score, r.rowKpm, r.directSe, r.rowSe).off);
  anyLive = anyLive || moved.length > 0;
  // THE COVERAGE RIDES IN THE NAME, because `check` prints a detail only when
  // it fails — and a check that quietly exercised one axis out of five reads
  // exactly like one that exercised all of them. Which axes are live is a fact
  // about this weapon in this fight (the Kuva Nukor's single firing mode is not
  // a wiring fault), so it is REPORTED beside the assertion rather than being
  // asserted.
  const inert = r.dropped.filter((x) => !moved.includes(x)).map((x) => x.axis);
  check(`${tag} the replay is READ — live: ${moved.map((x) => x.axis).join(",") || "none"}`
        + (inert.length ? `, inert here: ${inert.join(",")}` : ""),
    moved.length > 0,
    r.dropped.map((x) => `${x.axis}:${x.refused ? "refused" : n4(x.score)}`).join(" "));

  // THE SHARP ONE: the assertion above must be capable of failing.
  check(`${tag} ...and a build missing that axis FAILS the same assertion`,
    r.control !== null && (r.control.score === null
      || band(r.control.score, r.rowKpm, r.directSe, r.rowSe).off),
    r.control === null ? "no live axis to maim — the control never ran"
      : `dropped '${r.control.axis}': ${n4(r.rowKpm)} -> ${n4(r.control.score)}, which the band did not catch`);
}

check("at least one weapon proved the pipe", anyLive, String(anyLive));

await app.finish("a ranked row is a build you can re-run, and it comes back with its own number");
