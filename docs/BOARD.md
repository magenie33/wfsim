# The board

The official leaderboard: **builds players submit, scored here**.

One sentence carries the whole design — **a submission is a BUILD and never a
number**. Everything else follows from it:

- a forged score is impossible, because no score is ever accepted;
- a change RE-SCORES stored builds instead of invalidating them, and nobody is
  ever asked to resubmit — a CODE change rescores everything, and a DATA change
  rescores only the rows that READ the file that moved. Adding a
  weapon costs 0 of 885 rows; correcting one mod costs the rows carrying it.
  See `engine::data_fingerprint`;
- **THE STORE IS A LIBRARY OF BUILDS AND EVERY RULER CROSSES THE WHOLE OF IT**. A submission carries no score, so the ruler it happened to be
  measured under was never a property of the record — it was a gate, and the
  gate was expensive: of 914 distinct builds players had sent, only 46 had ever
  been scored on more than one board. ANY fight can upload now, and a new ruler
  is scored from the library the day it lands rather than waiting for anyone to
  resubmit. Measured on the first run after it landed: group_clear went from 106
  published rows to 551, single_target_no_aim from 113 to 498;
- **…AND EVERY MODE OF THE WEAPON IS SCORED FROM IT.** The mode a build was
  tuned for was never a property of the record either — mods are equipped on the
  WEAPON and a mode is how it is fired, so nothing about a build can become
  illegal by being played differently. One submission is now one row per
  sustainable mode: a Ballistica Prime build sent from its Incarnon cycle also
  answers `base`, `alternate` and `alternate_cycle`, and a melee build answers
  all seven. What it carries that pays nothing in a mode costs a low row, which
  the floor and the per-mode dedup drop — so the fan-out only ever ADDS the rows
  where a build happens to be good somewhere its submitter never tried it.
  `transformed` and its kind are still refused: a gauge you must fill and run
  dry is not a way to play for three hundred seconds;
- **A ROW IS SCREENED BEFORE IT IS MEASURED.** Most of what the mode fan-out
  adds is a build in a mode nobody tuned it for, and paying the ruler's full
  1000 runs to rediscover that every hour is the cost that grows fastest. So a
  row is probed at 100 runs first, and the full measurement is skipped only
  where it reads under a QUARTER of its group's leader — `PROBE_MARGIN x FLOOR`,
  a 2x margin on top of a floor that is itself half the leader. A probe is the
  same fight at a tenth of the precision, so its error is about 3x larger and
  4x is far outside it. It is the same two-decision structure the riven search
  already uses to cost 2.6x a plain row instead of 16x: **search cheaply, then
  measure the winner.** A screened row is RECORDED (`probe: true`) and never
  published and never reused as a score — a probe is not a measurement — so the
  board still holds nothing but full ones. Riven rows are not screened: their
  riven is chosen by the search that follows, so a probe before it would measure
  the build without the thing it is built around;
- **EVERY FINALIST OF A SEARCH IS UPLOADED**, not just the build somebody ran in
  the simulator. A path to the store that runs off a simulator run alone takes
  one build at a time, so a search ranking twenty sends none of them and the
  strongest thing this app produces reaches the board only when a player copies
  a row into the builder by hand and runs it again. There is no cap: the
  finalist count is the searcher's own setting, they are all real builds, and
  the store is keyed by identity, so twenty submissions of which twelve are
  already held collapse onto twelve rows. **Nothing is pre-filtered on the
  page, from any path**: how full a build must be is the searcher's setting
  too, so a seven-mod scope produces seven-mod winners — and those are refused
  by `/api/board/check`, which IS `validate_for_board` rather than a copy of
  it, because a second implementation is a second answer. The SIMULATOR's path
  kept such a copy — its own count of mods, tiers and seats, and a capacity
  floor that ignored the capacity a stance hands back — so a melee build that
  fits could be refused by the page and never reach the door. There is one
  rule, it is the engine's, and the page ASKS it (`boardDoor`). The panel reports the run
  in aggregate ("7 of 10 uploaded · 3 × needs 8 mods"), since twenty rows off
  one search differ in their mods and not in why the board would not take them;
- every row is reproducible by anyone with the repo, since the score was
  computed by the engine that ships to their browser under the benchmark's own
  pinned seed. Measured 2026-08-04: wasm and native agree to the last digit
  (`0.9647804061510868` both ways).

## The pieces

| what | where | who runs it |
| --- | --- | --- |
| the ruler | `data/benchmarks/*.yaml` | — |
| the board | `boards/*.yaml` | generated, committed |
| what the page reads | `site/board.json` | fetched at runtime, not compiled in |
| consent + submit | `web/src/static/app.js` (`offerBoardSubmit`) | the player's browser |
| the submissions | a Cloudflare KV namespace (binding `SUBMISSIONS`) | written by the endpoint |
| the deploy | `wrangler.jsonc` | `scripts/deploy.sh`, from a git push |
| the endpoint | `worker/index.js` | the Cloudflare Worker, same origin |

**THE ENDPOINT STORES THE WHOLE BUILD, and it has failed to twice.** `mode` was
sent by the page and never written down; `valence` was, and seven
Kuva Nukor submissions were refused on every scoring run since they arrived —
"Kuva Nukor has no Valence element" — while the panel had told each submitter
"sent". `/api/board/check` cannot catch this one: it
validates the payload the page is about to send, which DID carry the element,
and the field was lost afterwards. Both times the identity hash was wrong the
same way too, so two builds differing only in the dropped axis collapsed onto
one key and the second overwrote the first. `scripts/check_board_submit.mjs`
now asserts both properties against every axis, derived from a real payload
rather than listed — the stranded records themselves are unrecoverable, since
what they are missing was never stored.
| the scorer | `cli/src/bin/wfsim-board.rs` | the scheduled job |
| the automation | `.github/workflows/board.yml` | GitHub Actions |

**The board is in the repo, not in a database.** That is what turns
"reproducible" from a claim into a property, and it means the LOCAL build has
the board with no network at all — `data/` is embedded at compile time. Only
submitting needs a network.

**The endpoint is on the site's own origin** (`wfsim.app/api/board/submit`). A
separate api domain would be a second DNS name and a second thing that can be
blocked, which is the failure the same-origin art rule was written about.

### Why the page FETCHES the board

Everything else in `data/` is embedded into the wasm at compile time. The board
is the one piece that changes without a release — three times an hour, if people
are playing
— and compiling it in made every update cost a full site rebuild: install
wasm-bindgen, fetch 300 images, recompile, to change a few numbers. It is a
small file on the same origin instead, written by the scoring job beside the
canonical yaml, and `build_site_app.py` regenerates it so a LOCAL build holds
the same board.

An unreachable or absent `board.json` is an EMPTY board, never an error: before
the first submissions there is nothing to show, and the page has to render that
state anyway.

## When it updates

| trigger | scope | cost |
| --- | --- | --- |
| `:00`, `:20`, `:40` | score what is new | seconds; no commit when nothing changed |
| a push touching `engine/`, `webapi/` or the scorer | **everything** — the CODE fingerprint moved | ~2h20m measured 2026-08-25, 32 ways at once |
| a push touching `data/` | only the rows that READ the file that moved | a new weapon costs **0 of 885 rows**; one mod costs the rows carrying it |
| a change to a benchmark's terms | everything on THAT ruler | the builds carry over; only the numbers change |
| Actions → board → Run workflow, **full = true** | everything, whatever the fingerprint says | the same ~2h20m |

**THE FULL RESCORE GOT MUCH BIGGER WHEN THE LIBRARY LANDED, and that is the
price of it**: 2,223 builds crossed with three rulers rather than 967 rows each
scored once. Measured on the two runs that completed on 2026-08-25: 137 and 138
minutes. The per-row fingerprint above is what keeps that off the ordinary day —
a data change costs the rows that read it and nothing else — but a CODE change
still pays in full.

**A DAY OF CODE PUSHES IS A DAY WITH NO BOARD.** The workflow keeps one pending
run and cancels the rest, so a rescore that takes 2h20m is cancelled by any push
inside that window: on 2026-08-25, 18 of 20 runs were cancelled and two
completed. That is the concurrency rule working as designed and it is worth
knowing before wondering why the board has not moved. If a rescore matters more
than the next push, stop pushing for two hours or run it by hand from Actions.

