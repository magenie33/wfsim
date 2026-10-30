// SPDX-License-Identifier: AGPL-3.0-or-later
// FIRED UP PAYS ON A ROBOTIC WEAPON: seated on a Sentinel, it is marked as the
// card the fight runs, the Verglas linking that Sentinel sends its cards as the
// wielder, and the score climbs with the card's rank; the floor host, which
// cannot seat it, and no card at all send nothing.
//
//   WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_fired_up.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/companions/Sentinel'); route(); await sleep(4500);
  const seat = async (i, id) => {
    document.querySelectorAll('#comp-mod-slots .slot')[i].click(); await sleep(700);
    const row = [...document.querySelectorAll('#wf-menu .opt[data-id]')].find((o) => o.dataset.id === id);
    if (!row) return false;
    row.click(); await sleep(1200);
    return true;
  };
  const offered = { fired: await seat(0, 'fired_up'), vitality: await seat(1, 'enhanced_vitality') };
  const slots = [...document.querySelectorAll('#comp-mod-slots .slot.filled')];
  const chips = slots.map((s) => ({ pays: !!s.querySelector('.exchip:not(.seated)') && !s.querySelector('.wf-adm'),
    inert: !!s.querySelector('.wf-adm.unmodelled') }));
  const built = loadPresetList('companions', 'sentinel');
  const id = built[0] && built[0].id;
  // THE FLOOR HOST NEVER OFFERS IT.
  const floorOffers = compPool('prototype_companion').some((m) => m.id === 'fired_up');

  history.pushState({}, '', weaponPath('verglas') + '/builder'); route(); await sleep(4000);
  setWielder({ frame: 'sentinel', preset: id }); await sleep(1500);
  const payload = wielderPayload();
  const body = { ...buildPayload(), runs: 4, seed: 7 };
  const score = async (b) => { const x = await api('/api/simulate', b); return x && x.score; };
  const withCard = await score(body);
  const bare = await score({ ...body, wielder: undefined });
  const rank0 = await score({ ...body, wielder: { companion: 'sentinel', mods: [{ id: 'fired_up', rank: 0 }] } });
  const onFloor = await score({ ...body, wielder: { companion: 'prototype_companion', mods: [{ id: 'fired_up' }] } });
  setWielder({ frame: 'prototype_companion', preset: null }); await sleep(800);
  const floorPayload = wielderPayload();
  return { offered, chips, id, floorOffers, payload, withCard, bare, rank0, onFloor, floorPayload };
})()`);
console.log(JSON.stringify(r, null, 1));
check("a Sentinel offers Fired Up and the floor host does not",
  r.offered.fired && r.offered.vitality && !r.floorOffers, JSON.stringify([r.offered, r.floorOffers]));
check("Fired Up is marked as paying and Enhanced Vitality as not computed",
  r.chips.length === 2 && r.chips[0].pays && !r.chips[0].inert && r.chips[1].inert && !r.chips[1].pays,
  JSON.stringify(r.chips));
check("the Verglas sends the Sentinel's cards, each at its rank",
  r.payload && r.payload.companion === "sentinel" && r.payload.mods.length === 2
  && r.payload.mods[0].id === "fired_up" && r.payload.mods[1].id === "enhanced_vitality",
  JSON.stringify(r.payload));
check("Fired Up at max rank raises the Verglas's score, and rank 0 less",
  r.withCard > r.rank0 && r.rank0 > r.bare, JSON.stringify([r.withCard, r.rank0, r.bare]));
check("on the floor host it pays nothing, and the floor host with no build sends nothing",
  r.onFloor === r.bare && r.floorPayload === undefined, JSON.stringify([r.onFloor, r.bare, r.floorPayload]));
await app.finish("Fired Up pays on a robotic weapon");
