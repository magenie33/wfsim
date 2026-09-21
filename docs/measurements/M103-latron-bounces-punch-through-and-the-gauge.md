# M103 — Latron: the Incarnon bounce is three reflections and usually lands on nothing, the Incarnon form takes no punch through and the base form does, and a round through two weak points charges the gauge twice (owner, 2026-09-21)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Latron Prime, both forms, Simulacrum.** Three questions, all of them about
where a shot GOES rather than about how big a number is.

### The bounce

| what was read | reading |
| --- | --- |
| bounces before the projectile ends | **three**; the FOURTH contact detonates it and it is gone |
| a flight that finds nothing | it detonates on its own past some distance |
| where a bounce goes | a **reflection** off what it touched, and DETERMINISTIC — the same shot from the same spot takes the same path every time, like a bouncing ball |
| a bounce off a body the collision KILLED with a weak-point hit | almost straight UP, landing on nothing |

The last row is the one that decides what to model: killing with a headshot is
what this weapon is FOR, and the projectile leaves a corpse that is no longer
standing where it was — so in ordinary play most bounces hit nobody. The
wiki's *"exploding up to 6 times"* is a ceiling reached by a shot that kills
nothing.

### Punch through

| form | a punch-through mod |
| --- | --- |
| base | **pierces** |
| Incarnon | **pierces nobody**, as the page states |

### The gauge

One round through two enemies, **both entered at a weak point, charges the
gauge twice**. A weak-point hit is a LANDING, not a trigger pull.

### What it settles

1. **The bounces are worth nothing this arena can state.** Three reflections
   off surfaces a 2D plane does not have, most of them leaving the fight
   entirely, is not a number to add up — and nearest-body seeking, which is
   what this engine's `ricochet:` walks, is the wrong mechanic for a
   reflection: it keeps the projectile inside the crowd, which is the one
   thing the reading says it does not do.
2. **Punch through is a property of the FORM.** Both halves were already in
   the data; one list of bodies per ENGAGEMENT was not — it was built from the
   transformed form's reach and answered the rebuild phase with it, so a
   cycle's base form pierced nobody whatever its mods said.
3. **Everything a weak point fires, a punched weak point fires too.** The
   round enters the same part of each body, so one shot through two heads is
   two weak-point hits: two gauge charges, two Death Knell stacks, two
   Lethal Rearmament stacks, two Primary Crux stacks, two entries in
   Lingering Judgement's streak, two Exact Penance rolls.

### What is implemented

The three `latron*_incarnon` entries declare no `ricochet:` and carry the
bounce as an admission instead; `punch_through_mods: false` was already there
and is now measured. `FightParams::struck_bodies` is computed per FORM
(`fight::open`), and `spread::PunchedWeakPoints` carries the punched weak
points back to the aimed path, which fires each trigger once per landing and
counts them in `RunResult::headshots_on_others` for the gauge.

Pinned by the `m103_*` tests in
`engine/src/fight/tests/m103_latron_punch_through.rs`.

### Still open

The reflection itself. A deterministic angle-of-incidence bounce needs
surfaces, which is a 2D-arena decision and not a weapon one — recorded here
rather than modelled, and the weapons say so on the page.
