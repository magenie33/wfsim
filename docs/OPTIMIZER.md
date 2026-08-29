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

## The optimizer tab is TWO HALVES (2026-08-02, drawn as two boxes 2026-08-29)

Two preset bars, and the page is cut cleanly between them — nothing on it
belongs to neither, which is what makes the two domains legible instead of a
rule to remember (user).

**AND NOW THEY ARE TWO CONTAINERS** (owner, 2026-08-29). The split was the rule
for four weeks and only headings said so, which cannot tell a reader WHICH
preset bar owns the thing they are editing. A box says it without a sentence.

```
┌ THE SEARCH ─────────┐   everything in this box, and only it,
│ preset bar: SEARCH  │   is what a search preset saves.
│   1 · Mode          │
│   2 · Mods          │   The axes, their order, their numbers and
│         Exilus      │   their names are the BUILDER's — read off
│   3 · Arcane        │   its blocks rather than restated here.
│   4 · Evolution     │
│   5 · Valence       │
│   Search            │   finalists
└─────────────────────┘
┌ THE FIGHT ──────────┐   the SIMULATOR's, shown READ-ONLY. Edited
│ preset bar: SCENARIO│   there, because a preset is edited in
│   The fight         │   exactly one place.
│   The Tenno         │
│   Limits            │
│   Buffs             │
└─────────────────────┘
  Final-round runs        IN NEITHER BOX, AND IN NEITHER PRESET.
  Run Optimizer
```

| what | where it lives | why |
|---|---|---|
| the scope, and `finalists` | the SEARCH preset | both are decisions about a search: what to look through, and how many winners survive to the last round |
| the fight, the player, the buffs | the SCENARIO preset, read-only here | a preset is edited in exactly one place; the winner has to be scored under the fight the replay will run |
| the final round's run count | **neither** — `OPT_RUNS_KEY`, a preference | how hard you want to measure right now is a fact about the person, not about the search and not about the fight |
| how many cores to use | **neither** — the topbar's compute share | one setting for the whole page; a per-search override is two controls for one fact |

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

### The run count left the preset (owner, 2026-08-29)

It rode the search preset with a BLANK box meaning *"the fight's own count"*.
That is one control with two readings, and the wrong home for both. A run count
is not what to search; and it is not the fight either — `sim.runs` has never
existed, because *"how hard do I want to measure right now"* is a fact about
the person and not about the engagement (`SIM_RUNS_KEY`, 2026-08-13).

So it is a preference with a key of its own (`OPT_RUNS_KEY`), TYPED rather than
defaulted from somewhere else, saved by no preset and pinned by no ruler, drawn
outside both boxes. The same shape as the simulator's Runs, because it is the
same question asked in the other module — written twice now, rather than
answered two different ways.

**The cost is stated rather than hidden.** The two counts can differ, so a
winner may be crowned at a precision the replay will not use. That was already
possible — a typed number already overrode the fight's — and the ranking
already reports it: every row is re-run through `/api/simulate` and marked `≠`
when the search's figure and the simulator's disagree by more than 4σ of their
two standard errors.

### …and so did CPU threads

Same argument, other direction. How much of this machine the page may use is
ONE setting and it lives in the topbar beside the language and the theme
(`compute-select`, a share of the reported cores, 2026-08-18). A `CPU threads`
box in the search preset was a per-search override of a global preference —
two controls for one fact — and it put that override on the one thing most able
to cook a phone, which is the last place a global heat setting should be
ignorable. `woptWorkerCount()` is `poolSize()`.

An older preset may still carry `threads` and `runs`. Neither is read, neither
is migrated into the new homes — guessing which of a weapon's saved searches
meant the reader's current preference would be worse than the default — and the
auto-save drops them the first time that scope is touched.

`check_run_counts.mjs` asserts all of it, including the negative control that
the threads box is gone and that no `threads` reaches the request;
`check_search.mjs` asks for one worker through the compute share instead, and
asserts the share actually moved the lane count — otherwise its "a fleet covers
more ground than one worker" assertion would pass for the wrong reason.

## The optimizer is the BUILDER, in bulk (owner, 2026-08-29)

