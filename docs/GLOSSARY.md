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

## Fire rate and ammo

- **Fire rate mod bonus** — an **additive** fire-rate bonus from ordinary mods
  (e.g. Gunslinger, Anemic Agility). All such bonuses share one bucket:
  `base × (1 + Σ fire_rate_mod_bonuses)`.
- **Fire rate multiplier** — an **independent multiplicative** factor applied on
  its own bucket, *not* added into the fire-rate-mod bucket. Example: Dual
  Toxocyst's **Frenzy** "+150%" is actually **×2.5** applied independently.
  Multiple such multipliers multiply together.
  ```
  effective_fire_rate = base × (1 + Σ fire_rate_mod_bonuses) × Π fire_rate_multipliers
  ```
  > ⚠️ "+150% fire rate" from Frenzy ≠ "+150%" from a fire-rate mod. The mod goes
  > in the additive bucket; Frenzy is its own ×2.5 multiplier. Always name which.
- **Ammo efficiency** — reduces how often a shot consumes ammo. Bonus `e`:
  ```
  shots_per_ammo = 1 / (1 - e)
  ```
  Sources stack **additively** with each other, **except Energized Munitions**
  which stacks multiplicatively. `e = 1.0` (e.g. Frenzy's +100%) → infinite ammo.

## Perks and buffs

The grantor and the granted are different things — keep them distinct:

- **Perk** — a **held/equipped capability** (arcane, weapon/Warframe **passive**,
  Incarnon evolution) whose possession lets you *trigger* a buff. Holding the
  perk is what enables the buff. **All passives are called perks internally** —
  we do not use the word "passive" as a separate concept. E.g. Secondary Enervate
  and Dual Toxocyst's Frenzy are perks.
- **Buff** — the **runtime overlay you gain** when a perk's trigger fires. It has
  stacks, an optional duration, a scope, and the contributions it grants. It is
  what the **buff bar** shows.
  > A perk and the buff it grants **often share a name** (the *Frenzy perk*
  > grants the *Frenzy buff*). Warframe reuses names heavily; always say which —
  > "perk" for the grantor, "buff" for the granted state.
- **Buff bar** — the single container of all active buffs, mirroring the player's
  HUD. Every buff appears here regardless of scope. We can display **more, and
  more finely, than the in-game HUD** (exact scope, contributions, verification
  status).
- **Buff scope** — what a buff applies to: **Weapon** (the granting weapon),
  **Warframe** (the player), **Companion**, **Companion weapon**, or **Squad** —
  extensible. The HUD shows a buff regardless of scope, so "shown in the UI" ≠
  "applies to this weapon"; we track the precise target.
- **Stack** — one unit of an accumulating buff. "7 stacks of Secondary Enervate"
  = the buff has accumulated 7 units.
- **Trigger** — the event condition that fires a perk (OnHit, OnHeadshot, OnKill,
  OnBigCrit, ...).
- **Rate cap** — a maximum trigger frequency, in triggers/second (e.g. Secondary
  Enervate caps stack gain at 30/s). Expressed in seconds internally so it is
  independent of the simulation tick rate (`fps`).

## Modifier buckets and mod order

- **Bucket** — a group of bonuses that add together before multiplying against
  other buckets (see `docs/MECHANICS.md` §2). "Flat crit chance" and "crit
  chance multiplier" are separate buckets; likewise "fire rate mod bonus" vs
  "fire rate multiplier".
- **Mod order** — the sequence of mods in the configuration is **significant**,
  not just a set. Elemental combination depends on it (see `MECHANICS.md` §3):
  the same two element mods in a different order can produce different combined
  elements. The model must therefore carry an explicit, ordered mod list.
- **Injected mod** — an Effect can inject a modifier into the mod list as if it
  were a mod, **at a defined position**. Example: Dual Toxocyst's Frenzy adds
  "+100% Toxin" that behaves like a Toxin mod **appended at the end of the mod
  order**, so it combines elementally as the last mod.
