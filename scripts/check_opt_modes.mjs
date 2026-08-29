// HOW A WEAPON IS PLAYED IS A SEARCH DIMENSION, and it is the BUILDER's control
// everywhere else.
//
// The report (owner, 2026-08-11): pick the Phantasma's charged mode on the
// optimizer tab, run it, and the winner is a base-form build. Two faults in
// one, and they pull in opposite directions — the control on that tab was the
// BUILDER's Mode block, drawn there because nothing hid it, and the optimize
// request carried no mode at all. So the page offered a choice it did not send.
//
// The answer is both halves: the builder's block belongs to the builder (it is
// part of a build, and it saves in a build preset), and the optimizer gets a
// real AXIS — pool/req marks, like the arcanes' — because the optimizer binds a
// SET where the builder binds a value.
//
//   node scripts/check_opt_modes.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  // The PHANTASMA, because it is the weapon the report was about: its second
  // mode is a charged alt-fire rather than an Incarnon cycle, so nothing about
  // it is covered by the cycle's own plumbing.
  history.pushState({},'','/weapons/Phantasma'); route(); await sleep(3500);
  const out = {};
  const w = weaponInfo(document.getElementById('weapon').value) || {};
  out.modes = (w.modes || []).slice();

  // ---- the BUILDER owns the control ------------------------------------
  const vis = (id) => { const e=document.getElementById(id); return !!e && e.offsetParent !== null; };
  const go = async (path) => { history.pushState({},'','/weapons/Phantasma'+path); route(); await sleep(700); };
  await go('');
  out.builderShowsBlock = vis('mode-block');
  // Picking a mode is a BUILD edit, so it lands in the build's own snapshot.
  mode = 'alternate'; renderMode(); await sleep(200);
  out.inBuildState = snapshotState().mode;

  await go('/optimizer');
  out.optimizerShowsBlock = vis('mode-block');
  await go('/rivens');
  out.rivensShowsBlock = vis('mode-block');
  // A SCOPE SEEDS FROM THE BUILD YOU HOLD, once — the same moment a weapon
  // switch reseeds it. Forced here because the boot already seeded one before
  // the mode was picked, and what is being checked is the seeding, not when.
  optSeeded = false; opt.modes = {};
  await go('/optimizer');
  renderOpt(); await sleep(400);   // what a weapon switch does

  // ---- the OPTIMIZER owns an AXIS --------------------------------------
  out.axisShown = vis('opt-modes-sect');
  out.seeded = { ...opt.modes };           // seeded from the build you hold
  const segs = () => [...document.querySelectorAll('#opt-modes .seg')];
  out.segCount = segs().length;

  const sendOnce = async () => {
    const seen=[]; const real=window.api;
    window.api = async (p,b) => { seen.push([p,b]); throw new Error('stop'); };
    try { await runOptimize(); } catch {}
    window.api = real;
    return (seen.find(([p])=>p==='/api/optimize')||[])[1]||{};
  };
  // MODS FROM THIS WEAPON'S OWN POOL — the Phantasma is a shotgun, and a rifle
  // mod is refused by the server rather than ignored.
  const poolIds = [...document.querySelectorAll('#opt-mods .seg[data-m]')].map(e => e.dataset.m);
  const picks = [...new Set(poolIds)].slice(0, 3);
  picks.forEach(i=>{opt.mods[i]='search';});
  out.picks = picks;
  updateOptEstimate(); renderOptMods(); await sleep(300);

  // The seeded pin travels — this is the bug: the request used to carry no
  // mode whatsoever, so the server played the weapon's default.
  const sent1 = await sendOnce();
  out.sentModes1 = { ...sent1.modes };
  out.dbgMopts = modeOpts(weaponInfo(document.getElementById('weapon').value) || {}).length;
  out.dbgMode = mode;
  out.sentMode1 = sent1.mode;

  // POOL both modes: the axis becomes a real dimension.
  const pool = (id) => { const s = segs().find(e => e.dataset.m === id && e.dataset.s === 'search'); s.click(); };
  pool('base'); await sleep(150); pool('alternate'); await sleep(150);
  out.pooled = { ...opt.modes };
  const sent2 = await sendOnce();
  out.sentModes2 = sent2.modes;

  // It is part of the SEARCH preset, so it survives a round trip.
  const snap = snapshotOpt();
  opt.modes = {}; applyOptState(snap); await sleep(200);
  out.restored = { ...opt.modes };

  // ---- and the SERVER actually searches what the axis says --------------
  // Two real runs, small enough to exhaust. PINNING is the sharp test: every
  // row must come back in that mode, and it must be the one that was pinned
  // rather than the weapon's default — which is exactly what the report was.
  opt.mods = {}; picks.forEach(i=>{opt.mods[i]='search';});
  // The final-round count is a PREFERENCE now (owner, 2026-08-29), not a
  // field on optRun — set it where it lives. (No backticks: this whole body
  // is a template literal.)
  opt.size = 2; opt.min = 2; setFinalRuns(12); optRun.finalists = 6;
  const runFull = async () => {
    optLast = null;
    updateOptEstimate(); renderOptMods(); await sleep(300);
    await runOptimize();
    for (let i = 0; i < 240 && (!optLast || !optLast.results); i++) await sleep(500);
    return optLast || {};
  };
  opt.modes = { alternate: 'fixed' }; renderOptModes();
  const A = await runFull();
  out.pinnedRows = (A.results || []).length;
  out.pinnedModes = [...new Set((A.results || []).map(x => x.mode))].sort();
  out.pinnedCands = A.candidates ?? null;
  // A row carries the mode it was scored in into the build it becomes.
  const first = (A.results || [])[0];
  out.rowToBuild = first ? resultToState(first).mode : null;

  // POOLING BOTH doubles the space — one candidate per (build, mode).
  opt.modes = { base: 'search', alternate: 'search' }; renderOptModes();
  const B = await runFull();
  out.bothCands = B.candidates ?? null;
  out.bothModes = [...new Set((B.results || []).map(x => x.mode))].sort();
  return out;
})()`);

check("the Phantasma has two modes", (r.modes || []).length === 2, JSON.stringify(r.modes));
check("the BUILDER shows the Mode block", r.builderShowsBlock === true);
check("...and picking one is a build edit", r.inBuildState === "alternate", r.inBuildState);
check("the OPTIMIZER does not show it", r.optimizerShowsBlock === false);
check("the RIVENS editor does not show it", r.rivensShowsBlock === false);

check("the optimizer has a mode AXIS instead", r.axisShown === true);
check("...with a pool/req pair per mode", r.segCount === 4, `${r.segCount} segs`);
check("...seeded from the build you hold", JSON.stringify(r.seeded) === '{"alternate":"fixed"}', JSON.stringify(r.seeded));
check("...and the request carries it", JSON.stringify(r.sentModes1) === '{"alternate":"fixed"}', JSON.stringify(r.sentModes1));
check("...beside the build's own mode, for a caller with no axis", r.sentMode1 === "alternate", r.sentMode1);
check("...pooling both makes it a dimension", JSON.stringify(r.sentModes2) === '{"base":"search","alternate":"search"}'
  || JSON.stringify(r.sentModes2) === '{"alternate":"search","base":"search"}', JSON.stringify(r.sentModes2));
check("...it survives a search-preset round trip", JSON.stringify(r.restored) === JSON.stringify(r.pooled), JSON.stringify(r.restored));

check("a pinned run ranks something", r.pinnedRows > 0, `${r.pinnedRows} rows`);
check("...every row in the mode that was PINNED", String(r.pinnedModes) === "alternate", String(r.pinnedModes));
check("...carried into the build a row becomes", r.rowToBuild === "alternate", String(r.rowToBuild));
check("pooling both doubles the space", r.bothCands === 2 * r.pinnedCands, `${r.pinnedCands} -> ${r.bothCands}`);
check("...and the ranking may hold either", (r.bothModes || []).every((m) => m === "base" || m === "alternate")
  && (r.bothModes || []).length > 0, String(r.bothModes));

await app.finish("mode is the builder's control and the optimizer's dimension");
