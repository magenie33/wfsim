// EVERY AXIS SAYS HOW MANY OF ITS SLOTS A BUILD FILLS, and it says it the same way.
//
// The optimizer's axes are all one shape — N slots, an option set, and a range
// saying how many of the slots a searched build must fill — and the page said
// it three different ways (owner, 2026-08-29):
//
//   mods              a numeric range, 0–8, on screen
//   exilus            0–1 reachable, but only by pooling a hidden `none` row
//   arcane seats      0–1 not reachable at all
//   evolution tiers   0–1 not reachable at all
//
// …so which of 0–0 / 0–1 / 1–1 you got was decided by whether you had marked
// anything, on three of the four. This walks all three states on all of them
// and asserts them ON THE WIRE, because a range that draws correctly and sends
// nothing looks exactly like a working control (check_opt_modes' own lesson).
//
// IT IS DERIVED FIRST AND ADJUSTED SECOND, and the derived half is the one
// that must not regress: marking candidates and saying nothing else still
// means 1–1, so no scope that exists today grows. That is the 2026-08-01
// decision about empty arcane seats, kept — it was against the empty seat
// being a DEFAULT, not against asking for it.
//
//   node scripts/check_slot_ranges.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 14000 });
const { evaluate, check } = app;

// The Laetum: one arcane seat, five evolution tiers, an exilus slot.
const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Laetum'); route(); await sleep(3500);
  document.querySelector('[data-tab="optimizer"]')?.click(); await sleep(600);
  const out = {};

  // What the request says, without running anything.
  const sent = async () => {
    const seen = [];
    const real = window.api;
    window.api = async (p, b) => { seen.push([p, b]); throw new Error('stop'); };
    try { await runOptimize(); } catch {}
    window.api = real;
    return (seen.find(([p]) => p === '/api/optimize') || [])[1] || {};
  };
  // Drive the control the way a reader does: type into the row's own inputs.
  const setRange = async (key, lo, hi) => {
    const row = document.querySelector('[data-range-row="' + key + '"]');
    if (!row) return false;
    const put = (end, v) => {
      const el = row.querySelector('input[data-end="' + end + '"]');
      if (!el || el.disabled) return false;
      el.value = String(v);
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    };
    const ok = put('hi', hi) && put('lo', lo);
    await sleep(200);
    return ok;
  };
  const shown = (key) => {
    const row = document.querySelector('[data-range-row="' + key + '"]');
    if (!row) return null;
    const v = (end) => row.querySelector('input[data-end="' + end + '"]').value;
    return v('lo') + '-' + v('hi');
  };

  opt.mods['hornet_strike'] = 'search';

  // ---- THE ARCANE SEAT --------------------------------------------------
  const seat = META.weapons.find(w => w.id === 'laetum').arcane_pools[0];
  const arc = (META.arcanes || []).filter(a => (a.pools || [a.slot]).includes(seat));
  opt.arcanes[arc[0].id] = 'search';
  renderOptArcanes(); updateOptEstimate(); await sleep(200);
  // DERIVED: a marked seat is a filled seat, which is what it has always been.
  out.arcDerived = shown('arc:' + seat);
  out.arcDerivedSent = (await sent()).arcanes;
  // 0–1: the empty seat asked for out loud.
  out.arcWidened = await setRange('arc:' + seat, 0, 1) && shown('arc:' + seat);
  out.arcWidenedSent = (await sent()).arcanes;
  // 0–0: searched unworn, and the candidate is KEPT — going down and back up
  // must not cost the reader their marks.
  await setRange('arc:' + seat, 0, 0);
  out.arcEmpty = shown('arc:' + seat);
  out.arcEmptySent = (await sent()).arcanes;
  out.arcKeptMark = opt.arcanes[arc[0].id];
  await setRange('arc:' + seat, 1, 1);
  out.arcBack = shown('arc:' + seat);

  // ---- AN EVOLUTION TIER ------------------------------------------------
  const tiers = weaponAxes('laetum').evolutions;
  const t1 = tiers[0];
  opt.evos[t1.tier] = { [t1.options[0].id]: 'search' };
  renderOptEvos(); updateOptEstimate(); await sleep(200);
  out.evoDerived = shown('evo:' + t1.tier);
  out.evoWidened = await setRange('evo:' + t1.tier, 0, 1) && shown('evo:' + t1.tier);
  out.evoWidenedSent = ((await sent()).evolutions || {})[String(t1.tier)];
  // …AND 0–1 STILL OPENS THE TIER ABOVE, because half of its sets carry the
  // rung. 0–0 must NOT: every set that reached the tier above would then be
  // truncated, and the marks up there would price nothing.
  renderOptEvos(); await sleep(150);
  out.openAbove01 = !document.querySelector('.opt-tier-block:nth-child(2)')?.classList.contains('locked');
  await setRange('evo:' + t1.tier, 0, 0);
  renderOptEvos(); await sleep(150);
  out.openAbove00 = !document.querySelector('.opt-tier-block:nth-child(2)')?.classList.contains('locked');
  out.evoEmptySent = ((await sent()).evolutions || {})[String(t1.tier)];
  await setRange('evo:' + t1.tier, 1, 1);

  // ---- THE EXILUS SLOT --------------------------------------------------
  const ex = weaponAxes('laetum').exilus;
  if (ex.length) {
    opt.exilus[ex[0].id] = 'search';
    renderOptExilus(); updateOptEstimate(); await sleep(200);
    out.exDerived = shown('exilus');
    out.exWidened = await setRange('exilus', 0, 1) && shown('exilus');
    out.exWidenedSent = (await sent()).exilus;
  }

  // ---- A PIN SETTLES THE SLOT, and the range says so rather than lying ---
  opt.arcanes = { [arc[0].id]: 'fixed' };
  renderOptArcanes(); await sleep(200);
  out.arcPinned = shown('arc:' + seat);
  out.arcPinnedLocked = !!document.querySelector('[data-range-row="arc:' + seat + '"] input[disabled]');
  out.arcPinnedSent = (await sent()).arcanes;

  // ---- THE TWO AXES THAT HAVE NO RANGE SAY SO ---------------------------
  //
  // A build is played exactly one way and an adversary weapon has exactly one
  // progenitor element, so mode and valence are 1–1 and cannot be anything
  // else. They carry the row anyway, read-only: an axis that simply omitted it
  // would be the axis the rule forgot, which is the shape this whole change is
  // about (owner, 2026-08-29).
  const readonlyRow = (key) => {
    const row = document.querySelector('[data-range-row="' + key + '"]');
    if (!row) return null;
    const v = (e) => row.querySelector('input[data-end="' + e + '"]');
    return { at: v('lo').value + '-' + v('hi').value, locked: v('lo').disabled && v('hi').disabled };
  };
  out.modeRow = readonlyRow('mode');

  // ---- …AND EVERY AXIS IS A FACTOR OF THE CANDIDATE COUNT ---------------
  //
  // The server's variant table is modes x evo_sets x valences, so pooling a
  // second mode genuinely doubles the search. The estimate counted only the
  // evolution sets, so it under-reported by exactly the two axes that had no
  // range row — the same blind spot, seen from the other side.
  // THE COUNT ITSELF, off its own element — reading the panel text with a
  // regex picks up whatever number happens to be in an error message instead,
  // which is how this first read "1 candidate" off "pooled mods reserve 1
  // open slot" and called an invalid scope valid.
  const jobsNow = () => {
    const b = document.querySelector('#opt-estimate b');
    return b ? Number(b.textContent.replace(/,/g, '')) : null;
  };
  opt.arcanes = {}; opt.evos = {}; opt.exilus = {};
  opt.mods = { hornet_strike: 'search', barrel_diffusion: 'search' };
  const modeIds = (weaponInfo('laetum').modes || []);
  opt.modes = { [modeIds[0]]: 'fixed' };
  renderOptMods(); updateOptEstimate(); await sleep(200);
  out.jobsOneMode = jobsNow();
  opt.modes = {}; modeIds.slice(0, 2).forEach(id => { opt.modes[id] = 'search'; });
  renderOptModes(); updateOptEstimate(); await sleep(200);
  out.jobsTwoModes = jobsNow();
  out.modeCount = modeIds.length;
  opt.modes = { [modeIds[0]]: 'fixed' };

  // ---- A CEILING OF 0 IS THE BARE WEAPON, AND KEEPS THE MARKS -----------
  const put = async (id, v) => {
    const el = document.getElementById(id);
    el.value = String(v); el.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(200);
  };
  await put('opt-size', 0);
  out.zeroCeilJobs = jobsNow();
  out.zeroCeilRuns = !document.getElementById('run-opt').disabled;
  const z = await sent();
  out.zeroCeilSent = [z.build_min, z.build_size];
  out.zeroCeilKept = Object.keys(opt.mods).length;
  await put('opt-size', 8);
  out.zeroCeilBack = jobsNow();

  // ---- AND IT SURVIVES A SEARCH-PRESET ROUND TRIP -----------------------
  opt.arcanes = { [arc[0].id]: 'search', ['none:' + seat]: 'search' };
  const snap = snapshotOpt();
  opt.arcanes = {};
  applyOptState(snap); renderOptArcanes(); await sleep(200);
  out.roundTrip = shown('arc:' + seat);
  return out;
})()`);

const j = (v) => JSON.stringify(v);

// ---- the derived answer is untouched, which is what keeps this safe -------
check("an arcane seat with a candidate marked is 1–1, as it always was",
  r.arcDerived === "1-1", j(r.arcDerived));
check("...and sends the candidate with no empty seat beside it",
  r.arcDerivedSent && !Object.keys(r.arcDerivedSent).some((k) => k.startsWith("none")),
  j(r.arcDerivedSent));

// ---- …and every state is now reachable, on the wire ----------------------
check("...widening it to 0–1 holds", r.arcWidened === "0-1", j(r.arcWidened));
check("...and reaches the request as the seat's own empty mark",
  !!r.arcWidenedSent && Object.entries(r.arcWidenedSent)
    .some(([k, v]) => k.startsWith("none:") && v === "search"),
  j(r.arcWidenedSent));
check("...0–0 searches the seat unworn", r.arcEmpty === "0-0", j(r.arcEmpty));
check("...sent as a PIN on the empty seat, not as a lost candidate",
  !!r.arcEmptySent && Object.entries(r.arcEmptySent)
    .some(([k, v]) => k.startsWith("none:") && v === "fixed"),
  j(r.arcEmptySent));
check("...and the candidate is kept, so widening back costs nothing",
  r.arcKeptMark === "search" && r.arcBack === "1-1",
  `${j(r.arcKeptMark)} / ${j(r.arcBack)}`);

check("an evolution tier says the same three things", r.evoDerived === "1-1", j(r.evoDerived));
check("...0–1 travels in the tier's own list", r.evoWidened === "0-1"
  && Array.isArray(r.evoWidenedSent) && r.evoWidenedSent.includes("none"),
  `${j(r.evoWidened)} ${j(r.evoWidenedSent)}`);
check("...0–0 sends that tier as empty alone",
  Array.isArray(r.evoEmptySent) && r.evoEmptySent.join() === "none", j(r.evoEmptySent));
// THE LADDER IS THE SHARP ONE. A tier searched BOTH ways still reaches the
// tier above; a tier searched only empty must not, or every set up there is
// truncated and those marks price nothing.
check("...0–1 still opens the tier above it", r.openAbove01 === true);
check("...and 0–0 does not", r.openAbove00 === false);

check("the exilus slot draws the same control", r.exDerived === "1-1", j(r.exDerived));
check("...and its 0–1 reaches the request", r.exWidened === "0-1"
  && !!r.exWidenedSent && r.exWidenedSent.none === "search",
  `${j(r.exWidened)} ${j(r.exWidenedSent)}`);

// ---- a pin is not a range, and the row says so ---------------------------
check("a pinned candidate settles its slot at 1–1", r.arcPinned === "1-1", j(r.arcPinned));
check("...and the range cannot be moved off it", r.arcPinnedLocked === true);
check("...and no empty mark rides along to contradict it",
  !!r.arcPinnedSent && !Object.keys(r.arcPinnedSent).some((k) => k.startsWith("none")),
  j(r.arcPinnedSent));

check("a range survives a search-preset round trip", r.roundTrip === "0-1", j(r.roundTrip));

// ---- the two axes with no range still carry the row ----------------------
check("the mode axis states its 1–1", r.modeRow && r.modeRow.at === "1-1", j(r.modeRow));
check("...read-only, because a build is played exactly one way",
  !!r.modeRow && r.modeRow.locked === true, j(r.modeRow));

// ---- …and every axis is a factor of the candidate count ------------------
check(`pooling a second mode doubles the candidate count (${r.jobsOneMode} → ${r.jobsTwoModes})`,
  r.modeCount < 2 || r.jobsTwoModes === r.jobsOneMode * 2,
  `${r.jobsOneMode} → ${r.jobsTwoModes} over ${r.modeCount} modes`);

// ---- a ceiling of 0 is the bare weapon -----------------------------------
check("a ceiling of 0 searches the bare weapon", r.zeroCeilJobs === 1 && r.zeroCeilRuns === true,
  `${r.zeroCeilJobs} candidates, run enabled ${r.zeroCeilRuns}`);
check("...and sends 0–0 rather than being refused",
  String(r.zeroCeilSent) === "0,0", j(r.zeroCeilSent));
check("...with the marks kept, so raising it back costs nothing",
  r.zeroCeilKept === 2 && r.zeroCeilBack > 1,
  `${r.zeroCeilKept} marks, back to ${r.zeroCeilBack}`);

await app.finish("every axis says how many of its slots a build fills");
