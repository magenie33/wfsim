# wfsim — Core Design

> A hardcore Warframe damage simulator. The goal is not "compute a DPS number"
> but to **physically reproduce a real fight**, so that the damage this tool
> computes matches in-game measurements — and, on top of that, to solve for
> the optimal mod build.

---

## 1. Vision (What & Why)

**One sentence:** given "weapon + mods + target + combat scenario", output
damage and kill performance that matches in-game measurements **item by
item**, and search backwards for the optimal mod combination.

How it differs from the usual Warframe DPS calculators (Overframe, various
spreadsheets):

| Typical DPS calculator | wfsim |
|---|---|
| Computes a steady-state DPS number | Simulates the **per-shot / per-hit** damage sequence over time |
| Ignores range, ballistics, accuracy | Treats range / ballistics / hit resolution / AoE as first-class |
| Simplifies elemental combination | Follows the game's elemental combination order and rules exactly |
| Abstracts the target into one health bar | Models armor / shields / health / unit type / mitigation curves |
| Outputs "theoretical" values | Outputs values that **align with measurements** (verifiable) |

**Definition of Done for correctness:** for a set of known weapon + mod
combinations, wfsim's per-shot damage, critical damage, status-proc damage,
and actual damage against specific (armored) enemies must match in-game
Simulacrum measurements **within rounding error**. This is the project's
north-star metric.

---

## 2. Guiding Principles

1. **Correctness > performance > convenience.** First match the game, then be
   fast, then be nice to use. Every simplification must be explicitly marked
   and traceable.
2. **Simulation, not formulas.** A fight is a process that advances through
   time (fire cadence, combo buildup, status stacking, magazine/reload,
   target movement). Steady-state DPS is just one derived statistic of the
   simulation — never the core of the computation.
3. **Data and engine are separate.** Weapon/mod/enemy numbers are **data**
   (importable from game data sources, versioned); damage rules are the
   **engine** (pure functions, testable). When the game patches, swap the
   data — don't touch the engine.
4. **Every rule is sourced.** Elemental combination order, armor mitigation
   formula, crit tiers, status weighting… every formula in the code carries a
   comment pointing at its source (wiki / datamining / in-game measurement)
   so it can be verified and corrected.
5. **Verifiable.** Every layer of the engine gets test cases calibrated
   against in-game measurements (golden tests). Without a measurement to
   compare against, it doesn't count as "correct".

---

## 3. The Damage Pipeline

This is the heart of the project. Damage is not one multiplication — it is a
pipeline with a **strict order**. Get the order wrong and the result is wrong.
Layered draft below (each layer is an independent, testable pure function):

```
Weapon base stats
    │  base damage split across IPS/elements, base crit chance/multiplier,
    │  base status chance, fire rate, magazine, reload, multishot...
    ▼
[1] Mod resolution
    │  collect every active bonus, classified correctly into
    │  "additive group vs multiplicative group"
    │  (+Damage of the same kind sums, then multiplies with the other
    │  buckets; base damage, elements, crit, status, multishot are each
    │  their own bucket)
    ▼
[2] Elemental combination
    │  merge base elements in the mods' **configured order** into combined
    │  elements (Cold+Electricity=Magnetic, Heat+Toxin=Gas, ...);
    │  innate weapon elements and mod elements follow separate merge rules
    │  (innate elements usually combine first)
    ▼
[3] Per-hit damage vector
    │  one shot's damage components across all damage types
    │  {Puncture, Impact, Slash, Heat, Cold, Electricity, Toxin, combined...}
    ▼
[4] Critical tiers
    │  crit chance can exceed 100% → tiered crits (tier 1/2/3...),
    │  probability-weighted; combo's effect on crit (melee)
    ▼
[5] Status / procs
    │  per-shot proc chance (distribution under multishot), per-element
    │  proc weighting, DoT (Heat/Toxin/Slash/Gas...) over time, stack
    │  caps, durations
    ▼
[6] Hit resolution   ← the "hardcore" differentiator
    │  multishot projectile count, range/falloff, ballistics/arc, accuracy,
    │  AoE radius and falloff, headshot multipliers, punch-through
    ▼
[7] Target mitigation
    │  armor mitigation (armor → damage-reduction curve), shields vs
    │  health, per-faction/unit-type resistances and weaknesses,
    │  armor/shield stripping
    ▼
[8] Temporal integration
    │  advance along the time axis: fire cadence, magazine
    │  depletion/reload, combo buildup/decay, DoT stacking, buff duration
    │  and refresh → the damage-over-time series
    ▼
Outputs
    per-shot / burst / sustained DPS, TTK against a given enemy, damage
    curve across effective range, crit/status breakdown, bottleneck
    analysis
```

