// SPDX-License-Identifier: AGPL-3.0-or-later
// THE WIELDER'S ABILITY STRENGTH REACHES THE CLAWS. Valkyr Talons' damage is
// Hysteria's at 100% strength, so a Valkyr build carrying strength mods raises
// the weapon's own panel — end to end, through the linked build the weapon
// holds rather than through a number typed into the fight.
//
//   node scripts/check_exalted_strength.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Valkyr_Talons'); route(); await sleep(4500);
  await loadWarframeCatalog();
  // WHAT THE PAGE ITSELF ASKS FOR, through the link the weapon holds — not a
  // number typed into the fight.
  const hit = async () => {
    const p = await api('/api/panel', buildPayload());
    return Number(p.forms[0].parts[0].damage_total);
  };
  const seat = async (mods) => {
    const st = wfNormalize(null, 'valkyr');
    mods.forEach((m, i) => { st.slots[i].mod = m; });
    storePresetList(WF_BUILDS, [{ id: 'v1', name: 'preset 1', savedAt: 1, state: st }], 'valkyr');
    setWielder({ frame: 'valkyr', preset: 'v1' });
    await sleep(1800);
    return hit();
  };
  const bare = await seat([]);
  // INTENSIFY IS +30% ABILITY STRENGTH.
  const strong = await seat(['intensify']);
  // …AND AN ORDINARY WEAPON IN THE SAME HANDS IS UNTOUCHED.
  history.pushState({}, '', '/weapons/Magistar'); route(); await sleep(3500);
  setWielder({ frame: 'valkyr', preset: 'v1' }); await sleep(1800);
  const otherStrong = await hit();
  setWielder({ frame: 'prototype', preset: null }); await sleep(1800);
  const otherBare = await hit();
  return { bare, strong, otherBare, otherStrong };
})()`);
console.log(JSON.stringify(r, null, 1));
check("the claws state the page's own 250 at 100% strength", r.bare === 250, String(r.bare));
// INTENSIFY IS +30%, so the claws swing for 1.3x — the proportion the page states.
check("a Valkyr built for strength swings for exactly that much more",
  r.strong === 325, `${r.bare} -> ${r.strong}`);
check("...and an ordinary weapon in the same hands is untouched",
  r.otherBare === r.otherStrong && r.otherBare > 0, `${r.otherBare} vs ${r.otherStrong}`);
await app.finish("an exalted weapon takes its wielder's ability strength");
