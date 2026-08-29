// STORAGE IS BOUNDED, AND A RESULT IS NOT WHAT FILLS IT.
//
//   node scripts/check_storage.mjs
//
// The THIRTY-FOURTH check, and the only one about how much room the app takes
// on a reader's own machine. It exists because the answer was "all of it".
//
// A REPLAY IS 41x THE RESULT IT BELONGS TO — measured here, not estimated:
// 66 KB of frames, debuff series and hit accounts against a 1.6 KB summary of
// every number a card, a share or the board ever reads. One was stored per
// WEAPON, so about seventy-five weapons filled a 5 MB origin, and the roster is
// 136. Past that the failure is not "storage is full": `localStorage.setItem`
// throws, the throw lands in the save path of the run that just finished, and
// the reader is told "sim failed: QuotaExceededError" for a simulation that
// worked perfectly.
//
// Four claims:
//
//   · THE DISK NEVER TAKES A REPLAY. Not "sheds it under pressure" — never
//     takes it. That is what makes the footprint of a measurement bounded by
//     its summary rather than by how hard it was measured.
//   · …AND THE PANEL STILL DRAWS ONE, because `resultMem` keeps it for the
//     session. A fix that quietly removed the replay would pass the first
//     assertion and break the feature.
//   · A SHED SWEEPS THE ORIGIN. A quota belongs to the origin and the old shed
//     belonged to the list being written, so a write for one weapon would fail
//     on space held by another, shed its own list to nothing, and still fail.
//     Asserted by filling the disk from OTHER weapons' keys and then saving.
//   · WHAT IS ALREADY THERE COMES BACK. Growth stopping is not the same as
//     space returning, and a reader at their quota fails the NEXT write rather
//     than the one that filled it — so the boot reclaims every replay written
//     under the old rule.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Phantasma_Prime/simulator'); route(); await sleep(3000);

  // A scenario of our own — an official ruler is pinned — with a CROWD on it,
  // because a replay follows up to REPLAY_TRACKED bodies and one target is
  // the cheapest case rather than the representative one.
  // A BUILD OF OUR OWN, FIRST. This check is about what a saved result COSTS,
  // and nothing is saved without something to save it into: since
  // "nothing is owned until it is made" a first-time visitor has
  // NO build, activePreset is blank, and saveSimResult returns having written
  // neither disk nor resultMem. That state is real and has its own check
  // (check_zero_presets.mjs); it is simply not the one this file is about, and
  // the file failed here rather than reporting it.
  const bbar = document.querySelector('#preset-bar-builder-builds');
  const badd = bbar && bbar.querySelector('.pchip.add');
  if (badd) { badd.click(); await sleep(1200); }
  out.hasBuild = loadPresetList(BUILDS).length > 0 && !!activePreset;

  const bar0 = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar0 && bar0.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1200); }
  sim.duration = 5; sim.runs = 20; sim.formation = [];
  for (let i = 0; i < 24; i++)
    sim.formation.push({ id: 'e' + (i + 2), at: [10 + (i % 6), -5 + Math.floor(i / 6) * 2] });
  renderSim(); await sleep(400);
  await runSim(); await sleep(2500);

  const key = Object.keys(localStorage)
    .find(k => /presets-phantasma_prime-builder-builds/.test(k));
  const raw = key ? localStorage.getItem(key) : '';
  out.ran = (document.getElementById('sim-results') || {}).textContent.length > 200;
  out.storedReplay = /"replay":\s*\{/.test(raw);
  out.storedChars = raw.length;

  // WHAT IT WOULD HAVE COST, so the assertion carries its own evidence.
  const mem = [...resultMem.values()][0];
  out.memEntries = resultMem.size;
  out.replayChars = JSON.stringify((mem && mem.r && mem.r.replay) || null).length;
  out.summaryChars = JSON.stringify({ ...(mem && mem.r), replay: null }).length;
  out.memHasReplay = !!(mem && mem.r && mem.r.replay);
  // …and the panel drew it. The .rp- prefix is the replay's own markup; the
  // hit account is the other thing that lives only in there.
  out.panelDrewReplay = !!document.querySelector(
    '#sim-results [class^="rp-"], #sim-results [class*=" rp-"]');

  // ---- A SHED SWEEPS THE ORIGIN -----------------------------------------
  // Fill the disk from OTHER weapons, the way a reader does by opening them,
  // then ask this weapon to save. The old shed could only drop rows from the
  // list it was writing, so it had nothing to give and the save was lost.
  const junk = 'x'.repeat(40000);
  let planted = 0;
  for (let i = 0; i < 400; i++) {
    try {
      localStorage.setItem('wfsim-presets-filler' + i + '-builder-builds',
        JSON.stringify([{ name: 'b1', state: {}, lastResult: { at: i, key: 'k', r: { pad: junk } } }]));
      planted++;
    } catch (_) { break; }
  }
  out.planted = planted;
  out.filledUp = planted > 0 && (() => {
    try { localStorage.setItem('wfsim-probe', junk); localStorage.removeItem('wfsim-probe'); return false; }
    catch (_) { return true; }
  })();
  await runSim(); await sleep(2500);
  const raw2 = localStorage.getItem(key) || '';
  out.savedOnFullDisk = /"lastResult"/.test(raw2);
  out.noteShown = !!document.getElementById('page-note');

  // ---- AND THE OLD REPLAYS COME BACK ------------------------------------
  localStorage.clear();
  const k2 = 'wfsim-presets-boar_prime-builder-builds';
  localStorage.setItem(k2, JSON.stringify(
    // GUARDED, so a missing precondition FAILS the assertion above instead of
    // throwing an unread TypeError out of the page-side body. A check that
    // crashes says less than one that fails.
    [{ name: 'b1', state: {}, lastResult: { at: 1, key: 'x', r: { ...((mem && mem.r) || {}) } } }]));
  out.beforeReclaim = localStorage.getItem(k2).length;
  out.freed = reclaimStoredReplays();
  out.afterReclaim = localStorage.getItem(k2).length;
  out.reclaimKeptTheResult = /"lastResult"/.test(localStorage.getItem(k2));
  return out;
})()`);

check("the run produced a result", r.ran === true, JSON.stringify(r.ran));
// The precondition, stated: without a build there is nothing to save into, and
// every measurement below would be measuring zero. It threw an unread
// TypeError instead until 2026-08-20.
check("...into a build of our own, which is what a result is saved against",
  r.hasBuild === true && r.memEntries === 1,
  `build ${r.hasBuild}, resultMem ${r.memEntries}`);
check(`a replay is ${Math.round(r.replayChars / 1024)} KB against a `
  + `${Math.round(r.summaryChars / 1024 * 10) / 10} KB summary — `
  + `${Math.round(r.replayChars / r.summaryChars)}x`,
  r.replayChars > 8000 && r.replayChars > r.summaryChars * 5,
  `${r.replayChars} / ${r.summaryChars}`);
check("…and the disk never takes it",
  r.storedReplay === false && r.storedChars < 40000,
  `replay in storage ${r.storedReplay}, key ${r.storedChars} chars`);
check("…while the session still has it, and the panel drew it",
  r.memHasReplay === true && r.panelDrewReplay === true,
  `mem ${r.memHasReplay}, drawn ${r.panelDrewReplay}`);
check("a full disk is really full for this test",
  r.planted > 0 && r.filledUp === true, `${r.planted} filler keys, full ${r.filledUp}`);
check("…and a save still lands, because the shed sweeps the ORIGIN",
  r.savedOnFullDisk === true, JSON.stringify({ saved: r.savedOnFullDisk, note: r.noteShown }));
check(`the boot reclaims replays written under the old rule (${Math.round(r.freed / 1024)} KB)`,
  r.freed > 8000 && r.afterReclaim < r.beforeReclaim / 4,
  `${r.beforeReclaim} -> ${r.afterReclaim}`);
check("…without throwing the measurement away with them",
  r.reclaimKeptTheResult === true, JSON.stringify(r.reclaimKeptTheResult));

await finish("storage is bounded by the summary, not by how hard you measured");
