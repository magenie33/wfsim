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

  // ---- WHAT CROSSES BETWEEN WEAPONS, and what must not.
  //
  // A BUILD, a SEARCH and a RIVEN are statements about one weapon and may never
  // cross. A FIGHT is not — it is shared across the roster (owner, 2026-08-09),
  // because comparing two guns under one fight is what a scenario is FOR, and
  // the official rulers were always like this.
  $$('#opt-results').innerHTML = '<div id="stale-marker">last weapon ranking</div>';
  sim.duration = 77; markScenarioDirty(); await sleep(900);
  const aFight = { level: sim.level, dur: sim.duration, name: activeScenario };
  const aBuildMods = () => slots.filter(s => s.mod).map(s => s.mod);
  const aMods = aBuildMods();
  history.pushState({},'','/weapons/Dual_Toxocyst'); route(); await sleep(3000);
  const bFight = { level: sim.level, dur: sim.duration, name: activeScenario };
  const bMods = aBuildMods();
  // …and there is ONE list, not one per weapon.
  const perWeaponKeys = Object.keys(localStorage)
    .filter(k => /^wfsim-preset(s|-active)-.+-simulator-scenarios$/.test(k));
  const sharedList = JSON.parse(localStorage.getItem('wfsim-presets-simulator-scenarios') || '[]');
  const staleResults = !!document.getElementById('stale-marker');
  sim.level = 4242; markScenarioDirty(); await sleep(900);
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3000);
  const backFight = { level: sim.level, dur: sim.duration };

  return { names, carriesSim, levelBefore, levelAfter, onScreen, modAfter,
           buildUntouched: buildJson === buildJson2, scope, fin,
           buffId, mirroredValue: mirrored ? mirrored.value : null,
           mirroredLocked: mirrored ? mirrored.disabled : null, optOwnsBuffs,
           aFight, bFight, backFight, staleResults, aMods, bMods,
           perWeaponKeys, sharedNames: sharedList.map(p => p.name),
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
// THE FIGHT FOLLOWS YOU, and that is the point of it (owner, 2026-08-09).
// Measuring your own roster under your own fight is the thing a
// per-weapon scenario made impossible — you had to rebuild it on every
// gun.
check("a new weapon keeps the fight you were measuring under",
  r.bFight.dur === r.aFight.dur && r.bFight.level === r.aFight.level
    && r.bFight.name === r.aFight.name,
  `${JSON.stringify(r.aFight)} -> ${JSON.stringify(r.bFight)}`);
check("...from ONE list, not one per weapon",
  r.perWeaponKeys.length === 0 && r.sharedNames.length >= 1,
  `${r.perWeaponKeys.length} weapon-scoped keys, ${r.sharedNames.length} shared`);
// …AND THE BUILD STILL DOES NOT. This is the half the amendment narrows rather
// than removes: a build is a statement about one weapon, and inheriting the
// last one's is how you measure a gun you are not looking at.
check("...while the BUILD does not follow you",
  JSON.stringify(r.bMods) !== JSON.stringify(r.aMods) || r.aMods.length === 0,
  `${JSON.stringify(r.aMods)} vs ${JSON.stringify(r.bMods)}`);
check("switching weapon clears the last one's optimizer ranking", !r.staleResults);
check("coming back finds the fight where you left it",
  r.backFight.level === 4242 && r.backFight.dur === r.aFight.dur,
  JSON.stringify(r.backFight));
// A SCENARIO IS APPLIED ONTO THE DEFAULTS, NOT ONTO THE ONE YOU ARE LEAVING.
//
// The checks above compare fields both scenarios declare, and that is what let
// this through: a benchmark yaml states only what it has an opinion about, so a
// field it OMITS used to keep the outgoing scenario's value. Ticking Eximus on
// a copy of the official ruler and switching back left the official fight
// against an Eximus — `single_target.yaml` never says `eximus:` (owner,
// 2026-08-07). `invisible` survived the same test only because that yaml
// happens to state it, which is why one field is checked and the other is the
// control.
const leak = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await s(3200);
  const official = () => scenarioList().find((p) => p.builtin);
  const yamlSays = (k) => Object.prototype.hasOwnProperty.call(official().state, k);
  copyActiveScenario(); await s(1000);
  sim.eximus = true; sim.invisible = true; markScenarioDirty(); await s(800);
  const off = official();
  activeScenario = off.name; applyScenario(off.state); await s(800);
  return { onCopy: true, eximus: sim.eximus, invisible: sim.invisible,
           yamlStatesEximus: yamlSays('eximus'), yamlStatesInvisible: yamlSays('invisible') };
})()`);

check("the ruler does not state Eximus — so it is the field that can leak",
  leak.yamlStatesEximus === false, String(leak.yamlStatesEximus));
check("an edit to a COPY does not follow you back to the official fight",
  leak.eximus !== true, `eximus = ${JSON.stringify(leak.eximus)}`);
check("...and the field the ruler DOES state was never at risk",
  leak.invisible === false && leak.yamlStatesInvisible === true,
  `invisible = ${JSON.stringify(leak.invisible)}`);

// HOW A WEAPON IS PLAYED IS PART OF THE BUILD, and of nothing else.
//
// It was a scenario field, which let the FIGHT decide how the weapon was fired
// — so a ruler could pin an Incarnon weapon at its cycle and "never
// transmuting" was unaskable. It saves with the build now, and the simulator
// shows it without offering to move it: the fight owns the fight.
const md = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid'); route(); await s(3000);
  // ON ONE OF YOUR OWN. An official build is read-only by construction — its
  // mode is the one it was MEASURED in and nothing may write it — so testing
  // "a mode change is saved" against one would be testing the opposite rule.
  if (officialBuildActive()) {
    const own = [...document.querySelectorAll('#preset-bar-builder-builds .pchip')][0];
    if (own) own.click();
    await s(1500);
  }
  const out = { start: mode, official: officialBuildActive() };
  // ...and one that EXISTS in storage. Auto-save writes into the stored list
  // and does nothing when the active preset is not in it — which is the state
  // right after a localStorage.clear(), where the default build is in memory
  // only. Saving it once is the setup, not the thing under test.
  const ps0 = loadPresetList('builder-builds');
  if (!ps0.some((x) => x.name === activePreset)) {
    storePresetList('builder-builds',
      ps0.concat([{ name: activePreset, savedAt: Date.now(), state: snapshotState() }]));
  }
  document.querySelector('#mode-row [data-dd]').click(); await s(800);
  const base = [...document.querySelectorAll('#dd-menu .opt[data-v]')].find((o) => o.dataset.v === 'base');
  if (base) base.click();
  await s(1600);
  out.after = mode;
  const p = loadPresetList('builder-builds').find((x) => x.name === activePreset);
  out.stored = p && p.state ? p.state.mode : null;
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await s(2200);
  // NO REGEX HERE. A backslash in page-side code passes through the quoting
  // between this file and the browser, and /\s+/ arriving as /s+/ replaces every
  // letter s — which turned "Base Form" into "Ba e Form" and failed a check
  // about something that was working.
  out.simText = (document.getElementById('sim-build-info').textContent || '')
    .split(String.fromCharCode(10)).map((x) => x.trim()).filter(Boolean).join(' | ').slice(0, 160);
  out.simMode = mode;
  // AGAINST THE APP'S OWN LABEL, not against an English word. This check runs
  // in the machine's language, so 'Base Form' is 基础形态 here — the same trap
  // that broke check_opt_gain when an evolution got a Chinese name. Comparing
  // to what the app would print for this mode is true in every language.
  out.simShows = out.simText.includes(modeLabel(weaponInfo($('weapon').value), mode));
  // A CONTROL THAT BINDS THE MODE, not any control at all.
  //
  // It was "is there a dropdown in either block", which was a PROXY for "can
  // the mode be changed here" and held only while neither block had a dropdown
  // of its own. The Warframe picker became one on 2026-08-18 and this read it
  // as a mode control — the same stale proxy check_equip_rules carried, found
  // the same night.
  const bindsMode = (el) => /mode|form/i.test(
    (el.dataset.k || '') + ' ' + (el.dataset.dd || '') + ' ' + (el.id || ''));
  out.simCanEdit = [...document.querySelectorAll(
    '#sim-build-info [data-dd], #sim-build-info [data-k], #sim-technique [data-dd], #sim-technique [data-k]')]
    .some(bindsMode);
  return out;
})()`);

