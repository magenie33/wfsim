// EVERYTHING THAT DRIVES THIS PAGE FROM OUTSIDE GOES THROUGH ONE DOOR, AND AN
// ACTION IS SOMETHING A READER CAN DO.
//
// `window.wfsim` is the page's only programmatic surface: `observe()` to see,
// `do()` to act, `tools()` to learn the table. The checks, an in-page agent and
// a bot are all meant to stand on it, which is what makes the two properties
// above worth a check rather than a convention —
//
//   * an ACTION NAMES ITS CONTROL and that control is on the page, so the door
//     cannot outlive the button or grow an entrance no reader has;
//   * the ids are `<module>.<subject>.<verb>`, the domain namespace, because an
//     id is a wire an agent has already learned;
//   * `observe()` is BOUNDED — an agent pays for every byte in the window it
//     has to think in, so a scenario's formation reports its size, not itself;
//   * a REFUSAL IS AN ANSWER: a wrong id, a wrong slot, a field with its own
//     editor all come back as `ok:false` with a reason, and nothing throws;
//   * the UI MOVES when the door is used, which is the whole difference
//     between an agent working the page and an agent working behind it;
//   * and the decisions the door shares with the page's own handlers exist
//     ONCE — a second copy is how the click and the call start to disagree.
import { readFileSync } from "node:fs";
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;

// ---- one implementation, not two -------------------------------------------
//
// Asserted on the SOURCE, because the page cannot see its own duplication: two
// copies of the exchange rule behave identically until one of them is edited.
const src = readFileSync(new URL("../web/src/static/app.js", import.meta.url), "utf8");
const once = (needle) => src.split(needle).length - 1;
check("the mod-exchange decision exists once", once("[a.mod, b.mod] = [b.mod, a.mod]") === 1,
  once("[a.mod, b.mod] = [b.mod, a.mod]"));
check("clearing back to innate polarities exists once", once("s.pol = innate[i]") === 1,
  once("s.pol = innate[i]"));
check("the weapon control goes through the door", src.includes('wfsim.do("builder.weapon.set"'));
check("which slot a mod may sit in is decided once", once("=== !!m.stance") === 1, once("=== !!m.stance"));

await app.load("/weapons/Torid");

// ---- the table ---------------------------------------------------------------

const table = await evaluate(`(() => {
  const bad = [];
  const ok = /^[a-z]+\\.[a-z]+\\.[a-z]+$/;
  const heads = ["shell"].concat(AGENT_MODULES);
  for (const a of window.wfsim.actions) {
    if (!ok.test(a.id)) bad.push(a.id + ": not <module>.<subject>.<verb>");
    if (!heads.includes(a.id.split(".")[0])) bad.push(a.id + ": no such module");
    if (!a.what) bad.push(a.id + ": says nothing about itself");
    if (!document.querySelector(a.anchor)) bad.push(a.id + ": anchor " + a.anchor + " is not on the page");
  }
  const tools = window.wfsim.tools();
  const untooled = window.wfsim.actions.filter(a => !tools.some(t => t.name === a.id && t.input_schema));
  return { bad, n: window.wfsim.actions.length, untooled: untooled.map(a => a.id) };
})()`);
check("every action is named and anchored on a control", table.bad.length === 0, table.bad.join(" · "));
check("every action is a tool definition", table.untooled.length === 0, table.untooled.join(" · "));
check("the table is not empty", table.n > 0, table.n);

// ---- the observation ---------------------------------------------------------

const obs = await evaluate(`(() => {
  const o = window.wfsim.observe();
  return {
    bytes: JSON.stringify(o).length,
    ready: o.ready, module: o.route.module, weapon: o.route.weapon,
    can: o.can.length, keys: Object.keys(o),
    // The fight carries a formation; the observation must carry its SIZE.
    bodies: o.scenario ? o.scenario.bodies : "no scenario",
  };
})()`);
check("the observation is bounded", obs.bytes <= 4096, obs.bytes + " bytes");
check("it says where the reader is", obs.ready === true && obs.module === "builder" && obs.weapon === "torid",
  JSON.stringify([obs.ready, obs.module, obs.weapon]));