Every axis on this tab is a question the builder already asks. The only
difference is what gets bound: the builder binds a **value**, the optimizer
binds a **set**. That is the whole of the relationship, and the page did not
say it — the optimizer opened on Mods and put Mode fourth, called the builder's
*Arcane* block *Arcanes* and its *Evolution* block *Evolutions*, and numbered
nothing. Three chances for a reader crossing between the tabs to conclude they
are about different things.

So the scope is **the builder's blocks, in the builder's order, under the
builder's numbers and the builder's names** — and the exilus slot sits INSIDE
Mods, because that is where the builder's exilus slot sits.

**NOTHING DECLARES THAT ORDER TWICE.** `orderOptScope` walks
`section.block[data-module="builder"]` in DOM order and appends each axis's
section as it meets one, stamping the heading from that block's own `.n` and
`<h2>` — already translated by `applyI18n`, so the label is the builder's word
in the reader's language rather than a second string to keep in step. Reorder a
builder block, renumber one, rename one, and this tab follows with no edit
anywhere. `OPT_SCOPE_OF` — which section is which block's bulk form — is the
only hand-written half, and it is touched only when an axis is added or
removed. `check_parity.mjs` asserts it, and **scrambles the sections first**:
the markup is authored in the right order, so reading it as it stands would
pass just as well on a page where nothing orders anything. Verified to bite:
an `orderOptScope` that returns early reddens it, reporting the scrambled
sequence with every heading empty.

The same argument one level down. The `.opt` row is one function
(`modRow`) with the trailing control as its parameter — the drain for the
builder, the pool/req segs for the optimizer — and the segs are one function
(`oseg`) that six lists call. It was two copies of the row with
`// The picker's .opt row markup verbatim` written over the second, which is a
comment that stops being true in silence, and it did: the optimizer's copy
never grew the builder's **stance filter**, so every melee weapon offered its
stances as MAIN-slot marks — a build nobody can hold.

### What the scope still cannot reach

Searching the **stance slot** itself. A stance decides what the weapon swings
(Crushing Ruin against Shattering Storm is 1,275 against 1,162 DPS on the same
Magistar in the same mode), so it is a real axis and a large one — it wants the
treatment the exilus slot has, in `optimizer/` as well as on the page. Today
the builder has the slot and the optimizer has nothing, which is the one place
these two tabs still disagree about what a build is.

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

## …AND SO MUST THE BUILD (2026-08-16)

The section above is about the FIGHT, and it fixed the fight. The build had the
same disease one layer out, and it took three years of calendar and four
separate patches to see it as one thing.

A build travels through eight representations — live page state, a stored
preset, a simulate request, an optimize scope, a ranked row, a board
submission, a board record, a share link — and each held a hand-written answer
to "which axes are there". A missing axis and a defaulted axis are the same
absence on the wire, so a producer that had never heard of an axis was
indistinguishable from one that meant the default, and no consumer could
complain. `mode` was lost from the board submission (2026-08-09), `valence` from
the worker's table (2026-08-14), both from the share tuple (2026-08-15), and
`valence` from the optimizer's "+ add" (2026-08-16).

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
optimizer rides along, including ones nobody has invented yet — and `runs`
becomes the final round's, so the row's precision is the replay's precision.
POST it and you get the row's number, with no assembly at any caller.

**And the ranking reports the simulator.** Each row is re-run through
`/api/simulate` and the KPM on screen is what came back, marked ✓. The search's
own figure keeps exactly one job — ordering the list — and the two are compared
at 4σ of their combined standard errors (`kill_progress_se` on the row,
`score_se` from the sim), so a divergence is arithmetic rather than a tolerance
somebody chose. A row that fails it is marked `≠`.

That comparison is the durable part. Every earlier guard was a LIST of axes,
and a list has to be maintained by whoever adds the fifth; this one is an
ANSWER that has to match, so it covers axes that do not exist yet.
`scripts/check_opt_replay.mjs` asserts it in CI and is verified to bite —
reinstating the bug moves the Nukor from 0.6514 to 0.2118.
`engine::builds::BUILD_AXES` plus `scripts/check_build_axes.mjs` cover what an
answer cannot reach: a share link nobody has clicked, a board record nobody has
submitted.

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

## EVERY AXIS SAYS HOW MANY OF ITS SLOTS A BUILD FILLS (owner, 2026-08-29)

