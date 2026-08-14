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

## The optimizer tab is TWO HALVES (2026-08-02)

Two preset bars, and the page is cut cleanly between them — nothing on it
belongs to neither, which is what makes the two domains legible instead of a
rule to remember (user).

```
preset bar: SEARCH  ─┐
  Mods                │  the search preset. What to look through,
  Exilus              │  and how to look: scope + finalists + threads.
  Arcanes             │
  Evolutions          │
  Search  ────────────┘   finalists · CPU threads
preset bar: SCENARIO ─┐
  The fight           │  the SIMULATOR's, shown READ-ONLY. Edited there,
  The Tenno           │  because a preset is edited in exactly one place.
  Limits              │
  Buffs   ────────────┘
```

| what | where it lives | why |
|---|---|---|
| scope, finalists, CPU threads | the SEARCH preset | all three are decisions about a search: what to look through, how many winners to keep, how much of the machine to spend |
| the fight, the player, the buffs | the SCENARIO preset, read-only here | a preset is edited in exactly one place; the winner has to be scored under the fight the replay will run |

The BUFFS were the last thing to move (2026-08-02). The optimizer kept its own
scope-wide config — a union over everything searchable, with its own stack
settings — on the reasonable ground that a candidate carries mods the current
build does not. That bought one real thing and cost a worse one: the two
modules scored the same fight under different buffs, and "add this winner, then
Run Sim" only agreed because adding a winner secretly copied the search's
config into the user's scenario. One fight, one buff config, and the
disagreement cannot exist. The section still shows the WIDE list — every buff
this weapon could produce, which is what the scenario's "all potential buffs"
view is for — because a search does cover builds you are not holding; a buff
nobody set simply falls to its own default, which for anything timed is now 0.
| final runs | the scenario's `runs` | how hard you measure is the FIGHT's question, and a second box crowns a winner at a precision the replay never used |

`threads` does describe this MACHINE rather than the search — the earlier
reading, and why it used to live in its own localStorage key. But an optimizer
preset never leaves this machine (a share link carries builds, scenarios and
rivens, not searches), so the only thing that reading bought was a second place
to look; a heavy scope wanting more cores than a light one is a real setting to
save. The old key is read once, as a migration.

`runs` is the FINAL ROUND's, and 0 (a blank box) means the fight's own — see
AGENTS.md. An older preset may still carry `final_runs` from the era when the
setting meant something else; that key is ignored on load rather than migrated,
and 0 is what we would read out of it anyway.

## The search and the replay must be the SAME fight (2026-08-03)

Three ways they were not, all found by running one build through both:

| what | the simulator | the optimizer (was) |
|---|---|---|
| `infinite_ammo` | applied — `infinite_reserve = infinite_ammo \|\| !panel.finite_reserve` | IGNORED; the panel's own reserve stood |
| `StackPolicy` for a SENTINEL | `BaseOnly` — nothing on the field triggers a companion gun's conditionals | `Emergent`, hardcoded |
| the Incarnon-form unlock | applied only when the request CARRIED an `evolutions` key | applied unconditionally |

The first is why the search reported LOWER: Larkspur Prime bare, Thrax Lv 300
SP, 300 s — **0.301 with a reserve, 0.149 without**, and the optimizer always
searched it without. Now 0.30085 vs 0.30074, which is seed noise.

The third produced an eye-watering 8x for anything that skipped the key — the
Torid's cycle for free (5.400 vs 0.663). The web always sent it, so only the
CLI, the API and anyone testing by hand ever saw it. The guard is gone: no
unlock, no transformation, whoever is asking.

`Scenario` carries `infinite_ammo` and `policy` now, so a scenario fact the
simulator applies has a field the optimizer applies it from — the two cannot
drift by omission again.

## ACCURACY IS MEASURED, NOT ASSERTED (2026-08-03)

A search strategy cannot vouch for itself. "The funnel kept the best build" is
a claim about an answer nobody computed, and the failure mode it hides has no
other symptom: a search that quietly loses the winner still returns a
plausible-looking leaderboard. So the optimizer is now GRADED against an
answer obtained a different way.

**The reference.** Take a scope small enough to EXHAUST, evaluate **every** job
in it flat at a high run count, rank by the objective. `optimizer/src/truth.rs`
(`Truth::measure`). No funnel, no culling — the reference must not share a
strategy with what it grades.