check("it says what can be done from here", obs.can > 0, obs.can);
check("a non-scalar fight field reports its size", obs.bodies === undefined || typeof obs.bodies === "object",
  JSON.stringify(obs.bodies));

// ---- the loop, and the page moving with it -----------------------------------

const loop = await evaluate(`(async () => {
  const out = {};
  const slotHtml = () => document.getElementById("mod-slots").innerHTML;
  const before = slotHtml();
  const mod = poolWithRivens()[0].id;
  out.seat = await window.wfsim.do("builder.mod.set", { slot: 0, mod });
  out.seated = window.wfsim.observe().build.slots.some(s => s.mod === mod);
  out.moved = slotHtml() !== before;
  out.clear = await window.wfsim.do("builder.mods.clear", {});
  out.emptied = window.wfsim.observe().build.slots.every(s => !s.mod);
  out.fight = await window.wfsim.do("simulator.scenario.set", { patch: { level: 130 } });
  out.level = window.wfsim.observe().scenario.level;
  out.runs = await window.wfsim.do("simulator.runs.set", { runs: 5 });
  out.open = await window.wfsim.do("shell.module.open", { module: "simulator" });
  await new Promise(r => setTimeout(r, 400));
  out.where = window.wfsim.observe().route.module;
  out.run = await window.wfsim.do("simulator.run.start", {});
  out.result = window.wfsim.observe().result;
  out.read = await window.wfsim.do("simulator.result.read", {});
  return out;
})()`, { awaitPromise: true });

check("seating a mod is accepted", loop.seat.ok === true, JSON.stringify(loop.seat).slice(0, 200));
check("...and the build says so", loop.seated === true);
check("...and the slot on screen redrew", loop.moved === true);
check("...and the change is reported back", !!(loop.seat.changed && loop.seat.changed.build),
  Object.keys(loop.seat.changed || {}).join(","));
check("clearing empties every slot", loop.clear.ok === true && loop.emptied === true);

// ---- the rest of the build ---------------------------------------------------

const build = await evaluate(`(async () => {
  const out = {};
  const slotHtml = () => document.getElementById("mod-slots").innerHTML;
  const before = slotHtml();
  out.pol = await window.wfsim.do("builder.polarity.set", { slot: 1, polarity: "Madurai" });
  out.polSeen = window.wfsim.observe().build.slots.some(s => s.seat === 1 && s.pol === "Madurai");
  out.polMoved = slotHtml() !== before;
  out.plan = await window.wfsim.do("builder.forma.plan", {});
  const arc = arcanePool(0)[0];
  out.arc = await window.wfsim.do("builder.arcane.set", { seat: 0, arcane: arc.id });
  out.arcSeen = window.wfsim.observe().build.arcanes[0].arcane === arc.id;
  out.arcOff = await window.wfsim.do("builder.arcane.set", { seat: 0, arcane: null });
  out.arcGone = window.wfsim.observe().build.arcanes[0].arcane === null;
  const mod = poolWithRivens()[0];
  out.rank = await window.wfsim.do("builder.mod.set", { slot: 0, mod: mod.id, rank: 0 });
  out.rankSeen = window.wfsim.observe().build.slots.find(s => s.seat === 0).rank === 0;
  out.capacity = window.wfsim.observe().build.capacity;
  const tiers = weaponEvos();
  out.hasEvos = tiers.length > 1;
  if (out.hasEvos) {
    out.evoLocked = await window.wfsim.do("builder.evolution.set", { tier: 2, evolution: tiers[1].options[0].id });
    out.evo = await window.wfsim.do("builder.evolution.set", { tier: 1, evolution: tiers[0].options[0].id });
    out.evoSeen = window.wfsim.observe().build.evolutions[1] === tiers[0].options[0].id;
  }
  await window.wfsim.do("builder.mods.clear", {});
  return out;
})()`, { awaitPromise: true });

