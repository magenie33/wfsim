// PRESET INDEPENDENCE: nothing outside a collection writes its state.
//
// Exists because picking a build used to change the scenario. A build carried
// a snapshot of the fight and `restoreState` applied it, so switching builds
// silently rewrote the fight you were working in — and the scenario bar, whose
// whole job is to be the one place a fight is edited, moved under you.
//
//   node scripts/check_preset_independence.mjs
//
// Asserts the SCREEN and the stored state, in both directions. Exits non-zero
// on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const $$ = (s) => document.querySelector(s);
  localStorage.clear();
  history.pushState({},'','/weapons/Torid'); route(); await sleep(2500);

  // Two builds, saved while two DIFFERENT scenarios are active. Under the old
  // model each build swallowed a copy of the fight it was made in.
  sim.level = 111; markScenarioDirty(); await sleep(700);
  slots[0].mod = 'serration'; slots[0].rank = modById('serration').max_rank;
  markPresetDirty(); renderMods(); await sleep(700);
  const bar = $$('#preset-bar-' + 'builder-builds');
  // "+ new" a second build, then give it a different mod under a different fight.
  bar.querySelector('.pchip.add').click(); await sleep(900);
  sim.level = 222; markScenarioDirty(); await sleep(700);
  slots[0].mod = 'split_chamber'; slots[0].rank = modById('split_chamber').max_rank;
  markPresetDirty(); renderMods(); await sleep(900);

  const names = loadPresetList('builder-builds').map(p => p.name);
  const carriesSim = loadPresetList('builder-builds').some(p => p.state && p.state.sim);

  // Now switch back to the FIRST build. The fight must not move.
  const levelBefore = sim.level;
  // BY NAME, not by position. The bar also carries the OFFICIAL builds (the
  // board's read-only rows, which come first), so "the first chip" stopped
  // meaning "the first build I made" the day those landed — and index was only
  // ever a proxy for the name anyway.
  const chips = [...bar.querySelectorAll('.pchip[data-name]')];
  chips.find((c) => c.dataset.name === 'build 1').click(); await sleep(1600);
  const levelAfter = sim.level;
  const onScreen = ($$('#sim-target [data-k="level"]')||{}).value;
  const modAfter = slots[0].mod;

  // ...and switching the SCENARIO must not rewrite the build.
  const buildJson = JSON.stringify(loadPresetList('builder-builds')[0].state.slots);
  const scBar = $$('#preset-bar-' + 'simulator-scenarios');
  scBar.querySelector('.pchip.add').click(); await sleep(1200);
  sim.level = 333; markScenarioDirty(); await sleep(900);
  const buildJson2 = JSON.stringify(loadPresetList('builder-builds')[0].state.slots);

  // ...and the SEARCH must survive a build switch too. Loading a build
  // rebuilds the editor for its weapon, which resets the scope in passing —
  // the active search preset is what has to put it back.
  document.querySelectorAll('.tab').forEach(x => { if(/Optim/i.test(x.textContent)) x.click(); });
  await sleep(1500);
  opt.mods = { serration: 'search', heavy_caliber: 'fixed' };
  optRun.finalists = 13;
  updateOptEstimate(); await sleep(900);
  document.querySelectorAll('.tab').forEach(x => { if(/Build/i.test(x.textContent)) x.click(); });
  await sleep(800);
  chips[1].click(); await sleep(1600);
  document.querySelectorAll('.tab').forEach(x => { if(/Optim/i.test(x.textContent)) x.click(); });
  await sleep(1800);
  const scope = JSON.stringify(opt.mods), fin = optRun.finalists;

  // The optimizer's BUFFS are the scenario's, read-only. Set one in the
  // simulator and the search must show it, without a control to change it.
  document.querySelectorAll('.tab').forEach(x => { if(/Build/i.test(x.textContent)) x.click(); });
  await sleep(700);
  slots[1].mod = 'galvanized_chamber'; slots[1].rank = modById('galvanized_chamber').max_rank;
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(x => { if(/Sim/i.test(x.textContent)) x.click(); });
  await sleep(1500);
  const simBox = $$('#sim-buffs');
  const one = simBox.querySelector('input[data-f="stacks"]');
  const buffId = one ? one.dataset.b : null;
  if (one) { one.value = '3'; one.dispatchEvent(new Event('change')); }
  await sleep(1200);
  document.querySelectorAll('.tab').forEach(x => { if(/Optim/i.test(x.textContent)) x.click(); });
  await sleep(2500);
  const optBox = $$('#opt-buffs');
  const mirrored = buffId ? optBox.querySelector('input[data-b="'+buffId+'"][data-f="stacks"]') : null;
  const optOwnsBuffs = typeof opt.buffs !== 'undefined';

  // ---- and NOTHING crosses between WEAPONS. A weapon that has never been
  // opened starts from the server's defaults, not from the fight on screen.
  $$('#opt-results').innerHTML = '<div id="stale-marker">last weapon ranking</div>';
  sim.duration = 77; markScenarioDirty(); await sleep(900);
  const aFight = { level: sim.level, dur: sim.duration };
  history.pushState({},'','/weapons/Dual_Toxocyst'); route(); await sleep(3000);
  const bFight = { level: sim.level, dur: sim.duration };
  const bStored = JSON.parse(localStorage.getItem('wfsim-presets-dual_toxocyst-simulator-scenarios'));
  const staleResults = !!document.getElementById('stale-marker');
  sim.level = 4242; markScenarioDirty(); await sleep(900);
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3000);
  const backFight = { level: sim.level, dur: sim.duration };

  return { names, carriesSim, levelBefore, levelAfter, onScreen, modAfter,
           buildUntouched: buildJson === buildJson2, scope, fin,
           buffId, mirroredValue: mirrored ? mirrored.value : null,
           mirroredLocked: mirrored ? mirrored.disabled : null, optOwnsBuffs,
           aFight, bFight, backFight, staleResults,
           bStoredLevel: bStored && bStored[0].state.level,
           defLevel: (META.defaults || {}).level };
})()`);

check("two builds exist", r.names.length >= 2, r.names.join(","));
check("no build stores a copy of the fight", !r.carriesSim);
check("switching build leaves the fight alone (state)", r.levelBefore === r.levelAfter, `${r.levelBefore} -> ${r.levelAfter}`);
check("...and on screen", String(r.onScreen) === String(r.levelAfter), `${r.onScreen} vs ${r.levelAfter}`);
check("the build itself did load", r.modAfter === "serration", String(r.modAfter));
check("editing the fight leaves the build alone", r.buildUntouched);
check("switching build leaves the SEARCH scope alone",
  r.scope === '{"serration":"search","heavy_caliber":"fixed"}', r.scope);
check("...and its finalists", r.fin === 13, String(r.fin));
check("the optimizer keeps no buff state of its own", !r.optOwnsBuffs);
check("it shows the scenario's buff value", r.mirroredValue === "3", `${r.buffId} = ${r.mirroredValue}`);
check("...and offers no way to change it", r.mirroredLocked === true, String(r.mirroredLocked));
check("a NEW weapon does not inherit the last one's fight",
  r.bFight.level === r.defLevel && r.bFight.dur !== r.aFight.dur,
  `${JSON.stringify(r.aFight)} -> ${JSON.stringify(r.bFight)}`);
check("...and its stored scenario is the DEFAULT, not a copy",
  r.bStoredLevel === r.defLevel, `${r.bStoredLevel} vs default ${r.defLevel}`);
check("switching weapon clears the last one's optimizer ranking", !r.staleResults);
check("coming back restores THIS weapon's fight",
  r.backFight.level === r.aFight.level && r.backFight.dur === r.aFight.dur,
  JSON.stringify(r.backFight));
await app.finish("every collection owns its own state");