**The reference is not one build.** The objective is a Monte-Carlo mean, so it
carries a standard error, and the top of a real scope is usually a CLUSTER no
run count can separate. Demanding rank 1 would fail a search for being unlucky
rather than wrong. The target is `Truth::indistinguishable(3.0)`: every job
whose mean is within 3 combined standard errors of the best. Returning any
member of it is correct — that is `Verdict::within_noise`, the pass/fail.
Alongside it: `rank`, `regret` (objective given up, as a fraction of the best),
`recall` (how much of the reference's top-k the search's own top-k contains — a
search can find the winner and still be blind to the field), and `sims` against
the reference's own cost, because accuracy is only interesting next to its price.

**The reference has to earn the name.** One measured at too few runs is just
another noisy ranking wearing a badge. Every grading run measures the scope
TWICE under different seeds and reports whether the two agree on the answer set
(`settled`) and how much of the top-k they share (`cross_seed_overlap`). Not
settled ⇒ raise the run count; every verdict under it is noise.

**Where it runs.**

- `cargo test -p wfsim-optimizer --test search_accuracy` — the CI guard. A
  10-mod Verglas Prime scope (129 jobs, exhaustive), 60-run reference. It also
  asserts the fixture is not degenerate: an answer set that is most of the scope
  grades nothing, so the test fails if the scope cannot separate builds.
- `wfsim-truth pool=<ids> [weapon=… level=… duration=… truth_runs=…]` — the
  same grading at real scale, through `parse_optimize`, i.e. the app's own
  request path. A grader that assembles its own fight grades a different one
  (see "The search and the replay must be the SAME fight"). It REFUSES a scope
  it cannot exhaust: a reference that samples is not a reference.

**Baseline (2026-08-03).** Verglas Prime, 10 pooled rifle mods, Thrax Centurion
Lv 1000 SP, 60 s, `truth_runs=200`:

| | |
|---|---|
| scope | 1,822 jobs, exhaustive |
| reference | 364,400 sims; answer set **1 build**; settled; top-10 overlap 1.00 |
| search | rank **1**, regret 0.000%, within noise, top-10 recall 100% |
| cost | 4,241 sims — **1.2%** of the reference |

The reference's own #1 is Viral+Heat (`cryo_rounds, malignant_force, hellfire`
+ the four damage mods), which is what the weapon's innate Cold makes reachable
under MECHANICS §3 rule 3 — the innate is pulled forward onto the Cold mod's
position, leaving Heat unpaired.

## The RANKING statistic needs its own σ (2026-08-03, finished 2026-08-14)

The funnel ranks by `mean_kill_progress` but took its spread from `std_kills` —
a different statistic that merely looks like it. Whole kills have no partial
credit, so a build that never finishes its second kill has `std_kills` 0 and a
kill progress that moves all run long; the amnesty band at a cut line and the
3σ racing cull were both sized from the wrong number.

**HALF THIS FIX SHIPPED AND THE PARAGRAPH READ AS IF ALL OF IT HAD.** `Summary`
gained `std_kill_progress` on the day above and the two call sites kept reading
`std_kills` for eleven days — the field was never used by anything. It is the
worst shape a half-fix takes: the doc says it is done, the field exists to
prove it, and the code is unchanged.

**What it was worth, graded** (`wfsim-truth`, Torid, 12-mod pool, size 6, 1638
jobs, reference 400 runs each, identical seeds and scope on both sides):

| | rank | regret | within noise | top-10 recall | sims |
| --- | --- | --- | --- | --- | --- |
| `std_kills` | 2 | 0.012% | yes | **90%** | 5558 |
| `std_kill_progress` | 2 | 0.012% | yes | **100%** | 5558 |

Identical cost, one more of the true top ten recovered: the racing cull had
been dropping a genuine contender because it judged it with the wrong σ. The
winner did not move — a build good enough to lead is not the one a mis-sized
band eliminates — which is why this survived eleven days and why RECALL is the
column that catches it.

The rule generalises past this one field: **every statistical decision is sized
by the spread of the statistic it decides about.** A σ that merely looks like
the right one is not a cheaper approximation, it is a different question.

## Exhaustive enumeration does not survive a real scope (2026-08-03)

Measured on Verglas Prime's rifle pool, min 1 / max 8 slots:

| pooled mods | candidates (complete walk) | native single-thread |
|---|---|---|
| 22 | 571,569 | 2.3 s |
| 26 | 2,634,467 | 10.4 s |
| 30 | 9,241,964 | 128 s |
| 60 (the whole pool) | ~10⁹–10¹⁰ | days |

