// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SIMULATOR SHOWS THE LIST IT RAN. A mode has always been an action
// priority list written in Rust; the build card now prints that list in the
// order it is scanned, in the combat record's own vocabulary — and it prints
// the FIGHT'S answer, because which press a mode is played on is the form's
// and whether a flash converts a swing is the build's.
//
//   node scripts/check_apl_shown.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  const ran = async (weapon, mode, mods) => (await api('/api/simulate',
    { weapon, mode, mods: mods || [], runs: 10, seed: 24301, duration: 60 })).apl;
  // A PLAIN WEAPON, chosen by its MODES rather than named — plenty of
  // ordinary-looking guns have an Incarnon, and naming one is how this check
  // comes to assert a cycle by accident.
  const flat = META.weapons.find((w) => (w.modes || []).length === 1
    && (w.mode_kinds || {})[w.modes[0]] === 'base');
  const plain = { weapon: flat.id, lines: await ran(flat.id, flat.modes[0]) };
  // …ONE PLAYED AS A CYCLE, which is the mode that was never just shooting…
  const cyc = META.weapons.find((w) => (w.modes || []).some((m) => (w.mode_kinds || {})[m] === 'cycle'));
  const cycMode = cyc.modes.find((m) => (cyc.mode_kinds || {})[m] === 'cycle');
  const cycle = { weapon: cyc.id, mode: cycMode, lines: await ran(cyc.id, cycMode) };
  // …AND A MELEE ONE, whose press is the mode and whose flash outranks it.
  const melee = { bare: await ran('praedos', 'slide'),
                  flash: await ran('praedos', 'slide', ['dreamers_wrath']),
                  heavy: await ran('praedos', 'heavy', ['dreamers_wrath']) };
  // WHAT THE PAGE PRINTS, after a run of its own on the plain weapon.
  history.pushState({}, '', weaponPath(flat.id) + '/simulator'); route(); await sleep(4500);
  renderResults({ apl: plain.lines, target: {}, duration: 60 }, Date.now()); await sleep(600);
  const shown = [...document.querySelectorAll('#sim-build-info .sb-apl li')]
    .map((li) => li.textContent.replace(/\\s+/g, ' ').trim().replace(' if ', ',if='));
  return { plain, cycle, melee, shown };
})()`);
console.log(JSON.stringify(r, null, 1));
check("a plain weapon's list is reload-when-dry then the trigger",
  JSON.stringify(r.plain.lines) === JSON.stringify(["reload,if=!can_fire", "shoot"]),
  JSON.stringify(r.plain));
check("a cycle names both ends of the transmute, in scan order",
  r.cycle.lines.length === 4
  && r.cycle.lines[0] === "transform_in,if=gauge.pct>=1"
  && r.cycle.lines[1] === "transform_out,if=gauge.pct<=0",
  JSON.stringify(r.cycle));
check("a melee list names the PRESS, not a trigger pull",
  JSON.stringify(r.melee.bare) === JSON.stringify(["reload,if=!can_fire", "slide"]),
  JSON.stringify(r.melee.bare));
check("...and a Tennokai build converts one swing, above the combo it replaces",
  r.melee.flash[0] === "heavy,if=tennokai",
  JSON.stringify(r.melee.flash));
check("...while a heavy build converts nothing — the flash pays the other way",
  !r.melee.heavy.some((l) => /tennokai/.test(l))
  && r.melee.heavy[r.melee.heavy.length - 1] === "heavy",
  JSON.stringify(r.melee.heavy));
check("the page prints the list the fight answered with, line for line",
  JSON.stringify(r.shown) === JSON.stringify(r.plain.lines), JSON.stringify(r.shown));
await app.finish("the simulator shows the list it ran");