check("a slot takes a polarity", build.pol.ok === true && build.polSeen === true, JSON.stringify(build.pol).slice(0, 200));
check("...and the slot on screen redrew", build.polMoved === true);
check("the Forma plan runs", build.plan.ok === true, JSON.stringify(build.plan).slice(0, 200));
check("an arcane seats and is observed", build.arc.ok === true && build.arcSeen === true, JSON.stringify(build.arc).slice(0, 200));
check("...and empties again", build.arcOff.ok === true && build.arcGone === true);
check("a mod seats at the rank asked for", build.rank.ok === true && build.rankSeen === true);
check("capacity is observed", !!build.capacity && typeof build.capacity.used === "number" && build.capacity.max > 0,
  JSON.stringify(build.capacity));
if (build.hasEvos) {
  check("a tier past the open one is refused", build.evoLocked.ok === false && build.evoLocked.reason === "tier_locked",
    JSON.stringify(build.evoLocked));
  check("an evolution installs and is observed", build.evo.ok === true && build.evoSeen === true, JSON.stringify(build.evo).slice(0, 200));
}
check("the fight takes a new level", loop.fight.ok === true && loop.level === 130, loop.level);
check("the run count is a preference the door can set", loop.runs.ok === true);
check("a module opens", loop.open.ok === true && loop.where === "simulator", loop.where);
check("the fight runs and answers", loop.run.ok === true && loop.run.result && loop.run.result.value > 0,
  JSON.stringify(loop.run).slice(0, 200));
check("...and the number is marked as this build's", !!(loop.result && loop.result.fresh === true),
  JSON.stringify(loop.result));
check("the run reads back with where its damage came from",
  loop.read.ok === true && loop.read.headline && loop.read.headline.fresh === true && loop.read.damage_sources.length > 0,
  JSON.stringify(loop.read).slice(0, 300));

// ---- presets: an agent works on a copy ---------------------------------------

const pre = await evaluate(`(async () => {
  const out = {};
  await window.wfsim.do("builder.mod.set", { slot: 0, mod: "serration" });
  await new Promise(r => setTimeout(r, 700)); // auto-save births the reader's build
  const mine = window.wfsim.observe().build.preset;
  out.mine = mine;
  out.copy = await window.wfsim.do("shell.preset.copy", { bar: "build" });
  out.onCopy = window.wfsim.observe().build.preset;
  await window.wfsim.do("builder.mods.clear", {});
  await new Promise(r => setTimeout(r, 700));
  out.list = await window.wfsim.do("shell.presets.list", { bar: "build" });
  out.back = await window.wfsim.do("shell.preset.open", { bar: "build", preset: mine });
  out.mineKept = window.wfsim.observe().build.slots.some(s => s.mod === "serration");
  out.fresh = await window.wfsim.do("shell.preset.new", { bar: "build" });
  out.blank = window.wfsim.observe().build.slots.every(s => !s.mod);
  out.bad = await window.wfsim.do("shell.preset.open", { bar: "build", preset: "no such build" });
  return out;
})()`, { awaitPromise: true });

check("the reader's build exists before the agent branches", !!pre.mine, JSON.stringify(pre.mine));
check("a copy opens as a new build", pre.copy.ok === true && pre.onCopy && pre.onCopy !== pre.mine,
  JSON.stringify([pre.copy, pre.onCopy]).slice(0, 200));
check("presets list, marking the open one", pre.list.ok === true && pre.list.rows.some(r => r.active && r.id === pre.onCopy),
  JSON.stringify(pre.list).slice(0, 300));
check("the reader's own build is untouched by work on the copy", pre.back.ok === true && pre.mineKept === true);
check("a new build opens blank", pre.fresh.ok === true && pre.blank === true, JSON.stringify(pre.fresh));
check("a build that does not exist is refused", pre.bad.ok === false && pre.bad.reason === "unknown_preset", JSON.stringify(pre.bad));

// ---- rivens, the wielder, a Kitgun's parts -----------------------------------