Every axis of a search is one shape: **N slots, an option set, and a range**
saying how many of the slots a searched build must fill. The mods axis is 8
slots and the range is a number 0–8; every other axis is ONE slot and the range
is 0–0, 0–1 or 1–1. They are the same question, so the page asks it the same
way — one row, after each axis's list, because a range is a **conclusion** of
the marking and means nothing before it.

It was three different ways of saying one thing:

| axis | slots | how it said it, before |
|---|---|---|
| mode | 1 | fixed at 1–1, and said nothing |
| mods | 8 | a numeric range on screen (`build_min`/`build_size`) |
| exilus | 1 | 0–1 reachable, but only by pooling a `none` row nothing pointed at |
| arcane seat | 1 each | **0–1 not reachable at all** |
| evolution tier | 1 each | **0–1 not reachable at all** |
| valence | 1 | fixed at 1–1, and said nothing |

…so on three of the four adjustable axes, which of 0–0 / 1–1 you got was
decided by whether you had marked anything, and the middle answer did not
exist.

**IT IS DERIVED FIRST AND ADJUSTED SECOND**, which is the whole of what makes
this safe. The derived answer is exactly what the scope did before the control
existed — nothing marked is 0–0, a mark is 1–1 — so **no existing scope grows**.
That matters most on the arcane seats, where the empty seat was ruled out on
evidence: *"an arcane slot costs nothing — no capacity, no Forma — so leaving
it empty can never beat filling it with something that helps, and marking a
candidate IS the statement that the slot should be filled"* (user,
2026-08-01). That decision was against the empty seat being a **default**;
asking for it out loud is a different thing, and the exilus slot could always
do it. `an_arcane_seat_marked_none_is_not_a_default` is that decision, kept as
an assertion.

**THE EMPTY CHOICE IS A MARK LIKE ANY OTHER** — `none` on the exilus slot and
on an evolution tier, `none:<pool>` on an arcane seat, which names its seat
because a weapon can hold two and the marks are one flat map. So the range is a
**view over the option set** rather than a second thing to store: it travels in
the search preset, in the request and through the round trip with no field of
its own anywhere, and the server reads it as one more option in the list.

**A PIN IS NOT A RANGE.** A pinned candidate settles its slot at 1–1 and the
row says so with its inputs disabled, rather than showing a number the search
will not honour. `slotRange` asks for a real pin FIRST so a stale empty mark
cannot outrank one.

**AND 0–0 KEEPS THE CANDIDATES.** Going down to "searched empty" and back must
not cost the reader what they marked. That is what forced the evolution
LADDER to key on the range rather than on the marks: a 0–0 tier still has
marks, and counting them opened the tier above over sets whose every rung
`ladder_prefix` then truncates — the marks up there would price nothing while
the scope said otherwise. `evoFillsRung` is the question the ladder actually
means. 0–1 **does** open the tier above: half its sets carry the rung, and the
other half being truncated is the ladder working.

**IT FOUND A DISAGREEMENT BETWEEN THE ESTIMATE AND THE SEARCH.**
`arcaneOptionsIn` counted `marked + 1` — the empty seat, always — while
`parse_optimize` has dropped it beside marked candidates since 2026-08-01. So
the candidate count over-reported by one factor per arcane seat on every scope
with an arcane in it. Both sides read the range now.

`scripts/check_slot_ranges.mjs` walks all three states on all four axes and
asserts them ON THE WIRE, because a range that draws correctly and sends
nothing looks exactly like a working control. Verified to bite: a `setSlotRange`
that returns early reddens 8 of its 18.

### All six axes, and what the count comes to (owner, 2026-08-29)

| axis | slots | range | adjustable |
|---|---|---|---|
| mode | 1 | 1–1 | no — a build is played exactly one way |
| mods | 8 | 0–8 | yes |
| exilus | 1 | 0–0 / 0–1 / 1–1 | yes |
| arcane seat | 1 each | 0–0 / 0–1 / 1–1 | yes |
| evolution tier | 1 each | 0–0 / 0–1 / 1–1 | yes |
| valence | 1 | 1–1 | no — the weapon always has one progenitor element |

**THE TWO FIXED ONES CARRY THE ROW ANYWAY**, read-only. An axis that simply
omitted it would be the axis the rule forgot, which is the shape this whole
change is about; and "1–1, and here is why" is a fact worth stating once rather
than a gap the reader has to explain to themselves.