**The clock is a best effort, not a promise.** GitHub delays scheduled runs
under load and says so, and this repo's own history is the evidence: while the
job was set to `:17` its commits landed at `:33`–`:35`. Three slots 20 minutes
apart is the answer to that — a submission waits ~20 minutes rather than an
hour, and a slipped run is covered by the next one instead of costing you the
whole hour. If you want a result NOW, Actions → board → Run workflow.

The second row is the point: the maintainer's ordinary work — fixing a bug,
correcting a number, changing the benchmark to 480 s — IS the trigger. There is
no dependency analysis deciding which rows were affected, because that question
can be answered wrong silently.

### What decides "new" — the ENGINE FINGERPRINT

A FULL rescore on both triggers is what the fingerprint exists to prevent. It
is affordable at 100 runs a build and is not at 1000: scoring goes from ~7
minutes to **1h07m**, the schedule fires every 20, and `concurrency` keeps only
ONE pending run — so every queued run is cancelled by the next and the board
stops updating altogether. Five `cancelled` in a row is what that looks like.

A score is a pure function of `(build, the ruler's terms, this code, this
data)`. So the board records the hash of everything on that list which is not
the build — `engine/`, `webapi/`, `cli/`, and `data/` minus the boards
themselves — as `engine:` at the top of the file. Next run:

- **fingerprint unchanged** → every stored score is not merely probably still
  right, it is the number this run would compute. It is reused, and only
  submissions with no row cost anything. Measured: 0.05 s to reproduce a board
  byte-for-byte, 1.5 s when one new build arrived.
- **fingerprint moved** → the run does not assume every row is stale. It
  PROVES it, one way or the other — see below.

### When the code moved: prove whether any NUMBER did

The fingerprint is a hash of three whole trees, so any edit to them declares
every stored score stale. Most edits cannot move a number: a comment, a test, a
validation rule, a field only the page reads. Measured on 2026-09-03, the cost
of assuming otherwise was **7,808 minutes of work across 128 shards, median
shard 55 minutes and worst 192, four hours of wall clock** — against a schedule
firing every twenty, so every successor was discarded. Eleven engine commits in
one morning produced no completed run for ten hours.

So the workflow measures instead of guessing. `scripts/board_sample.py` takes
**one published row per weapon** — the weapon's best, which is the row every
other row in its group is measured against — and `wfsim-board --verify`
reproduces them under the new code and compares each with what the board says.

- **every sampled row identical** → the code is score-equivalent. The published
  numbers are what this code computes, not merely what some older code did, so
  the scorers are given `--code-verified` and reuse them across the fingerprint
  difference. Cost: about two minutes of scoring plus a cached build.
- **any row different, or too few compared to mean anything** → full rescore,
  which is what would have happened anyway.

**Exact equality**, not a tolerance: a score is a pure function and an `f64`
round-trips through the yaml, so "close enough" would be a number nobody can
defend. **Stratified by weapon**, because a change that moves a number moves it
for some weapon, and a uniform sample of a 7,000-row board would miss a
melee-only mechanic outright. A **floor of 100 rows compared** across the
shards, because a verification that cannot fail is worse than none.

