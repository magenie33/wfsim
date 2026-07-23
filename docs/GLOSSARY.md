# wfsim — Glossary (Internal Terminology)

Official/wiki wording is inconsistent and often imprecise (e.g. "+crit chance"
can mean two mathematically different things). This file defines **one precise
vocabulary** that the whole project uses — in code identifiers, comments, data
field names, and docs. When a source term is ambiguous, we map it to a precise
term here and use ours everywhere.

Rule: if you introduce a new game concept, define it here first, then use that
exact term in code. Prefer these terms over the in-game/wiki phrasing.

---

## Firing and hits

- **Shot** — one activation of the weapon's trigger: the rounds produced by a
  single pull, *before* Multishot expansion.
- **Multishot instance** — one of the projectiles/pellets a Shot expands into
  due to Multishot. A Shot with 3.0 Multishot yields ~3 Multishot instances.
- **Hit** — a single damage instance registered on a target that can trigger
  on-hit effects. **A Hit is not the same as a Multishot instance or an enemy
  touched.** How many Hits a Shot produces is weapon-archetype dependent
  (source: wiki *Secondary Enervate* notes):
  - **Hitscan** hitting multiple enemies at once (via Multishot or Punch
    Through) → counts as **1 Hit**.
  - **Projectile** / **non-chained Beam** hitting multiple enemies at once →
    counts as **multiple Hits** (one per enemy).
  - **AoE** explosion → always **1 Hit**.
  - **Shotgun sidearm** pellets are tied to Multishot → **not** separate Hits.
  - Expiring **Blast** proc damage → counts as a Hit; a Blast explosion from
    reaching max stacks or killing the target → does **not** count as a Hit.

  These rules govern how many `Hit` events the timeline emits per Shot; they are
  a hit-resolution ([`engine`] layer [6]) concern.

## Critical hits

- **Crit tier** — the integer multiplier count applied on a critical hit. In-game
  colors map to tiers: **tier 0** = non-crit (white), **tier 1** = crit
  (yellow), **tier 2** = orange, **tier 3+** = red. Tiers above 1 appear when
  effective critical chance exceeds 100%.
- **Big crit** — a critical hit of **tier ≥ 2** (orange or red). Term used by
  effects that treat these specially (e.g. Secondary Enervate's reset). Source:
  wiki (confirmed definition; damage still to be measured).
- **Flat crit chance** *(a.k.a. additive / absolute crit chance)* — a bonus
  expressed in **absolute percentage points** added to the final critical
  chance, **not** scaled by the weapon's base. Example: Secondary Enervate gives
  **+10 flat crit chance** per stack. A Lato (10% base) with 7 stacks has
  `10% + 7×10% = 80%`.
- **Crit chance multiplier** — a bonus that **scales the weapon's base** critical
  chance. Example: Point Strike "+150%" → `base × (1 + 1.5)`. This is a
  different bucket from flat crit chance.
- **Effective crit chance** *(draft formula — order to confirm by golden test)*:
  ```
  effective_cc = base_cc × (1 + Σ crit_chance_multipliers) + Σ flat_crit_chance
  ```
  > ⚠️ The two crit-chance buckets combine differently; conflating them (as the
  > word "+crit chance" does) produces wrong numbers. Always say **flat crit
  > chance** vs **crit chance multiplier**.

## Effects and stacking

- **Effect** — a stateful modifier (arcane, conditional mod, combo) that lives
  on the timeline and reacts to events. See `docs/EFFECTS.md`.
- **Stack** — one unit of an accumulating Effect. "7 stacks of Secondary
  Enervate" = the Effect has accumulated 7 units.
- **Trigger** — the event condition that advances an Effect (OnHit, OnKill,
  OnBigCrit, ...).
- **Rate cap** — a maximum trigger frequency, in triggers/second (e.g. Secondary
  Enervate caps stack gain at 30/s). Expressed in seconds internally so it is
  independent of the simulation tick rate (`fps`).

## Modifier buckets

- **Bucket** — a group of bonuses that add together before multiplying against
  other buckets (see `docs/MECHANICS.md` §2). "Flat crit chance" and "crit
  chance multiplier" are separate buckets.