check("the check is standing on a build of your own", md.official === false, String(md.official));
check("changing the mode is a change to the BUILD",
  md.after === "base" && md.stored === "base", `state ${md.after}, stored ${md.stored}`);
check("...which the simulator shows", md.simShows === true, md.simText);
check("...and does not offer to change", md.simCanEdit === false);

// ...AND NO SCENARIO CARRIES ONE — official or your own, saved before or after.
//
// A scenario written while the mode lived in the fight still holds a `form`,
// and applying it would put that back on the live fight, where the next
// auto-save would write it out again and keep writing it forever. A fight has
// no opinion about how the weapon is fired (owner, 2026-08-07).
const clean = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await s(2200);
  const out = {};
  // Every ruler, and every scenario of your own.
  out.official = scenarioList().filter((p) => p.builtin)
    .filter((p) => 'form' in (p.state || {}) || 'mode' in (p.state || {})).map((p) => p.name);
  // A stored one written the OLD way, planted and then opened.
  const ps = loadPresetList('simulator-scenarios');
  storePresetList('simulator-scenarios',
    ps.concat([{ name: 'legacy fight', savedAt: Date.now(),
                 state: { ...snapshotScenario(), form: 'incarnon_cycle', level: 55 } }]));
  renderScenarioBar();
  await s(600);
  pickPreset(scenarioBarCfg(), 'legacy fight');
  await s(1200);
  out.appliedForm = sim.form === undefined ? null : sim.form;
  out.tookTheRest = sim.level;
  out.snapshotHasForm = 'form' in snapshotScenario();
  return out;
})()`);

check("no official ruler carries a mode", clean.official.length === 0, clean.official.join(","));
check("opening a scenario written the old way drops its form",
  clean.appliedForm === null, String(clean.appliedForm));
check("...while everything else about that fight still applies", clean.tookTheRest === 55,
  String(clean.tookTheRest));
check("...and nothing writes one back out", clean.snapshotHasForm === false);

// A WEAPON OPENED FOR THE FIRST TIME IS A BARE WEAPON — the cross-weapon rule
// ("绝对不能串"), which the SCENARIO has obeyed since 2026-08-02 and the BUILD
// did not. Its first build was seeded from `snapshotState()`, i.e. from the
// weapon you just left; mods survived that only because `restoreState` prunes
// them against the new pool, and an arcane has no such prune when it fits. So a
// Primary Crux picked up from a board build followed you onto every primary you
// opened afterwards (owner, 2026-08-08).
//
// Driven through a BOARD ROW because that is where a loaded arcane comes from
// without anyone choosing one, and it is how the report was produced.
const cross = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  const out = {};
  // A weapon whose board leader carries an arcane, opened the way the board
  // page links to it.
  const row = (BOARD['boar'] || []).find(r => (r.arcanes || []).length);
  out.seeded = row ? row.arcanes[0] : null;
  history.pushState({}, '', '/weapons/Boar?bench=' + (row || {}).benchmark + '&mode=' + ((row || {}).mode || 'base'));
  route(); await s(2600);
  out.fromBoard = arcanes.slice();
  // ...then a weapon never visited, reached the way the search bar reaches one.
  switchWeapon('sybaris'); await s(2000);
  out.next = { arcanes: arcanes.slice(), mods: slots.filter(x => x.mod).length,
               evos: Object.values(evoSel).filter(Boolean).length };
  const stored = JSON.parse(localStorage.getItem('wfsim-presets-sybaris-builder-builds') || '[]')[0];
  out.stored = (stored || {}).state || null;
  return out;
})()`);

