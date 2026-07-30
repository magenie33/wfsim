# wfsim — UI Vision

What sets wfsim apart from predecessor calculators (Overframe-style form
pages): besides a build/config UI, there is a **live 2D top-down view of
the fight**.

## Core decisions

- **Two surfaces**:
  1. **Config UI** — build/weapon/enemy/scenario setup. Deliberately simple;
     can be much sparser than predecessor tools.
  2. **Arena view** — a 2D top-down rendering of the simulated fight, so
     real environments and **AoE / multi-target damage** can be tested
     spatially instead of as scalar "assume N enemies in radius" checkboxes.
- **Geometry**: every actor (Warframe, enemy) is a **circle, radius 0.25 m**
  (assumption; refine later). The world is a plane — **the Z axis is dropped**
  for now.
- **"Feel" is probability**: aim wobble, headshot ratio, reaction time are
  modeled as probabilities (e.g. body-part aim weights), not simulated motor
  control.
- **No wasted DPS while measuring**: the standard measuring scenario is one
  Warframe vs one target circle with `TargetMode::InstantRespawn` — the
  target respawns in place the instant it dies (no on-death transformations).

## Engine mapping (already in place)

| UI concept | engine |
|---|---|
| plane, positions, ranges | `world::Vec2` |
| actor footprint | `world::Circle` (`Circle::actor`, `ACTOR_RADIUS_M`) |
| AoE primitive | `world::Circle::intersects` (blast circle vs footprint) |
| one-vs-one scenario | `world::Engagement` (shooter, target circle, weapon range, combat params) |
| target that never wastes DPS | `dummy::TargetMode::InstantRespawn` |
| aim quality / headshot feel | `dummy::BodyPart::aim_weight` |

## Planned

- **Surface each attack part's CO anomalies in the builder panel** (user,
  2026-07-30). Condition Overload is full of per-entry quirks that no rule
  predicts — the CO catalog lists them one attack at a time, and weapon families
  split down the middle (Lato Vandal has a row, Lato Prime does not; Zylok Prime
  is docked to 94%, the plain Zylok is not). MECHANICS §6 has the evidence.
  The panel already renders per-part rows, so each part should state its own CO
  standing: the behaviour class, whether that part receives CO at all (an AoE
  part normally does not — the Torid's cloud is an exception), and the base
  fraction with what dilutes it. Today a build can silently differ from another
  by a factor the panel never mentions.

## Not decided yet

- Rendering cadence vs simulation tick (fixed 240 fps sim clock exists in
  `sim::SimConfig`).
- How movement paths (target walking, player strafing) are authored in the
  arena view.
