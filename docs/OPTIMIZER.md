# Optimizer search design

How the optimizer walks the mod-combination space without wasting
evaluations. Companion to
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
   freely repositionable **pool** (`engine::rules::capacity::plan_forma` step 1):
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

## 5. Implementation

- §1 and §2: canonical enumeration, legalized by
  `engine::rules::capacity::plan_forma` per subset.
- §3: `StackPolicy::AssumedMax` in `engine::build::loadout::resolve`.
- Constraint hooks: `require=<mod_id>` / `forbid=<mod_id>` filter the space
  before enumeration (`wfsim-optimizer`).
- Resumable rounds and the screen's resume cut: docs/WASM.md §Checkpoint.
- Best-so-far snapshots: the screen publishes its top slice every 4096
  candidates and every completed round publishes its leaderboard, both
  result-shaped. A browser cancel TERMINATES the worker, so a leaderboard
  that has not already left it cannot be recovered — this is what makes a
  cancelled run show its ranking instead of an empty page.

## The optimizer tab is THREE BLOCKS

The simulator's shape — a block per thing, each with its own number, title and
fold:

```
⚙ THE SEARCH      preset bar · ① starts (build cards, edited in the builder)
                  · ② limits                  — what a search preset saves
▶ THE FIGHT       the scenario, a link to edit it in the Simulator, and the
                  fight as a card (`fightCardHtml`) with no control in it
★ OPTIMIZE        run · runs per candidate · the estimate,
                  then the progress and the results
```

| what | where it lives | why |
|---|---|---|
| the starts, runs per candidate | the SEARCH preset | decisions about one search |
| the fight, the player, the buffs | the SCENARIO preset, read here as a card | the winner is scored under the fight the replay runs |
| the final round's run count | the simulator's Runs (`simRuns()`) | the final round IS a simulation |
| how many cores to use | **neither** — the topbar's compute share | one setting for the whole page |

There is no scope to mark: what may change is what the quick calc offers, and
a pin on a start is the one way to keep something. The final round is the
simulator's own run (§"The answer"), so a row's number is what simulating its
build answers.

**The stance slot is not searched.** A stance decides what the weapon swings
(Crushing Ruin against Shattering Storm is 1,275 against 1,162 DPS on the same
Magistar), so it is a real axis; every candidate carries the builder's.

## A card is searched at max rank, unless it is named

