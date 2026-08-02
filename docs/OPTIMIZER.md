# Optimizer search design

How the optimizer walks the mod-combination space without wasting
evaluations. Recorded 2026-07-24. Companion to
[`CORE.md`](CORE.md) §5 (objectives, constraints, engine-only principle)
and [`MECHANICS.md`](MECHANICS.md) §2–§3 (capacity/polarity, elemental
hierarchy).

## 1. Build equivalence — canonical form, never test twice

Damage output of a build depends on exactly two things:

1. **which mods** are equipped (a multiset), and
2. the **relative order of position-sensitive mods** — today that is
   only **elemental-primary mods**, because slot layout drives the
   combination hierarchy (MECHANICS §3). Everything else (crit, damage,
   multishot, status, dual-stats' non-element half, …) feeds order-free
   additive/multiplicative buckets.

Slot assignment and polarity layout are **not** part of a build's
damage identity — they affect only *legality* (capacity). Therefore:

**Canonical form** = the position-sensitive mods first, in their chosen
order (slots 1..k), followed by the remaining mods in a fixed sort (by
mod id). Two selections that differ only by permuting position-free
mods — or by where the polarized slots sit — are the **same build** and
must be evaluated once.

Search accordingly enumerates canonical forms directly:

- choose the **element order** (the only ordering degree of freedom),
- choose the **rest as an unordered set** (drain numbers are ignored at
  this stage — selection first, legality later),
- prune any candidate whose canonical form was already evaluated
  (cache keyed by canonical form + scenario + buff policy).

**Second-level dedup (cheap, pre-sim):** different element orders can
still resolve to the *same* combined-element damage vector (e.g.
swapping the two members of a single combining pair: Cold,Elec vs
Elec,Cold → the same Magnetic pool). Run the pure elemental-combination
layer (pipeline [2]) on each candidate — it is trivially cheap — and
dedup on the **resolved post-[2] vector** before any Monte Carlo.

Position-sensitivity notes:

- **Combined-element mods** (Magnetic Might family) sit outside the
  primary hierarchy (MECHANICS §3 rule 7) → position-free.
- **Innate elements** take their position by *rule* (last, or pulled
  forward by a same-element mod) — never a search dimension.
- **Buff-injected elements** (Frenzy's +100% Toxin) enter at their
  defined position — also not a search dimension.

## 2. Best-effort legalization — a filter, not a search dimension

Because legality never changes damage, it runs *after*
canonicalization and *before* evaluation, per candidate:

1. **Rearrange the innate polarity slots.** Innate polarities are a
   freely repositionable **pool** (`engine::mods::plan_forma` step 1):
   spend them on the biggest-drain matching mods.
2. **Spend Forma greedily** on the biggest-drain unmatched mod until
   the build fits the capacity cap (`plan_forma` step 2). Mismatched
   placement is never used — a blank slot is strictly better.
3. Still over cap fully forma'd → the candidate is **impossible** and
   is rejected (hard error, consistent with `validate_loadout`).

Forma count is not free in-game; later it can become a secondary
objective or constraint (e.g. "best build within 3 Forma") — but it
never affects the damage ranking of legal builds.

## 3. Conditional-effect policy — the search runs EMERGENT

Stacking/triggered effects (Galvanized Diffusion's on-kill multishot
stacks, Galvanized Shot's CO stacks, Fevered Frenzy's 20, Frenzy's
headshot buff, …) evaluate under one of three policies:

1. **`assumed_max`:** every conditional buff at full stacks / 100% uptime —
   the community "assume max stacks" convention. This is what the PANEL shows:
   a build's ceiling.
2. **`configured`:** explicit per-buff stack counts / uptimes (the buff
   cards) for what-if comparisons.
3. **`emergent`:** no assumption — stacks rise and decay from the simulated
   timeline itself (kills grant, Galvanized decay drops one stack and resets
   duration, deaths clear). **What the search and the sim both run.**

The policy is part of the evaluation cache key (§1), since it changes
results.

Emergent is not a detail here — it is most of the ranking. Every timed buff
now STARTS AT ZERO ([`BUFFS.md`](BUFFS.md) §Activation policy), so a build that
depends on on-kill stacks is priced by whether this fight can actually produce
them. Against a Lv 9999 Steel Path target that dies twice a minute, it cannot,
and the Galvanized family is worth a fraction of its card — which is the whole
reason the optimizer should be trusted to rank a boss build differently from a
horde build. Measured, both directions: MEASUREMENTS M27.

## 4. Evaluation

Canonical, legal candidates go to the engine scenario (CORE.md §5:
analytic expectation for coarse ranking, Monte Carlo for finals; the
optimizer never reimplements damage math). Results are cached by
(canonical form, scenario, policy); equivalent combinations are never
re-simulated.

## 5. Implementation status (2026-07-24)

Implemented in `optimizer/` (`wfsim-optimizer` binary):

- §1 canonical enumeration: 8-of-23 subsets with family exclusivity
  (155,727 subsets by the generating function — pinned by test), ×
  distinct-element-order permutations, second-level dedup on the
  resolved post-[2] vector (1,452,146 order variants → 391,789
  candidates, ~1 s).
- §2 legalization via `engine::mods::plan_forma` per subset.
- §3 `StackPolicy::AssumedMax` in `engine::loadout::resolve`.
- Constraint hooks (prescribed-mods presets): `require=<mod_id>` / `forbid=<mod_id>`
  CLI args filter the space before enumeration.
- Evaluation: **successive halving** across all cores — rounds of
  (runs, keep): 3→16384, 12→3072, 48→512, 200→64, 1000→24; early
  rounds rank by mean effective damage (continuous, low variance),
  the final rounds by mean kills (the objective). Deterministic
  per-candidate seeds.
- Benchmark scenario: Dual Toxocyst Incarnon (fixed evolutions, no
  arcanes) vs Thrax Centurion @9999 Steel Path, instant respawn, 100%
  headshots, 60 s, finals at 1000 runs.
- Resumable rounds: `run_funnel` takes `start_round` and an
  `on_checkpoint` sink, so a browser run that a page reload killed
  continues from the last COMPLETED round instead of the beginning.
  Seeds key off the ABSOLUTE round index, so the resumed run is not
  merely similar to the uninterrupted one — it is the same
  (`a_resumed_funnel_lands_on_the_same_leaderboard`). The screen resumes
  too, from a cut of the walk rather than a round boundary
  (`a_resumed_screen_lands_on_the_same_survivors`). See docs/WASM.md for
  the checkpoint format and what it deliberately does not cover.
- Best-so-far snapshots: the screen publishes its top slice every 4096
  candidates and every completed round publishes its leaderboard, both
  result-shaped. A browser cancel TERMINATES the worker, so a leaderboard
  that has not already left it cannot be recovered — this is what makes a
  cancelled run show its ranking instead of an empty page.

## The optimizer configures NOTHING (2026-08-02)

Its tab shows the fight, shows the scope, and runs them. Every number that
decides the search comes from somewhere that already owns it:

| what | where it comes from | why not here |
|---|---|---|
| the fight | the SIMULATOR's scenario, read-only | a preset is edited in exactly one place |
| final runs | the scenario's `runs` | how hard you measure is the scenario's question; a second box crowns a winner at a precision the replay never used |
| finalists | fixed at **10** | it is how many answers a person reads, not a property of a search — nobody ever tuned it, and a knob nobody turns only disagrees with itself across presets |
| CPU threads | auto (cores − 2), low priority | a property of the MACHINE, not of a search; it has no control until there is a machine-settings surface, and a stored value is still honoured |

The optimizer preset therefore holds the SCOPE and nothing else. An older one
may still carry `final_runs`/`finalists`; they are deliberately ignored on load
rather than migrated — reading them back would resurrect the second opinion
this removed.