const more = await evaluate(`(async () => {
  const out = {};
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  await window.wfsim.do("shell.preset.new", { bar: "build" });
  // A RIVEN, written from the numbers printed on a card and seated.
  out.stats = await window.wfsim.do("rivens.stats.list", {});
  const b = out.stats.stats.filter(x => x.bonus && x.modeled !== false).slice(0, 2).map(x => x.id);
  const m = out.stats.stats.find(x => x.malus && !b.includes(x.id)).id;
  out.made = await window.wfsim.do("rivens.card.new", {});
  out.card = await window.wfsim.do("rivens.card.set", { shape: "2+1",
    bonuses: [{ stat: b[0], roll: 1.1 }, { stat: b[1], roll: 0.95 }], malus: { stat: m, roll: 0.9 } });
  out.badShape = await window.wfsim.do("rivens.card.set", { shape: "2+1", bonuses: [{ stat: b[0], roll: 1 }], malus: null });
  out.list = await window.wfsim.do("rivens.cards.list", {});
  out.seat = await window.wfsim.do("builder.mod.set", { slot: 0, mod: out.card.seat_as });
  out.seated = window.wfsim.observe().build.slots.some(s => s.mod === out.card.seat_as);
  // THE WIELDER.
  out.wielders = await window.wfsim.do("builder.wielders.list", {});
  const f = out.wielders.frames[0];
  out.held = await window.wfsim.do("builder.wielder.set", { frame: f.id });
  out.heldSeen = (window.wfsim.observe().build.wielder || {}).frame === f.id;
  out.proto = await window.wfsim.do("builder.wielder.set", { frame: null });
  await window.wfsim.do("builder.mods.clear", {});
  // A KITGUN'S PARTS, on a Kitgun.
  const kit = (META.weapons || []).find(w => w.assembly);
  out.onKit = await window.wfsim.do("builder.weapon.set", { weapon: kit.id });
  await wait(600);
  out.parts = await window.wfsim.do("builder.parts.list", {});
  const g = out.parts.grips.find(x => x.id !== out.parts.installed.grip);
  out.part = await window.wfsim.do("builder.part.set", { part: "grip", id: g.id });
  out.partSeen = window.wfsim.observe().build.assembly.grip === g.id;
  out.badPart = await window.wfsim.do("builder.part.set", { part: "loader", id: "no_such_loader" });
  await window.wfsim.do("builder.weapon.set", { weapon: "torid" });
  await wait(600);
  out.noParts = await window.wfsim.do("builder.part.set", { part: "grip", id: g.id });
  return out;
})()`, { awaitPromise: true });

check("a riven is made and written, and the engine prints it back",
  more.made.ok === true && more.card.ok === true && more.card.stats.length === 3 && more.card.stats.every(x => x.text),
  JSON.stringify(more.card).slice(0, 300));
check("...a card whose shape and stats disagree is refused", more.badShape.ok === false, JSON.stringify(more.badShape));
check("...it lists, and seats as a mod", more.list.rivens.some(r => r.seat_as === more.card.seat_as) && more.seat.ok === true && more.seated === true,
  JSON.stringify(more.seat));
check("a Warframe takes the weapon, and the Prototype takes it back",
  more.held.ok === true && more.heldSeen === true && more.proto.ok === true, JSON.stringify([more.held, more.proto]));
check("a Kitgun's grip swaps and is observed", more.part.ok === true && more.partSeen === true, JSON.stringify(more.part));
check("...a part that does not exist is refused, and so is a part on a weapon without them",
  more.badPart.ok === false && more.noParts.ok === false && more.noParts.reason === "no_parts",
  JSON.stringify([more.badPart, more.noParts]));

// ---- the board: the measured leaders, and opening one -------------------------

const board = await evaluate(`(async () => {
  const out = {};
  for (let i = 0; i < 40 && !BOARD[$("weapon").value]; i++) await new Promise(r => setTimeout(r, 250));
  out.read = await window.wfsim.do("builder.board.read", { riven: "any", limit: 1 });
  const top = out.read.ok && out.read.rows[0];
  if (top) {
    out.open = await window.wfsim.do("shell.preset.open", { bar: "build", preset: top.id });
    out.onIt = window.wfsim.observe().build.preset === top.id;
  }
  await window.wfsim.do("shell.preset.new", { bar: "build" });
  return out;
})()`, { awaitPromise: true });

check("the board reads back, ranked, each row with its mods",
  board.read.ok === true && board.read.rows.length > 0 && board.read.rows.every(r => r.rank === 1 && r.mods.length > 0),
  JSON.stringify(board.read).slice(0, 300));
