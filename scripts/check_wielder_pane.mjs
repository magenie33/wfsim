// SPDX-License-Identifier: AGPL-3.0-or-later
// THE WIELDER'S PANE IS THE WARFRAME PAGE ITSELF, framed: opening a weapon's
// Wielder block loads `/warframes/<frame>?embed=1`, with the Operator page
// framed inside that, and an edit made in it is born as the frame's first build,
// linked as the wielder and sent with the request — without reloading the pane.
//
//   node scripts/check_wielder_pane.mjs
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Praedos'); route(); await sleep(3500);
  const start = { link: JSON.stringify(buildWielder), stored: loadPresetList(WF_BUILDS, 'prototype').length };
  document.querySelector('#wielder-block > .bh').click(); await sleep(6000);
  const fr = document.querySelector('#wielder-detail iframe');
  const w = fr.contentWindow;
  const framed = { embed: w.eval('EMBED') === true, chrome: getComputedStyle(w.document.querySelector('.topbar')).display,
    opFrame: !!w.document.querySelector('#wf-operator iframe'), wf: w.eval('wf && wf.frame') };
  const t0 = w.performance.timeOrigin;
  // A REAL EDIT INSIDE THE FRAME: a shard into socket 0, through the page's own code.
  const sh = w.eval('SHARDS()[0]');
  w.eval('wf.shards[0] = ' + JSON.stringify({ shard: sh.id, effect: sh.options[0].id, tauforged: false }));
  w.eval('wfChanged()'); await sleep(2500);
  const born = { link: JSON.stringify(buildWielder), resolved: wielderIdOf(buildWielder), stored: loadPresetList(WF_BUILDS, 'prototype').map(p => p.name), ids: presetListWithIds(WF_BUILDS, 'prototype').map(p => p.id),
    shards: ((wielderPayload() || {}).shards || []).length, sub: document.getElementById('wielder-sub').textContent,
    reloaded: fr.contentWindow.performance.timeOrigin !== t0, height: fr.style.height };
  // EDITED BACK TO THE BLANK INSIDE THE PANE: the preset is deleted, and the link
  // lands on the default (sent as no wielder for the Prototype).
  w.eval('wf.shards[0] = null'); w.eval('wfChanged()'); await sleep(3000);
  const emptied = { stored: loadPresetList(WF_BUILDS, 'prototype').length, resolved: wielderIdOf(buildWielder),
    wire: wielderPayload() === undefined };
  // …AND THE NEXT EFFECTIVE EDIT WRITES ONE AGAIN, through the reloaded pane.
  await sleep(4000);
  const w2 = document.querySelector('#wielder-detail iframe').contentWindow;
  w2.eval('wf.shards[0] = ' + JSON.stringify({ shard: sh.id, effect: sh.options[0].id, tauforged: false }));
  w2.eval('wfChanged()'); await sleep(2500);
  const reborn = { stored: loadPresetList(WF_BUILDS, 'prototype').length, resolved: wielderIdOf(buildWielder) };
  history.pushState({}, '', '/weapons/Braton'); route(); await sleep(3500);
  const follows = { link: JSON.stringify(buildWielder), shards: ((wielderPayload() || {}).shards || []).length };
  return { start, framed, born, emptied, reborn, follows };
})()`);
console.log(JSON.stringify(r, null, 1));
check("nothing stored and the unbuilt Prototype linked to begin with", r.start.stored === 0 && r.start.link === '{"frame":"prototype","preset":null}');
check("the pane is the Warframe page, framed, with the shell's chrome off and the Operator inside it",
  r.framed.embed && r.framed.chrome === "none" && r.framed.opFrame && r.framed.wf === "prototype", JSON.stringify(r.framed));
check("an edit inside the pane is born as preset 1 and becomes the wielder", r.born.stored.length === 1 && r.born.stored[0] === "preset 1"
  && r.born.resolved === r.born.ids[0] && r.born.shards === 1, JSON.stringify(r.born));
check("edited back to the blank in the pane, the preset is deleted and the link is the default",
  r.emptied.stored === 0 && r.emptied.resolved === "default" && r.emptied.wire === true, JSON.stringify(r.emptied));
check("...and the next effective edit writes one again", r.reborn.stored === 1 && r.reborn.resolved !== "default",
  JSON.stringify(r.reborn));
check("another weapon that never chose a preset follows the first one once it is written",
  r.follows.link === r.start.link && r.follows.shards === 1, JSON.stringify(r.follows));
check("...without reloading the pane, and it sized itself", r.born.reloaded === false && /px$/.test(r.born.height), JSON.stringify(r.born));
await app.finish("wielder");