**A CARD BELOW ITS MAX RANK IS AN ID OF ITS OWN**, `<card>@<rank>`
(`data::mods::RANK_MARK`), resolved by `data::mods::at_rank` with the same linear
ladder the card text is filled from. It rides every mod list — a request, a
scope, a result row, a board record — so no surface grows a rank field it could
drop. Max rank is the bare id. A card whose ladder is not linear says
`lower_ranks_unmodelled` and refuses (Double Tap's stack cap).

**WHICH CARDS ARE TRIED AT EVERY RANK IS A LIST OF IDS**, never a rule about
effects: `data/search/every_rank.yaml` is the default, served as
`/api/meta.every_rank`, and the quick calc's `every rank` control replaces it
for one reader. Named cards enter the quick calc's lists and the optimizer's
scope once per rank; the variants share the card's family, so two ranks of one
card are never one build. The reason the list exists is Status Duration: a
malus past -100% nullifies every status, and a low-rank card lifts it back
above the line where a max-rank one would erase the malus (M99).

The same list decides what the board's intake asks — docs/BOARD.md §Rivens.

## The search and the replay must be the SAME fight

**THE OPTIMIZER BUILDS NO FIGHT.** A candidate is a request — `Replay::request`,
the function a row's `replay` comes from — and it is scored by
`simulate::ready` on that request, the one construction `/api/simulate` and
`/api/log` run. `Scenario.params` is that call, and the optimizer crate holds
no fight term of its own: the reserve, the ammo economy, Frenzy, a melee
Incarnon's window, a Tome's cycle, a kitgun's parts, the roster, the arcane's
stack policy are all the simulator's, so none can be missing from the search.
A build the simulator refuses scores nothing (`Summary::refused`). A test or
the CLI, which have no simulator to call, state their own fight
(`Scenario::from_panels`); the product never does.

A candidate's PANEL is still resolved at enumeration, for legality and dedup
only, from the simulator's own base (`registry::base_for`).

Going through the request costs 5–25% CPU per search over a hand-built fight
(five weapons, 10 runs a candidate), and every answer was unchanged where the
two already agreed.

## …AND SO MUST THE BUILD

The section above is about the FIGHT, and it fixed the fight. The build had the
same disease one layer out, and it took three years of calendar and four
separate patches to see it as one thing.

A build travels through eight representations — live page state, a stored
preset, a simulate request, an optimize scope, a ranked row, a board
submission, a board record, a share link — and each held a hand-written answer
to "which axes are there". A missing axis and a defaulted axis are the same
absence on the wire, so a producer that had never heard of an axis was
indistinguishable from one that meant the default, and no consumer could
complain. `mode` was lost from the board submission, `valence` from
the worker's table, both from the share tuple, and
`valence` from the optimizer's "+ add".

The last one is the one that mattered, because it is the one a player could
see. A search won on Magnetic became a build fired on Impact — `defaultValence`
opens on the spec's first element — and he reported 26 KPM on the ranking
against 15 in the simulator for what he had been told was the same build.
Measured on a Kuva Nukor, Thrax Lv 100, 180 s, an exhaustive 12-mod scope:

| | KPM |
|---|---|
| the ranking's #1 (valence = Magnetic) | 22.34 |
| the same build simulated, **with** Magnetic | 22.36 |
| the same build as "+ add" handed it over (Impact) | **17.44** |
| the same build at the optimizer's own seed | 22.23 |

So the engine was never the problem — the two agree to 0.1%, and a Torid pass
over modes, evolutions and arcanes agrees to 0.3% on all six ranked rows. The
seed and the winner's curse are worth 0.5%. What diverged was the page's
translation, which the rule above never covered.

**The fix is that a row stops describing a build and starts carrying one.**
`entry()` emits `replay`: a complete simulate request, built by cloning the
optimize request and overwriting only the axes the search ranged over. Cloning
rather than assembling is the whole trick — every field that reaches the
optimizer rides along, including ones nobody has invented yet — and `runs` is
the fight's own, so the row's precision is the replay's precision.
POST it and you get the row's number, with no assembly at any caller.

**And the quick search's final round IS the simulator.** Each contender's
`replay` goes through `simulate_json` and the row's number and rank are what it
answers (`measured_by_the_simulator`), so simulating a row's build reproduces it
to the last digit (`a_quick_row_is_the_simulators_own_number`). The other
strategies keep a funnel final round, and `check_opt_replay` compares their
rows with the simulator at 4σ of the two standard errors.

That comparison is the durable part. Every earlier guard was a LIST of axes,
and a list has to be maintained by whoever adds the fifth; this one is an
ANSWER that has to match, so it covers axes that do not exist yet.
`scripts/check_opt_replay.mjs` asserts it in CI and is verified to bite —
reinstating the bug moves the Nukor from 0.6514 to 0.2118.
`engine::board::builds::BUILD_AXES` plus `scripts/check_build_axes.mjs` cover what an
answer cannot reach: a share link nobody has clicked, a board record nobody has
submitted.

## ACCURACY IS MEASURED, NOT ASSERTED

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

**Recall counts DISTINCT builds.** The flat measurement gives every job its own
seed, so two jobs that are one build to the fight — a utility exilus, an
evolution the engine does not load — rank apart by noise and a top ten can hold
one build five times. `Truth::merge_twins` groups the jobs that are
`one_build` on one paired 10-run pass — the list's own rule — and `recall`
counts groups on both sides: a search that lists a build once is not missing
its twins.

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

**Baseline.** Verglas Prime, 10 pooled rifle mods, Thrax Centurion
Lv 1000 SP, 60 s, `truth_runs=200`:

| | |
|---|---|
| scope | 1,822 jobs, exhaustive |
| reference | 364,400 sims; answer set **1 build**; settled; top-10 overlap 1.00 |
| search | descent from 4 starts; rank **1**, regret 0.000%, within noise, top-10 recall 100% |
| cost | 1,821 sims — **0.5%** of the reference |

The reference's own #1 is Viral+Heat (`cryo_rounds, malignant_force, hellfire`
+ the four damage mods), which is what the weapon's innate Cold makes reachable
under MECHANICS §3 rule 3 — the innate is pulled forward onto the Cold mod's
position, leaving Heat unpaired.

