// A MELEE WEAPON HAS TWO SLOTS A GUN DOES NOT, AND ONE OF THEM DECIDES WHAT IT
// SWINGS.
//
// The STANCE slot is the first slot in this app whose contents change what the
// weapon FIRES rather than what it fires with: a stance publishes a combo per
// mode, and installing one replaces the weapon entry's own script. So the same
// Magistar in the same mode is a different sequence of swings under Crushing
// Ruin and under Shattering Storm, and a slot that drew correctly and sent
// nothing would look exactly like a working feature — which is
// `check_opt_modes`' own lesson, and why every assertion here is either ON THE
// WIRE or on a real `/api/simulate` in the shipping wasm build.
//
// IT NEEDS NO FIELD OF ITS OWN, and that is the claim worth testing hardest. A
// stance mod is legal in the stance slot and NOWHERE else, so a flat mod list
// can say which entry is the stance by looking at it — where an exilus-eligible
// mod is legal in a main slot too, which is why THAT one travels in a field of
// its own (AGENTS.md, 2026-08-25). The consequence is that the stance rides
// `mods`, and the round trip through `stateFromBuild` has to put it back in the
// slot it came out of rather than in slot 9.
//
// THE EXILUS SLOT IS HERE FOR THE OPPOSITE REASON: every one of its eleven
// melee cards is either Tennokai (a window this engine does not model) or
// blocking and movement (which this arena has neither of), so the slot draws,
// offers, equips and pays NOTHING — and each card says which of the two it is.
// A slot that silently paid something would be the worse bug.
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
    silent: pool
      .filter((m) => !m.not_modeled && !m.out_of_scope
        && !(m.unmodeled_effects || []).length)
      .map((m) => m.id),
  };
})()`);
check(`${tag} the melee exilus slot offers its whole pool`,
  exilus.n >= 8, JSON.stringify(exilus.n));
check(`${tag} ...and not one of them pays without saying why`,
  (exilus.silent || []).length === 0, JSON.stringify(exilus.silent));

await finish("a melee weapon's two extra slots");