It is superexponential, and evaluating one candidate costs a full engagement:
~200 sims/s per native thread, and the browser is single-threaded. So a search
in the browser can afford on the order of **10⁴ evaluations** against a space of
**10⁹**.

The old answer was `ENUM_BUDGET_MS` in `wasm/src/lib.rs`: stop walking after
20 s. Three things were wrong with it, in increasing order of seriousness.

1. **It bought almost nothing.** On the streaming path the wasm screen runs
   INLINE in the producer — one full simulation per candidate emitted — so the
   budget covered walking *and* screening. 20 s ≈ 3,000 candidates.
2. **The truncation is systematic, not a sample.** `enumerate_rec` is a
   depth-first descent over pool indices, so what survives a cut is a
   lexicographic prefix: builds made of the first few mods plus one varying
   tail. Measured on a 22-mod pool, the complete walk carries Heat in 2.77% of
   candidates; truncated to 8.7% of it, 1.45%; truncated to 3,000 candidates of
   the full 60-mod pool, **0%**. Viral+Heat needs `cryo_rounds`(11) +
   `malignant_force`(26) + `hellfire`(20) together with `serration`(50) and
   `split_chamber`(54) — a subset that appears astronomically late in DFS order.
   This is why the optimizer could return a build with no Heat on a weapon where
   Heat is worth 4.5× (user, 2026-08-03).
3. **It did not say so.** `stream_screen` treats only `cancel` as "did not
   finish"; a walk stopped by the budget returns `complete = true` and renders
   as a completed search.

Raising the budget does not fix (2): at 5 minutes the coverage of a full pool
goes from one ten-millionth to one millionth, and it is still the same corner.
The fix is to stop substituting exhaustive enumeration for search — ONE path at
every scope, graded by the harness above (user, 2026-08-03: rigour over
convenience; a user who wants less work should pool fewer mods).

## How full a build must be is a RANGE (2026-08-03)

The scope had a ceiling (`build_size`, "max mods / build") and a derived floor:
`required + 1 if anything is pooled`. So "search only full 8-mod builds" was
not a thing you could ask for, and every search paid for the sizes below its
ceiling — on a 14-mod pool that is more than half the space, spent on builds
that leave slots empty for no reason.

`build_min` is its own request field now, and the UI is one control with two
ends: **exactly 8** is 8–8, **up to 8** is 1–8, **up to 7** is 1–7 (user,
2026-08-03). Three settings, not three behaviours.

The derived floor stays a FLOOR rather than being replaced: pooling mods is the
statement that they should be used, so every searched build carries at least one
pooled mod and all of the required ones. A `build_min` below that is raised to
it — it asks for builds the scope has already ruled out — while one above it
wins. `scripts/check_build_size.mjs` asserts both ends on screen, in the preset
and in the request.

## The search (2026-08-03)

Candidate GENERATION and candidate RANKING are different problems, and only
the second was ever solved here. The funnel culls 22,316 jobs to 10 for 1.5%
of the flat cost and loses nothing (§Accuracy) — it never needed replacing.
What did was the enumeration in front of it.

**The space is an index range, not a walk.** `optimizer/src/space.rs`:
`SubsetSpace::nth(i)` unranks the i-th subset in colex order, O(k log n).
Family exclusivity is REJECTED rather than folded into the index — measured
over the four shipped pools, family-legal subsets are 79–85% of C(n, 8), so
rejection costs ~25% of a walk against an evaluation that costs a whole
simulated engagement.

**One loop is both regimes.** `Shuffle` is a pseudorandom bijection on
`0..len` (a 4-round Feistel network with cycle-walking). The search walks it:
reaching the end visits every subset exactly once, so the run IS an exhaustive
enumeration; stopping early leaves a uniform sample WITHOUT REPLACEMENT. There
is no mode to select and no size threshold — the 2,000,000-candidate
materialize/stream split is gone, and with it the class of bug where one
regime had a fix the other did not (the tenno/policy leak was in exactly one).
`SearchStats::exhaustive` reports which a run turned out to be, and
`coverage()` is exact because the denominator is a counted index range.

**Sampling alone is not an answer**, so the budget splits: `explore_frac` of it
samples, the rest climbs. The climb is BEST-FIRST and takes the WHOLE
neighbourhood of an elite — every 1-swap, add and drop — because that
neighbourhood is small (62 subsets for 8-of-14) and enumerating it is both
cheaper and far better than sampling it. Measured, 14-mod scope, 22,316 jobs,
graded against ground truth:

