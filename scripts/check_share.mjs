// A SHARE LINK, end to end, in a browser that has never seen the build.
//
// Exists because this path broke twice in ways a state check could not see.
// Both times the presets landed correctly and `slots` held the right mods —
// and the visitor still stared at an empty page, because what is VISIBLE is
// decided somewhere else. So this asserts the screen, not the variables:
// the builder is shown, the home grid is not, and the stats panel the
// recipient renders is the one the sender saw, character for character.
//
//   node scripts/check_share.mjs        (serves site/, drives headless Chrome)
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;

// SHARING CAN BE SWITCHED OFF, and while it is, THIS is what has to hold: no
// way to make a new link, and every link already posted still opens a page.
// A blank is the one outcome a posted URL must never reach — it is what got the
// feature switched off (owner, 2026-08-07).
//
// The round trip below runs again the moment the flag goes back to true, so
// this file is the check for both states rather than a file to remember to
// restore.
const on = await evaluate("typeof SHARE_ENABLED === 'undefined' ? true : SHARE_ENABLED");
if (!on) {
  const off = await evaluate(`(async () => {
    const s = (ms) => new Promise(r => setTimeout(r, ms));
    localStorage.clear();
    history.pushState({}, '', '/weapons/Torid'); route(); await s(2500);
    const out = { button: !!document.querySelector('.pchip.share') };
    // A link from when it was on: the query goes, a page arrives, and it says why.
    history.pushState({}, '', '/weapons/Torid?b=1abcNOTAREALCODE'); route(); await s(2500);
    out.query = location.search;
    out.drew = document.body.innerText.trim().length;
    out.onWeapon = !document.querySelector('.config-page').hidden;
    await s(1200);
    out.said = (document.getElementById('toast') || {}).textContent || '';
    return out;
  })()`);
  check("no way to make a new link while sharing is off", off.button === false);
  check("a link already posted still opens a page", off.drew > 400 && off.onWeapon === true,
    `${off.drew} chars, weapon page ${off.onWeapon}`);
  check("...the query is stripped, so a refresh cannot retry it", off.query === "");
  check("...and it says why", /sharing is off|分享功能暂时关闭/.test(off.said), JSON.stringify(off.said));
  await app.finish("sharing is off, and every posted link still opens a page");
}

// ---- the SENDER: a build with a riven and a non-default scenario ---------
const sent = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid/rivens'); route(); await sleep(1800);
  document.querySelector('.cu-new').click(); await sleep(700);
  riven.bonuses[0] = { id: 'damage', roll: 1.05 };
  riven.bonuses[1] = { id: 'critical_damage', roll: 1.0 };
  riven.bonuses[2] = { id: 'multishot', roll: 0.95 };
  riven.malus = { id: 'zoom', roll: 1.0 };
  markRivenDirty(); await sleep(1000);
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(1400);
  ['serration','split_chamber','point_strike','vital_sense','hellfire']
    .forEach((m, i) => { if (modById(m)) { slots[i].mod = m; slots[i].rank = modById(m).max_rank; } });
  slots[6].mod = rivenMods()[0].id;
  arcanes = ['primary_deadhead'];
  evoSel = { 1: 'torid_evo1_incarnon_form', 2: 'torid_final_fusillade' };
  sim.level = 155; sim.steel_path = false; sim.headshot_pct = 40;
  markPresetDirty(); markScenarioDirty(); renderMods(); refreshPanel(); await sleep(2000);
  const panel = ['stats-rows','stats-damage']
    .map(id => (document.getElementById(id) || {}).textContent || '').join(' | ').replace(/\\s+/g,' ').trim();
  // AND WHAT THE PANEL ACTUALLY OFFERS. Everything else here calls the codec
  // directly, so the panel could be wired to the wrong one of the two and no
  // assertion would notice — which is exactly what a sabotage of the panel
  // proved on the way in: three assertions stayed green while the button
  // handed out a link carrying the fight.
  const bar = document.querySelector('#preset-bar-builder-builds');
  await openSharePanel(bar); await sleep(900);
  const shown = ((bar.querySelector('.pshare .sh-url') || {}).value) || '';
  const dec = shown.includes('?b=') ? await decodeShare(shown.split('?b=')[1]) : null;
  return { url: await shareUrl(true), urlBuild: await shareUrl(false),
           panel, mods: slots.map(s => s.mod),
           shownIsBuildOnly: !!dec && dec.sc === null && dec.m === null,
           shownHasMods: !!dec && (dec.slots || []).some(x => x && x.mod) };
})()`);
// THE PANEL'S DEFAULT IS THE BUILD, not the claim: the claim costs a
// simulation and is one click further in, which is the whole point of the
// split — a panel that opens on a spinner is a panel nobody uses to paste a
// build into a chat.
check("the share panel offers the BUILD first", sent.shownIsBuildOnly === true,
  `decoded sc/m from the panel's own link`);
check("...and it is a real build, not an empty one", sent.shownHasMods === true);
check("a link is produced", !!sent.url, sent.url);
check("the link is under 600 characters", sent.url.length < 600, `${sent.url.length} chars`);
// A BUILD-only link drops fields 7 and 8 — the fight and the measurement — so
// it is strictly shorter than the claim it came from. Length is a feature
// here: these are posted into chat windows and printed into QR codes.
check("a build-only link is shorter than the claim", sent.urlBuild.length < sent.url.length,
  `build ${sent.urlBuild.length} vs claim ${sent.url.length} chars`);