check("a board build loads its own arcane", (cross.fromBoard || [])[0] === cross.seeded,
  `${JSON.stringify(cross.fromBoard)} vs ${cross.seeded}`);
check("...and NOTHING of it crosses to the next weapon",
  (cross.next.arcanes || []).every((a) => a === "none")
    && cross.next.mods === 0 && cross.next.evos === 0,
  JSON.stringify(cross.next));
check("...not even into what gets written as that weapon's first build",
  ((cross.stored || {}).arcane || ["none"]).every((a) => a === "none"),
  JSON.stringify((cross.stored || {}).arcane));

// THE SIMULATOR PICKS A BUILD; IT DOES NOT EDIT ONE (owner, 2026-08-07).
// The mode is part of the build, so the builder's control for it must not
// be on this tab — and the read-only Build card must state it instead, or
// the tab has simply lost a field. Both halves, because hiding the control
// alone would lose it.
const tabs = await evaluate(`(async () => {
  const s = (ms) => new Promise(r => setTimeout(r, ms));
  const shown = (sel) => {
    const el = document.querySelector(sel);
    return el ? getComputedStyle(el).display !== 'none' : null;
  };
  const out = {};
  // A weapon with a CHOICE of modes, so the control has something to offer.
  history.pushState({}, '', '/weapons/Torid'); route(); await s(2200);
  out.builderOffersIt = shown('#mode-block');
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await s(2200);
  out.simOffersIt = shown('#mode-block');
  out.simPresetBar = shown('#preset-bar-' + 'builder-builds');
  const card = document.querySelector('#sim-build-info');
  out.cardText = card ? card.textContent : '';
  out.cardSaysMode = out.cardText.indexOf(modeLabel(weaponInfo(document.getElementById('weapon').value), mode)) >= 0;
  // ...and a weapon with ONE way to be fired still has that stated: a summary
  // of the build may not silently drop a field the build has.
  history.pushState({}, '', '/weapons/Ocucor/simulator'); route(); await s(2400);
  const one = document.querySelector('#sim-build-info');
  out.oneModeCard = one ? one.textContent : '';
  out.oneModeStated = out.oneModeCard.indexOf(modeLabel(weaponInfo('ocucor'), mode)) >= 0;
  return out;
})()`);

check("the builder offers the mode", tabs.builderOffersIt === true);
check("...and the simulator does not", tabs.simOffersIt === false,
  `#mode-block display on the simulator: ${tabs.simOffersIt}`);
check("...it picks a build instead", tabs.simPresetBar === true);
check("...and its read-only card states the mode", tabs.cardSaysMode === true,
  JSON.stringify(tabs.cardText.slice(0, 120)));
check("...including a weapon with only one", tabs.oneModeStated === true,
  JSON.stringify(tabs.oneModeCard.slice(0, 120)));

await app.finish("every collection owns its own state");
