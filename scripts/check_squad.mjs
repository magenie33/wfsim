// WHAT THE WARFRAME BRINGS reaches the fight — the THIRTY-FIFTH check.
//
// A squad AURA and an ARCHON SHARD are the first things this app models that
// belong to neither the weapon nor the build: they ride on the fight's Tenno,
// exactly as a Warframe ability buff does, which is what gives the optimizer
// them for free and keeps them off the board.
//
// The failure this exists to catch is the one that LOOKS like it works. Both
// families are offered from a roster and both write into `sim`, so a panel that
// drew every card correctly and sent nothing would read as a working feature —
// the same shape as the mode control that was picked on the optimizer tab and
// never sent (check_opt_modes.mjs). Every assertion below is either
// ON THE WIRE or on a real `/api/simulate` answer in the shipping wasm build.
//
// The other half is the ADMISSION. Twenty of the twenty-seven shard effects pay
// nothing in this arena, and three of those are real weapon-damage quantities
// the engine has no bucket narrow enough for — so "modelled" cannot be read off
// `OutOfScope` and the page must print the ENGINE's own answer. A socket that
// quietly does nothing is worse than one that says so.

import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const out = {};
  localStorage.clear();
  history.pushState({}, '', '/weapons/Braton/simulator'); route(); await sleep(3000);

  // A SCENARIO OF YOUR OWN FIRST. A first-time visitor lands on the OFFICIAL
  // ruler, whose fight is pinned — every edit below would be refused by a lock
  // that has nothing to do with this feature (check_arena.mjs).
  const bar0 = document.querySelector('#preset-bar-simulator-scenarios');
  const add0 = bar0 && bar0.querySelector('.pchip.add');
  if (add0) { add0.click(); await sleep(1500); }
  out.startedEditable = typeof officialScenarioActive === 'function' && !officialScenarioActive();

  // ---- 1. THE ROSTER IS SERVED, and it is the ENGINE'S ------------------
  const A = META.auras || [], S = META.shards || [];
  out.auras = A.length;
  out.stacking = A.filter((x) => x.squad_stacking).length;
  out.shards = S.length;
  out.options = S.reduce((n, d) => n + d.options.length, 0);
  // The ENGINE decides who an aura pays; meta states the consequence PER
  // WEAPON, which is evo_forbids' own pattern. The page never re-derives it.
  out.onWeapon = Array.isArray((weaponInfo(document.getElementById('weapon').value) || {}).auras);
  out.unmodelled = S.reduce((n, d) => n + d.options.filter((o) => !o.modelled).length, 0);
  // …AND NOT ONE OF THEM IS SILENT. A blank reason is the failure.
  out.silent = S.reduce((n, d) => n + d.options.filter((o) => !o.modelled && !o.why_not).length, 0);

  // ---- 2. THE PANEL DRAWS BOTH, and a fight nobody touched brings neither
  const box = () => document.getElementById('sim-squad');
  out.hostDrew = !!box();
  out.addAura = !!document.getElementById('sq-aura-add');
  out.addShard = !!document.getElementById('sq-shard-add');
  out.emptyRows = box() ? box().querySelectorAll('.sq-row').length : -1;

  // PICK ONE OF EACH.
  sim.auras = [{ id: 'corrosive_projection', count: 4 }];
  sim.shards = [{ shard: 'emerald_archon_shard', effect: 'corrosion_stack_cap', tauforged: true }];
  renderSim(); await sleep(400);
  out.rows = box().querySelectorAll('.sq-row').length;
  out.count = (box().querySelector('[data-sqcount]') || {}).value;
  out.tau = !!(box().querySelector('[data-sqtau]') || {}).checked;
  // THE NUMBER IS ON THE ROW: "+2 (+3)" is what a reader compares, and it is
  // printed from the effect's own value rather than written into a string, so
  // a number DE moves costs no translation.
  out.shardLabel = ((box().querySelector('#sq-shard-0 .dd-v') || {}).textContent || '').trim();

  // ---- 3. ON THE WIRE ---------------------------------------------------
  // theFight() is the ONE spelling of the fight, so this is what every module
  // sends — Run Sim, the share card, the quick calc and the optimizer.
  const f = theFight();
  out.wireAuras = JSON.stringify(f.auras || null);
  out.wireShards = JSON.stringify(f.shards || null);

  // ---- 4. AND IT REACHES THE ANSWER ------------------------------------
  // The sharp one, and the reason the three above are not enough: a payload
  // that travels and changes nothing is indistinguishable from a decoration.
  // Corrosive Projection is -18% armour PER SQUAD MEMBER, so four of them must
  // move a real simulate in the shipping wasm build.
  //
  // THE FIGHT IS CHOSEN SO THAT ARMOUR IS THE BINDING CONSTRAINT, which took a
  // wrong version to learn: at the default level an unmodded rifle never gets a
  // target off its shields, so the armour term is never read and the two runs
  // come back byte-identical — a passing engine and a failing assertion. The
  // measured quantity is 'score' (kill progress) rather than 'dps', because dps
  // is what the weapon PUTS OUT and armour decides what arrives.
  const build = buildPayload();
  const shot = (auras) => api('/api/simulate',
    { ...theFight(), ...build, auras, shards: [], enemy: 'corrupted_heavy_gunner',
      level: 1, duration: 20, runs: 20, seed: 12345 }).then((x) => x.score);
  out.bare = await shot([]);
  out.squad = await shot([{ id: 'corrosive_projection', count: 4 }]);

  // AN AURA THAT PAYS THIS WEAPON NOTHING SAYS SO ON THE ROW THAT OFFERS IT.
  // The amp family does not share one gate — Rifle Amp asks a mod POOL and
  // reaches bows, Dead Eye asks a CLASS and does not — so "this is a bow, why
  // is Dead Eye dead" is a question the list has to answer where it is asked.
  const items = (ddReg.get('sq-aura-add') || {}).items || [];
  const paid = new Set((weaponInfo(document.getElementById('weapon').value) || {}).auras || []);
  out.offered = items.length;
  out.dead = items.filter((i) => i.hint).length;
  out.alive = items.filter((i) => !i.hint).length;
  out.gateAgrees = items.every((i) => paid.has(i.value) === !i.hint);

  // ---- 5. THE OPTIMIZER SHOWS THE SAME FIGHT, READ-ONLY ------------------
  // A preset is edited in exactly ONE place: the optimizer
  // runs the simulator's fight and does not own a second squad.
  history.pushState({}, '', '/weapons/Braton/optimizer'); route(); await sleep(1500);
  const ob = document.getElementById('opt-squad');
  out.optDrew = !!ob && ob.querySelectorAll('.sq-row').length === 2;
  const ctl = ob ? [...ob.querySelectorAll('button,input')] : [];
  out.optReadonly = ctl.length > 0 && ctl.every((el) => el.disabled);
  return out;
})()`);

check("a scenario of your own is the active one for this run", r.startedEditable === true);
check("the aura roster is served", r.auras >= 8, `${r.auras} auras`);
check("six shard colours, every effect on each",
  r.shards === 6 && r.options >= 27, `${r.shards} colours / ${r.options} effects`);
check("at least one aura stacks across the squad", r.stacking >= 1);
check("the weapon carries WHICH auras pay it — the engine's answer, not the page's",
  r.onWeapon === true);
check("most shard effects admit they pay nothing here",
  r.unmodelled >= 15, `${r.unmodelled} of ${r.options}`);
check("...and not one of them is silent about why", r.silent === 0, `${r.silent} silent`);

check("the wielder block draws both lists, each with a way to add one",
  r.hostDrew && r.addAura && r.addShard);
check("...and a fight nobody has touched brings neither (the negative control)",
  r.emptyRows === 0, `${r.emptyRows} rows`);
check("one aura and one socket draw a row each", r.rows === 2, `${r.rows} rows`);
check("the aura carries how many of the squad run it", r.count === "4", String(r.count));
check("the socket carries whether the shard is Tauforged", r.tau === true);
check("...and the row prints both values, plain and Tauforged",
  /\+2\b/.test(r.shardLabel) && /\+3\b/.test(r.shardLabel), r.shardLabel);

check("the aura is on the wire with its count",
  r.wireAuras.includes("corrosive_projection") && r.wireAuras.includes("4"), r.wireAuras);
check("the socket is on the wire, Tauforged included",
  r.wireShards.includes("corrosion_stack_cap") && r.wireShards.includes("true"), r.wireShards);

check("four Corrosive Projections strip armour and the KILL PROGRESS follows",
  r.squad > r.bare * 1.05, `${r.bare.toFixed(3)} -> ${r.squad.toFixed(3)} kills`);

check("the picker separates the auras that pay from the ones that do not",
  r.dead > 0 && r.alive > 0, `${r.alive} pay, ${r.dead} do not, of ${r.offered}`);
check("...and it separates them exactly as the ENGINE said, row for row",
  r.gateAgrees === true);

check("the optimizer shows the same aura and the same socket", r.optDrew === true);
check("...read-only, because a fight is edited in one place", r.optReadonly === true);

await finish("what the Warframe brings reaches the fight");
