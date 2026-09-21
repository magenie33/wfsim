// SPDX-License-Identifier: AGPL-3.0-or-later
// A ROBOTIC WEAPON IS HELD BY A COMPANION, not a Warframe: its Wielder offers the
// companion hosts and no frame, its block frames the companion's own page, and
// the request carries no wielder — the server answers with the companion floor,
// which spans the Sentinels and the MOA.
//
//   node scripts/check_companion_host.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  const weapon = META.weapons.find((w) => w.sentinel);
  const frame = META.weapons.find((w) => !w.sentinel);
  history.pushState({}, '', weaponPath(weapon.id)); route(); await sleep(4000);
  const start = { link: JSON.stringify(buildWielder), payload: wielderPayload() === undefined,
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
  const held = { resolved: wielderIdOf(buildWielder), payload: wielderPayload() === undefined,
    sub: document.getElementById('wielder-sub').textContent };
  // AND AN ORDINARY WEAPON IS UNTOUCHED: it still holds a Warframe.
  history.pushState({}, '', weaponPath(frame.id)); route(); await sleep(3500);
  const ordinary = { link: JSON.stringify(buildWielder),
    offers: [...document.getElementById('dd-wielder').parentElement.querySelectorAll('button')].length };
  return { weapon: weapon.id, start, offered, framed, held, ordinary };
})()`);
console.log(JSON.stringify(r, null, 1));
check("the floor spans the Sentinels and the MOA: the MOA's 367 health",
  r.start.floor.health === 367 && r.start.floor.shield === 130 && r.start.floor.armor === 80,
  JSON.stringify(r.start.floor));
check("a robotic weapon is held by the companion host, and sends no wielder",
  r.start.link === '{"frame":"prototype_companion","preset":null}' && r.start.payload, JSON.stringify(r.start));
check("...and its picker offers the hosts, never a Warframe",
  r.offered.length === 1 && r.offered[0] === "prototype_companion", JSON.stringify(r.offered));
check("its Wielder frames the companion's own page, which frames nothing further",
  /^\/companions\/Prototype_Companion\?embed=1/.test(r.framed.src) && r.framed.bar && !r.framed.nested,
  JSON.stringify(r.framed));
check("a build of the host is what the weapon holds, and still sends no wielder",
  r.held.resolved === "c1" && r.held.payload && /preset 1/.test(r.held.sub), JSON.stringify(r.held));
check("an ordinary weapon still holds a Warframe", r.ordinary.link.includes("prototype"), JSON.stringify(r.ordinary));
await app.finish("a companion weapon is held by a companion");