check("a board row opens as the build", !!board.open && board.open.ok === true && board.onIt === true,
  JSON.stringify(board.open));

// ---- the fight's own editors: triggers, stat bonuses, class rules ------------

const fight = await evaluate(`(async () => {
  const out = {};
  out.triggers = await window.wfsim.do("simulator.triggers.list", {});
  const g = out.triggers.groups[0];
  out.off = await window.wfsim.do("simulator.trigger.set", { id: g.triggers[0].id, off: true });
  out.offSeen = (sim.buff_triggers_off || []).includes(g.triggers[0].id);
  out.on = await window.wfsim.do("simulator.trigger.set", { id: g.triggers[0].id, off: false });
  out.extra = await window.wfsim.do("simulator.extra.set", { stat: "crit_chance", percent: 30 });
  out.extraSeen = (sim.extra_stats || {}).crit_chance === 0.3;
  out.extraOff = await window.wfsim.do("simulator.extra.set", { stat: "crit_chance", percent: null });
  out.extraGone = !(sim.extra_stats || {}).crit_chance;
  out.rules = await window.wfsim.do("simulator.rules.list", {});
  const r = out.rules.rules.find(x => typeof x.default === "boolean");
  if (r) {
    out.rule = await window.wfsim.do("simulator.rule.set", { class: r.class, rule: r.rule, value: !r.default });
    out.ruleSeen = classRuleOf(r.class, r.rule) === !r.default;
    out.ruleSame = await window.wfsim.do("simulator.rule.set", { class: r.class, rule: r.rule, value: r.default });
    out.rulePruned = classRuleOf(r.class, r.rule) === undefined;
  }
  out.badTrigger = await window.wfsim.do("simulator.trigger.set", { id: "no_such_trigger", off: true });
  out.auras = await window.wfsim.do("simulator.auras.list", {});
  const au = out.auras.auras.find(x => x.stacks) || out.auras.auras[0];
  out.aura = await window.wfsim.do("simulator.aura.set", { aura: au.id, count: 4 });
  out.auraSeen = (sim.auras || []).find(a => a.id === au.id);
  out.auraMax = au.stacks ? 4 : 1;
  out.auraOff = await window.wfsim.do("simulator.aura.set", { aura: au.id, count: 0 });
  out.auraGone = !(sim.auras || []).some(a => a.id === au.id);
  out.abilities = await window.wfsim.do("simulator.abilities.list", {});
  const ab = out.abilities.abilities.find(x => x.elements) || out.abilities.abilities[0];
  out.ability = await window.wfsim.do("simulator.ability.set", { ability: ab.id, on: true, ...(ab.elements ? { element: ab.elements[ab.elements.length - 1] } : {}) });
  out.abilitySeen = wfPick(ab.id);
  out.abilityEl = ab.elements ? ab.elements[ab.elements.length - 1] : null;
  out.strength = await window.wfsim.do("simulator.strength.set", { percent: 250 });
  out.strengthSeen = sim.ability_strength === 2.5;
  out.abilityOff = await window.wfsim.do("simulator.ability.set", { ability: ab.id, on: false });
  out.abilityGone = !wfPick(ab.id);
  return out;
})()`, { awaitPromise: true });

check("a buff trigger switches off and back on", fight.off.ok && fight.offSeen && fight.on.ok, JSON.stringify(fight.off));
check("a fight stat bonus is set in percent and stored as a fraction, and clears",
  fight.extra.ok && fight.extraSeen && fight.extraOff.ok && fight.extraGone, JSON.stringify(fight.extra));
if (fight.rule) {
  check("a class rule is written, and one that agrees with the default is not stored",
    fight.rule.ok && fight.ruleSeen && fight.ruleSame.ok && fight.rulePruned, JSON.stringify([fight.rule, fight.ruleSame]));
}
check("a trigger that does not exist is refused", fight.badTrigger.ok === false, JSON.stringify(fight.badTrigger));
check("a squad aura is run by as many as it stacks for, and removed",
  fight.aura.ok && fight.auraSeen && fight.auraSeen.count === fight.auraMax && fight.auraOff.ok && fight.auraGone,
  JSON.stringify([fight.aura, fight.auraSeen]));
