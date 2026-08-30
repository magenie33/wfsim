// A MELEE WEAPON HAS TWO SLOTS A GUN DOES NOT, AND ONE OF THEM DECIDES WHAT IT
// SWINGS.
//
// The STANCE slot is the first slot in this app whose contents change what the
// weapon FIRES rather than what it fires with: a stance publishes a combo per
// mode and installing one replaces the entry's own script, so the same Magistar
// in the same mode is a different sequence of swings under Crushing Ruin and
// under Shattering Storm. A slot that drew correctly and sent nothing would
// look exactly like a working feature, which is why every assertion here is ON
// THE WIRE or on a real `/api/simulate` in the shipping wasm build.
//
// IT NEEDS NO FIELD OF ITS OWN, the claim worth testing hardest: a stance mod
// is legal in the stance slot and NOWHERE else, so a flat mod list can say
// which entry is the stance by looking at it, where an exilus-eligible mod is
// legal in a main slot too. The stance therefore rides `mods`, and the round
// trip through `stateFromBuild` must put it back in the stance slot.
//
// THE EXILUS SLOT IS HERE FOR THE OTHER REASON: its whole pool is a single
// mechanic plus four cards this arena has no room for. Seven of the eleven PAY
// and four declare, so the assertion is not "nothing pays" but the invariant
// underneath it — NO CARD IS SILENT: each either has an effect the engine
// computes or says on its own card why it has none.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 22000 });
const { evaluate, check, finish } = app;
const tag = "melee slots";

await app.load("/weapons/Magistar", 25000);

// ---------------------------------------------------------------------------
// 1. THE SLOT IS THERE, AND IT IS MELEE'S ALONE.
const slot = await evaluate(`(() => ({
  drawn: !!document.querySelector('#stance-block')
    && getComputedStyle(document.querySelector('#stance-block')).display !== 'none',
  slots: slots.length,
  pool: weaponAxes().stance.map((m) => m.id),
  // THE FILTER RUNS BOTH WAYS, which is the half that could silently not: a
  // stance offered in a main slot is a build nobody can hold.
  mainOffersAStance: (META.mods || [])
    .filter((m) => m.stance)
    .some((m) => (weaponInfo($("weapon").value).mods || []).includes(m.id)
      && !weaponAxes().stance.some((s) => s.id === m.id)),
}))()`);

check(`${tag} a melee weapon has a stance slot`, slot.drawn === true, JSON.stringify(slot));
check(`${tag} ...with its own class's stances in it`,
  (slot.pool || []).length >= 2, JSON.stringify(slot.pool));
check(`${tag} ...as a tenth slot beside the eight and the exilus`,
  slot.slots === 10, String(slot.slots));

// ---------------------------------------------------------------------------
// 2. THE NEGATIVE CONTROL. A gun shows nothing — not an empty slot, which
//    reads as "something is missing here" (the rule the exilus block already
//    follows for sentinels).
await app.load("/weapons/Laetum", 22000);
const gun = await evaluate(`(() => ({
  drawn: !!document.querySelector('#stance-block')
    && getComputedStyle(document.querySelector('#stance-block')).display !== 'none',
  pool: weaponAxes().stance.length,
}))()`);
check(`${tag} a gun shows no stance slot at all`,
  gun.drawn === false && gun.pool === 0, JSON.stringify(gun));

// ---------------------------------------------------------------------------
// 3. IT REACHES THE FIGHT, and two stances are two fights.
await app.load("/weapons/Magistar", 22000);
const fight = await evaluate(`(async () => {
  const run = async (stanceId) => {
    slots = slots.map((s, i) => (i === STANCE ? { ...s, mod: stanceId } : s));
    const body = { ...theFight(), ...buildPayload(), runs: 12, seed: 4 };
    const r = await api("/api/simulate", body);
    return { dps: r && r.ok !== false ? Math.round(r.dps) : -1, sent: (body.mods || []).slice() };
  };
  const none = await run(null);
  const ruin = await run("crushing_ruin");
  const storm = await run("shattering_storm");
  return { none: none.dps, ruin: ruin.dps, storm: storm.dps, sent: storm.sent };
})()`);