What the sample can still miss is a change that moves only SOME builds of one
weapon. Nothing bounds that but measuring everything, so **a full rescore runs
weekly regardless** — `0 0 * * 1`, Monday 00:00 UTC, which is Warframe's own
weekly reset (*"Other content uses a different server-side reset timer that
resets only every Monday at 0:00 UTC"*, wiki `Reset`).

TIME IS NOT AN INPUT, which is why there is no cooldown and never will be
(asked and answered,). An untouched row is valid forever; a row
whose engine moved is wrong immediately, not in an hour. A cooldown would be
both too slow and too fast at once.

**The manual button is the escape hatch.** Actions → board → Run workflow with
`full` ticked ignores the fingerprint and rescores every row — for when
something outside the hash changed, or when you simply want to see it done.

### Why it is sharded

Every row is an independent fight, so the scoring splits by submission index
across eight jobs, each writing only the scores it computed; a merge job
validates, deduplicates, ranks and writes, simulating nothing. Verified before
it shipped: 24 submissions through 8 shards reproduced every published score to
1e-9, and the merge ran in 0.064 s.

**A score file says which board it is.** A shard's key is `identity#mode` and
carries no ruler, so two boards scoring one build produce the SAME key with
different numbers — and the merge job is handed ONE directory holding every
ruler's shards. Merging them published one ruler's score under the other's name:
the Torid's aimed **28.44229348067104** kpm sat at the top of the NO-AIM board,
digit for digit, where that build actually scores **0.170**. Ten
Torid rows and much of the no-aim top were the aimed board's numbers.

It read as a scenario leak and was not one — every score was computed under its
own ruler's terms, then overwritten on the way out. What made it selective is
that the merged number also WINS over the board's own history: `--reuse` fills
only where `--scores` left a hole, so exactly the rows the OTHER ruler happened
to rescore that run were the ones that went wrong. `--emit-scores` now writes
`{"benchmark": …, "scores": {…}}` and `load_scores` refuses a file that names a
different board;
`a_score_file_belongs_to_one_board_and_another_boards_is_refused` asserts it in
both directions, since which ruler wins is decided by a sort over file names.

The generated files are NEVER rebased. There is no sense in which two versions
of a computed board each hold something worth keeping, so a three-way merge can
only produce a conflict — which is exactly what threw away 83 minutes of
completed scoring on 2026-08-11. The run that just scored takes whatever base is
current and writes its numbers on top.

## Consent

Asked ONCE, inline, the first time a run finishes under the official scenario —
never on load, never as a native dialog (they are blocked in this project), and
never blocking the result. Running your own scenario neither asks nor sends.

What travels: the weapon, its mods, evolutions and arcanes, and which
benchmark. No account, no identifier, no riven, none of the names you chose,
and no score. `scripts/check_official.mjs` asserts on the WIRE that nothing
leaves before consent and nothing leaves after declining.

The endpoint stores no IP, no token and no timestamp finer than the day.

## One representative per build

A board row is keyed by what makes it a different FIGHT, and mod ORDER is part
of that — mods combine ELEMENTS in the order they are listed. Measured on the
Torid, six mods:

| spelling | pairs to | DPS |
| --- | --- | --- |
| Heat, Cold, Toxin, Electric | Blast + Corrosive | **12,424** |
| Heat, Toxin, Cold, Electric | Gas + Magnetic | **46,583** |

The identity SORTED the mods for a day, on the strength of one measurement that
happened to reorder mods whose pairing did not change. Two different fights
collapsed into one row, and the score published was whichever pairing the sort
produced — belonging to neither submitter.

**And the MODE is the other half of it**. A Torid through its
Incarnon cycle and a Torid that never transmutes are two entrants, so the key is
`identity(build)#mode` — which the SCORER has always done and the ENDPOINT did
not. The worker hashed weapon+mods+evolutions+arcanes and never stored `mode` at
all, so two modes of one build overwrote each other in storage and every record
reached the scorer mode-less, where the migration fallback turned it into "the
cycle where there is one".

That is the whole reason the published boards read 306 `cycle` rows, 158 `base`
ones and not a single weapon with both: every Incarnon weapon cycle, every other
weapon base. It looked like a fact about how people play. It was one line.

Old records stay readable — the fallback is what they are for — and
`wfsim-board` now prints how many arrived without a mode, so the migration is
visible and ends at zero instead of being permanent.

Raw order is not the answer either: three elementals in slots 1-3, the same
three in 4-6, the same three interleaved with the rest, and the non-elementals
reshuffled all score an identical 146,707.582. Only the elementals' order
**relative to each other** is the build.

So `builds::canonical_mods` gives every build ONE representative: elementals
LAST in the order they arrived, everything else ahead of them by biggest drain
then by DE's own English name. The endpoint stores what was
submitted verbatim — it has no mod pool and cannot tell an elemental mod from
any other — and the scorer collapses spellings after `validate` has canonicalised
them.

**Rows submitted while the endpoint sorted on the way in are unrecoverable**:
the order those players built is gone, and they re-score as "elements in
alphabetical order" — a legal build, and probably not theirs. Submissions keep
what was placed.

## The pipeline, stated once

Every step below is a rule, not a description — each one is somewhere a wrong
answer could be published.

1. **One representative per build.** `builds::canonical_mods` — elementals last
   in the order that pairs them, everything else ahead by biggest drain then by
   DE's English name. Substantively identical builds are one row.
2. **We collect builds. We compute the score.** No submission carries a number
   and none would be believed.
3. **We validate legality ourselves**, including Forma: pool, families, eight
   slots, capacity.
4. **Forma, in priority order**. The order is the rule, not
   a preference — 2 before 3 means the answer is never "spend one more Forma to
   leave more room":

   1. **reach max rank** — five polarizations on a rank-40 weapon, because that
      is what full mastery affinity takes. A floor, not a budget.
   2. **then as few Forma as possible to make it legal.** Umbra Forma only when
      refusing would invent a rule the game does not have; a weapon born with an
      Umbra polarity keeps it, and is never billed for it.
   3. **then as much spare capacity as possible** — every polarization bought
      anyway goes on the biggest mod still unpolarized. Three 16-drain mods on a
      rank-40 weapon: 24 drain and 56 spare, not 48 and 32, at the same cost.

5. **No Omni Forma.** `BENCHMARK_INVESTMENT` leaves it off: a board build should
   be one an ordinary player can reach.
6. **Published IN THE BENCHMARK'S OWN METRIC.** `score` off the wire is kill
   PROGRESS — kills plus the depleted fraction of the current target — over the
   whole engagement, and the benchmark says `metric: kpm`. Publishing the raw
   figure under a "kill rate" label overstated every row by the length of the
   fight: 55.26 on screen for a build that kills 11.05 a minute over the 300 s
   the ruler ran at the time (found 2026-08-04; the ruler is 180 s now, which
   changes the multiplier and not the bug). Ranking never noticed — it is a linear rescale — but a
   ranking is not what people read.
7. **Shown at four significant figures AND four decimals**,
   by `boards_data::format_score`. Four decimals is where two builds a player is
   choosing between stop tying; four significant figures is what keeps a small
   metric from publishing as `0.0001`. The RECORD keeps full precision — the
   yaml writes the shortest string that reads back identical, and the scorer
   puts the formatted one beside it as `shown` — so the page prints a string it
   did not compute and rows that tie on screen still rank underneath.

## Ammo on the board

The benchmark sets `infinite_ammo: true`, and that setting means **ammo pickups
are modelled** — the sim has no pickup entities, so ignoring the reserve is how
it stands in for them. Over 180 s with kills happening, a real player is being
resupplied; starving every weapon would measure who brought the biggest magazine
rather than who kills fastest.

It does not hand ammo to a weapon that cannot receive any. `reserve_is_infinite`
reads three facts, and two of them were one field until 2026-08-04:

| fact | where from | false for |
| --- | --- | --- |
| `has_reserve` | derived from `ammo_max` | sentinel weapons — no pool at all |
| `no_resupply` | the weapon's own YAML | *true* only for a ground Arch-Gun |
| `infinite_ammo` | the scenario | whatever the player set |

`!has_reserve \|\| (infinite_ammo && !no_resupply)`. So a ground Arch-Gun runs
on its real 400 rounds whatever the scenario says — it is "removed and can only
be called down again after a 5-minute cooldown" once they are gone. Ignoring
that scored it as though it fired for the whole engagement when it has about a
minute of ammo: 0.0436 against 0.0139 unmodded, a 3.1x overstatement measured on
the 300 s ruler of the day. Boar Prime scores identically either way, because it resupplies.

One term, no weapon named, right for the whole roster.

## No version numbers

A benchmark has an `id` and no `version`. There is one board
per benchmark, it is regenerated whole whenever anything upstream of it changes,
and what is deployed is always the current answer — so a version would mark a
distinction nobody could act on. Git holds the history of what the file said.

Changing a term therefore retires nothing. Every stored build is re-scored under
the new terms and keeps competing; whatever beats it displaces it. That is what
storing BUILDS rather than scores was always for — if a changed standard threw
the builds away, storing builds would have bought nothing.

`wfsim-board` still strips a trailing `_v<n>` when matching a record to a
benchmark. That is a MIGRATION SHIM and nothing else: records already in the
store name `single_target_v1`, and they are builds like any other.

The bill still reports what is SPENT, not what earned room: a build with fewer
mods than mastery has polarizations buys all five, and the last land on empty
slots.

## Rivens: a SHAPE, not an item

A riven was off the board until now, and the reason still holds: *"they are
personal random items, so a board that counted them would rank luck"*. What is on the board is not the item.

**A ROW HOLDS A SHAPE** — which stats it rolled, and which one is the malus.
Nothing else. It is a statement anybody can act on ("roll this weapon for these
stats"), where a roll is one person's luck, and it carries no free-text field a
player authors.

**AND THE SHAPE IS SCORED AT ITS OWN CEILING.** `rivens_data::perfect` searches
every corner of the roll band and keeps the best — which is the same rule that
scores every row at full Forma, every mod at max rank and every valence at the
roll's maximum: anything a player can eventually reach is not part of what a
row states. Two players who rolled the same stats submitted the same build.

**WHICH END IS "BEST" IS ASKED OF THE FIGHT, NEVER OF THE CARD.** DE's `+` and
`-` describe the STAT, not the build. A riven whose malus is critical chance is
a *bonus* on the three weapons whose Incarnon form pays "+2000% damage on
non-critical hits" — a Laetum Incarnon crit is worth ×2.2 where a non-crit is
worth `0.5×21 + 0.5×1 = 11` — and on an ordinary weapon the same malus wants to
be as shallow as it goes. A per-stat table could state neither case.

**WHERE IT SITS IS PART OF THE BUILD.** An elemental riven pairs with the
build's other elementals, so the record carries the bare `riven` at the riven's
own position in `mods`. A riven may bring TWO elements, which makes it an ATOM
in the pairing — adjacent, in its own order, unsplittable — and
`builds::canonical_mods_with` searches for a representative rather than
constructing one when an atom is present.

**THE RANKING IS ONE LIST; THE FLOOR IS NOT.** A riven build does not always
beat a plain one, so ranking them apart would publish a comparison the fight
does not make. But the floor's group gains riven-ness beside weapon and mode,
for the reason it has mode: a shared reference would let whichever is stronger
on this weapon decide what the other may show, and the rows that would vanish
are the plain ones — the builds most readers can actually make. The board page
carries three views for the same reason: **all builds / no riven / riven only**,
deciding which subset each weapon's shown row is drawn from.

**TAKING A RIVEN ROW GIVES YOU THE RIVEN.** The record names no item, so the
page creates one, named after the shape so taking the same row twice reuses it.

**WHAT IT COSTS.** Sixteen corners per riven row, probed at 60 runs to choose
and then measured once at the ruler's own count — about 2.6× a plain row rather
than 16×. The chosen rolls travel beside the score and reuse on the same
fingerprint, because they *are* its argmax.

## What is not on the board

*(The exilus slot is on the board — see below.)*

## What a row has to be

A ruler that wants a complete build wants exactly this, and the four rows are
the whole rule:

| | |
| --- | --- |
| the 8 main slots | **FULL** |
| every arcane seat | **FULL** |
| every evolution tier | **FULL** |
| the stance, the exilus | **OPTIONAL**, each on its own |

Full where the game gives no reason to leave a slot empty, optional where it
does. `validate_for_board_with` is the rule and
`the_entry_standard_takes_a_full_build_with_or_without_the_optional_slots` is
the table above, asserted.

**A STANCE IS NOT A NINTH MOD AND NEITHER IS AN EXILUS.** Both have slots of
their own, so a full melee build is TEN mods against nine planned slots — and
counting either among the eight refuses the very builds a board exists to rank.
The stance rides inside `mods` because a stance mod is legal in the stance slot
and nowhere else; the exilus needs `exilus` as its own key, because an
exilus-eligible mod is legal in a main slot too and a flat list cannot say which
one came out of which.

## The exilus slot is OPTIONAL

A row MAY wear an exilus mod, and a row without one is not a lesser build. Both
sit on the same board and the better number wins.

It was EXCLUDED from 2026-08-04, on the reasoning that "exilus mods are handling
and mobility with no single-target damage model". That is true of most of the
pool and false of the part that decides a fight: `vile_precision` is **−36% fire
rate**, which takes an Ignis Wraith from **11.9694 to 9.3737** on the group
ruler — a real 22% that the board could not see. Beam range is exilus too
(`sinister_reach`, `ruinous_extension`, `galvanized_acceleration`) and IS
modelled, though measurement found it does not bind on the current rulers: the
same Ignis scores 11.9694 with and without Sinister Reach. That is a finding
rather than a reason to keep the slot out — it is now something the board can
answer instead of something the rules assumed.

**Not `full`.** Requiring an exilus would force a choice worth nothing on most
weapons and publish whichever mod the dice favoured, which is what the quick
calc's `tied` marking exists to admit rather than to rank.

**It travels in a field of its own** (`exilus`), never as a ninth entry in
`mods`. An exilus-eligible mod is legal in a MAIN slot too, so a flat list
cannot say which one came out of the exilus slot — only the page has the slots.
For the same reason it is its own field in `ValidBuild`, in the worker's `AXES`,
in the board row, and in `builds::identity`: the last of those was found by
scoring two Atomos builds differing only in `ruinous_extension` and getting ONE
row back.

## How deep a board goes — the floor

A row is listed when it scores **at least half its group's leader**, where a
group is one weapon, in one mode, under one ruler. There is no count limit; a
group whose builds are genuinely close keeps all of them.

It replaces a COUNT — the top hundred per weapon and mode, itself raised from
ten on 2026-08-08. **The two bound different things.** A count bounds how LONG
the list gets and says nothing about whether the hundredth row is worth reading:
on the board of 2026-08-19 the three groups that reached the cap had a hundredth
row at **18.6%, 25.9% and 25.4%** of their leader, so the list had stopped being
about builds anybody would pick long before the cap cut it.

**What it removes is not the cheap build.** That was the objection, and the
board refutes it — the rows below the line carry 8 of 8 mods exactly like the
rows above, and differ by taking the worse arcane (Merciless where Deadhead
wins) or by spending slots on mods this fight cannot pay: Magazine Extension,
Parallax Scope, Quick Reload, all of which docs/UNMODELLED.md already says are
worth nothing against one standing target. Of 86 groups, **three** have ever
held a row with no arcane at all, and in each it was the leader.

**It is mechanical, and that is the decision.** The seed is pinned and a score
reproduces to the last digit, so 50.3% and 49.5% are two different NUMBERS
rather than two estimates of one. A board whose rows are exact has no tie band
to grant, and a ruler separating two builds is what a ruler is for.

**Fifty is a cut line, not a measurement.** The pooled distribution of
score-as-a-fraction-of-leader has no knee to sit on — the largest gap anywhere
below 90% is 1.2 points — so the data cannot pick the number. What it can say is
that the number is not fragile: about **12 of 1274 rows per point**, so 45 or 55
would cost a few per cent rather than a shape. Against the sports that draw the
same kind of line (F1's 107% qualifying rule, cycling's 3-20% time limit) half
the leader is very generous, which is the intent — it marks where a build stops
being a DIFFERENT answer, not where it stops being the best one.

Measured over the three boards of 2026-08-19: 1274 rows become 740.

**NOTHING IS DESTROYED.** The floor is a property of the published board, not of
the store: every submission stays in KV and every board is regenerated whole, so
a row displaced by a new leader comes back the moment that leader is displaced
or an engine fix lowers it.

**AND IT IS SAID OUT LOUD, on both sides.** A build below the line is stored,
scored and then not listed, which from the submitter's side is indistinguishable
from a submission that was LOST — the exact silence that cost this board `mode`
 and `valence`. So `wfsim-board` reports how many fell
below it on every run, and the submission panel states the RULE rather than a
count of hidden rows: the rule is what makes an absence readable, and it is
checkable against the board on screen.

## It is a Worker, not Pages

That distinction is worth stating because it looks like it should not matter and
it decides everything. `wrangler.jsonc` deploys `site/` as a Worker's static
assets, and until the board there was no script at all. Two consequences:

- **Pages conventions do nothing here.** A `functions/` directory is ignored;
  the endpoint is a route inside `worker/index.js`.
- **`assets.run_worker_first` is not optional.** Assets match before the script
  runs, and `not_found_handling: single-page-application` answers every
  unmatched path with index.html — so an api path came back as the SPA with a
  200. A 200 carrying the wrong content type is the quietest failure a client
  can get, and the only reason it was caught quickly is that the page reports
  "could not reach the board" rather than assuming success.

## Setup, once (repo owner)

1. **KV namespace** — create one, then declare it in `wrangler.jsonc` as
   `SUBMISSIONS`:

   ```jsonc
   "kv_namespaces": [{ "binding": "SUBMISSIONS", "id": "<namespace id>" }]
   ```

   **A PUSH DOES NOT DEPLOY THE WORKER.** A push deploys `site/`; the endpoint
   is deployed by `npx wrangler deploy` and by nothing else, so the code in
   `worker/index.js` can be right while wfsim.app runs an older one — and the
   failure that shape produces is a legal build refused at the one hop neither
   the engine nor the page is watching. `check_board_submit.mjs` asks the
   DEPLOYED endpoint whether it takes `MAX_MODS` ids and refuses one more,
   without writing anything: the shape pass stops at the first bad field, so a
   payload with a full mod list and a deliberately malformed arcane answers
   "bad mods" from a stale worker and "bad arcanes" from a current one.

   **In the file, not in the dashboard.** wfsim.app is a WORKER (static assets),
   deployed by `npx wrangler deploy`, and a deploy REPLACES the worker's
   bindings with what the config declares — a namespace added through the
   dashboard is removed by the next push. The id is an identifier, not a
   secret; it grants nothing without a token, and Cloudflare's own docs commit
   it.

   Named for what it HOLDS, which is not the board: the board is the generated
   YAML in `boards/`, and this namespace holds the builds people
   sent, waiting to be scored. The binding was briefly called `BOARD`, which is
   a debugging trap — "the board is empty but the BOARD binding looks fine" is a
   sentence that sends you looking in the wrong place.
2. **Repo secrets** — `CF_ACCOUNT_ID`, `CF_SUBMISSIONS_NAMESPACE_ID`,
   `CF_API_TOKEN` (a token with *Workers KV Storage: Read*).

   The middle one is the SUBMISSIONS namespace's id, and it is named that way
   for the same reason the binding is: it points at the builds waiting to be
   scored, not at the board. Every name in this pipeline says what it holds —
   the board is a file in the repo and nothing in Cloudflare is called after it.

3. **The library database** — OPTIONAL, and everything above works without it.
   It is the first step of moving the library out of KV, and it is written so
   that it can land long before the database does: without the binding the
   mirror in `worker/index.js` is a no-op, and `check_board_submit.mjs` asserts
   that.

   ```sh
   npx wrangler d1 create wfsim-library
   npx wrangler d1 execute wfsim-library --remote --file worker/schema.sql
   ```

   …then declare it beside the KV namespace, in the file for the same reason:

   ```jsonc
   "d1_databases": [
     { "binding": "LIBRARY", "database_name": "wfsim-library",
       "database_id": "<id from the create above>" }
   ]
   ```

   KV STAYS THE AUTHORITY. The scorer and the pending count read KV and keep
   reading it; this writes a second copy that nothing looks at yet, which is
   what makes the step reversible — drop the binding and the system is exactly
   what it was. What it buys immediately is that the library becomes something
   you can ASK A QUESTION about (`wrangler d1 execute wfsim-library --remote
   --command "SELECT weapon, count(*) FROM builds GROUP BY weapon ORDER BY 2
   DESC LIMIT 20"`) and something you can DUMP, which KV is not.

The token only ever READS. What the board says is computed in the repo from
data in the repo; nothing secret decides a rank.

Until step 1 is done the endpoint answers 503 and the page says "could not
reach the board — nothing was sent", which is the honest state rather than a
silent failure.

## Sizing an AoE ruler — what was measured

A crowd ruler was proposed as an odd-sided grid, so it has an exact centre to
aim at. `cargo run --release --bin formation_cost` answers what each size costs
and — the deciding column — how many bodies the weapon actually REACHES.

Torid Incarnon (a chaining beam with a 2.3 m sphere: every spread mechanism the
engine has, live at once), 2 m spacing, 180 s, per 1000 runs:

| grid | placed | touched | 1000 runs |
|---|---|---|---|
| 3x3 | 9 | 8 | 15.9 s |
| 5x5 | 25 | 10 | 17.3 s |
| 7x7 | 49 | **11** | 22.3 s |
| 9x9 | 81 | **11** | 37.1 s |
| 11x11 | 121 | **11** | 60.8 s |
| 15x15 | 225 | **11** | 88.1 s |

**It saturates at 7x7.** A 15x15 costs four times as much to learn the same
eleven bodies — the chain has five hops and the sphere has one radius, so the
extra 176 enemies are never touched. And 49 is the largest odd square under
`formation::MAX_BODIES`, so the size the measurement points at needs no cap
change.

**Punch-through does NOT saturate**, which is the other half and is a ruler
DESIGN problem rather than a cost one. An infinite-body weapon reaches exactly
as deep as the grid — Lanka and Phantasma touch N bodies on an NxN, all the way
to 15 — and it is cheap (5.6 s at 15x15, because extra direct instances cost
almost nothing next to chains). So the grid's DEPTH becomes the score for that
family, without limit. A 15-deep perfect column is also an arrangement no player
will ever line up, which is the argument from the product's own promise rather
than from the clock.

A weapon with neither mechanic touches 1 body at every size and costs what it
always did, so the ruler is free for most of the roster.

> **THE RULER MOVED TO 3 m ON 2026-08-22** and the analysis below is kept as it
> was measured. It is evidence about SATURATION — which mechanism stops growing
> at which grid size — and that question is unchanged; what changed is the
> spacing, and why is the section above and MEASUREMENTS M54. The one conclusion here that the move overturns is the last section's:
> 1.5 m separates all three steps of a radius mod and 3 m does not, which is
> the cost the move pays on purpose.

### The spacing is the ruler's ANSWER, not its arrangement

**THE SPACING IS THE GROUP RULER'S ANSWER, NOT ITS ARRANGEMENT.** A 5 m Blast
sphere holds `π·25/spacing²` bodies — 35 at 1.5 m, 5 at 4 m — so the grid's
spacing decides the whole splash-versus-single-target ordering before a weapon
is read. Measured on one weapon with one build per element and everything else
pinned, Blast swings **71×** across 1.5–6 m while Heat is FLAT (58–72),
because Heat is a DoT on one body. It stands at **3 m**, the near edge of the
crossover band. IT COSTS SOMETHING REAL: 1.5 m was the only spacing that
separated all three steps of a radius mod (6/9/13 bodies) and 3 m does not.

AND 3 IS FITTED, NOT MEASURED — it was chosen to make the ORDERING match play,
which is weaker evidence than measuring the parameter. The quantity to measure
is not the spacing but what it sets: how many enemies one blast detonation
actually reaches in a real fight (~9 at 3 m, ~20 at 2 m, ~5 at 4 m).

A RULER'S PROSE QUOTES ITS OWN NUMBERS. The spacing is written three times —
the field, the ruler's NAME, and the rule sentence — and a test reads the RAW
yaml (the grid is expanded into 361 positions at load) and asserts the prose
quotes the field.

### At 1.5 m, and where each mechanism stops growing

Measured across four weapons, one per mechanism, 180 s, per 1000 runs:

| grid | placed | Torid (chain 2.3 m) | Grattler (blast 9 m) | Morgha alt (blast 12 m) | Phantasma (∞ punch) |
|---|---|---|---|---|---|
| 7x7 | 49 | **13** · 34.8 s | 43 · 1.6 s | 49 · 1.6 s | 7 · 2.8 s |
| 11x11 | 121 | **13** · 86.6 s | 65 · 2.2 s | 84 · 2.7 s | 11 · 4.2 s |
| 15x15 | 225 | **13** · 135.8 s | 73 · 2.6 s | 106 · 3.4 s | 15 · 5.6 s |
| 17x17 | 289 | **13** · 160.5 s | 75 · 2.7 s | **110** · 3.6 s | 17 · 6.4 s |
| 19x19 | 361 | **13** · 188.1 s | 77 · 2.8 s | **110** · 3.7 s | 19 · 7.0 s |

### …and then the size was made free

The table above is what a chain cost BEFORE `chain::Layout`. Nothing in this
arena moves — the shooter stands still, the formation stands still, and a body
that dies respawns where it was — so both of the O(N) scans inside `resolve`
were asking a constant question once per landing pellet: which body the sphere
catches, and which body is nearest to this one. Precomputed once per run
(O(N^2), ~0.13 s over 1000 runs on a 19x19), a hop becomes "the first unvisited
entry in a list that is already in order".

| grid | placed | touched | before | after |
|---|---|---|---|---|
| 7x7 | 49 | 13 | 34.8 s | **19.0 s** |
| 13x13 | 169 | 13 | 116.4 s | **19.7 s** |
| 17x17 | 289 | 13 | 160.5 s | **19.9 s** |
| 19x19 | 361 | 13 | 188.1 s | **20.4 s** |

**A 19x19 now costs what a 7x7 costs** — 20.4 s against 19.0 — so the grid's
size stopped being an argument at all. The answer is identical, not
approximate: `near` is sorted by (distance, index), which is exactly the scan's
"nearest, ties to the lowest index", and
`chain::tests::a_layout_answers_exactly_what_the_scan_does` asserts it instance
for instance over every seed of a grid, at three spacings, for both chain
shapes.

It is built PER RUN rather than held on `DummyParams`, and that is deliberate: it
was a field for an hour and a test caught the trap at once — widen
`beam.damage_radius_m` after the params are built and the cached layout is
silently stale, which is the two-declarations bug wearing a cache.

### Where it stops mattering — 19x19

With cost flat, the size is settled by SATURATION alone. Measured at 1.5 m out
to 23x23 (bodies touched · seconds per 1000 runs):

| grid | placed | Torid (chain) | Morgha alt (12 m) | Grattler (9 m) | Phantasma (∞ punch) |
|---|---|---|---|---|---|
| 7x7 | 49 | 13 · 19.5 s | 49 · 1.6 s | 43 · 1.6 s | 7 · 2.7 s |
| 15x15 | 225 | 13 · 20.0 s | 106 · 3.6 s | 73 · 2.6 s | 15 · 5.6 s |
| **19x19** | **361** | 13 · 21.2 s | **110** · 3.8 s | 77 · 2.7 s | 19 · 7.1 s |
| 21x21 | 441 | 13 · 21.1 s | **110** · 3.7 s | 79 · 2.9 s | 21 · 7.8 s |
| 23x23 | 529 | 13 · 20.7 s | **110** · 3.9 s | 81 · 3.0 s | 23 · 8.4 s |

**19x19 is where the roster's largest blast stops growing.** The Morgha alt's
12 m reaches 110 bodies there and 110 at 23x23, so no weapon in the roster is
clipped by the arena any more — which was the only argument for going bigger.

Past 19 the extra rows change exactly two things, and neither is wanted: an
infinite-punch-through weapon's column runs one body deeper per row (19, 21,
23 — it never saturates, and a perfect column that long is an arrangement no
player will line up), and a spread weapon's wandering epicentre catches a
couple more on wide misses (the Grattler's 77 / 79 / 81, which is its pellets
missing rather than its radius reaching).

The other three mechanisms at 19x19 were never the cost and are unchanged:
Morgha alt 110 bodies for 3.8 s, Grattler 77 for 2.9 s, Phantasma 19 for 7.1 s.

Three different saturation points, and the surprise is which one WAS expensive:

- **A CHAIN saturates first and costs the most.** 13 bodies from 7x7 onward, and
  the price of not stopping there is 160 s against 35 s for the same thirteen.
- **A BLAST saturates late and costs almost nothing** — the Morgha alt reaches
  110 bodies for 3.6 s, because a sphere is one instance per body with no
  recursion. It stops growing at 17x17, which is where a 12 m radius (the
  roster's largest) is finally contained by the grid.
- **PUNCH-THROUGH never saturates** — exactly N on an NxN, all the way out.

So the CLOCK does not decide this. A full pass over the roster at 17x17 is
7 chaining entries at 160 s, 54 explosive at ~4 s and 162 at 0.6 s: about 24
minutes, which is the order the single-target board already costs. What decides
it is the two clipping failures — too small and the biggest blast measures the
ARENA, too deep and a line-piercing weapon is handed a perfect column no player
will ever line up.

### Spacing decides which radii it can tell apart

At a regular lattice the thresholds are `s`, `s*sqrt(2)`, `2s`, `s*sqrt(5)` —
and a radius mod is only visible when it crosses one. Measured with
`formation_value 7 7 <s>`, seeds for the Torid's three radii (2.30 / 2.85 /
3.31 m, plus a body radius of reach):

| spacing | bare | Firestorm | Primed Firestorm |
|---|---|---|---|
| **1.50 m** | **6** | **9** | **13** |
| 1.75 m | 6 | 6 | 9 |
| 2.00 m | 4 | 6 | 6 |
| 2.50 m | 4 | 4 | 4 |

At 2 m the ruler separates bare from Firestorm and is BLIND to the Primed
upgrade (6 seeds and 6); at 2.5 m it is blind to both. **1.5 m separates all
three** — 1.00x / 1.50x / 2.17x — and it is a round number rather than one
fitted to the mod pair: 1.45 m gives the same three-way split, so the answer is
a band and 1.5 sits in it.

## The group-clear ruler

`data/benchmarks/group_clear.yaml` — the second ruler, and the first that is
about a ROOM rather than a target. Its companion's name has said "Single
Target" first since it was written, precisely so this could exist beside it.

**19 x 19 at 1.5 m, Thrax Centurion Lv 9999 SP, 180 s, KPM.** The shooter
stands at CONTACT with the middle body of the front rank and fires along the
line that rank faces. Every number in it is measured — see the tables above —
and the two that were choices are:

- **19 x 19** is where the roster's largest blast (the Morgha alt's 12 m) stops
  growing at 110 bodies and stays there through 23x23. Past it the extra ranks
  only deepen an infinite-punch-through weapon's column and reward a spread
  weapon's wide misses.