## The RANKING statistic needs its own σ

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

## Exhaustive enumeration does not survive a real scope

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
**10⁹** — which is why the page's search is a descent (§"The search").

A walk that IS cut short must leave a sample, not a corner. A depth-first walk
over pool indices leaves a lexicographic prefix: measured on a 22-mod pool, the
complete walk carries Heat in 2.77% of candidates, truncated to 8.7% of it
1.45%, and truncated to 3,000 candidates of the full pool **0%** — so a build
with no Heat wins on a weapon where Heat is worth 4.5×. The walk therefore
runs over a shuffled index range, and a cut walk reports its coverage rather
than rendering as a completed search.

## A scope, for a caller that names one

The page sends none (`whole_scope`); the grader and `wfsim-truth` do, to
compare a search against an exhausted space. A scope is a MARK MAP per axis —
`fixed` pins, `search` pools — and its rules are the server's:

- **Every axis is N slots and a range.** Mods are 8 slots, `build_min` to
  `build_size` (0–8); every other axis is one slot, 0–0, 0–1 or 1–1. The empty
  choice is a mark like any other: `none` on the exilus slot, `none:<pool>` on
  an arcane seat. Nothing marked is 0–0, a mark is 1–1, so no scope grows by
  default; an arcane seat is never empty beside a marked candidate
  (`an_arcane_seat_marked_none_is_not_a_default`). An EVOLUTION TIER is never
  empty: unmarked, it holds its default (`evolutions::complete`); named with no
  option left in it — every option excluded — the request is refused, and the
  page's limits keep a tier's last option.
- **The floor is derived first.** Every required mod, plus one pooled mod when
  anything is pooled; `build_min` below that is raised to it
  (`min_slots = derived_min.max(build_min)`). A ceiling of 0 outranks the
  derived floor — the bare weapon with the marks kept — and an empty scope is
  the bare weapon (`an_empty_scope_searches_the_bare_weapon`).
- **A pin settles its slot.**
- **The variant table is `modes × evo_sets × valences`**, so pooling a second
  mode doubles the space.

## The search — every scope is descended

Candidate GENERATION and candidate RANKING are different problems. The funnel
ranks: it culls 22,316 jobs to 10 for 1.5% of the flat cost and loses nothing
(§Accuracy). Generation is the **DESCENT** (`optimizer/src/descent.rs`) on
every scope, whatever its size: one search everywhere, so the answer always
depends on one thing — the starts — and a small scope is not answered by a
different rule than a big one. Its answer is the best its starts reach, and it
never reports itself exhaustive.

**The WALK** (`optimizer/src/search.rs`) visits every subset and so answers
with the proven optimum; it runs only when a tool asks
(`"strategy": "exhaust"`, `walks_whole` in `webapi/src/optimize.rs`) — the
page never does. The result says which ran: `strategy` is `walk` or
`descent`, a walk carries `exhaustive` and `coverage`, a descent carries
`starts` and `cut` (the clock stopped it before every start settled).

**Inside a subset, everything stays exhaustive** under both: element orders,
exilus options, evolution sets. A couple of dozen cheap combinations each —
handing an exact subproblem to a stochastic search is how an answer gets lost
for no reason.

### The walk

