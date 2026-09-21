// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SIMULATOR SHOWS THE WIELDER, AND DOES NOT EDIT IT: the build card carries
// the Warframe with its Operator nested under it, read from the same stored
// builds the Builder edits, each with a link to the page that edits it.
//
//   node scripts/check_sim_wielder.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Praedos/simulator'); route(); await sleep(4000);
  const host = () => document.querySelector('#sim-build-info .sb-wielder');
  const links = () => [...host().querySelectorAll('a')].map(a => a.getAttribute('href'));
  const before = { text: host().textContent, links: links(), nested: !!host().querySelector('.wld-nest .wld-op') };
  await loadWarframeCatalog();
  const st = wfBlank('prototype');
  st.shards[0] = { shard: SHARDS()[0].id, effect: SHARDS()[0].options[0].id, tauforged: false };
  storePresetList(WF_BUILDS, [{ id: 'preset-1', name: 'preset 1', savedAt: 1, state: st }], 'prototype');
  setWielder({ frame: 'prototype', preset: 'preset-1' }); await sleep(1500);
  const after = { text: host().textContent, chips: host().querySelectorAll('.sb-chip').length };
  return { before, after };
})()`);
console.log(JSON.stringify(r, null, 1));
check("the summary names the Warframe and nests the Operator under it",
  r.before.text.includes("Prototype") && r.before.nested, r.before.text);
check("each half links to the page that edits it",
  r.before.links.some((h) => h.startsWith("/warframes/Prototype?build=")) && r.before.links.includes("/operator"),
  JSON.stringify(r.before.links));
check("what the Builder wrote shows up here, from the same stored build",
  r.after.chips > 0 && r.after.text.length > r.before.text.length, JSON.stringify(r.after));
await app.finish("the simulator shows the wielder");