- **1.5 m** is the only spacing that separates all three steps of a radius mod
  (6 / 9 / 13 seeds for bare / Firestorm / Primed Firestorm).

### A crowd in three numbers, expanded ONCE

`formation_grid: {cols, rows, spacing_m}` becomes an ordinary `formation` list
in `benchmarks_data`, at the moment the yaml is parsed — not at simulate time.
361 bodies written out is 360 lines nobody can check by reading, and a ruler
whose terms cannot be argued with is not a ruler.

**Why there and not in `parse_fight`:** the PAGE has to draw the crowd. The
arena is the source — what you see is what gets simulated — and it reads
`formation`. Expanding at simulate time would have left the canvas drawing one
body for a 361-body fight; expanding in both places is the two-implementations
bug this repo keeps paying for. One expansion, and every consumer downstream —
the canvas, the payload, `parse_fight`, the scorer — sees only bodies. The
board page's own arena had `sc.formation = []` hard-coded from when a ruler
could not have one, and now draws the ruler's real crowd.

### A second ruler broke two things that were the same thing

The rulers were in PATH ORDER, and that was indistinguishable from "the primary
one" while `single_target.yaml` sorted first. `group_clear.yaml` sorts before
it, so the board page opened on a brand-new EMPTY ranking — and, worse, every
first-time visitor's default SCENARIO became a 361-body fight, because the app
seeds the active scenario from the first builtin.