**The space is an index range.** `optimizer/src/space.rs`: `SubsetSpace::nth(i)`
unranks the i-th subset in colex order, O(k log n). Family exclusivity is
REJECTED rather than folded into the index — family-legal subsets are 79–85%
of C(n, 8) over the shipped pools, so rejection costs ~25% of a walk against
an evaluation that costs a whole simulated engagement.

`Shuffle` is a pseudorandom bijection on `0..len` (a 4-round Feistel network
with cycle-walking) and the walk follows it: reaching the end visits every
subset exactly once; a clock that stops it early leaves a uniform sample, and
`coverage()` is exact because the denominator is a counted index range.

A batch is trimmed to the budget left, converted from evaluations to subsets
at the rate the run has actually paid — batches are wide (4 proposals per
worker) and a subset costs several evaluations, so an untrimmed batch
overruns a small budget many times over.

**The depth-first walk is kept as the GRADER's enumeration.**
`enumerate_candidates_observed` is what `grade_optimize` exhausts a scope with
— a reference must not share machinery with what it grades.
`optimizer/tests/enumeration_equivalence.rs` pins the two together: a full
sweep of the index space is exactly the walk's output on a real pool.

**Resume is by round.** The funnel's ROUND checkpoint survives a reload; a
search in progress does not — its position is not a thing a checkpoint can
name.

### The browser runs a FLEET

The browser is where compute is smallest: one thread at ~150 simulated
engagements per second, against ~5,100 on a 26-thread desktop. Parameters
cannot close a 34x gap; workers can.

A WALK gives N Web Workers DISJOINT STRIDES of the shuffled index range —
worker `w` takes `w, w + N, w + 2N, …` (`SearchConfig::shard` / `shards`). The
strides are a partition; `shards_partition_the_shuffled_order_exactly` pins
that, because an overlap would waste the budget and a gap would let N shards
each report themselves exhaustive over a space they had not covered. A
DESCENT gives each worker its share of the starts (`index % shards`).

The count is the **topbar's compute share** and nothing else:
`woptWorkerCount()` is `poolSize()`. See §"…and so did CPU threads".

**Merging** is a sort: every row was produced by its shard's own funnel at the
same run count under the same scenario. Rows are deduplicated by identity
first — two descents can reach the same build. `exhaustive` is the AND of the
shards, coverage the SUM of their walked positions over the space, `starts`
the sum of theirs, `cut` the OR.

**Empty shards are not failures.** A shard that owns no ground — more workers
than index positions, or than starts — returns an empty, complete envelope;
"no legal builds in this scope" is reserved for a shard that searched and
found nothing.

## The descent — the axes ADD

A search that pays for every subset under every arcane and every variant (mode
× evolution set × valence) pays their PRODUCT: on Boar Prime with three
arcanes and eighteen evolution sets one subset costs ~180 evaluations, so
20,000 buy 212 subsets. The descent holds one build and sweeps one position at
a time, so a sweep costs the SUM of the option counts, and the whole pool
stays searchable.

1. **Starts** are the player's partial builds: a list of mod ids, or
   `{"mods": [...], "locked": [...], "arcane": id | [ids], "lock_arcane":
   bool}`. A start is where the descent begins, not a constraint — except
   what it LOCKS, which that start never swaps out (the scope's `fixed` mark
   locks it for every start). An id outside the scope is refused, not
   dropped: a start that silently lost its pin searches something the player
   did not ask for. Without any, there is one start per primary element, one
   card each — the element's DUAL-STAT card (element plus status chance, Rime
   Rounds' shape, which every class has per element), the strongest carrier
   only where there is none; the fill picks the partner. ONE start holding all four is the wrong
   shape: shedding an element costs its combination before the freed slot
   pays, so it stalls (49% regret below).
2. **Fill**: add the best card until the build is full.
3. **Sweep**: arcane → each mod → an empty slot → the variant. ANY accepted
   move restarts at the arcane, because a change anywhere moves what every
   other position wants. A sweep with no move is that start's answer.