// ---- the RECIPIENT: a real navigation, in a browser with nothing ---------
await evaluate(`(() => { localStorage.clear(); location.href = ${JSON.stringify("__URL__").replace("__URL__", sent.url)}; })()`);
await sleep(12000);
const got = await evaluate(`(async () => {
  const q = (s) => document.querySelector(s);
  await new Promise(r => setTimeout(r, 2500));
  const panel = ['stats-rows','stats-damage']
    .map(id => (document.getElementById(id) || {}).textContent || '').join(' | ').replace(/\\s+/g,' ').trim();
  return {
    search: location.search,
    homeVisible: !q('#home-page').hidden,
    configVisible: !q('.config-page').hidden,
    slotsDrawn: document.querySelectorAll('#mod-slots .slot').length,
    mods: slots.map(s => s.mod),
    rivens: loadPresetList('rivens').map(p => p.name),
    scenarioLevel: sim.level, headshot: sim.headshot_pct,
    activeBuild: activePreset,
    panel,
  };
})()`);
check("the builder is on screen without a refresh", got.configVisible && !got.homeVisible,
  `config=${got.configVisible} home=${got.homeVisible}`);
check("the mod slots are drawn", got.slotsDrawn === 8, `${got.slotsDrawn} slots`);
check("the query is stripped", got.search === "", got.search);
check("the build is the shared one", /\(shared\)/.test(got.activeBuild || ""), got.activeBuild);
check("the riven travelled", got.rivens.length === 1, JSON.stringify(got.rivens));
check("the riven is equipped in its slot", /^riven:/.test(got.mods[6] || ""), got.mods[6]);
check("the scenario travelled", got.scenarioLevel === 155 && got.headshot === 40,
  `level=${got.scenarioLevel} headshot=${got.headshot}`);
check("the panel reproduces the sender's exactly", got.panel === sent.panel,
  got.panel === sent.panel ? "" : `\n    sent: ${sent.panel.slice(0, 140)}\n    got : ${got.panel.slice(0, 140)}`);

// ---- ACT THREE: a BUILD is not a CLAIM, and it moves nobody's fight -------
//
// The sharp case, and the reason the split exists (owner, 2026-08-19). A build
// link posted into a chat is clicked by people who are in the middle of their
// own measurement; landing a scenario preset in their collection and switching
// them onto it is not a thing a build is allowed to do. `importShare` skips
// the whole scenario arm when no fight travelled, and this asserts the
// CONSEQUENCE rather than the branch — a reader's own level, their own active
// scenario, and the length of their own list, before and after.
//
// The recipient is set up on a DIFFERENT weapon with a DISTINCTIVE fight,
// because a scenario is shared across the roster (SHARED_DOMAINS) and the
// thing being tested is that switching weapons through a share link does not
// disturb it.
//
// A SCENARIO OF THEIR OWN FIRST. The app lands a first-time visitor on the
// OFFICIAL ruler, whose fight is PINNED — so writing `sim.level` on it changes
// an in-memory object that is never saved, and the reload reads the ruler's own
// 9999/100 back. The first version of this act did exactly that and its setup
// assertion still passed, because it read the same in-memory value it had just
// written. Same trap `check_arena.mjs` names: an editable fight has to be MADE,
// from the preset bar's "+ new", and the check has to assert that it was.
await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Braton/simulator'); route(); await sleep(3000);
  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1600); }
  sim.level = 90; sim.headshot_pct = 0; markScenarioDirty(); await sleep(1800);
})()`);
const mine = await evaluate(`({
  level: sim.level,
  official: typeof officialScenarioActive === 'function' ? officialScenarioActive() : null,
  active: activeScenario,
  scenarios: loadPresetList('simulator-scenarios').map(p => p.name),
})`);
// BOTH HALVES, because the in-memory value alone is what made the first
// version of this pass on a fight it could not actually have edited.
check("the reader has a fight of their own to disturb",
  mine.level === 90 && mine.official === false,
  `level=${mine.level} official=${mine.official} active=${mine.active}`);

await evaluate(`(() => { location.href = ${JSON.stringify(sent.urlBuild)}; })()`);
await sleep(12000);
const bo = await evaluate(`(async () => {
  await new Promise(r => setTimeout(r, 2500));
  return {
    weapon: document.getElementById('weapon').value,
    activeBuild: activePreset,
    mods: slots.map(s => s.mod),
    rivens: loadPresetList('rivens').map(p => p.name),
    level: sim.level, headshot: sim.headshot_pct,
    active: activeScenario,
    scenarios: loadPresetList('simulator-scenarios').map(p => p.name),
    said: (document.querySelector('.preset-toast, .ptoast') || {}).textContent || '',
  };
})()`);
check("a build-only link still lands the build", /\(shared\)/.test(bo.activeBuild || "")
  && bo.weapon === "torid", `weapon=${bo.weapon} build=${bo.activeBuild}`);
check("...and its riven with it", bo.rivens.length === 1 && /^riven:/.test(bo.mods[6] || ""),
  `${JSON.stringify(bo.rivens)} slot6=${bo.mods[6]}`);
check("...and the mods are the sender's", JSON.stringify(bo.mods) === JSON.stringify(sent.mods),
  `
    sent: ${JSON.stringify(sent.mods)}
    got : ${JSON.stringify(bo.mods)}`);
// THE THREE THAT MUST NOT HAVE MOVED.
check("the reader's fight is untouched", bo.level === 90 && bo.headshot === 0,
  `level=${bo.level} headshot=${bo.headshot} — the sender's was 155/40`);
check("...no scenario was planted in their list", bo.scenarios.length === mine.scenarios.length,
  `${JSON.stringify(mine.scenarios)} -> ${JSON.stringify(bo.scenarios)}`);
check("...and they are still on their own", bo.active === mine.active,
  `${mine.active} -> ${bo.active}`);

await app.finish("a shared link lands whole, on screen, first time");