check(`${tag} the stance travels in the request`,
  (fight.sent || []).includes("shattering_storm"), JSON.stringify(fight.sent));
check(`${tag} ...and two stances are two different fights`,
  fight.ruin > 0 && fight.storm > 0 && fight.ruin !== fight.storm, JSON.stringify(fight));
// THE ENTRY'S OWN SCRIPT IS THE UNSTANCED FALLBACK, and it happens to be
// Crushing Ruin's — which is what the Magistar's four combo entries were
// transcribed from. Asserted so that a stance failing to APPLY cannot read as a
// pass: if the swap did nothing, `storm` would equal `none` too.
check(`${tag} ...while an empty slot falls back to the entry's own script`,
  fight.none === fight.ruin && fight.none !== fight.storm, JSON.stringify(fight));

// ---------------------------------------------------------------------------
// 4. THE ROUND TRIP. `buildPayload` -> `stateFromBuild` has to put the stance
//    back in the STANCE slot, not in slot 9 — and that is the one thing a flat
//    mod list makes easy to get wrong.
const trip = await evaluate(`(() => {
  slots = slots.map((s, i) => (i === STANCE ? { ...s, mod: "shattering_storm" } : s));
  // Eight main mods beside it, so a stance landing in a main slot would be
  // visible as a ninth.
  const pool = weaponAxes().mods.filter((m) => !m.stance && !m.exilus).slice(0, 8);
  slots = slots.map((s, i) => (i < 8 ? { ...s, mod: pool[i] ? pool[i].id : null } : s));
  const p = buildPayload();
  const back = stateFromBuild(p, $("weapon").value);
  return {
    sentCount: (p.mods || []).length,
    stanceBackInItsSlot: back.slots[STANCE].mod === "shattering_storm",
    mainCount: back.slots.slice(0, 8).filter((s) => s.mod).length,
    noStanceInMain: !back.slots.slice(0, 8).some((s) => s.mod === "shattering_storm"),
  };
})()`);
check(`${tag} a stance survives the round trip into its own slot`,
  trip.stanceBackInItsSlot === true && trip.noStanceInMain === true, JSON.stringify(trip));
check(`${tag} ...without spending one of the eight`,
  trip.mainCount === 8 && trip.sentCount === 9, JSON.stringify(trip));

// ---------------------------------------------------------------------------
// 5. THE EXILUS SLOT, which for melee is eleven cards that pay nothing — and
//    every one of them SAYS which of the two reasons it is.
const exilus = await evaluate(`(() => {
  const pool = weaponAxes().exilus;
  return {
    n: pool.length,
    // not_modeled is META's own spelling of the yaml's unmodeled flag —
    // asked by the NAME the wire uses, not by the one the data file uses.
    // A card is SILENT when it neither computes anything nor admits anything.
    silent: pool
      .filter((m) => !(m.effects || []).length
        && !m.not_modeled && !m.out_of_scope
        && !(m.unmodeled_effects || []).length)
      .map((m) => m.id),
    // …and how the eleven actually split, so a change in the ratio is visible
    // in the output rather than only in a pass or a fail.
    paying: pool.filter((m) => (m.effects || []).length).length,
  };
})()`);
check(`${tag} the melee exilus slot offers its whole pool`,
  exilus.n >= 8, JSON.stringify(exilus.n));
check(`${tag} ...and no card in it is silent`,
  (exilus.silent || []).length === 0, JSON.stringify(exilus.silent));
// TENNOKAI IS WHAT MAKES THE SLOT A DECISION, so the count that pays is
// asserted rather than left to the eye: at zero this slot is eleven cards a
// player would never open the picker for.
check(`${tag} ...and Tennokai gives it something to decide`,
  exilus.paying >= 5, JSON.stringify(exilus.paying));

await finish("a melee weapon's two extra slots");
