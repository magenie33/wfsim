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