`primary: true` on `single_target.yaml` is the declaration, and
`benchmarks_data::all()` sorts on it, so both consumers inherit one answer
rather than each carrying its own idea of which ruler leads.

### An empty board is a real state

A benchmark exists before anyone has submitted to it: the page lists it, states
its twelve rules, draws its fight and reports "0 of 224 entrants measured".
`check_board_link.mjs` REPORTS a ruler with no rows rather than failing on it —
and says so on screen, because a check that quietly exercises nothing reads
exactly like one that exercised everything.

## Adding a ruler: what it costs (audited 2026-08-17)

There will be many. `single_target` was alone for months, then a companion,
then `group_clear` — so the chain was walked end to end asking what the FOURTH
one would cost.

**A ruler is a data file.** Nothing on the path holds a list of benchmark ids:

| link | how it learns of a new ruler |
|---|---|
| the engine | `benchmarks_data::all()` globs `data/benchmarks/*.yaml` |
| `/api/meta` | maps that list |
| the page's ruler picker, scenario bar, board page | read `META.benchmarks` |
| the worker | validates `benchmark` as an ID, holds no whitelist |
| the scoring workflow | `for f in data/benchmarks/*.yaml` |
| `site/board.json` | read back and merged, so each ruler replaces only its own rows |
| the site build | globs `boards/*.yaml` |

