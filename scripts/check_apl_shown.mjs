// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SIMULATOR SHOWS THE LIST IT IS RUNNING. A mode has always been an action
// priority list written in Rust; the build card now prints that list in the
// order it is scanned, in the combat record's own vocabulary.
//
//   node scripts/check_apl_shown.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  const lines = () => [...document.querySelectorAll('#sim-build-info .sb-apl li')]
    .map((li) => li.textContent.replace(/\\s+/g, ' ').trim());
  // A PLAIN WEAPON: shoot, and reload when it cannot. CHOSEN BY ITS MODES
  // rather than named — plenty of ordinary-looking guns have an Incarnon, and
  // naming one is how this check comes to assert a cycle by accident.
  const flat = META.weapons.find((w) => (w.modes || []).length === 1
    && (w.mode_kinds || {})[w.modes[0]] === 'base');
  history.pushState({}, '', weaponPath(flat.id) + '/simulator'); route(); await sleep(4000);
  const plain = { weapon: flat.id, lines: lines() };
  // …AND ONE PLAYED AS A CYCLE, which is the mode that was never just shooting.
  const cyc = (META.weapons.find((w) => (w.modes || []).some((m) => (w.mode_kinds || {})[m] === 'cycle')) || {});
  const cycMode = (cyc.modes || []).find((m) => (cyc.mode_kinds || {})[m] === 'cycle');
  history.pushState({}, '', weaponPath(cyc.id) + '/simulator?mode=' + cycMode); route(); await sleep(4500);
  const cycle = { weapon: cyc.id, mode: cycMode, lines: lines(), presets: Object.keys(META.apl_presets || {}) };
  return { plain, cycle };
})()`);
console.log(JSON.stringify(r, null, 1));
check("a plain weapon's list is reload-when-dry then shoot",
  r.plain.lines.length === 2 && /reload/.test(r.plain.lines[0])
  && /can_fire/.test(r.plain.lines[0]) && /shoot/.test(r.plain.lines[1]),
  JSON.stringify(r.plain));
check("every mode kind is served as a list", r.cycle.presets.length === 4, JSON.stringify(r.cycle.presets));
check("a cycle names both ends of the transmute, in scan order",
  r.cycle.lines.length === 4
  && /transform_in/.test(r.cycle.lines[0]) && /gauge/.test(r.cycle.lines[0])
  && /transform_out/.test(r.cycle.lines[1]),
  JSON.stringify(r.cycle));
await app.finish("the simulator shows the list it runs");
