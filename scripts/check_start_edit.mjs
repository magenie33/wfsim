// AN OPTIMIZER START IS EDITED IN THE BUILDER, AND THE PLAYER'S BUILD SURVIVES IT.
//
// A start is a build (docs/OPTIMIZER.md, "PLANNED"), so "edit" opens it in the
// builder itself with a banner and a FIXED pin on every position. The builder
// autosaves every edit into the open preset, so the one thing that must never
// happen is the start's contents reaching the player's own build. This walks
// it: add a start, edit it, pin a card, change another, Done — then Discard —
// and asserts the start took the edit and the pin, the player's build and
// preset came back with identical contents, and an optimize from the start is
// a quick descent whose every answer keeps the pinned card.
//
//   node scripts/check_start_edit.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const r = await app.evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Verglas_Prime'); route(); await sleep(3000);
  const out = {};
  // The player's own build: three cards, which autosave makes a preset.
  ['serration','split_chamber','vital_sense'].forEach((m, i) => equipMod(i, m, null));
  renderMods(); await sleep(900); flushPresetSaves();
  out.myPreset = activePreset;
  const mine = JSON.stringify(snapshotState().slots.map(s => s.mod));
  const content = () => JSON.stringify(loadPresetList(BUILDS).map(p => [p.name, canon(p.state)]));
  const presetsBefore = content();

  // A start from a different build: just Hellfire.
  history.pushState({}, '', '/weapons/Verglas_Prime/optimizer'); route(); await sleep(1500);
  renderOpt(); await sleep(300);
  // A new search opens with a blank start; this walks one start of its own.
  opt.starts = [];
  addStart(stateFromBuild({ mods: ['hellfire'] }, 'verglas_prime'));
  out.startsAfterAdd = opt.starts.length;
  out.cardsInList = document.querySelectorAll('#opt-starts .opt-start .sb-chip').length;

  // Edit it in the builder.
  document.querySelector('#opt-starts [data-edit="0"]').click(); await sleep(1200);
  out.bannerShown = !$('start-edit-banner').hidden;
  out.barHidden = $('preset-bar-builder-builds').hidden;
  out.builderHolds = slots.map(s => s.mod).filter(Boolean);
  await sleep(200);
  out.pins = document.querySelectorAll('.start-pin').length;
  // Pin slot 1 (Hellfire), add a card in slot 2.
  document.querySelector('#mod-slots > :nth-child(1) > .start-pin').click();
  equipMod(1, 'point_strike', null); renderMods(); await sleep(900);
  out.pinStillOn = !!document.querySelector('#mod-slots > :nth-child(1) > .start-pin.on');
  out.presetsDuringEdit = content() === presetsBefore;
  $('start-edit-done').click(); await sleep(1200);
  out.startNow = opt.starts[0].build.slots.map(s => s.mod).filter(Boolean);
  out.startFixed = opt.starts[0].fixed;
  out.backToMine = JSON.stringify(snapshotState().slots.map(s => s.mod)) === mine;
  out.presetBack = activePreset === out.myPreset;
  out.presetsAfter = content() === presetsBefore;
  out.bannerGone = $('start-edit-banner').hidden;
  out.pinsGone = document.querySelectorAll('.start-pin').length;
  out.fixedChip = document.querySelectorAll('#opt-starts .sb-chip.fixed').length;

  // Discard: open, change, discard — the start is unchanged.
  const before = JSON.stringify(opt.starts[0]);
  document.querySelector('#opt-starts [data-edit="0"]').click(); await sleep(1200);
  equipMod(2, 'hammer_shot', null); renderMods(); await sleep(300);
  $('start-edit-discard').click(); await sleep(1200);
  out.discardKept = JSON.stringify(opt.starts[0]) === before;
  out.discardBack = JSON.stringify(snapshotState().slots.map(s => s.mod)) === mine;

  // Optimize from the start: Hellfire is fixed, so every answer carries it.
  const body = { weapon: 'verglas_prime',
    mods: Object.fromEntries(['serration','split_chamber','vital_sense','point_strike','hellfire','cryo_rounds','infected_clip','hammer_shot','heavy_caliber'].map(m => [m, 'search'])),
    build_size: 8, build_min: 8, ...theFight(), duration: 20, final_runs: 10, finalists: 3,
    strategy: 'quick', candidate_runs: 3, starts: opt.starts.map(startPayload) };
  await api('/api/optimize', body);
  let s = null;
  for (let i = 0; i < 600 && !(s && s.done); i++) { await sleep(500); s = await api('/api/optimize/status', {}); }
  const res = (s && s.result) || {};
  out.optOk = res.ok; out.optStrategy = res.strategy; out.optStarts = res.starts;
  out.rows = (res.results || []).length;
  out.allHellfire = (res.results || []).every(x => (x.mods || []).includes('hellfire'));
  return out;
})()`);
console.log(JSON.stringify(r, null, 1));
const c = app.check;
c("adding a start shows it as the simulator's card", r.startsAfterAdd === 1 && r.cardsInList >= 1);
c("editing opens it in the builder with the banner, the build bar put away", r.bannerShown && r.barHidden && r.builderHolds.join() === "hellfire");
c("every position has a pin", r.pins >= 9, String(r.pins));
c("a pin stays on through a redraw", r.pinStillOn === true);
c("no preset is written while a start is open", r.presetsDuringEdit === true);
c("done writes the build and the pin into the start", r.startNow.join() === "hellfire,point_strike" && r.startFixed.includes("mods:0"));
c("...and the player's own build and preset come back untouched", r.backToMine && r.presetBack && r.presetsAfter);
c("...and the banner and pins go away", r.bannerGone && r.pinsGone === 0);
c("the list marks the fixed card", r.fixedChip === 1);
c("discard leaves the start as it was and restores the build", r.discardKept && r.discardBack);
c("an optimize from the start is a quick descent from 1 start", r.optOk === true && r.optStrategy === "descent" && r.optStarts === 1 && r.rows > 0);
c("...and every answer keeps the fixed card", r.allHellfire === true);
await app.finish("start edit");