**And the rules come with it.** `group_clear` refuses an incomplete build with
the same words `single_target` does — "0 mods, and this benchmark wants all 8
main slots", "0 of 4 evolution tiers" — because `validate_for_board` reads the
`build:` block out of the yaml. A new ruler's admission standard is written, not
coded.

### The one thing that was NOT frictionless

`.github/workflows/board.yml`'s publish step copied the prior board before
overwriting it:

```
cp "boards/$id.yaml" "/tmp/$id-prior.yaml"
```

unguarded, under `set -euo pipefail`. A brand-new ruler has no board yet — a
legal state, since the ruler exists before anyone has submitted to it — so
adding one would have **aborted the hourly board job** until somebody
hand-wrote an empty yaml. Guarded now. The SCORING step never had the problem:
it passes the path straight to the binary, which treats an unreadable prior as
"full rescore" and carries on.

`check_board_submit.mjs` holds the assertion: a benchmark id **nobody has ever
seen** is accepted, reaches storage under its own name, and sits BESIDE the same
build's row on another ruler rather than on top of it — the identity key carries
the benchmark, so two rulers scoring one build are two records.

## The library has a copy, and the copy has a restore

Until this, the one irreplaceable thing here lived in exactly one Cloudflare KV
namespace. The boards are derived from it, the site is generated, the code is in
git — the library is what players sent, and there was no second copy of it.

`.github/workflows/backup.yml` runs nightly and writes two:

| where | for how long | reach it with |
| --- | --- | --- |
| the `library-backups` branch | for ever, versioned by git | `git clone --branch library-backups` |
| a run artifact | 90 days | `gh run download --name library-<n>` |

**IT FETCHES COLD.** The board pipeline caches the library between runs; the
backup restores no cache and reads every key straight out of KV, because a copy
that shares its input with the thing it is copying shares its failure mode. It
costs about 7.5 minutes once a day.

**THE BRANCH IS CHEAP BECAUSE THE FILE IS SORTED.** One record per line
(`{"k": "<identity>", "v": {…}}`), keys sorted, object keys canonicalised — so a
night's change is ~35 added lines and git stores the delta. Measured on the
first real snapshot — 2,502 records, 1,598,310 bytes — **1.52 MB once and about
22 KB a night after it**, against 570 MB a year if each night were kept whole.

(The estimate that justified the design said 692 KB and 10 KB, from a record
shape sampled out of a board row. A real record is 639 bytes against the 287
that guessed, because a board row does not carry every axis. The conclusion
survives the correction by a wide margin, which is the only reason it is a
footnote rather than a redesign.)

**THE SNAPSHOT CARRIES ITS KEYS**, which `submissions.json` does not — that file
is the values, which is enough to score and not enough to restore. Recomputing
`identity()` in a restore script would be a second implementation of the one
thing that must not drift.

### Putting it back

`scripts/restore_library.sh` is the other half, and it is the reason this is a
backup rather than a hope: **a backup nobody has restored is a hope**. It is DRY
BY DEFAULT and `--self-test` runs the whole path against a stub, in CI, so the
day it is needed is not the first day it has ever run.

```sh
scripts/restore_library.sh library.ndjson            # says what it WOULD write
CF_ACCOUNT=… CF_NAMESPACE=… CF_TOKEN=<a WRITE token>   scripts/restore_library.sh library.ndjson --write
```

**THE TOKEN IS NOT THE REPO'S.** `CF_API_TOKEN` grants *KV: Read* and nothing
else, which is why a compromised repo cannot touch the library. A restore needs
*Write*, and the right way is to create that token, use it, and revoke it.

**IT IS ADDITIVE, NEVER DESTRUCTIVE.** It writes the records in the file and
touches nothing else, so restoring an old snapshot cannot delete newer
submissions — which is the failure a restore is most likely to cause and the one
nobody thinks about while restoring.

