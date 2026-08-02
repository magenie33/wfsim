# wfsim — UI Vision

What sets wfsim apart from predecessor calculators (Overframe-style form
pages): besides a build/config UI, there is a **live 2D top-down view of
the fight**.

## Kill score is reported as a RATE (KPM)

The kill score — whole kills plus the fraction of the current target's pool
already drained — grows with the engagement, so two runs of different length
could not be compared at a glance. The headline is now **KPM**, score per
minute, and the score itself sits beside it as the engagement total. That is
the same shape the damage numbers already had: a rate to compare with, a total
to read.

Simulator: `1.20 KPM · 2.40 kill score in 120s · …`
Optimizer row: `#1 · 1.20 KPM · 552,523 DPS · 2.40 kill score / 120s`

Presentation only — nothing was rescaled underneath. The optimizer still ranks
on the score, and at a fixed duration KPM is a monotone transform of it, so no
ordering moves. KPM is only as duration-invariant as DPS is: measured on one
Torid build, 30 s vs 120 s gave 0.044 vs 0.049 KPM while the totals went 0.022
vs 0.098 — the residual is ramp-up, reloads and the DoT tail, exactly the
drift DPS shows over the same pair (18,653 vs 20,551).

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

## Engine mapping

| UI concept | engine |
|---|---|
| the fight, both actors | `arena::Arena` (a `Tenno`, a target with its hitboxes, a duration) |
| the player | `tenno_data::Tenno` — stats, and a `state` every conditional mod is asked about |
| target that never wastes DPS | `dummy::TargetMode::InstantRespawn` |
| aim quality / headshot feel | `dummy::BodyPart::aim_weight` |
| plane, positions, ranges | **nothing yet** — see below |

**The Arena VIEW has no engine behind it.** There was an `engine::world`
(`Vec2`, `Circle`, an `Engagement` of shooter-vs-target-circle with a hard
range cutoff) written alongside these decisions in 2026-07-24. It was deleted
on 2026-08-02 with **zero callers**, having never been wired to anything: the
sim fights one target and assumes it is in range, so a plane had nothing to
decide. Two modules named after the same thing, one of them dead, is worse than
one honest gap — and the decisions above are the part worth keeping, which is
why they live here and not in code.

When positions become real they belong ON `arena::Arena`, beside the actors
that would have them, not in a parallel module.

## Replay (2026-08-02)

The Simulator's result carries the MEDIAN engagement, frame by frame: target
pools, cumulative damage, kills, and **live stacks per buff**. One row per
buff, each a short curve, all open by default — the question they answer is
"was this thing actually up", and a row you have to click to answer it will not
be clicked. `avg` and `uptime` sit in the header so the group reads at a
glance; play/pause + 1x/2x/5x/20x + a scrubber move one cursor across every
curve at once.

It is the same fight the headline number came from, not a fresh run and not an
average. `Rng` is SplitMix64 with a single `u64` of state, so a run records
what it started from (`RunResult::rng_state`) and `dummy::replay` re-runs that
one bit-for-bit. Cost: ONE extra engagement, and only when asked — the
marginal-gain scan calls the same endpoint once per candidate and shows no
replay, so `replay: true` is opt-in and only the Simulator's Run sends it.

Why it earns its space: it turns arguments into pictures. "Is Primary Frostbite
pinned at 40 stacks or decaying?" was a paragraph of reasoning; it is now a
curve that climbs 0 → 40 over sixty seconds and answers itself.

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