**THE COUNT IS THE PRODUCT OF ALL SIX**, and it was not. Completing the model
found the estimate wrong in both directions at once:

- `arcaneOptionsIn` counted `marked + 1` — the empty seat, always — while
  `parse_optimize` has dropped it beside marked candidates since 2026-08-01.
  **Over**-reported by a factor per arcane seat.
- `modes` and `valence` were not factors at all, though the server's variant
  table is `modes × evo_sets × valences`. Pooling a second mode genuinely
  doubles the search and the panel said nothing. **Under**-reported by exactly
  the two axes that had no range row — the same blind spot, seen from the
  other side.

**AND THE MODS CEILING MAY BE 0.** Every other axis can be set to "search this
slot empty, and keep the marks"; this one was clamped to 1, so the only way to
reach the bare weapon was to unmark everything — which costs the reader
precisely what 0–0 exists to protect. A ceiling of 0 OUTRANKS the derived floor,
in three places that all had to agree: `min_slots`, the guard that refuses
pooled mods with no slot to reserve, and the page's own `poolStarved`. Without
that the marks say "use these" and the ceiling says "not this time", the two
contradict, and `SubsetSpace::new(1, 0)` enumerates nothing — a legal request
reported as "no legal builds in this scope".

**ONE ASYMMETRY IS DELIBERATE AND IS NOT AN OVERSIGHT.** On a single-slot axis
the boxes show the EFFECTIVE range and lock when a pin forces it, because the
typed answer and the derived one live in the same three states. On the mods
axis the boxes show what YOU typed and the effective floor is a sentence beside
them, because there they are different numbers in a 0..8 space and both matter:
a derived floor of 2 does not stop you wanting 3. Stating it beside the boxes
is the resolution, not a second control.

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

### The floor starts at 0, and it closes the list (owner, 2026-08-29)

**IT IS A CONCLUSION, NOT A FILTER AND NOT A SUMMARY**, and that is what
decides where it goes: how full a build must be only means anything once the
required and the pooled have been chosen, so it comes AFTER the marking. It
took two tries to land — first it shared a flex row with the mod search box as
a column-stacked label (four lines tall, the filter pushed to the bottom of it,
reading as a setting *on the filter*), then it joined the marks summary, which
is still above the list and so still ahead of the act it concludes.

It closes the mod list now, under a rule, and before the Exilus block — because
the two numbers count the **8 main slots** and the exilus slot is the +1,
counted separately.

```
  … the mod list, where you mark …
  ────────────────────────────────────────────────────────────
  Mods / build [0] – [8]   actually 2–8: 1 required, plus at least one pooled
  EXILUS
```

**THE SENTENCE BESIDE IT EXISTS BECAUSE THE CONTROL WAS LYING.** The floor the search
uses is the larger of what you typed and what the marks imply
(`min_slots = derived_min.max(build_min)`), so a box reading 0 could sit over a
search that never looks below 3. It is stated only when the two DIFFER — a line
repeating the two numbers beside it distinguishes nothing.

**AND THE FLOOR STARTS AT 0** rather than at 1, which is the change that makes
the axis consistent with every other one. "Nothing marked" means the EMPTY
option everywhere else — an unmarked exilus slot stays empty, an unmarked
arcane seat searches no arcane, an unmarked evolution tier installs nothing —
and the mods axis alone answered it with *"no legal builds in this scope"*.
`updateOptEstimate` has carried the sentence *"an empty scope = the bare
weapon, still a legal search"* since it was written, and `build_min.clamp(1, 8)`
made it false.

It costs nothing anywhere else, by arithmetic: the moment anything is marked
`derived_min` is at least 1 and wins, so 0 and 1 differ in exactly that one
case. `an_empty_scope_searches_the_bare_weapon` pins both halves — the empty
scope enumerates one candidate, and the derived floor still wins over a typed 0
— and is verified to bite: restoring the clamp reddens it at `left: 1 right: 0`.

**THE OTHER AXES DO NOT GET A BOX OF THEIR OWN.** They are 0–1 by nature — a
slot holds one thing or nothing — and which of those it is, is already said by
whether anything is marked. A 0–1 control beside them would be a second control
for a fact the marks already state, which is the same shape as the CPU-threads
box that just left. The consistency is reached by lowering this floor, not by
adding boxes elsewhere.