check("a Warframe ability runs with the element asked for, at the strength set, and stops",
  fight.ability.ok && fight.abilitySeen && (!fight.abilityEl || fight.abilitySeen.element === fight.abilityEl)
  && fight.strength.ok && fight.strengthSeen && fight.abilityOff.ok && fight.abilityGone,
  JSON.stringify([fight.ability, fight.abilitySeen]));

// ---- the search's scope, set through the page's own rules ---------------------

const scope = await evaluate(`(async () => {
  const out = {};
  await window.wfsim.do("shell.module.open", { module: "optimizer" });
  out.req = await window.wfsim.do("optimizer.scope.mark", { axis: "mods", id: "serration", mark: "fixed" });
  out.pool = await window.wfsim.do("optimizer.scope.mark", { axis: "mods", id: "hellfire", mark: "search" });
  out.size = await window.wfsim.do("optimizer.scope.size", { min: 2, max: 4 });
  out.read = await window.wfsim.do("optimizer.scope.read", {});
  out.onScreen = !!document.querySelector('#opt-mods .seg.on[data-m="serration"]');
  out.off = await window.wfsim.do("optimizer.scope.mark", { axis: "mods", id: "serration", mark: "off" });
  out.after = await window.wfsim.do("optimizer.scope.read", {});
  out.bad = await window.wfsim.do("optimizer.scope.mark", { axis: "mods", id: "no_such_mod", mark: "fixed" });
  const t = weaponEvos()[0];
  if (t) {
    out.evo = await window.wfsim.do("optimizer.scope.mark", { axis: "evolutions", tier: t.tier, id: t.options[0].id, mark: "search" });
    out.empty = await window.wfsim.do("optimizer.scope.empty", { axis: "evolutions", key: String(t.tier), empty: "never" });
    out.evoRead = (await window.wfsim.do("optimizer.scope.read", {})).evolutions[t.tier];
  }
  await window.wfsim.do("optimizer.scope.mark", { axis: "mods", id: "hellfire", mark: "off" });
  await window.wfsim.do("optimizer.scope.size", { min: 0, max: 8 });
  await window.wfsim.do("shell.module.open", { module: "builder" });
  return out;
})()`, { awaitPromise: true });

check("the search's scope takes a required and a searched mod, and reads them back",
  scope.req.ok && scope.pool.ok && scope.read.mods.fixed.includes("serration") && scope.read.mods.search.includes("hellfire"),
  JSON.stringify(scope.read).slice(0, 300));
check("...the scope on screen moved with it", scope.onScreen === true);
check("...its size bounds hold", scope.size.ok && scope.read.size.min === 2 && scope.read.size.max === 4, JSON.stringify(scope.read.size));
check("...a mark clears", scope.off.ok && !scope.after.mods.fixed.includes("serration"), JSON.stringify(scope.after.mods));
check("...a mod the weapon cannot take is refused", scope.bad.ok === false && scope.bad.reason === "not_in_scope", JSON.stringify(scope.bad));
if (scope.evo) {
  check("an evolution tier is searched, and never left empty", scope.evo.ok && scope.empty.ok
    && scope.evoRead.search.length === 1 && scope.evoRead.empty === undefined, JSON.stringify(scope.evoRead));
}

// ---- the search: started, watched, stopped -----------------------------------
//
// A search is minutes long, so the door returns at once and is polled. The
// check stops it rather than waiting it out: what is asserted is the shape of
// the conversation, not the ranking.

const opt = await evaluate(`(async () => {
  const out = {};
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  await window.wfsim.do("builder.mod.set", { slot: 0, mod: "serration" });
  await window.wfsim.do("shell.module.open", { module: "optimizer" });
  await wait(400);
  out.idle = await window.wfsim.do("optimizer.search.stop", {});
  out.start = await window.wfsim.do("optimizer.search.start", {});
  out.twice = await window.wfsim.do("optimizer.search.start", {});
  out.running = await window.wfsim.do("optimizer.search.read", {});
  out.stop = await window.wfsim.do("optimizer.search.stop", {});
  for (let i = 0; i < 120 && optJobId != null; i++) await wait(500);
  out.stopped = optJobId == null;
  out.after = await window.wfsim.do("optimizer.search.read", {});
  if (out.after.ok && out.after.results && out.after.results.length) {
    out.save = await window.wfsim.do("optimizer.result.save", { rank: out.after.results[0].rank });
  }
  out.badRank = await window.wfsim.do("optimizer.result.save", { rank: 999 });
  await window.wfsim.do("shell.module.open", { module: "builder" });
  await window.wfsim.do("builder.mods.clear", {});
  return out;
})()`, { awaitPromise: true });