**AND IT WRITES IN BULK**, which is the one place KV is generous: there is no
bulk read (which is why a cold fetch costs 7.5 minutes) but there is a bulk
write of up to 10,000 pairs, so 2,474 records go back in one call.

### Three things fail differently, which is why there are three

- `guard_shrink` (the **tripwire**) refuses to publish a board from a short
  list. It is the only one that works while nobody is watching, and it cannot
  help if Cloudflare loses the namespace.
- The **backup** can, and cannot help if nobody notices for a month.
- The **board yaml in git** is a partial copy that has always existed — every
  PUBLISHED row carries its build, which is 2,185 of 2,474. What it misses is
  the builds under the 50% floor.

## What it costs as it grows, and where it moves next

The board is the thing nothing else in this space has, so the question is not
whether it survives more users but what it costs per user and which of those
costs are the wrong SHAPE. The rule the whole pipeline is measured against:

> **Every step's cost should be proportional to what CHANGED, not to what
> EXISTS.**

Scoring has obeyed it since the per-row fingerprint: a data change
costs the rows that read the file that moved. Reading did not, and that is what
made the board fall behind on 2026-08-26.

### What was fixed, and what it was

| | before | after |
| --- | --- | --- |
| read the library out of KV | **9 min**, every run, O(store) | seconds, O(new) — `scripts/fetch_submissions.sh` caches it between runs |
| a scheduled run behind a full rescore | cancelled by its successor | its own concurrency group, keyed by trigger |
| a truncated library | published a valid board with rows missing | refused — `guard_shrink`, floor at 90% of the last board's `submissions:` |
| the only copy of the library | one KV namespace | that, plus 30 days of rolling `submissions` artifacts |

KV has no bulk read and Cloudflare's API allows 1200 requests per five minutes —
4 a second, which is what the old loop was already doing. So **fetching faster
was never available; fetching fewer was.** The licence to cache a value is that
the KEY DETERMINES IT: the key is `identity(rec)`, and the scorer reads nothing
off a record that is not an identity axis (not `at`, not `benchmark`).

### The next wall, named in advance

1. **`submissions` is one unsharded job.** With the cache it is seconds on a
   warm run — but a cold one (a lost cache, a new namespace) still pays the full
   O(store) price, and that price grows with the library. It is bounded by the
   API's 4/s, so at 20,000 builds a cold start is 80 minutes.
2. **The board file holds every row.** 2,185 today across three rulers. It is
   committed on every update, so the repo grows with the community, and `site/`
   is regenerated and redeployed with it.
3. **Full rescores are O(store)**, and 128 shards buys only a constant factor —
   the ceiling is GitHub's 256-job matrix. A code change is no longer assumed to
   change every number (§"When the code moved"), and what a full rescore does
   pay for is bounded by the screen below.

### Where the 132 hours go, measured

Every row records what it cost, so the bill can be read straight off the boards
rather than estimated. Across the three:

| ruler | rows | total | median row | worst row |
| --- | --- | --- | --- | --- |
| `group_clear` | 7,493 | **6,153 min** | 20.0 s | **121 min** |
| `single_target` | 7,493 | 999 min | 3.6 s | 4.2 min |
| `single_target_no_aim` | 7,493 | 759 min | 2.6 s | 1.7 min |

**`group_clear` is 78% of it**, and inside that a handful of rows are the tail:
the top 100 rows of 7,493 are 31% of that ruler's bill, and thirteen of the
top fifteen are one weapon (Phantasma, a status beam against 361 bodies for
180 s). That is not a pathology to hunt — the cost of a row is how much the
build actually DOES, so the most expensive rows are the strongest builds on the
biggest ruler. It is the makespan floor: one row is one indivisible unit, so no
row-wise fan-out goes below the biggest row.

**RIVEN ROWS ARE 58% of the `group_clear` bill on 33% of its rows** (mean 86 s
against 31 s), which is the corner search: sixteen probes at `PROBE_RUNS` plus
one real measurement, ~2.6x a plain row.

### Not paying for rows that cannot be listed

A third of the bill goes on rows scoring under a quarter of their group's
leader — rows the floor will never list. The screen exists for exactly this and
turns two things on:

- **It needs a group LEADER**, which comes from the last board. A prior board
  whose scores are unusable is still READ for its leaders and its per-row costs,
  so a full rescore is screened like any other run. Without that, a full rescore
  screens nothing at all: measured, ZERO rows of 22,479. The `screen:` line in
  the log says how many thresholds a run had, because "0 screened" and "nothing
  deserved screening" are otherwise the same sentence.
- **A riven row is screened on the corner search's own best probe.** It cannot
  be screened before that — its riven is not chosen yet — but the search already
  prices every corner, so the best of them is a number already paid for. If it
  reads under the cut, no corner of that shape can be listed and the full
  measurement buys nothing: 38% off every riven row that is not going to place.
  This is sounder than the plain-row screen, since what is judged is the corner
  that would have been measured.

Measured on a real group, full rescore, published rows and every score
identical: 12.4 s → 7.5 s over 61 plain rows, and 16 of 41 rows screened where
none had been.

### What this system actually is

Strip the implementation and the board is **a materialised ranking over an
expensive pure function on a growing input set**:

```
score = f(build, ruler, engine_version, data_version)
```

Four properties decide everything downstream:

1. **`f` is deterministic** — the seed is pinned, so the same inputs give the
   same number for ever.
2. **`f` is expensive** — 5.8 seconds per `(build, ruler)` on average (712 CPU
   minutes over 7,341 pairs, measured 2026-08-26).
3. **The input set only grows**, apart from the one-year expiry.
4. **The output is a projection** — top N per (weapon, mode, ruler).

Anything with those four properties is a BUILD SYSTEM, and that is not an
analogy: Bazel and Nix exist for exactly this shape — an expensive pure function
over a versioned input set — so the answers can be taken from there rather than
invented.

#### A SCORE IS A FACT, NOT A STEP IN A PIPELINE

This is the one idea the rest follows from. `(build, ruler, fingerprint) ->
score` is true for ever once computed. It is a fact, not an intermediate result,
and a fact should be written down THE MOMENT IT IS COMPUTED rather than when a
batch finishes.

The board is recomputed as a batch today, and every symptom traces back to that:

| | batch (today) | facts |
| --- | --- | --- |
| a run is cancelled | everything it computed is lost | at most one score |
| a code push | a 2h20m blocking full rescore | N facts are missing; they backfill |
| a new submission | one cron period | seconds |
| adding a ruler | rescore everything | the missing facts enqueue; nothing else moves |

#### GENERATIONS — the least obvious part, and the most valuable

There is a real invariant here that must not bend: *a board whose rows were
measured by different engine versions is not a board.* It appears to conflict
with backfilling, since a half-finished backfill is a mixture.

It does not, and the resolution is one rule: **publish the newest COMPLETE
generation.**

```
fingerprint A (old):  7341 / 7341 facts   <- publish this one
fingerprint B (new):  3102 / 7341 facts   <- backfilling
```

When B completes it replaces A atomically. A reader never sees a mixed board,
AND never sees the board stop moving: new submissions keep landing under A,
because A is complete.

That rule is what turns a rescore from an EVENT into background noise. Without
it a six-hour backfill is a six-hour outage; with it, it is invisible. It
matters more than any hardware choice.

#### THREE TIERS, EACH WITH ITS OWN SCALING LAW

They are one lockstep batch in Actions today, and that is the whole of the
trouble.

| tier | scales with | needs | the right thing |
| --- | --- | --- | --- |
| ingest | new submissions | cheap, always up | Worker + queue |
| compute | missing facts | embarrassingly parallel, CPU-bound | wherever CPU is cheapest |
| serve | readers | fast, unblockable | a static file on the CDN — already right |

#### THE MOAT IS THE LIBRARY, SO IT IS A DATABASE AND NOT A CACHE

What compounds is COVERAGE — builds times rulers — and that part is already
architected correctly: the library model made a new ruler cost the community
nothing, because it is scored from the library the day it lands.

What follows is that KV is the wrong store for it. No queries, no transactions,
no bulk read, and listing is the only index — so "which weapons are
under-covered", "how much did the library grow this month", "which facts are
stale" are questions that cannot be asked. Those are exactly the questions
running a moat consists of. D1 is SQLite: it answers them, and it can be dumped
whole, which is a hard requirement for the one asset that cannot be regenerated.