**IT SURFACED A BUG OLDER THAN ITSELF.** `switchWeapon` resets the scope and
its object never carried `min` — the one field it forgot, since the range
landed on 2026-08-03. `Math.max(derived, undefined)` is NaN, so
`for (k = NaN; k <= size; k++)` never runs: on any weapon with no saved search,
the scope reported itself impossible ("more required (0) than slots (8)") and
Run stayed disabled until some control was touched. `check_build_size` could
not see it, because its first act was to type a floor.

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

The count is the **topbar's compute share** and nothing else (owner,
2026-08-29): `woptWorkerCount()` is `poolSize()`, a percentage of the cores the
machine reports. It used to be the search preset's own `CPU threads` box with
that share as its default — see §"…and so did CPU threads" for why a
per-search override of a global setting was the wrong shape, particularly on
the one thing here most able to cook a phone.

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

## FILLING A SCOPE IS THE UNSOLVED HALF (2026-08-29, not built)

A search preset is a **way of looking for a build on this weapon** — the
pool/req marks on every axis plus the funnel that spends them — and it is per
weapon by the same rule a build is (`wfsim-presets-<weapon>-optimizer`). That
model is right and is not what is wrong with the optimizer.

What is wrong is that the only ways to fill one are the mod list's sort, its
polarity filter and its search box, all of which are *"let me scroll less"*.
Marking a scope is still one click per card, and **a new weapon starts from
nothing** — which is exactly the moment a player has the least idea what to
mark. Two ways in, and each has a trap that is not obvious (owner, 2026-08-29).

### ① Import a ranked build's cards into the pool

`BOARD[weaponId]` is already on the page — every stored row carries a complete
build (mods, arcane, evolutions, valence, exilus, riven) — so pouring a
weapon's leading rows into `opt.mods` as **pool** marks costs no server work at
all. The appeal is real: those cards have been scored, so a search starts from
a set somebody already proved is worth something.

**THE TRAP IS THAT A NEW WEAPON HAS NO ROWS**, and a new weapon is the case
this exists for. Importing *this* weapon's board only helps the weapons that
least need help. The form that answers the actual complaint is **CROSS-WEAPON**:
take the leading rows of the other weapons sharing this one's `mod_pools`, and
filter what they carry through this weapon's own pool (`pool_for_weapon` /
`buildPool()`) on the way in.

That does not weaken **NOTHING CROSSES BETWEEN WEAPONS** (AGENTS.md), and the
distinction is the whole reason it is allowed: what crosses is a **SCOPE** —
a set of cards worth searching — never a BUILD. A build is a statement about
one weapon and stays one; "these are the mods people win with on rifles" is a
statement about the POOL.

Two decisions it still needs:

- **Pool, never req.** `req` pins a slot; pinning eight slots from a table is
  not a search, it is a copy. An import may only ever widen what is searched.
- **Which axes.** A board row is a build on four axes. Importing only `mods` is
  the honest minimum; arcanes and evolutions are cheap to add and valence is
  not (a progenitor element is a property of the COPY a player owns).

### ② Mark every card that does one thing

*"Click 多重 and every card carrying a multishot bonus joins the pool."*

`mod_category` (webapi `mods_json`) is **not** this and must not be stretched
into it. It is **single-valued and first-match** — element → crit → status →
handling → damage — so a dual-stat card lands in exactly one bucket, and there
is no multishot class at all. What this needs is a **multi-valued tag set**: a
card is *multishot* and *status* at once.

**IT IS DERIVED IN THE ENGINE, NOT LISTED IN THE PAGE.** The tags come off the
`ModEffect` variants a card actually carries and ride `/api/meta` beside
`category`, so a mod added tomorrow tags itself and a hand list cannot go
stale. A table of mod ids in `app.js` would be wrong within a week and nothing
would report it — the same failure `pool_for_weapon` was written to end (see
`applyWeaponInner`).

### Both of them have to show the bill

A tag button can put thirty cards in a pool in one click, and the candidate
count is combinatorial in pool size. `updateOptEstimate` already computes it;
a batch control that does not put that number next to itself is a button that
quietly makes the search unfinishable.