check("stopping with nothing running is refused", opt.idle.ok === false && opt.idle.reason === "no_search_running",
  JSON.stringify(opt.idle));
check("a search starts and returns at once", opt.start.ok === true, JSON.stringify(opt.start).slice(0, 200));
check("a second start is refused while one runs", opt.twice.ok === false && opt.twice.reason === "search_running",
  JSON.stringify(opt.twice));
check("a running search reports its phase", opt.running.ok === true && !!opt.running.phase, JSON.stringify(opt.running));
check("a search stops when asked", opt.stop.ok === true && opt.stopped === true, JSON.stringify([opt.stop, opt.running, opt.after]).slice(0, 400));
check("after stopping, it reads as ranked or as nothing yet",
  (opt.after.ok === true && Array.isArray(opt.after.results)) || (opt.after.ok === false && opt.after.reason === "no_search"),
  JSON.stringify(opt.after).slice(0, 300));
if (opt.save) check("a ranked build saves as a preset", opt.save.ok === true && /^opt /.test(opt.save.preset), JSON.stringify(opt.save));
check("a rank that does not exist is refused", opt.badRank.ok === false && opt.badRank.reason === "no_such_result",
  JSON.stringify(opt.badRank));

// ---- queries -----------------------------------------------------------------
//
// A QUERY CHANGES NOTHING, which is what lets a consumer with no page in front
// of it have them: the observation is identical on both sides of every one.

const q = await evaluate(`(async () => {
  const out = {};
  await window.wfsim.do("shell.module.open", { module: "builder" });
  await window.wfsim.do("builder.mod.set", { slot: 0, mod: "serration" });
  const before = JSON.stringify(window.wfsim.observe());
  out.stats = await window.wfsim.do("builder.stats.read", {});
  out.weapons = await window.wfsim.do("builder.weapons.find", { query: "torid" });
  out.mods = await window.wfsim.do("builder.mods.find", { query: "serration", slot: 1 });
  out.exilusMods = await window.wfsim.do("builder.mods.find", { slot: "exilus", limit: 40 });
  out.arcanes = await window.wfsim.do("builder.arcanes.find", {});
  out.evos = await window.wfsim.do("builder.evolutions.list", {});
  out.modes = await window.wfsim.do("builder.modes.list", {});
  out.enemies = await window.wfsim.do("simulator.enemies.find", { query: allEnemies()[0].name, limit: 3 });
  out.same = JSON.stringify(window.wfsim.observe()) === before;
  out.queries = window.wfsim.actions.filter(a => a.query).map(a => a.id);
  out.notExilus = await window.wfsim.do("builder.mod.set", { slot: "exilus", mod: "serration" });
  await window.wfsim.do("builder.mods.clear", {});
  return out;
})()`, { awaitPromise: true });

const cites = (q.stats.forms || []).some(f => (f.parts || []).concat([f]).some(p =>
  (p.stats || []).some(s => (s.sources || []).some(x => /Serration/.test(x)))));
check("the stats panel reads back, naming the mod behind a change", q.stats.ok === true && cites,
  JSON.stringify(q.stats).slice(0, 300));
check("...and says what this weapon's model leaves out", Array.isArray(q.stats.not_modelled));
check("a weapon is found by name", q.weapons.ok === true && q.weapons.rows.some(r => r.id === "torid"),
  JSON.stringify(q.weapons).slice(0, 200));
check("a mod is found by name", q.mods.ok === true && q.mods.rows.some(r => r.id === "serration"),
  JSON.stringify(q.mods).slice(0, 200));