#### WHERE THE COMPUTE GOES, AND A NUMBER WORTH KNOWING

Steady state is about **35 new builds a day times 3 rulers = ~10 CPU minutes a
day**. A €4 two-core box is 2,880 CPU minutes a day — **280x more than the
steady state needs**. Cost is not a steady-state question at all; it is a BURST
question, and the burst is a full rescore at 712 CPU minutes.

The two kinds of compute are good at opposite things, so use both against one
queue: GitHub Actions is free and unmetered on a public repo and absorbs bursts
256 ways, while a small always-on box gives SECOND-level latency for a new
submission — which is a product difference, not an ops one, for a tool whose
board is the reason people come back.

A box in Germany is the right place for scoring and the wrong place for anything
a player waits on: the players are in China, and what makes wfsim.app fast and
reachable there is that it is static and on Cloudflare. User-facing work stays at
the edge; CPU-bound work goes on the box.

### The order to move in, when each becomes the binding constraint

Each step is independently useful and none of them requires the next.

**A. Shard by COST, not by index.** *(cheap, do it first)* `idx % shards` assumes
rows cost the same; measured at 32 shards they ranged 9.3 to 52.9 minutes — a
5.7x spread, because a group-clear row reaches 361 bodies. Deal the rows out
longest-first (LPT scheduling) and the worst shard falls to near the mean. Ten
lines in the scorer, halves the full-rescore wall clock, costs nothing.

**B. Persist the SCORES, not just the board.** *(the architectural one)* A score
keyed by `(identity, ruler, data fingerprint)` is a fact that never expires. Put
those in **Cloudflare D1** — SQLite, free tier 5 GB and 5 M row-reads a day,
which is orders of magnitude more than this needs — and three things become
true at once: a rescore is a background BACKFILL rather than a blocking rebuild,
the board file becomes a cheap projection (read, sort, take the top N per weapon
per mode per ruler — seconds), and a run that dies loses nothing it had already
computed. This is the step that removes the whole class of problem, because
after it no ordinary run is O(store) at all.

**C. Publish a VIEW, not the library.** Once B is done the board file can hold
the top N per (weapon, mode, ruler) instead of every row, and the deeper ranks
the builder's picker shows can be fetched on demand. The file stays a static
asset on the CDN — fast, free, and unblockable from where the players are, which
is the property worth protecting above all the others.

**D. Move the queue, keep the compute.** The compute is not the problem:
**GitHub Actions is free and unmetered on a public repo**, which is a better
deal than any paid runner, and the engine is a Rust binary that runs anywhere.
What Actions is bad at is being a QUEUE — no backpressure, cancellation
discards finished work, a 256-job ceiling, ~15 minutes of scheduler slip. So
the move is a real queue in front of it (**Cloudflare Queues**, 1 M operations a
month free, $0.40/M after) with Actions draining it, rather than replacing the
runners.

**E. A persistent scorer, only if D is not enough.** In rough order of cost:
**Oracle Cloud Always Free** (4 ARM cores, 24 GB — genuinely free, genuinely
enough), then a **€4/month Hetzner** box, then Fly.io. A long-lived process
removes the per-run 30 s of checkout and cache restore that 128 shards pay 128
times, and it can hold the library in memory instead of fetching it at all.
Nothing above needs this; it is written down so the option is known rather than
discovered under pressure.

### What must not change

- **The board stays a static file on the CDN.** It is committed to the repo and
  served from the edge, which is what makes it fast and unblockable. Moving it
  behind a service would trade the thing that makes it good for a slow path and
  a second thing that can fail.
- **The store keeps nothing about submitters.** No IP, no token, no time finer
  than the day. Any service this moves to inherits that, and a queue or a
  database that would record more is the wrong service.
- **The library is the only irreplaceable thing.** Boards are derived, the site
  is generated, the code is in git. Anything that could truncate it needs a
  tripwire before it needs a backup.

---

## The store is a library of BUILDS, and every ruler crosses the whole of it

**THE STORE IS A LIBRARY OF BUILDS, AND EVERY RULER CROSSES THE WHOLE OF IT.**
A submission carries a BUILD and never a score; the number is produced by the
scorer under the ruler's own pinned seed. So the ruler a build was measured
under is provenance, not a gate. ANY FIGHT CAN UPLOAD, and the consent notice
is ONE story everywhere: what leaves is the BUILD, not the fight, and nothing
about you — the worker stores no IP, no token, and no time finer than the day,
and a record expires after a year. From a fight of your own the page asks the
door about EVERY ruler and reports "2 of 3 boards will take it"; it never
predicts a SCORE. A new ruler costs no community effort — it is scored from
the library the day it lands.

## A rescore costs the rows that read what changed

**A RESCORE COSTS THE ROWS THAT READ WHAT CHANGED.**
`engine::data_fingerprint` hashes what a row actually reads (its ruler, its
weapon and every form it fires, each mod, arcane and evolution, plus
everything no entity owns), the board stores it per row, and `--engine` is the
CODE alone. Measured on 24 real rows: 26.0 s full, **0.075 s** when nothing
changed, 2 of 24 for a Heavy Caliber edit, **0 of 24** for a whole new weapon.
The one hand list (`AFFECTS_NO_NUMBER`) can only cost TIME — anything
unclassified falls into the global bucket every row carries. Comments are
free, since `build.rs` embeds each file with them stripped.

## The board stays a static file, and says how far behind it is

**THE BOARD STAYS A STATIC FILE, AND SAYS HOW FAR BEHIND IT IS.** Committed to
the repo and served from the CDN, which is what makes it fast and unblockable.
`GET /api/board/pending` answers the one fact the file cannot carry about
itself: how many builds the library holds. A COUNT and nothing else. The
scorer writes `submissions:` per board and the difference is a footnote,

SILENT when the board is current.

## A fight is one document, and a scenario’s overrides sit behind legality

**A FIGHT IS ONE DOCUMENT, AND A SCENARIO'S OVERRIDES SIT BEHIND LEGALITY.** A
scenario holds everything a measurement needs — the target, the buffs, the
wielder — AND what it rules for each weapon CLASS, so any weapon can be tested
against one file and the official rulers are written in the same language a
player's own fight is.

THE ENGINE DECIDES WHAT MAY BE RULED ON, derived rather than listed.
`scenario::Capability::absence()` sorts every capability into two kinds and
that is the whole guard: a GAME FACT is the game's own rule — a Sentinel
cannot put a shot on a head — and a HOUSE RULE is ours. A scenario may say
*"in my fight, Arch-Guns have infinite ammo"* and may not say *"in my fight,
Sentinels land headshots"*. Exactly one of the four capabilities is a house
rule today. `overridable_pairs()` derives the legal (class, axis) set from the
two tables, `/api/meta` serves it, and the page draws exactly what is listed.
It is pinned as an EXACT set by a test, because the failure to guard against
is the list GROWING without anyone deciding it.
The resupply rule lives in the capability, not in `reserve_is_infinite`, which
takes the RESOLVED answer.

THE DEFAULT IS THE WEAPON IN FRONT OF YOU: the scenario blocks show what
applies here; the whole-fight panel is where the other classes are edited, and
a rule that merely AGREES with the capability is pruned rather than stored. A

RULER REFUSES ONE, like every other edit — `sim-whole-fight-body` is in
`lockOfficialScenario`'s sweep.

## A build the board already holds is not sent to it again

**A BUILD THE BOARD ALREADY HOLDS IS NOT SENT TO IT AGAIN, AND THE PAGE ASKS

THE ENGINE WHICH.** `officialBuildActive()` answers whether the ACTIVE PRESET
is a builtin, which is true of a board row opened from the picker and false of
the same build reached any other way. `/api/build/keys` keys a LIST of builds
through `builds::board_key`, so the build on screen and every row its weapon
holds are keyed by one engine in one pass. `builds::board_key` is that one
spelling — `format!("{}#{}", identity(&v), mode)`, defaulting a blank mode to
`base` — and THE MODE IS PART OF THE KEY, because one build played two ways is
two entrants. The one order that IS the identity is the elemental one: Torid
Heat/Cold/Toxin/Electric is Blast+Corrosive at 12,424 DPS against
Heat/Toxin/Cold/Electric's Gas+Magnetic at 46,583.