| | rank | regret |
|---|---|---|
| random mutation, 3,000 evals | 10 | 2.9% |
| whole neighbourhood, 800 evals | **1** | **0.000%** |
| whole neighbourhood, 1,500 evals | **1** | **0.000%** (top-10 recall 100%) |

`explore_frac` is 0.3 on measurement, not on taste — 0.45 fails to find the
optimum at 500 evals where 0.15 and 0.30 both find it, and 0.30 keeps twice
the exploration of 0.15 for the same result.

**Inside a subset, everything stays exhaustive**: element orders, exilus
options, evolution sets. A couple of dozen cheap combinations each — handing an
exact subproblem to a stochastic search is how an answer gets lost for no
reason.

**The walk is kept, as the GRADER's enumeration.**
`enumerate_candidates_observed` is no longer in the product path but is what
`grade_optimize` exhausts a scope with — a reference must not share machinery
with what it grades. `optimizer/tests/enumeration_equivalence.rs` pins the two
together: a full sweep of the index space is exactly the walk's output on a
real pool, in both directions, with required mods and family collisions in it.

**One regression, recorded as a decision.** Mid-search resume is gone; the
funnel's ROUND checkpoint stays. The old one stored a position in a
depth-first walk plus the survivors at that cut, and a position in a shuffled
range with an elite pool behind it is not the same thing. Restoring it means
checkpointing the elites by identity and re-screening them on resume.

### A batch must not overrun its phase (2026-08-03)

Batches are wide — 4 proposals per worker — so every core stays fed. That made
the explore/exploit split meaningless at small budgets: with 120 evaluations
and a batch of 104 subsets, the explore share was over before the first batch
was, and the climb never ran. Graded, that cost rank 5 and **22.5% regret** on
a scope the same budget now solves outright.

The batch is trimmed to what is left of the current phase's limit, converted
from evaluations to subsets at the rate the run has actually been paying (a
subset costs several evaluations — its element orders, exilus options and
evolution sets). `a_budget_it_cannot_finish_leaves_an_honest_sample` is the
regression guard; it asserts `neighbours > 0`, which is what failed.

**Where the pipeline stands** (Verglas Prime, 14 pooled mods, 12,910 subsets /
22,316 jobs, Thrax Centurion Lv 9999 SP, 60 s, reference at 120 runs):

| search budget | coverage | rank | regret | recall |
|---|---|---|---|---|
| unbudgeted | 100%, **exhaustive** | 1 | 0.000% | 100% |
| 800 evals | 2.07% | **1** | **0.000%** | 60% |
| 300 evals | 0.96% | 8 | 2.280% | 10% |

Two per cent of the space buys the optimum; one per cent does not. The
depth-first walk had no coverage at which it did — its sample was a corner, not
a sample.

### The browser runs a FLEET (2026-08-03)

The browser is where coverage is scarcest and compute is smallest: one thread
at ~150 simulated engagements per second, against ~5,100 on a 26-thread
desktop. Parameters cannot close a 34x gap; workers can.

N Web Workers walk DISJOINT STRIDES of the shuffled index range — worker `w`
takes `w, w + N, w + 2N, …` (`SearchConfig::shard` / `shards`). The strides are
a partition, so nothing is evaluated twice and nothing is missed;
`shards_partition_the_shuffled_order_exactly` pins that, because an overlap
would waste the budget and a gap would let N shards each report themselves
exhaustive over a space they had not covered.

Each shard also CLIMBS on its own. That is a feature rather than a compromise:
N independent hill-climbs from N independent samples is exactly the basin
diversity one best-first climb lacks.

The count is the search preset's own **CPU threads** — the setting already
existed and meant this on the native server. Blank = cores − 1, capped at 8
(past that the strides shorten, each worker costs its own 2.3 MB wasm
instance, and a phone starts swapping).

**Merging** is a sort: every row was produced by its shard's own funnel at the
same run count under the same scenario, so the scores are directly comparable.
Rows are deduplicated by identity first — strides are disjoint but the climb is
not, so two workers can reach the same build. `exhaustive` is the AND of the
shards; coverage is the SUM of their walked positions over the space.

**Empty shards are not failures.** With more workers than index positions —
8 workers over a scope holding one build — every shard but the first owns no
ground, and each answered "no legal builds in this scope (Forma / family
constraints eliminated all)", which the fleet then surfaced as the whole run's
error. Walking nothing differs from walking and finding nothing; only the
second is that message.

**Resume is unsharded.** A checkpoint is one worker's field, so resuming a run
starts a single worker rather than a fraction of a fleet.