check("the exilus slot is offered only what it takes",
  q.exilusMods.ok === true && !q.exilusMods.rows.some(r => r.id === "serration"), JSON.stringify(q.exilusMods).slice(0, 200));
check("arcanes, evolutions and modes list", q.arcanes.ok && q.arcanes.found > 0 && q.evos.ok && q.modes.ok && q.modes.modes.length > 0);
check("a target is found by name", q.enemies.ok === true && q.enemies.found > 0, JSON.stringify(q.enemies).slice(0, 200));
check("no query changes the page", q.same === true);
check("queries are marked as such", q.queries.length >= 8, q.queries.join(","));
check("a mod the slot cannot take is refused", q.notExilus.ok === false && q.notExilus.reason === "not_equippable",
  JSON.stringify(q.notExilus));

// ---- refusals ----------------------------------------------------------------
//
// Each of these is a QUESTION WITH AN ANSWER, not a crash: an agent that gets
// an exception learns only that it failed, where a reason plus the alternatives
// is the next call it should make.

const no = await evaluate(`(async () => {
  const ask = async (id, args) => {
    try { return await window.wfsim.do(id, args); } catch (e) { return { threw: String(e) }; }
  };
  return {
    action: await ask("builder.mod.setx", { slot: 0, mod: "serration" }),
    slot: await ask("builder.mod.set", { slot: 99, mod: "serration" }),
    mod: await ask("builder.mod.set", { slot: 0, mod: "no_such_mod" }),
    weapon: await ask("builder.weapon.set", { weapon: "no_such_weapon" }),
    field: await ask("simulator.scenario.set", { patch: { nope: 1 } }),
    nested: await ask("simulator.scenario.set", { patch: { buffs: {} } }),
    runs: await ask("simulator.runs.set", { runs: 0 }),
    extra: await ask("builder.mods.clear", { hurry: true }),
    overRank: await ask("builder.mod.set", { slot: 0, mod: poolWithRivens()[0].id, rank: poolWithRivens()[0].max_rank + 1 }),
    arcane: await ask("builder.arcane.set", { seat: 0, arcane: "no_such_arcane" }),
    mode: await ask("builder.mode.set", { mode: "no_such_mode" }),
    valence: await ask("builder.valence.set", { bonus: 0.5 }),
    polarity: await ask("builder.polarity.set", { slot: 0, polarity: "Sparkly" }),
  };
})()`, { awaitPromise: true });

const refused = (r, reason) => r && r.ok === false && r.reason === reason && !r.threw;
check("an unknown action is refused with near ids",
  refused(no.action, "unknown_action") && no.action.alternatives.length > 0, JSON.stringify(no.action));
check("a slot that does not exist is refused", refused(no.slot, "bad_argument"), JSON.stringify(no.slot));
check("a mod that does not exist is refused", refused(no.mod, "unknown_mod"), JSON.stringify(no.mod));
check("a weapon that does not exist is refused", refused(no.weapon, "bad_argument"), JSON.stringify(no.weapon));
check("a fight field that does not exist is refused with the ones that do",
  refused(no.field, "unknown_scenario_field") && no.field.alternatives.length > 0, JSON.stringify(no.field));
check("a field with its own editor is refused", refused(no.nested, "not_a_scalar_field"), JSON.stringify(no.nested));
check("a run count out of range is refused rather than clamped",
  refused(no.runs, "out_of_range"), JSON.stringify(no.runs));
check("an argument nothing reads is refused", refused(no.extra, "unknown_argument"), JSON.stringify(no.extra));
check("a rank past the card's maximum is refused", refused(no.overRank, "out_of_range"), JSON.stringify(no.overRank));
check("an arcane this seat cannot take is refused with ones it can",
  refused(no.arcane, "not_equippable") && no.arcane.alternatives.length > 0, JSON.stringify(no.arcane));
check("a mode the weapon does not have is refused", refused(no.mode, "bad_argument"), JSON.stringify(no.mode));
check("valence on a weapon without one is refused", refused(no.valence, "no_valence"), JSON.stringify(no.valence));
check("a polarity that does not exist is refused", refused(no.polarity, "bad_argument"), JSON.stringify(no.polarity));

await finish("the door is one door");
