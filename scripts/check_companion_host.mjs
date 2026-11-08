// SPDX-License-Identifier: AGPL-3.0-or-later
// Weapon-specific carriers, derived floors and saved assemblies reach the fight.
//
//   node scripts/check_companion_host.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const guards = await evaluate(`(async () => {
  const result = {};
  for (const [weapon, companion] of [['deconstructor_prime', 'shade'], ['smeeta_claws', 'adarza_kavat'], ['verglas', 'hound']]) {
    const response = await api('/api/simulate', {weapon, duration:0.1, runs:1, distance:0, mods:[], wielder:{companion}});
    result[weapon] = !!response?.error;
  }
  const parts = await api('/api/simulate', {weapon:'verglas', duration:0.1, runs:1, distance:0, mods:[],
    wielder:{companion:'moa', assembly:{core:'adlet_core'}}});
  result.foreignParts = !!parts?.error;
  return result;
})()`);
check("the fight refuses hosts outside each weapon's legal carrier scope", Object.values(guards).every(Boolean), JSON.stringify(guards));
if (process.env.CHECK_COMPANION_GUARDS_ONLY) await app.finish("companion carrier and part guards");
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  const weapon = META.weapons.find((w) => w.id === 'verglas');
  const frame = META.weapons.find((w) => !w.sentinel);
  history.pushState({}, '', weaponPath(weapon.id) + '/builder'); route(); await sleep(4000);
  const start = { link: JSON.stringify(buildWielder), payload: wielderPayload(),
    floor: META.sentinel_floor };
  // THE PICKER OFFERS HOSTS AND NO WARFRAME.
  document.getElementById('dd-wielder').click(); await sleep(500);
  const offered = [...document.querySelectorAll('#dd-menu .opt')].map((o) => o.dataset.v);
  document.getElementById('dd-popover').hidden = true;
  document.querySelector('#wielder-block > .bh').click(); await sleep(9000);
  const pane = document.querySelector('#wielder-detail iframe');
  const framed = { src: pane.getAttribute('src'),
    bar: !!pane.contentDocument.querySelector('#preset-bar-companions'),
    nested: !!pane.contentDocument.querySelector('#wielder-detail iframe, #comp-owner-detail iframe') };
  // A BUILD OF THE HOST IS THE HOST'S, and the weapon holds it.
  storePresetList(COMP_BUILDS, [{ id: 'c1', name: 'preset 1', savedAt: 1, state: { companion: 'prototype_companion' } }], 'prototype_companion');
  setWielder({ frame: 'prototype_companion', preset: 'c1' }); await sleep(1500);
  const held = { resolved: wielderIdOf(buildWielder), payload: wielderPayload(),
    sub: document.getElementById('wielder-sub').textContent };
  // AND AN ORDINARY WEAPON IS UNTOUCHED: it still holds a Warframe.
  history.pushState({}, '', weaponPath(frame.id) + '/builder'); route(); await sleep(3500);
  const ordinary = { link: JSON.stringify(buildWielder),
    offers: [...document.getElementById('dd-wielder').parentElement.querySelectorAll('button')].length };
  await nav('/companions/Moa/builder'); await sleep(3000);
  document.getElementById('dd-comp-core').click(); await sleep(200);
  document.querySelector('#dd-menu .opt[data-v="drex_core"]').click(); await sleep(700);
  const saved = compList('moa').find(p => p.id === compActive);
  const assembled = {core:comp.assembly.core, saved:saved?.state.assembly?.core,
    choices:document.querySelectorAll('#comp-assembly .dd').length};
  const cat = WFCAT;
  WFCAT = null;
  const withoutCatalog = wielderPayloadOf({frame:'moa', preset:saved.id});
  WFCAT = cat;
  const deconstructor = weaponInfo('deconstructor_prime');
  const claw = weaponInfo('smeeta_claws');
  const apiHost = await api('/api/panel', {weapon:'deconstructor_prime', wielder:{companion:'helios'}});
  await nav(weaponPath('deconstructor_prime') + '/builder');
  const doorDefault = await window.wfsim.do('builder.wielder.set', {frame:null});
  const doorHost = wielderPayload().companion;
  const doorRefused = await window.wfsim.do('builder.wielder.set', {frame:'shade'});
  return { weapon: weapon.id, start, offered, framed, held, ordinary, assembled, withoutCatalog,
    doorDefault, doorHost, doorRefused,
    expectedCarriers: weapon.companion_carriers,
    helios:deconstructor.companion_carriers, claw:claw.companion_carriers,
    bodies:META.companions.length, parts:WFCAT.companion_parts.length, apiHost:apiHost.wielder,
    moaFloor:compHost('prototype_moa').body, houndFloor:compHost('prototype_hound').body };
})()`);
console.log(JSON.stringify(r, null, 1));
check("the default floor derives the MOA's unrounded 367.5 health",
  r.start.floor.health === 367.5 && r.start.floor.shield === 130 && r.start.floor.armor === 80,
  JSON.stringify(r.start.floor));
check("a robotic weapon names its actual companion host even without mods",
  r.start.link === '{"frame":"prototype_companion","preset":null}' && r.start.payload?.companion === 'prototype_companion', JSON.stringify(r.start));
check("...and its picker offers the hosts, never a Warframe",
  r.offered.length === r.expectedCarriers.length && r.expectedCarriers.every(id => r.offered.includes(id)),
  JSON.stringify(r.offered));
check("its Wielder frames the companion's own page, which frames nothing further",
  /^\/companions\/Prototype_Companion\?embed=1/.test(r.framed.src) && r.framed.bar && !r.framed.nested,
  JSON.stringify(r.framed));
check("a build of the host keeps its identity when no card is seated",
  r.held.resolved === "c1" && r.held.payload?.companion === 'prototype_companion' && /preset 1/.test(r.held.sub), JSON.stringify(r.held));
check("an ordinary weapon still holds a Warframe", r.ordinary.link.includes("prototype"), JSON.stringify(r.ordinary));
check("all concrete bodies, five prototypes and modular parts are in the catalogue", r.bodies === 41 && r.parts === 49);
check("the Deconstructors offer only the Helios pair and their scoped Prototype",
  r.helios.length === 3 && ['helios','helios_prime','prototype_helios'].every(id => r.helios.includes(id)), JSON.stringify(r.helios));
check("Smeeta Claws offer only Smeeta", JSON.stringify(r.claw) === '["smeeta_kavat"]', JSON.stringify(r.claw));
check("the selected unmodded body reaches the weapon panel", r.apiHost.health === 560 && r.apiHost.shield === 250 && r.apiHost.armor === 80, JSON.stringify(r.apiHost));
check("changing a modular part through the picker is saved with the build", r.assembled.core === 'drex_core' && r.assembled.saved === 'drex_core', JSON.stringify(r.assembled));
check("a saved assembly reaches the fight before the companion catalogue loads", r.withoutCatalog.assembly.core === 'drex_core', JSON.stringify(r.withoutCatalog));
check("the agent door selects the weapon's scoped default and refuses an illegal host", r.doorDefault.ok
  && r.doorHost === 'prototype_helios' && !r.doorRefused.ok, JSON.stringify([r.doorDefault,r.doorHost,r.doorRefused]));
check("MOA and Hound prototypes show minima with three concrete witnesses", r.moaFloor.stats.health === 367.5
  && r.houndFloor.stats.health === 315 && r.moaFloor.floor_sources.length === 3 && r.houndFloor.floor_sources.length === 3);
await app.finish("a companion weapon is held by a companion");