> ⚠️ **Known high-risk traps** (the places most likely to disagree with the
> game — write tests for these first):
> - Elemental combination **order** depends on mod arrangement in the config;
>   when innate elements merge vs mod elements.
> - Bucket classification: which bonuses stack additively vs multiply
>   independently. Misclassify and results diverge badly.
> - The armor mitigation formula (and how Corrosive / armor strip affect it
>   dynamically).
> - Tiered crits (crit chance > 100%) interacting with combo.
> - Status weighting: with multiple elements present, per-element proc
>   weights are not an even split.
> - How multishot distributes "per-shot status chance" and crit rolls.
> - AoE edge cases: self-damage/falloff, whether headshots crit, etc.

*(The order above is a design draft; each layer must be calibrated and
corrected against in-game measurements during implementation.)*

---

## 4. Architecture & Modules

```
wfsim/
├── docs/                 # design docs (this file lives here)
├── data/                 # data layer: weapons, mods, enemies, faction
│   ├── weapons/          #   resistance tables (versioned)
│   ├── mods/
│   ├── enemies/
│   └── factions/
├── engine/               # engine layer: pure functions, one per pipeline
│   ├── modResolution     #   layer
│   ├── elements
│   ├── crit
│   ├── status
│   ├── hit               # range / ballistics / multishot / AoE
│   ├── mitigation        # armor / shields / resistances
│   └── simulate          # temporal-integration main loop
├── optimizer/            # inverse search for the best mod combination
├── tests/
│   └── golden/           # golden tests vs in-game measurements (north star)
└── cli/                  # the wfsim command-line entry point
```

**Data model (core entities, to be refined):**
- `Weapon` — base damage vector, base crit/status, fire rate, magazine,
  reload, multishot, ballistic properties, AoE properties, mod slots /
  polarities.
- `Mod` — affected bucket/field, value, polarity, conditions (combo /
  on-kill / faction-limited, etc.).
- `Enemy` — health/shields/armor, unit type, faction, level (for level
  scaling), weaknesses/resistances.
- `Scenario` — distance, headshots or not, combo state, buffs, engagement
  duration — the combat context.
- `Build` — weapon + chosen mod set (the optimizer's search unit).

---

## 5. Optimizer (best-build search)

- **Switchable objective:** steady-state DPS / TTK against a given enemy /
  total damage within effective range / burst damage, etc.
- **Constraints:** mod slot count, polarity/capacity, mod exclusivity,
  arcanes, etc.
- **Method (coarse first, then fast):** exhaustive search + pruning first to
  establish a correct baseline, heuristics later (greedy / genetic / beam
  search) for speed.
- **Principle:** the optimizer only ever calls the engine — it never gets its
  own simplified damage formula, or the "optimum" it finds is fake.
- **Search strategy:** deduplicate by "canonical form" (position-sensitive
  mods = element order first, everything else unordered; polarity layout is
  not part of a build's identity); candidates pass "best-effort legalization"
  (innate polarity reassignment → greedy Forma → reject if it can't fit);
  conditional buffs default to full stacks (configurable / fully simulated in
  the future). Details in [`OPTIMIZER.md`](OPTIMIZER.md).

---

## 6. Technology Choices (Decided)

- **Language/runtime:** **Rust** (2021 edition), version pinned via **mise**
  (currently `1.97.1`). Chosen because the optimizer = "a huge mod
  combination space × several Monte-Carlo simulation runs per candidate" — a
  CPU-bound search×simulation double loop that Rust is uniquely suited to
  among the candidates considered; strong typing and numeric reliability also
  fit "correctness first". A future web UI can reuse the engine compiled to
  WASM.
- **Project shape:** a Cargo workspace with three crates: `engine/` (pure
  pipeline functions), `optimizer/` (only calls the engine), `cli/` (the
  `wfsim` entry point).
- **Optimizer evaluation:** **hybrid** — analytic expectation (fast, for
  coarse filtering during search) + Monte Carlo (slow, for final calibration
  and distributions, SimCraft-style).
- **Code/docs language:** English (public repository).
- **Data sourcing:** bulk entry from the wiki's structured Lua modules,
  normalized into versioned YAML — see [`DATA_SOURCES.md`](DATA_SOURCES.md).
- **Accuracy verification:** systematic Simulacrum measurement protocol and
  the golden-test baseline — see [`MEASUREMENTS.md`](MEASUREMENTS.md).

---

## 7. Roadmap

- **M0 — skeleton:** pick language/stack, set up repo structure, define the
  core data-model schema.
- **M1 — single shot correct:** implement pipeline [1]–[5]
  (mods/elements/crit/status); **per-shot damage** matches measurements on an
  unarmored target.
- **M2 — target mitigation:** add [7] armor/shields/resistances; actual
  damage against **armored enemies** matches measurements.
- **M3 — hit resolution:** add [6] range/multishot/ballistics/AoE/headshots;
  support "damage as a function of distance".
- **M4 — temporal simulation:** add [8] temporal integration
  (reload/combo/DoT/buffs); output TTK and the damage-time series.
- **M5 — optimizer:** best-build search on top of a fully correct engine.
- **Throughout:** every milestone must ship golden tests against in-game
  measurements, or it doesn't count as done.

