// SPDX-License-Identifier: AGPL-3.0-or-later
// THE ONE WHO LINKS CHOOSES, AND THE LINKED ONLY SAYS SO. Two weapons link two
// different presets of one Warframe; that frame's own page lists each preset
// with who links it, and choosing a preset on that page moves neither link.
//
//   node scripts/check_link_direction.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  // A WEAPON NOBODY LINKED ON, so the two under test are opened fresh below.
  history.pushState({}, '', '/weapons/Lex'); route(); await sleep(3500);
  await loadWarframeCatalog();
  storePresetList(WF_BUILDS, [
    { id: 'p1', name: 'preset 1', savedAt: 1, state: wfBlank('prototype') },
    { id: 'p2', name: 'preset 2', savedAt: 1, state: wfBlank('prototype') },
  ], 'prototype');
  const mk = (weapon, preset) => [{ name: 'preset 1', savedAt: 1,
    state: { ...blankBuildState(), weapon, wielder: { frame: 'prototype', preset } } }];
  storePresetList(BUILDS, mk('praedos', 'p1'), 'praedos');
  storePresetList(BUILDS, mk('braton', 'p2'), 'braton');
  localStorage.setItem(presetActiveKey(BUILDS, 'praedos'), 'preset 1');
  localStorage.setItem(presetActiveKey(BUILDS, 'braton'), 'preset 1');
  const names = { praedos: META.weapons.find(w => w.id === 'praedos').name, braton: META.weapons.find(w => w.id === 'braton').name };
  history.pushState({}, '', '/'); route(); await sleep(1500);

  history.pushState({}, '', '/warframes/Prototype'); route(); await sleep(3500);
  const chips = () => [...document.querySelectorAll('#preset-bar-warframes .pchip:not(.add)')]
    .map(c => ({ name: c.dataset.name, by: (c.querySelector('.pby') || {}).title || '' }));
  const listed = chips();
  // CHOOSING ON THE FRAME'S OWN PAGE: the second preset.
  const two = document.querySelector('#preset-bar-warframes .pchip[data-name="preset 2"]'); two.click(); await sleep(1200);
  const chosen = { active: wfActive };

  history.pushState({}, '', '/weapons/Praedos'); route(); await sleep(3500);
  const praedos = { link: JSON.stringify(buildWielder), shown: (document.getElementById('dd-wielder-preset') || {}).value };
  history.pushState({}, '', '/weapons/Braton'); route(); await sleep(3500);
  const braton = { link: JSON.stringify(buildWielder), shown: (document.getElementById('dd-wielder-preset') || {}).value };
  return { listed, chosen, praedos, braton, names };
})()`);
console.log(JSON.stringify(r, null, 1));
const by = (n) => (r.listed.find((c) => c.name === n) || {}).by || "";
check("each preset says who links it", by("preset 1").includes(r.names.praedos) && by("preset 2").includes(r.names.braton), JSON.stringify(r.listed));
check("choosing one on the frame's own page is only that page's", r.chosen.active === "preset 2");
check("...and it moved neither weapon's link",
  r.praedos.shown === "p1" && r.braton.shown === "p2", JSON.stringify([r.praedos, r.braton]));
// A PRESET THAT IS LINKED IS DELETED ON THE SECOND CLICK, and every link to it
// lands on the DEFAULT — the read-only blank — not on some other preset.
const d = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/warframes/Prototype'); route(); await sleep(3500);
  document.querySelector('#preset-bar-warframes .pchip[data-name="preset 2"]').click(); await sleep(800);
  const del = () => document.querySelector('#preset-bar-warframes .pchip.sel .pop.del');
  del().click(); await sleep(400);
  const armed = { text: del().textContent, kept: presetListWithIds(WF_BUILDS, 'prototype').length };
  del().click(); await sleep(1200);
  const left = presetListWithIds(WF_BUILDS, 'prototype').map(p => p.id);
  history.pushState({}, '', '/weapons/Lex'); route(); await sleep(2500);
  history.pushState({}, '', '/weapons/Braton'); route(); await sleep(3500);
  const braton = { link: JSON.stringify(buildWielder), shown: document.getElementById('dd-wielder-preset').textContent,
    wire: wielderPayload() === undefined };
  history.pushState({}, '', '/weapons/Lex'); route(); await sleep(2500);
  history.pushState({}, '', '/weapons/Praedos'); route(); await sleep(3500);
  const praedos = { link: JSON.stringify(buildWielder) };
  return { armed, left, braton, praedos };
})()`);
console.log(JSON.stringify(d, null, 1));
check("deleting a linked preset says who is affected first, and keeps it",
  /1/.test(d.armed.text) && d.armed.kept === 2, JSON.stringify(d.armed));
check("...and the second click deletes it", d.left.length === 1 && d.left[0] === "p1", JSON.stringify(d.left));
check("a link to it lands on the default, not on another preset",
  d.braton.link.includes('"default"') && d.braton.wire === true, JSON.stringify(d.braton));
check("...and the other weapon, linking the surviving one, is untouched", d.praedos.link.includes('"p1"'), JSON.stringify(d.praedos));
await app.finish("the linker chooses; the linked says so");
