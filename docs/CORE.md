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
   **engine** (pure functions, testable). Data-only patches update data; a
   native code change requires rebinding its affected consumers and kernels.
4. **Every rule is sourced.** Elemental combination order, armor mitigation
   formula, crit tiers, status weighting… every formula in the code carries a
   comment pointing at its source (wiki / datamining / in-game measurement)
   so it can be verified and corrected. On a conflict, in-game measurement
   outranks the matching client's package/code, then the wiki, then DE's export.
5. **Verifiable.** Every layer of the engine gets test cases calibrated
   against in-game measurements (golden tests). Without a measurement to
   compare against, it doesn't count as "correct".
6. **Native calculation is the update path.** Build prepares the native
   inputs and queries; simulate executes the packet pipeline; solve calls
   and replays that same execution. Preserve recovered field/id names,
   operation order, scalar widths, integer/RNG semantics and materialization boundaries.
   Keep frozen wire names through adapters. Unbound ownership, eligibility,
   scheduling and scene inputs remain explicit in `NATIVE_BINDINGS.md` and
   `UNMODELLED.md`. Updates extend this shared path. Historical engines remain
   immutable for comparisons and work already bound to their versions; their
   outputs do not override source evidence or create another current formula path.

---

## 3. The Damage Pipeline

Damage follows the operation and event order of each bound native route.
The following map groups responsibilities; it is not the native call graph
and must not reorder real queries, packet writes or callbacks. The binding
ledger and `MECHANICS.md` own each route's order and precision boundaries.

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

The three verbs are three layers of one engine, and every module sits in one
layer. **A module names its own layer and the ones below it, never one above,
and no two modules reach each other** — `scripts/check_engine_layers.mjs`
fails the first edge that does, because the compiler accepts a cycle inside a
crate and nothing else would notice one.

```
┌─ transports ─ web/ (native server) · wasm/ · cli/ · desktop/ ──────────────┐
│  webapi/      the endpoints: request JSON → a fight / a panel / a search   │
├─ optimizer/   SOLVE: enumerate, evaluate by calling the fight, schedule ───┤
├─ engine/src/                                                               │
│   board/      what makes two builds one row; the rulers rows are scored by │
│   fight/      SIMULATE: the run, shot by shot — with target, formation,    │
│   target arena formation record      arena (its inputs) and record (out)   │
│   build/      BUILD: a loadout resolved into the panel a fight reads       │
│   data/       the catalogs: data/*.yaml read into the model                │
│   model/      the vocabulary: what a card, a buff, a weapon IS             │
│   rules/      primitives and the game's formulas, pure functions           │
└────────────────────────────────────────────────────────────────────────────┘
```

- **`rules/`** holds the stateless pipeline layers of §3 — `damage`,
  `elements`, `status`, `scaling`, `capacity`, `space`, `chain` — and the buff
  machinery the fight reads (`buffs`, `perks`, `sim`). It knows no weapon.
- **`model/`** is the vocabulary every catalog writes in. `ModEffect`,
  `EvoEffect` and `ArcEffect` are what a mod, an evolution and an arcane card
  say; `BuffTrigger`, `BuffGrant`, `TennoCondition`, `IndirectStat` and their
  `from_id` are the ONE spelling a data file uses for each word, in every
  catalog (`data/README.md` §Conventions). A method that needs a Tenno, a
  catalog or a fight lives in the layer that has one.
- **`data/`** reads `data/` into the model — one module per family
  (`data::weapons`, `data::mods`, …), over the compile-time-embedded tree.
- **`build/`** resolves a loadout: `build::loadout::resolve` turns a weapon,
  its mods and its evolutions into a `ResolvedPanel`.
- **the fight** — `target` is who is shot, `formation` the crowd, `arena` both
  actors, `fight` the simulation (`FightParams`, `run_once`, `monte_carlo`,
  `replay`) and `record` what it wrote down. `fight` is itself split by the
  MOMENT of a shot — `open` the engagement, `before` and `after` the shot,
  `resolve` its numbers, `pellet` one of its pellets — so the loop in `run` is
  the order those happen in and nothing else.
- **`board/`** is build identity (`board::builds`) and the rulers
  (`board::benchmarks`).

**UTILITY IS NOT A FOURTH VERB.** The game's live state and the reader's
reminders on it (`/utility`, docs/UI.md §"Utility") are read and named in the
worker (`worker/world.js`), never in `engine/`, and nothing there reaches a
build, a fight or a search.

A directory module is split by the question each file answers (`resolve`,
`panel`, `parse`, `apply`, `describe`, …) with its tests under `tests/`; its
`mod.rs` re-exports the public items, so a caller names `data::weapons::spec`
and never the file.

---

## 5. Optimizer (best-build search)

- **Switchable objective:** steady-state DPS / TTK against a given enemy /
  total damage within effective range / burst damage, etc.
- **Constraints:** mod slot count, polarity/capacity, mod exclusivity,
  arcanes, etc.
- **Method:** exhaustive search + pruning is the correct baseline a faster
  search is checked against.
- **Principle:** the optimizer only ever calls the engine — it never gets its
  own simplified damage formula, or the "optimum" it finds is fake.
- **Search strategy:** deduplicate by "canonical form" (position-sensitive
  mods = element order first, everything else unordered; polarity layout is
  not part of a build's identity); candidates pass "best-effort legalization"
  (innate polarity reassignment → greedy Forma → reject if it can't fit);
  conditional buffs default to full stacks. Details in [`OPTIMIZER.md`](OPTIMIZER.md).

---

## 6. Technology Choices (Decided)

- **Language/runtime:** **Rust** (2021 edition), version pinned via **mise**
  (currently `1.97.1`). Chosen because the optimizer = "a huge mod
  combination space × several Monte-Carlo simulation runs per candidate" — a
  CPU-bound search×simulation double loop that Rust is uniquely suited to
  among the candidates considered; strong typing and numeric reliability also
  fit "correctness first".
- **Project shape:** a Cargo workspace — `engine/` (every mechanic, layered as
  §4), `optimizer/` (only calls the engine), `webapi/` (the endpoints), and the
  transports `web/`, `wasm/`, `cli/`.
- **Optimizer evaluation:** **hybrid** — analytic expectation (fast, for
  coarse filtering during search) + Monte Carlo (slow, for final calibration
  and distributions, SimCraft-style).
- **Code/docs language:** English (public repository).
- **Data sourcing:** bulk entry from the wiki's structured Lua modules,
  normalized into versioned YAML — see [`DATA_SOURCES.md`](DATA_SOURCES.md).
- **Accuracy verification:** systematic Simulacrum measurement protocol and
  the golden-test baseline — see [`MEASUREMENTS.md`](MEASUREMENTS.md).
