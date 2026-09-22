// SPDX-License-Identifier: AGPL-3.0-or-later
// CASTING IS A TICK ON THE FIGHT, AND IT COSTS SOMETHING. A ticked ability is
// assumed up by default — what every board row was measured under — and the
// Cast them box makes the fight pay for it in energy and in shooting time.
//
//   node scripts/check_ability_casting.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Valkyr_Talons/simulator'); route(); await sleep(4500);
  // A FIGHT OF THE READER'S OWN: a ruler pins its fields.
  document.querySelector('#preset-bar-simulator-scenarios .pchip.add').click(); await sleep(1500);
  const box = document.getElementById('sim-wfbuffs-cast');
  const before = { there: !!box, ticked: !!(box && box.checked), field: sim.cast_abilities };
  box.click(); await sleep(1200);
  const after = { ticked: box.checked, field: sim.cast_abilities };
  // WHAT THE FIGHT ACTUALLY SENDS — theFight() is the one payload builder.
  const sent = !!theFight().cast_abilities;
  // A RE-RENDER REPLACES THE NODE, so the box has to be found again — clicking
  // the old one clicks something no longer on the page.
  document.getElementById('sim-wfbuffs-cast').click(); await sleep(1200);
  const off = { field: sim.cast_abilities, key: 'cast_abilities' in sim,
    ticked: document.getElementById('sim-wfbuffs-cast').checked };
  return { before, after, sent, off };
})()`);
console.log(JSON.stringify(r, null, 1));
check("the fight offers a Cast them box, off by default", r.before.there && !r.before.ticked && !r.before.field,
  JSON.stringify(r.before));
check("...ticking it sets the fight's own field", r.after.ticked === true && r.after.field === true,
  JSON.stringify(r.after));
check("...and it reaches the request", r.sent === true, String(r.sent));
check("...and unticking clears the key rather than sending false",
  !r.off.key && !r.off.ticked, JSON.stringify(r.off));
await app.finish("abilities can be cast for");