On the page a start is taken from the builder ("add the current build as a
start"): put in only what you mean — one card is a start — and click a card or
the arcane in it to lock it there.

Slot POSITION is not part of a start: element order inside a subset is still
enumerated exhaustively, so "Cold in slot 1" and "Cold somewhere" are one
start. Every build is scored on ONE random stream, so a comparison is paired
and the score is a fixed function of the build — each accepted move strictly
raises it over a finite set, which is why the loop ends.

**Measured against ground truth** (`wfsim-truth`, each scope against its own
exhausted reference): at its default starts the descent lands at rank 1 or
within noise on every weapon class — melee, incarnon, valence, sniper, modes —
and matches or beats a uniform sample + climb at the same screen evaluations;
on the whole pool, where no reference exists, its winners replay well above
the sampler's. One start holding all four elements stalls at 49% regret.

`the_descent_reaches_the_answer_set_from_any_start` is the CI guard; with
moves never accepted it fails at rank 440.
`a_locked_card_stays_in_every_build_its_start_scores` and
`a_locked_arcane_is_the_only_one_its_start_scores` guard the locks; each fails
with its lock ignored.

### One change at a time

A move changes ONE position. A valley where each change alone loses and two
together win is crossed by a START on its far side — a player's own, or a
default one per element — never by trying every pair: with every card a
candidate, a pair move is C(8,2)·85² ≈ 200,000 builds a round, hours in a
browser.


**THE BUILT-IN SEARCHES** are `data/search/presets.yaml`, read-only like the
official rulers: never stored, never edited, ⧉ copies one into a search of your
own (`80-optimizer-preset.js`). Owning no search, a reader has the first listed
one — four starts, one per primary element, each its 60/60 card the weapon
equips. The search is handed their starts like any other; nothing in it knows
a preset exists.
## The quick descent — the quick calc, repeated

The page's search (`"strategy": "quick"`): from each start, run the QUICK CALC
on one position of the current build, keep the best candidate if it beats what
is there, and start over at the first position — until no position offers
anything better. It is not a search beside the quick calc; it IS the quick
calc, applied until the build stops changing, so what the quick calc learns —
an axis, a legality rule — the optimizer has by construction.
`optimizer::quick` is the loop behind a `QuickSpace` trait; webapi's
`optimize::quick` implements it over the plan's tables. The subset descent
above runs only when a tool asks for it by name.

### Starts

- A start is A BUILD, and it need not be a good one — a perfect start would
  leave nothing to optimize. The optimizer lists each as the simulator's own
  build card (`cardOfState`).
- A start is edited IN THE BUILDER (`79-start-edit.js`): a banner names it, the
  build bar is put away, and every position carries a pin. The builder's
  autosave is suspended while one is open, so the player's own build is never
  written with it; Done writes the build and the pins into the start, and both
  Done and Discard put the player's build back. `check_start_edit`.
- **Fixed** is the pin: a fixed position is never swept, so every answer from
  that start carries it. A start's cards join the scope when it is saved.
- THERE IS ALWAYS A START, the way the builder always holds a build: a new
  search holds ONE BLANK START, which the fill completes in the search's order,
  and removing the last start leaves a blank one. The page guides the player
  to add their own — the current build, a saved one — and to add starts that
  differ, since that is what crosses a valley one change cannot.

### Positions and candidates

- The positions are the quick calc's axes, in this order: mode, each
  evolution tier, valence, each arcane seat, mod slots 1–8, the exilus slot.
  WHAT SHAPES THE WEAPON COMES FIRST: a card chosen before the mode or the
  evolutions is chosen for another weapon. With mode last, Burston Prime spent
  56% of its work on the base form before the cycle doubled the score.
- The candidates of a position are `/api/candidates`' (`webapi/src/candidates.rs`),
  the quick calc's own list: family exclusivity, what an evolution set forbids,
  an evolution that would evict an equipped card, a mode a mod takes away, the
  every-rank list — kept when they map onto the plan's tables. A quick request
  that names no scope gets the WHOLE one (`whole_scope`): every card, exilus
  card, arcane, evolution, mode and element the weapon takes, with the lower
  ranks of the request's `every_rank` list — so the tables are exactly what
  the quick calc offers. The page sends no scope; only the grader names one.
  No candidate is EMPTY. It took the page's generator's place after reproducing it
  on 91 of 91 positions over six weapons; like it, it offers a stance card for
  a main slot, which the builder's picker does not.
- **A build scores at its BEST ELEMENT ORDER.** Slot position decides only
  what combines, and a candidate seated in the slot being swept cannot move its
  element behind another: Magnetic + Toxin on Sancti Magistar needs a card swap
  AND a reorder at once, each worse alone — rank 4, 1.6% short, unmoved at 30
  runs and by a reorder move of its own. Each build is scored over its distinct
  element orders (the enumerator's `expand_one`) and keeps the best.

### One step

1. LEGALITY FIRST: a candidate whose build Forma cannot fit (the auto-Forma
   planner, through the enumerator) is dropped before it is simulated —
   the same answer as simulating all and walking down the ranking, for less.
2. Score the rest on ONE paired stream, `candidate_runs` each — **10 by
   default**, the quick calc's count. With one answer per start no funnel sits
   behind a step to undo a noisy one: at 1 run, Viral + Heat on Boar Prime lost
   to Magnetic + Heat 5% below it and the answer came 13th. 1 is the fast
   option.
3. The best legal candidate that beats the build replaces it, and the sweep
   restarts at the first position.

### Each step is screened

Step 2 measures in full only what a SHORT measurement cannot rule out. A
step's candidates with no full score yet first fight ⌈`SCREEN_RATIO` ×
`candidate_runs`⌉ on the same paired stream, and the best ⌈`KEEP_RATIO` × k⌉ of
those k go on to `candidate_runs`; the rest keep only their short score, so
they are never a move and never on the list. Both ratios are 0.2
(`webapi::optimize::quick`), overridable per request (`screen_ratio`,
`keep_ratio`). Below two short fights there is no spread to rank by, so at
`candidate_runs` under 6 there is no screen. On the fleet a short measurement
is a pending item of its own, `{build, runs}`, answered `rough`.

**A FULL MEASUREMENT KEEPS THE SHORT ONE'S RUNS.** They are the first runs of
the same paired stream, so the full count continues them (`shard_onto`) rather
than fighting them again, and lands on the same bits; on the fleet the short
answer carries its runs per element order (`shards`) and the leader hands them
to whichever worker measures that build in full. A contender's summary is its
full measurement's own runs, never a second fight of them.

Measured over Rubico Prime, Torid, Kuva Nukor, Sancti Magistar and Acceltra
Prime at 10 runs: every answer was the unscreened search's own, for 23–36% of
its full evaluations (CPU, measured at a keep of 0.25: 26–39% less).

The FILL is one pass in the same order, before any sweep. It fills what the
start left EMPTY (a mod slot: its best legal candidate) and what it did not
NAME (a mode, an evolution tier, the valence, an arcane seat, the exilus: the
axis's default is kept only when no candidate beats it), and leaves what the
start named alone — judged on a half-empty build, Primed Cryo Rounds lost to
Hellfire on Burston Prime and the answer lost Viral (74.7 against 166.4). A
blank start names nothing. Every choice is legal, so a build
is legal from the moment it is full. On that Burston Prime start the fill cut
the work from 6,569 builds to 1,876 and reached the same build.

### The answer

- **THE BEST N BUILDS THE SEARCH SCORED**, N the request's `finalists` (the
  page's "How many results", 10 by default; 1 is the best of the starts).
  Each start settles on ONE build, its ANSWER; starts that settle on the same
  canonical build — the same cards in any order, the same resolved damage, the
  same exilus, arcane and variant — merge. The list is not the answers alone:
  a start's last sweep has scored every build one change from its answer, and
  that is where the runner-ups are — the answer with one card swapped.
- **THE POOL** is every whole build a sweep scored, on the one paired stream:
  the answers and every build one change from where a start stood. The fill's
  half-empty builds are not in it. THE SAME SCORE IS THE SAME BUILD for the
  list (`one_build`: both counts within a relative 1e-9 on the paired stream)
  — a utility exilus, an evolution the engine does not load, Serration for
  Heavy Caliber whose accuracy penalty the arena does not read — and the
  answer is the one kept: re-measured on separate streams, such twins pushed
  the answer itself off the list.
- **THE CONTENDERS** are the pool's best N and every one below that ties the
  N-th — `tied_at_the_line`, the funnel's own cut: ±3·SE from the pooled σ of
  kill progress, capped at 2N. They go STRAIGHT TO THE SIMULATOR: each one's
  `replay` is simulated at the fight's own runs, and the best N by what it
  answers are the rows.
- A row a start settled on carries `from_starts` — per start, its score before
  the descent and the changes it took. Any other row carries `near`: the
  starts of the answer on the list it is nearest to, and `changes`, one
  `{axis, from, to}` per position that differs. A start that reached no legal
  build is listed in `failed_starts`. The fleet merges shards' rows for one
  build into one row.

### Measured

`wfsim-truth … strategy=quick` (defaults: 10 runs), 60 s, Thrax Lv
9999 SP, reference 100 runs:

| scope | rank | regret | within noise | simulated |
|---|---|---|---|---|
| Verglas Prime, 14 mods | 1 | 0% | yes | 12,020 |
| Boar Prime, 11 mods × 2 arcanes × 8 evolution sets | 2 | 0.3% | yes | 5,760 |
| Lex Prime, 10 mods × base / cycle | 1 | 0% | yes | 3,400 |
| Kuva Hind, 11 mods × 5 valence elements | 1 | 0% | yes | 5,270 |
| Sancti Magistar, 11 mods × 2 arcanes | 2 | 0.6% | yes | 3,090 |

THE LIST, graded: `finalists=10`, 30 s, reference 100 runs, recall over
distinct builds, against the answers alone (one row per start):

| scope | answers alone | the list | simulated |
|---|---|---|---|
| Boar Prime, 9 mods × 2 arcanes × 8 evolution sets | 10% | 100% | 2,780 → 3,780 |
| Braton Prime, 13 mods | 10% | 80% | 5,050 → 5,950 |
| Kuva Hind, 11 mods × 4 valence elements | 10% | 90% | 4,680 → 5,880 |

What the list misses is TWO changes from every answer — on Braton Prime,
Stormbringer for High Voltage and a damage card swapped at once: the pool holds
one change from where a start stood, and a start that stands there is what
reaches further.

A Lex Prime scope that also names `transformed` ranks it 3rd: the reference's
winner fires that mode, which the builder and the quick calc never offer (it
is not sustainable), so the scope asks a question the page does not.
`the_quick_descent_lands_in_the_answer_set` is the CI guard, on a Boar Prime
scope where only the sweep reaches the answer: with it disabled the guard
fails at rank 14, 25.5% regret.

### The page

THE SEARCH block: the search preset bar; ① the starts, each the simulator's
build card, edited in the builder; ② the limits (below). THE FIGHT block: the
simulator's scenario as a card. THE OPTIMIZE block: the run bar (run, how many results,
runs per candidate 1 / 10, final-round runs, what the run will do); while it runs, a
line per start (filling k of n, round r at position k of n, settled) and a bar
of settled starts, since how many rounds a start takes is found by taking
them; then the results, one row per build with the starts it came from or
the answer it is near and what differs, a tie with the leader marked, "+ add"
and "use as a new start". A search preset saves the starts, the limits, how
many results and the runs per candidate. There is no scope to
mark: what may change is what the quick calc offers less the limits, and a pin
on a start is the one way to keep something.

### Limits

What the search may not use, and how full it fills — NOTHING BY DEFAULT. The
page lists every option of every axis expanded, in the builder's order under
the builder's own numbers and names (mode, mods with the exilus, arcane,
evolution tiers, element), and a click excludes one: a mod by the builder's
rows, so one rank of an every-rank card (`card@2`) can go and the others stay;
an arcane at every rank; an evolution, a mode, a valence element (never the
last mode or element). The fill: at most `mods` cards (0–8), the exilus filled
or left empty, each arcane seat filled or left empty.

The request carries `limits: { exclude: { mods, arcanes, evolutions, modes,
valence }, mods, exilus, arcane_seats }`; `whole_scope` leaves the excluded
options out, pins an empty exilus or seat (`none`, `none:<pool>`), and sets
`build_size`, and the descent sweeps only that many mod slots. A search saved
with only an excluded-mods list carries it in as `exclude.mods`.

A START AND A LIMIT MAY DISAGREE. What a start PINS and a limit rules out —
an excluded option, a card past the cap, a filled exilus or seat set to empty —
blocks the run until one side changes; the start card says which, in red. What
a start only HOLDS is replaced: the search treats that position as unnamed and
fills it. `a_limit_is_in_no_answer` is the guard.

## One mod row

The `.opt` row is ONE function (`modRow`) with the trailing control as its
parameter — the picker's drain, the every-rank list's ×. A copied row is a
comment that stops being true in silence.

## A ranked row is a build you can re-run

**A RANKED ROW IS A BUILD YOU CAN RE-RUN, AND THE NUMBER ON IT IS THE

SIMULATOR'S.** The row CARRIES a build rather than describing one: `entry()`
emits `replay`, a complete simulate request written by the same code that
built the candidate, from the optimize request itself — so every field that
reaches the optimizer rides along, including ones nobody has invented yet, and
only the ranged axes are overwritten. POST it and you get the row's number.
"+ add" applies it through `stateFromBuild`, the inverse of `buildPayload` and
the ONLY translation between a request and the page; the pair round-trips.

AND THE RANKING IS THE SIMULATOR'S: the quick search's final round simulates
each contender's `replay`, so the page shows that number and has nothing to
re-run.

## A build’s axes are declared once — in the engine

**A BUILD'S AXES ARE DECLARED ONCE — IN THE ENGINE.**
`engine::board::builds::BUILD_AXES` is the list, served at `/api/meta.build_axes`.
The SPELLINGS stay per-protocol (`arcane` on a request, `arcanes` on a board
record, `arcaneRank` in page state) because renaming them would migrate every
stored preset; what is shared is the list, and each surface declares which
axis its own fields carry — `BUILD_STATE_KEYS` and `SHARE_AXES` in `app.js`,
`axis:` per row in the worker's `AXES`.
`buildState()` REQUIRES a value for every state key, so the five producers of
a build state — the live page, "+ new", a board row, a share link, an
optimizer result — must each name every axis. `undefined` stays a legal value
meaning "the weapon's own default"; what is not legal is not MENTIONING one.
`restoreState` fills a missing axis with the weapon's default, which is RIGHT
— and is why a producer that meant the default and one that never heard of the
axis hand over the same object.

## The simulator is the truth; the optimizer obeys it

**THE SIMULATOR IS THE TRUTH; THE OPTIMIZER OBEYS IT.** A search's winner is
replayed under the simulator's fight, so any rule the optimizer applies that
the simulator does not — or omits that the simulator applies — scores builds
nobody can reproduce. The optimizer must CALL the simulator's code and add
only its own scope and budget. `parse_fight` is that shared parse:
`simulate_json` reads `replay` and nothing else; `parse_optimize` reads
`build_size`, `build_min`, `finalists`, `deployment` and nothing
else. Neither builds a second Tenno. Anything that is a property of the FIGHT
goes in `parse_fight`. A shared helper is not enough — the DECISIONS around it
have to be shared too.
