# The board

The official leaderboard: **builds players submit, scored here**.

One sentence carries the whole design — **a submission is a BUILD and never a
number**. Everything else follows from it:

- a forged score is impossible, because no score is ever accepted;
- a change RE-SCORES stored builds instead of invalidating them, and nobody is
  ever asked to resubmit. What retires a stored score is a person deleting its
  row — §"A stored score is reused because it EXISTS";
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
  the per-mode dedup drops and the entry line refuses outright — so the fan-out
  only ever ADDS the rows where a build happens to be good somewhere its
  submitter never tried it.
  `transformed` and its kind are still refused: a gauge you must fill and run
  dry is not a way to play for three hundred seconds;
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
  pinned seed. Measured: wasm and native agree to the last digit
  (`0.9647804061510868` both ways). What made that untrue for a while was not
  the engine but the CARRY — see `exact_score` in the scorer: a score read
  back through `serde_json`'s number parser, which is not correctly rounding,
  publishes `1.1070976928071057` where the engine computes `...055`, and a
  reader reproducing the row is right while the board is wrong.

## Four actions, and ONE SOURCE FOR EACH FACT

**THE RULE THE WHOLE PIPELINE IS BUILT ON:**

> **EVERY FACT HAS EXACTLY ONE AUTHORITATIVE SOURCE. A value with two sources
> needs a rule for which one wins, and that rule is where every defect this
> pipeline has produced has lived.**

The answer is never "pick a better rule" — it is **delete the second source.**
The defects, each one a second source and the rule that arbitrated it: a merge
decided by sorting filenames, which published one ruler's score under another's
name; an artifact whose absence skipped the assembly, which then preferred the
older merged set, wrote it back and swept the deltas; a prior board republished
beside freshly measured rows half its size. None was a wrong rule. Each was a
second source.

| the fact | its one source | what is DERIVED from it |
| --- | --- | --- |
| a build exists | `builds` | the count at `/api/board/pending` |
| a build is legal | `engine::builds::validate_for_board` | the page's door, the worker's door — both ASK it |
| a build's axes | `engine::builds::BUILD_AXES` | every surface's fields, `/api/meta` |
| a ruler's terms | `data/benchmarks/*.yaml` | the picker, the arena, the scenario bar |
| a row's score | the `scores` row for what the build READS | everything below |
| what is published | `site/board/<weapon>.json` | `board/index.json`, `board.meta.json`, `data/board_state.yaml` |

**DERIVED IS NOT A SECOND SOURCE, and the difference is whether it can
DISAGREE.** `index.json` is computed FROM the weapon files in the same pass that
writes them, so no state of the world makes the two differ. A cross-weapon file
written BESIDE them from the same facts would be a second source: same inputs, a
second code path, and nothing stopping it from drifting. That is the whole test
to apply — *can this disagree with the thing it is about?*

**AND A COPY IS NOT A SECOND SOURCE WHEN IT ANSWERS A DIFFERENT QUESTION.** The
ruler's file says what it ranks by NOW; the `metric` on a score row says what
THAT NUMBER is in. The column exists because the row OUTLIVES the file: a
measurement a year old is still what that fight produced, and without its units
written down beside it, reading one back means checking out the commit that
produced it. Apply the same test: it cannot disagree with the thing it is about,
because it is not about the ruler — it is about the number. It therefore decides
nothing, on the same terms as `measured_by`.

**A TEST HAS ONE CORE.** A ruler names exactly one metric and that one is what
ranks; `engine::benchmarks_data::core_metric` is where the rule is applied, at
the moment a file becomes a benchmark, so no consumer downstream has to decide
what an unnamed one means — and both answers available there are wrong. Refusing
at the point of use is a whole run wasted on a file that could have been read in
a millisecond; defaulting ranks a whole board in kills per minute whatever it
was built to ask, every number on it looking exactly like a right one. A
SCENARIO may still leave the metric unsaid:
somebody running one for themselves is not publishing a ranking. A ruler may
also READ more than one metric off the same fight — those are readings of the
row and belong beside it, never in rows of their own, which would put two
answers under one key and hand the publisher a ranking with two units in it.

### The four actions

Three produce the board. The fourth produces nothing, and that is why it is not
one of the three.

| | who runs it | reads | writes | may not |
| --- | --- | --- | --- | --- |
| **INGEST** | the Worker, one request | — | one `builds` row | know what a score is |
| **COMPUTE** | Actions shards, as many as the work needs | `builds`, `scores` | `scores`, row by row | write a file anyone reads |
| **PUBLISH** | one Actions job | `scores` | `site/board/` | compute a number |
| **AUDIT** | one Actions job, hourly | `builds`, `scores` | nothing | publish, or gate anything |

**THEY ARE JOINED BY A QUERY, NOT BY A HAND-OFF.** Nothing is enqueued and
nothing is passed:

```
what is outstanding = (builds x rulers x modes)  MINUS  what is measured
                                                 MINUS  what the entry line parked
```

Four consequences, and they are the point of the shape:

- **nobody waits for anybody.** Publish can run at any moment; it needs no
  scoring run to have finished.
- **nobody can destroy anybody's output.** No sweep, no merge, no delta, no
  expiry. The only writer of a fact is the thing that measured it.
- **computing a row twice costs time and nothing else**, because a score is a
  pure function of (build, ruler, mode, what it read, what measured it).
- **a shard killed at 95% keeps 95%**, because a fact is written when it is
  computed rather than when a batch ends.

**AND IT IS RATCHETED, NOT TRUSTED.** `scripts/check_rescore_paths.mjs` asserts
that the assembly is handed `--facts-in` and that no second source reaches it;
the flags a second source would arrive on are named in the check, so adding one
back fails CI rather than being discovered in a published number.

## The pieces

| what | where | who runs it |
| --- | --- | --- |
| the ruler | `data/benchmarks/*.yaml` | — |
| the board | `site/board/<weapon>.json` | generated, committed, fetched at runtime |
| ranked across weapons | `site/board/index.json` | derived from the files beside it |
| which board this is | `site/board.meta.json` | a digest per published file |
| consent + submit | `web/src/static/app.js` (`offerBoardSubmit`) | the player's browser |
| the library | one D1 database, `wfsim` (binding `LIBRARY`) | written by the endpoint |
| the facts | the `scores` table in it | written by `ship_facts.sh` |
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
| the automation | `.github/workflows/scores.yml` | GitHub Actions |

**The board is in the repo AND in the database, and they are different
things.** `site/board/` is what is PUBLISHED — committed, diffable, served
from the CDN — and the `scores` table is where a fact is DURABLE the instant
it is computed. The repo copy is what makes "reproducible" a property rather
than a claim; the table is what means a shard killed at nine tenths keeps nine
tenths.

**The endpoint is on the site's own origin** (`wfsim.app/api/board/submit`). A
separate api domain would be a second DNS name and a second thing that can be
blocked, which is the failure the same-origin art rule was written about.

### Why the page FETCHES the board

Everything else in `data/` is embedded into the wasm at compile time. The board
is the one piece that changes without a release — once an hour, if people are
playing — and compiling it in made every update cost a full site rebuild:
install wasm-bindgen, fetch 300 images, recompile, to change a few numbers.

**ONE FILE PER WEAPON, because a weapon page shows one weapon.** A reader who
opens the Braton Prime fetches the Braton Prime, and a rescore that moved twenty
weapons moves twenty small files in git rather than rewriting the board. The one
surface that ranks ACROSS weapons reads `index.json`, which holds each weapon's
leader per (ruler, mode, riven) — 37 KB on the wire where the whole board is
249 — and which is DERIVED from the weapon files, so it cannot disagree with
one.

An unreachable or absent file is an EMPTY board, never an error: before the
first submissions there is nothing to show, and the page has to render that
state anyway. Every weapon in the roster has a file for the same reason —
including the empty ones, because a 404 and an empty list are the same thing to
`fetch` and opposite things to a page that states a weapon's standing.

## When it updates

**THREE FILES, ONE HOP EACH, AND ONLY ONE OF THEM FIGHTS.**

| | who starts it | what it does | what it costs |
| --- | --- | --- | --- |
| `queue.yml` | the clock, hourly at `:00` | what ARRIVED becomes a build; what has no score is asked for | one runner, minutes |
| `queue.yml` | the clock, once at 16:00 UTC | …and the weapons that have gone LONGEST are asked for again | the same runner |
| `scores.yml` | the clock, hourly at `:30` | what is asked for is MEASURED | as many shards as the work needs |
| `publish.yml` | the clock, 00/04/08/12 UTC | `scores` is ranked and written to `site/board` | one runner, seconds |

**EVERY ONE OF THEM IS ALSO A BUTTON, AND IT IS THE SAME RUN.** None takes an
input, so a hand-started run and a scheduled one differ in nothing at all.

**AN HOUR THAT DOES NOT FINISH LOSES NOTHING.** A queue row is deleted in
exactly one place — beside the fact that settles it, after the score is banked —
so a row a run did not reach, did not take, or was killed halfway through is
still owed when the next hour reads the list. **A cancelled tick costs a tick,
never a row.** GitHub keeps one run pending per concurrency group and drops a
second; the rows it would have taken are the rows the next one takes.

That is what the cadence rests on, rather than on any run being long enough.
How many hours it takes to drain a deep queue is a question about the WORK — the
clock does not have to answer it. The budget still bounds a shard at
`SCORE_DEADLINE_MINUTES` whatever the depth — shared out across the rulers,
because the binary runs once each — and an hour with nothing owed costs ONE job,
because the gate answers `todo=0` and the fan-out never happens.

**AND NOTHING ELSE BOUNDS THE CADENCE.** The repository is public, so Actions
minutes are unlimited. One run reads about 40,000 rows of D1 against a free five
million a day, and writes two per score against a hundred thousand — which caps
a day at 50,000 scores, or 221 core-hours of fighting. The CPU gives out an
order of magnitude before either quota does.

**A RUN'S WORK IS DECIDED AT ITS START AND NEVER CHANGES.** The queue is read
once, in the first job, and handed to every shard as an artifact — which is what
lets them agree on who owns which row without talking. Builds that arrive while
a run is fighting are picked up by the next hour's reconciliation.

**AND ONCE A NIGHT, WHAT HAS GONE LONGEST IS ASKED FOR AGAIN.** Nothing here
retires a fact on its own: a board is a claim about what the code computes
TODAY, and a row measured under an engine six weeks old is a claim nobody has
checked. The sweep says nothing about whether that number is WRONG — it says
nobody has looked.

**WHOLE WEAPONS, AND A FIFTH OF THE LIBRARY IS A TARGET RATHER THAN A CEILING.**
A weapon's file is written whole, so half of one re-measured ranks two
generations against each other — splitting one is off the table. Weapons go in
until the share is PASSED, which makes every night a little over it, by at most
the last weapon in. Used as a ceiling instead, a weapon bigger than a fifth
would fit no night ever, and that would be the BIGGEST weapon: the one most
people submit to.

**WHICH ONES IS A DRAW, NOT A SORT.** Everything in the pool has already passed
the age threshold, so which of them goes tonight has no right answer — and the
same answer every night means the same weapons are always sampled and the rest
reached only when those go quiet. `scripts/pick_stale.sh` draws them WEIGHTED BY
HOW OVERDUE: uniformly random, a weapon can be unlucky for a month; weighted,
the longer one waits the harder it is to keep missing. `SWEEP_SEED` pins a draw,
because a run nobody can reproduce is a run nobody can ask "why that weapon".

A fifth a night crosses the library in five. It only ADDS to the queue — the
hourly scorer pays for it in the hours after, which is what spreads the bill.

**IT LANDS IN ITS OWN BATCH**, named for the night, so it can be reordered ahead
of the arrivals or dropped — `DELETE FROM batches WHERE id = ?`.

**THE PUBLISH DOES NOT CARE WHAT IS BEING COMPUTED.** It reads the table, ranks
it, writes the files and commits if they moved. A scoring run may be halfway
through a batch when it fires; it publishes what is there, and the next one
publishes what is there then. It has no `needs:` and waits for nothing.

**NOTHING IS LOST BY PUBLISHING EARLY**, and that is what makes the indifference
safe. It was not always: while asking for a row again meant DELETING its fact, a
publish over a half-finished rescore would have written those rows out of
existence, which is what the old `unready` hold-back protected against. Asking
is a QUEUE row now and `scores` only ever grows — a build being re-measured
keeps its old number until the new one replaces it, and a build with no number
was never on the board.

**AND IF NOTHING MOVED IT DOES NOT PUBLISH.** The commit is guarded on the diff
of the generated files, so a run over an unchanged table costs one job and
leaves no line in a history nobody can read.

**WHY THE CLOCK IS ON THE FIRST HALF.** A submission is the one thing here
nobody can derive — an inbox row is its only record — so the sooner it becomes a
build the sooner it is safe. A row with no score has no such urgency: it keeps
not having one until somebody asks.

**AND HOURLY IS WHAT MAKES THE INTAKE HOP CHEAP.** Resolving a riven's card is a FIGHT,
about four core-minutes for the one submission in ten that needs it. That is an
hour's lump once a day and two or three minutes on the hour — the same work,
paid in a size that fits beside everything else.

**THE BOARD RUN TAKES NO INPUT.** There is nothing to name: what it does is
decided entirely by what `queue` holds and in what order.

A PUSHED RUN DID EXACTLY WHAT THE NEXT SCHEDULED ONE DOES, so it duplicated a
run that was coming anyway while competing for the same forty slots. Over 95
measured runs, 21 of 22 pushed runs were cancelled by the next push and the
board published nothing.

**ONE PENDING RUN, AND THE REST ARE CANCELLED.** That is GitHub's rule, not a
setting, and it is why a run has to fit inside the cadence that starts the next
one. A row too expensive to finish inside its budget is PAUSED and resumed by
the next run rather than cancelled — §"A row is paid for in sittings".

### A stored score is reused because it EXISTS

That is the whole of the rule, and there is nothing else in the key: not a
clock, not a fingerprint, not the binary that wrote it. `scores` is keyed by
`(build, ruler, mode)` and holds one row per key, the last one taken.

**A FACT IS NOT WRONG FOR BEING OLD.** A measurement a year old, taken by a
binary a thousand commits behind, is still what that fight produced — and the
board publishes from `scores` and from nothing else, so a rule that quietly
withheld old rows would be a board with no source at all.

**WHAT RETIRES A FACT IS A PERSON DELETING ITS ROW.** That is SQL, and precision
comes with it: one row, one weapon, one ruler, whatever the case needs. The row
comes back absent on the next run, and an absent row is computed.

**A HASH OF THE INPUTS WAS TRIED AND WAS A WORSE INSTRUMENT.** It fired on every
edit to a file no entity owns — comments, tests, validation rules, fields only
the page reads — and stayed silent on the one case that matters, a code change
that moves a number. Measured, the cost of assuming a fingerprint difference
meant "wrong" was **7,808 minutes across 128 shards, median shard 55 minutes and
worst 192, four hours of wall clock** against a schedule firing every twenty, so
every successor was discarded: eleven engine commits in one morning produced no
completed run for ten hours. A hash of `engine/` is worse still — it says the
BYTES moved, which is a different question from whether any NUMBER did, and it
cost a full rescore on 55.6% of commits to answer a question it was not asking.

TIME IS NOT AN INPUT, which is why there is no cooldown and never will be
(asked and answered). An untouched row is valid forever; a row whose engine
moved is wrong immediately, not in an hour. A cooldown would be both too slow
and too fast at once.

### One backlog, and one bound

Builds with no fact are the only work a run has. `NEW_ROWS` caps how many it may
take on — a ceiling, not a plan — and `SCORE_DEADLINE_MINUTES` is what actually
stops it. The cap is counted BEFORE the shard filter, so every shard stops at
the same row and they agree on the deal without talking.

**THERE IS NO SLICE AND NO CURSOR.** A computed row leaves the backlog, so a run
that walks it from the top converges; what a run does not reach is computed by
the next one, and until then the assembly publishes what the facts already say.

**THE SPLIT IS SIZED AGAINST THE WORK, NOT THE CEILING.** A shard costs 2.6
minutes before it scores anything — checkout, cache restore, load — so past a
point the split buys startup rather than parallelism: 128 shards pay 333 minutes
of it, 32 pay 83. `MAX_SHARDS` is the ceiling; how many a run actually takes is
computed from the work in front of it, because rows differ by 79x and a count
cannot tell a two-hour slice from a two-minute one.

**THE CEILING IS NOT FORTY IN PRACTICE.** Counted on live runs, GitHub granted
12 to 26 concurrent jobs, so `max-parallel` is an upper bound the account
reaches only sometimes, and the wave count is set by what is granted rather than
by what is asked.

**AND IT REACHES A BOARD NOBODY IS OVERWRITING.** Two assemblies is
last-writer-wins over a whole run's KNOWLEDGE rather than over one file: the
loser is whichever read the score store first, and what it publishes is the
store as it was then, rolling back the board and the store together. So the
ASSEMBLY is serialised and reads the store last; the scoring may overlap freely,
since it only ever adds.

### A fact is durable the instant it is computed

A shard appends each score to a log the moment it has it, flushed per row, and a
process running beside it ships the log to the `scores` table in batches. **A run
cancelled at 95% has banked 95% of its work**, and what it did not write is
simply missing — which is the same thing as never having started it.

**A ROW HAS ONE FACT: THE LAST MEASUREMENT OF IT.** The key is
`(build, ruler, mode)` — what makes it a different question — and measuring it
again overwrites. Everything else the row carries DESCRIBES that measurement and
decides nothing.

**AGE IS NOT EVIDENCE.** A fact measured a year ago, by a build a thousand
commits behind, is not thereby wrong — it is a measurement, and only another
measurement can disprove it. So nothing branches on `measured_by` and nothing
branches on a clock. What they are for is finding a broken build's rows
afterwards (`WHERE measured_by = ?`) and showing a reader how old a row is.

**NOTHING DOWNSTREAM MAY DESTROY A FACT.** No sweep, no merge, no delta, no
expiry, and no generation. One thing removes one: a person deleting its row.
Overwriting a row with a newer measurement of the same row is not destroying a
fact — it IS the fact.

**KEEPING SEVERAL GENERATIONS OF ONE ROW WAS THE ALTERNATIVE, AND ITS BILL WAS
UNBOUNDED.** It bought one thing — reverting a data file restored its answer
without recomputing — against a table that minted a fresh copy of the whole
board whenever a file no entity owns was edited. Measured over five weeks: 72
such commits, against a generation of 23,260 rows.

### A row is paid for in sittings

The store made a run's work survive the run. It did not make a ROW survive one,
and a row is where the tail lives: the board holds 35 rows costing over twenty
minutes each and eight over an hour, against a schedule that fires every hour.
0.4% of the rows are 20% of the group-clear bill.

A `--deadline` asked only before a row is dealt says when to stop TAKING rows
and cannot touch one already in flight. **One row then sets the makespan**, and
the worst of them sets it at ninety-five minutes.

So it reaches inside. A row is not one measurement — a riven row is sixteen
corner probes at 100 runs and one measurement at the ruler's 1000, of which the
corners are 62% — and the clock is asked between every run of every one of them.
What a row has finished is banked: the corner scores by their own index, and the
one sub-measurement that is partway as a cursor. The next run resumes there.

**THE RUN COUNT IS UNTOUCHED.** It is the accuracy promise, so the only thing
that bends is how many board runs those runs are spread over. Given any row,
however slow, that advances by at least one run a sitting, it finishes.

**A PIECE IS ONE RUN, and it may not be more.** A single call folds the runs one
at a time into one `Shard`; float addition is not associative, so a coarser
piece regroups the sums and moves the last bit of everything derived from one.
Measured over a 200-run crowd fight: pieces of 1 come out identical to a single
call and pieces of 2, 5, 10, 25, 50 and 100 all differ. A ULP is not below
notice here — a score is a pure function and the carry between processes is
lossless — so an interrupted row that regrouped its sums would not be the same
number. It costs 2.5% of a crowd fight.

A paused row is an outcome of its own beside published, refused and under the
entry line: the build reached no row on this board and is not lost either. The
run says so (`paused: N row(s) banked partway`) and the accounting knows about
every one of them, because a run that quietly dropped a build looks exactly like
one that ranked it.

**What this does NOT bound is the number of benchmarks.** The scoring step runs
`--deadline` once per ruler, so a shard's ceiling is three deadlines plus three
runs — 36 minutes at the current twelve. That ceiling is reachable only when
every ruler has a full deadline of work; the median shard finishes in 17 minutes
because it runs out of rows first.

### Publish assembles; it does not fight

`--project` is the guarantee rather than the habit. The publish pass groups what
is known, ranks it and writes the files — **measured at one to two seconds**
against a board of 7,659 rows — and never simulates. A row
nobody has banked is absent from this board and lands on the next one, which is
the convergence rule applied to the assembly.

WHAT IT BUYS is that a shard falling over costs a row rather than an hour. A
merge that picks up whatever the fan-out missed does it alone and unsharded,
which is the one place a single slow row can hold the whole publish.

**IT KEEPS EVERY ROW IT IS HANDED**, stale or not, and the two rules under that
are the same rule. It fights nothing and it drops no row whose data moved: a
pass that cannot refight a row must not remove it, or a published row
would vanish because a file it reads was corrected. Refighting is the shards'
job, and the board says how old it is.

**THE PUBLICATION UNIT IS A WEAPON, AND A FILE IS WRITTEN WHOLE.** The directory
the publisher is handed is both what it writes and where a weapon's rows under
every OTHER ruler are read back from, so a file is only ever written with all of
them in hand. A weapon with one unmeasured row keeps the rows it has, because a file written from an incomplete
source is a file missing whatever the source lacks. A carried row is COPIED
rather than reparsed, so a publish that measures nothing changes nothing: one
ULP either way is a number the engine did not produce.

### The standing a submitter sees at once

The wait for a ROW is the pipeline's and it is minutes at best: the board is a
static file a scheduled run writes. The question behind the wait is not. "Is my
build any good" is answerable on the page, from the number already on screen and
the board already fetched — no server, no round trip, no wait.

`boardProjection()` answers it, and returns null wherever the two numbers are
not ONE number:

* **The scenario must BE the ruler.** The board scores a build under its own
  fight, so a run of the player's own is a different measurement and ranking it
  against rows would be a number naming nothing. Where the scenario is the
  benchmark, the same engine ran the same fight for the same metric.
* **No riven and no valence.** The board scores those at their CEILING — the
  best corner of a riven's shape, a valence at the roll's maximum — where the
  run used what the player actually has, so the scorer's row comes back higher.
* **The board must have loaded.** A fetch that failed and a weapon nobody has
  submitted both leave the rows undefined; the second is ordinary and the first
  would answer "#1 of 1" to a reader whose network dropped one file.

A PROJECTION IS NOT A ROW. It is against the board as it stands, it is shown to
the submitter alone, and nothing about it is sent anywhere. The ranking holds
numbers this project measured — that is the whole of where it gets its
authority, and a client-supplied figure inside it would end that. It is the same
row that reached no board is not a row with a number nobody computed.

### Which rows carry the thing you just fixed

`scripts/board_select.py` answers that. What retires a fact is a person deleting
its row, so the rows have to be findable by hand — and no hash can help here: it
would say whether a FILE moved, where the question is which BUILDS contain a
thing.

```
python scripts/board_select.py --element heat --delete
python scripts/board_select.py --mod 'galvanized_*' --board single_target --plain
python scripts/board_select.py --weapon 'torid*' --mode cycle --rows
```

`--element` reads `data/` rather than a list: every mod, arcane, evolution and
weapon whose file grants it, so a card added tomorrow is found by the same walk.
The rest are globs, repeatable, any-of within a flag and all-of across them.

IT PRICES THE ANSWER BEFORE THE DELETE, and the price comes from the one place
that holds it. A published row does not carry what it cost, so `--delete` prints
a SELECT over exactly the rows the DELETE names — run that first. `heat` reaches
7,099 of the 10,867 published rows and 238 groups; `heat` on one board and
without rivens reaches far fewer, and that difference is a decision.

`--delete` prints one statement per (ruler, weapon, mode), which is the GROUP —
a published row names no build id, because the id is DERIVED from the build and
a stored copy of a derived fact is the one that goes stale. So it retires the
whole group rather than only the rows that matched, and over-deleting costs
TIME: the rows that did not need it come back the same. The other way round
leaves a repaired mechanic under a stale number nobody can argue the board out
of.

BATCH THE FIXES, THEN RESCORE ONCE. Ten corrections landing separately are ten
rescores of overlapping rows; landing together they are one. That is the whole
reason this prints SQL instead of running anything.

### Why it is sharded

Every row is an independent fight, so the scoring splits across `SHARDS` jobs —
each row charged to the least loaded of them, so a shard's slice is its own
share of the WORK rather than of the row count — and each writes only the scores
it computed; a merge job validates, deduplicates, ranks and writes, simulating
nothing. Verified before it shipped: 24 submissions through 8 shards reproduced
every published score to 1e-9, and the merge ran in 0.064 s. What `SHARDS` can
and cannot buy is §"Two ceilings, and neither is the shard count".

**A FACT CARRIES ITS RULER, and the key would not be a key without it.** A row
key is `identity#mode` and carries no ruler, so two boards scoring one build
produce the SAME key with different numbers. A store that held both published
whichever landed last under a ruler that never measured it: the Torid's aimed
**28.44229348067104** kpm sat at the top of the NO-AIM board, digit for digit,
where that build actually scores **0.170**.

It read as a scenario leak and was not one — every score was computed under its
own ruler's terms, then overwritten on the way out. The `scores` table keys on
`(build, ruler, mode)`, and `load_facts` filters on the ruler as it reads, so
the two cannot meet. That is the general shape of every
defect this pipeline has produced: a value identified by less than what
determines it.

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

**A SUBMISSION CARRIES A SHAPE** — which stats the card rolled, and which one
is the malus. Nothing else: a roll is one person's luck, and the player's own
numbers never leave the browser. It is a statement anybody can act on ("roll
this weapon for these stats"), and it carries no free-text field a player
authors.

**AND THE LIBRARY HOLDS A BUILD**, which needs numbers. `wfsim-intake` stores
the GOD ROLL — every bonus at its ceiling, the malus at its floor — which is the
same rule that scores every row at full Forma, every mod at max rank and every
valence at the roll's maximum: anything a player can eventually reach is not
part of what a row states. Two players who rolled the same stats submitted the
same build.

**AND A FIGHT IS ASKED ONLY WHERE THE SIGN HAS STOPPED ANSWERING**
(`rivens_data::ambiguous_stats`). Two sources and no third:

- **the three physical stats, on every weapon.** A physical bonus does not add
  damage beside the rest, it changes the SHARE each damage type holds of the
  total — and a status proc is drawn in proportion to that share, so more Impact
  is fewer Viral and Corrosive procs.
- **a stat the WEAPON takes the sign off**, one row each in
  `rivens_data::SIGN_IS_NOT_THE_ANSWER`. A weapon earns a row by paying for NOT
  having something, which makes whatever supplies it a cost: `+2000% on
  non-critical hits` or a crit multiplier granted only BELOW a threshold makes
  critical chance one, a multiplier granted below a status count makes status
  chance one, and a bonus earned by reloading from EMPTY makes magazine capacity
  one — a bigger magazine earns it less often.

  **BY WEAPON AND NOT BY PERK**, so it stays a list a person can read and audit.
  A build on one of those weapons that never took the perk is asked anyway and
  the fight answers "the god roll": a few hundred extra fights against a rule
  that cannot go stale differently from the weapon it is about. What keeps the
  list honest is a test that DERIVES membership from the effects and fails when
  a row is missing — an extra row costs two fights, a missing one publishes a
  card the fight would have argued with.

Measured over the library: 1,948 of 2,418 riven builds are answered by the god
roll and never reach a fight; 470 name a stat worth asking about, and all but
five of those name exactly one — two fights, not sixteen.

**WHAT IT TAKES TO MOVE A STAT OFF THE GOD ROLL** is beating it by more than the
RULER'S OWN standard error, at the ruler's own run count. Two cards the published
measurement cannot separate are not two builds, and between them the player gets
the better one. That threshold is fixed by the ruler rather than by the probe: a
test against the probe's own noise gets sharper the more you spend on it, so
every stat eventually "separates" and the corner count grows without ever
settling — measured, a build went from 2 corners at 40 runs to 4 at 2,560.

**AND WHEN TWO RULERS DISAGREE, THAT IS TWO BUILDS.** One cannot speak for
another and the card that wins a crowd need not win one target, so each
`(ruler, mode)` names its own and what enters the library is the SET. The rolls
are part of the id, so two ends of one shape cannot be filed under one another.

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

**THE RANKING IS ONE LIST; THE DEPTH'S GROUP IS NOT.** A riven build does not
always beat a plain one, so ranking them apart would publish a comparison the
fight does not make. But the depth's group gains riven-ness beside weapon and
mode,
for the reason it has mode: a shared reference would let whichever is stronger
on this weapon decide what the other may show, and the rows that would vanish
are the plain ones — the builds most readers can actually make. The board page
carries three views for the same reason: **all builds / no riven / riven only**,
deciding which subset each weapon's shown row is drawn from.

**TAKING A RIVEN ROW GIVES YOU THE RIVEN.** The record names no item, so the
page creates one, named after the shape so taking the same row twice reuses it.

**WHAT IT COSTS, AND WHO PAYS IT.** `wfsim-intake` does, once, when the record
enters the library, and for four builds in five it costs nothing at all — the
god roll needs no fight. A riven build is then an ordinary build with numbers on
it, scored once like any other, and the scorer probes nothing.

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

## The entry line

**A BUILD IS ON THE BOARD ONLY WHERE IT REACHES A TENTH OF ITS GROUP'S LEADER.**
`boards_data::KEEP_LEADER_SHARE`, and it decides two things at two
granularities:

| | | |
| --- | --- | --- |
| **a ROW** is published | `clears_entry` | it reaches a tenth of its own group's leader |
| **a BUILD** earns more fights | `keeps_earning` | it reaches a tenth in at LEAST ONE (ruler, mode) it has a fact for |

**THE TWO MAY NOT BE COLLAPSED**, and that is the whole shape of the rule. The
board asks for one good answer rather than for a build that is good everywhere,
so a build that leads the crowd ruler keeps earning single-target rows it will
score badly on — and those rows, measured and under the line, are not published.
Unified, one bad row would retire a build that leads another board.

**IT IS A SHARE OF THE LEADER, NOT A RANK.** A percentile is a QUOTA: it admits
a fixed PROPORTION of whatever arrives, so ten thousand junk builds would admit
a thousand of them, and the quality it enforces drifts with the crowd — measured
on two Torid groups of identical size, "the top 10%" cut at 0.281 and at 0.687 of
the leader. A share admits none of a flood at any volume, and it is a statement a
submitter can act on. Where the data cannot help is the NUMBER: the pooled
distribution of score-as-a-share-of-leader has no knee, so what places it is the
margin under the page's shallowest view. A tenth keeps 82.6% of published rows
and 93.3% of builds; a hundredth keeps 98.0% and gates nothing.

**WHAT IT CANNOT DO IS REFUSE THE FIRST FIGHT.** Knowing whether a build clears
the line means measuring it, so a build with no fact anywhere is owed one — and
it is owed exactly one: the (ruler, mode) its submission NAMED, which is the
fight its submitter actually ran. Clearing the line there earns the other
eighty-odd rows. A record naming no fight — provenance the endpoint has not
always stored — widens to every row rather than being stranded.

**EVERY AUTOMATIC PATH OBEYS IT AND THE RESCORE BUTTON DOES NOT.** `--gate` is
passed by the arrivals reconciliation and by the nightly sweep, which is where
the line pays for itself: arrivals are a one-off bill and the sweep recurs for
the life of the board. A person asking for a ruler again after a model
correction is the one path that may reach a parked build, and
`check_rescore_paths.mjs` asserts all three.

**NOTHING IS DELETED, AND THAT IS WHAT MAKES THE LINE REVERSIBLE.** A parked
build keeps its row in `builds` and every fact in `scores`; what stops is the
spending. The line is relative to a leader and a leader can be CORRECTED
DOWNWARDS — a group whose best falls from 50 to 1 makes a row that was under the
line a row well over it — and because the facts are still there, the next
publish re-ranks it back onto the board and the next reconciliation starts
asking for it again, with nobody doing anything. Deleting the facts would make
that recovery impossible and invisible: a board that looks entirely normal and
is missing the builds that now deserve to be on it.

## How deep a board goes — the reader decides

**EVERY SCORED ROW THAT CLEARS THE ENTRY LINE IS PUBLISHED.** A row is a fact — a
build measured under a pinned seed — and `site/board/<weapon>.json` carries all
of them down to that line. How deep to read is a question about what a reader
wants to see, not about what is true, so the page answers it: it shows builds
scoring **at least half their group's leader** by default, and offers a quarter
and everything. `BOARD_DEPTHS` holds `0` rather than a copy of the line for
"everything", because "no filter" stays true wherever the line is drawn.

A GROUP IS ONE WEAPON, IN ONE MODE, UNDER ONE RULER, and riven builds are a
group of their own. A riven build and a plain one compete with each other for
nothing, and one ruler's leader says nothing about another's; a shared
reference would let whichever group is stronger decide what the others may
show, which on most weapons means the builds most players can make are the ones
to disappear. There is no count limit, and a group whose builds are genuinely
close keeps all of them.

**HALF IS A CUT LINE, NOT A MEASUREMENT.** The pooled distribution of
score-as-a-fraction-of-leader has no knee to sit on — the largest gap anywhere
below 90% is 1.2 points — so the data cannot pick the number. What it can say
is that the number is not fragile: about **12 of 1274 rows per point**, so 45
or 55 would cost a few per cent rather than a shape. Against the sports that
draw the same kind of line (F1's 107% qualifying rule, cycling's 3-20% time
limit) half the leader is very generous, which is the intent — it marks where a
build stops being a DIFFERENT answer, not where it stops being the best one.

**WHAT IT HIDES IS NOT THE CHEAP BUILD.** That was the objection, and the board
refutes it — the rows below the line carry 8 of 8 mods exactly like the rows
above, and differ by taking the worse arcane (Merciless where Deadhead wins) or
by spending slots on mods this fight cannot pay: Magazine Extension, Parallax
Scope, Quick Reload, all of which `UNMODELLED.md` already says are worth
nothing against one standing target. Of 86 groups, **three** have ever held a
row with no arcane at all, and in each it was the leader.

**IT IS MECHANICAL.** The seed is pinned and a score reproduces to the last
digit, so 50.3% and 49.5% are two different NUMBERS rather than two estimates
of one. The boundary is INCLUSIVE: a row exactly on the line is shown, because
a cut drawn with `>` deletes the one row a reader is most likely to go looking
for. A group whose leader scored ZERO is never emptied — every row ties it, and
a ratio has nothing to say with no scale to say it on.

**AND `#1` STILL MEANS `#1`.** The list is descending, so what a depth keeps is
always a prefix of a group: the ranks a reader sees do not move when they widen
the view, only the rows below them arrive.

`check_board_depth.mjs` holds all of it, on injected rows — the published board
is whatever the scoring bot last wrote, and a fixture is the only way to ask
about a row at exactly half.

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

1. **The database** — one D1 database holds everything:

   ```sh
   npx wrangler d1 create wfsim
   npx wrangler d1 execute wfsim --remote --file worker/schema.sql
   ```

   …then declare it in `wrangler.jsonc`:

   ```jsonc
   "d1_databases": [
     { "binding": "LIBRARY", "database_name": "wfsim",
       "database_id": "<id from the create above>" }
   ]
   ```

   **In the file, not in the dashboard.** wfsim.app is a WORKER (static assets),
   deployed by `npx wrangler deploy`, and a deploy REPLACES the worker's
   bindings with what the config declares — a binding added through the
   dashboard is removed by the next push. The database id is an identifier, not
   a secret; it grants nothing without a token, and Cloudflare's own docs commit
   it.

   Named for what it HOLDS, which is not the board: the board is
   `site/board/`, and this holds the builds people sent and the facts computed
   from them. A binding called `BOARD` is a debugging trap — "the board is
   empty but the BOARD binding looks fine" is a sentence that sends you looking
   in the wrong place. The resource NAME follows `docs/NAMING.md` §8: the
   granularity it is provisioned at is the database, so that is what is named.

   **A PUSH DEPLOYS THE WORKER TOO.** Cloudflare's Workers Build runs
   `scripts/deploy.sh` on a push to `main`, so `worker/index.js` ships with
   everything else. What still has to be CHECKED is that it answered at all: the
   code can be right while wfsim.app runs an older one, and the failure that
   shape produces is a legal build refused at the one hop neither the engine nor
   the page is watching. `check_board_submit.mjs` asks the DEPLOYED endpoint
   whether it takes `MAX_MODS` ids and refuses one more, without writing
   anything: the shape pass stops at the first bad field, so a payload with a
   full mod list and a deliberately malformed arcane answers "bad mods" from a
   stale worker and "bad arcanes" from a current one.

2. **Repo secrets** — `CF_ACCOUNT_ID`, `CF_D1_DATABASE`, `CF_API_TOKEN` (a
   token with *D1: Read*, and *Edit* for the workflows that WRITE: the fact
   shipper and the restore).

   **A DATABASE ID WITH NO CREDENTIALS IS A MISCONFIGURATION, NOT AN ABSENCE.**
   The scripts treat "nothing configured" as a working state — a run with no
   database computes everything, which is what it did before there was one —
   but a half-configured one fails loudly, because the alternative is a shipper
   that runs green and sends no rows.

3. **The facts table is already in the schema above.** Nothing else has to be
   provisioned: the scorer writes a log, `scripts/ship_facts.sh` sends it, and
   `scripts/fetch_facts.sh` reads it back. Both carry their own self-tests,
   driven against a stub `curl`, so every hop is testable without a network.

   ASK IT A QUESTION, which is most of why it is a database. ON ONE LINE: a
   continuation is the one piece of shell syntax that differs between the two
   shells this might be pasted into, and `\` is an ARGUMENT to PowerShell, which
   then reports an unknown one and never says which.

   ```sh
   npx wrangler d1 execute wfsim --remote --command "SELECT weapon, count(*) FROM builds GROUP BY weapon ORDER BY 2 DESC LIMIT 20"
   ```

   **AND IT NEEDS AN API TOKEN, NOT THE LOGIN.** `wrangler login` is enough to
   `d1 list` this database and not enough to QUERY it: the same account, holding
   a `d1 (write)` scope, is answered `7403 — not authorized to access this
   service` on `/d1/database/<id>/query`. Set `CLOUDFLARE_API_TOKEN` to a token
   with *Account · D1 · Edit* — which is what the workflows use, and what makes
   them work where a laptop does not — and wrangler stops consulting the login
   at all.

The token only READS on the publishing path. What the board says is computed in
the repo from data in the repo; nothing secret decides a rank.

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
| `site/board/` | a weapon's file holds every ruler's rows, so one replaces only its own |
| the site build | globs `data/benchmarks/*.yaml` for the roster's files |

**And the rules come with it.** `group_clear` refuses an incomplete build with
the same words `single_target` does — "0 mods, and this benchmark wants all 8
main slots", "0 of 4 evolution tiers" — because `validate_for_board` reads the
`build:` block out of the yaml. A new ruler's admission standard is written, not
coded.

### A new ruler costs nothing to add

Nothing on the path holds a prior board to copy or an empty file to hand-write.
The scorer is handed the facts, and a ruler with none yet produces a board with
no rows — which is what a ruler nobody has submitted
to should look like.

## The library has a copy, and the copy has a restore

The library is the one irreplaceable thing here: the boards are derived from it,
the facts are computed from it, the site is generated, the code is in git. It is
what players sent, and a copy of it has to live somewhere that is not the vendor
holding the original.

`.github/workflows/backup.yml` runs nightly and writes two:

| where | for how long | reach it with |
| --- | --- | --- |
| the `library-backups` branch | for ever, versioned by git | `git clone --branch library-backups` |
| a run artifact | 90 days | `gh run download --name library-<n>` |

**IT FETCHES COLD.** The backup reads the rows straight out of the database
with no cache of any kind, because a copy that shares its input with the thing
it is copying shares its failure mode.

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

**THE SNAPSHOT CARRIES ITS KEYS**, which `library.json` does not — that file
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
CF_ACCOUNT=… CF_D1_DATABASE=… CF_TOKEN=<an EDIT token>  scripts/restore_library.sh library.ndjson --write
```

**THE TOKEN IS NOT THE REPO'S.** A restore needs *D1: Edit*, and the right way
is to create that token, use it, and revoke it.

**IT IS ADDITIVE, NEVER DESTRUCTIVE.** It writes the records in the file and
touches nothing else, so restoring an old snapshot cannot delete newer
submissions — which is the failure a restore is most likely to cause and the one
nobody thinks about while restoring.

**NINE ROWS A STATEMENT**, three bound parameters each against D1's limit of a
hundred per query. Slower than a bulk put and it does not matter — a restore
runs once, under pressure, and what it owes is certainty. The parameters are
BOUND because an identity is built from ids that arrived at a public endpoint,
and a restore is the worst moment to discover one of them carried a quote.

### Three things fail differently, which is why there are three

- `guard_shrink` (the **tripwire**) refuses to publish a board from a short
  list. It is the only one that works while nobody is watching, and it cannot
  help if Cloudflare loses the database.
- The **backup** can, and cannot help if nobody notices for a month.
- **`site/board/` in git** is a partial copy: every PUBLISHED row carries its
  build. What it misses is the builds under the 50% floor.

## What it costs as it grows, and where it moves next

The board is the thing nothing else in this space has, so the question is not
whether it survives more users but what it costs per user and which of those
costs are the wrong SHAPE. The rule the whole pipeline is measured against:

> **Every step's cost should be proportional to what CHANGED, not to what
> EXISTS.**

Scoring obeys it: a build with a fact costs nothing, so a run's bill is the
builds that arrived. Reading did not, and that is what made the board fall
behind on 2026-08-26.

### What was fixed, and what it was

| | before | after |
| --- | --- | --- |
| read the library | **9 min**, every run, one request per build | one ordered query — `scripts/fetch_library.sh` |
| a scheduled run behind a full rescore | cancelled by its successor | its own concurrency group, keyed by trigger |
| a truncated library | published a valid board with rows missing | refused — `guard_shrink`, floor at 90% of the last board's `submissions:` |
| the only copy of the library | one store at one vendor | that, plus a nightly snapshot on a git branch |

**THE READ STOPPED BEING A LOOP.** KV has no bulk read and Cloudflare's API
allows 1200 requests per five minutes — 4 a second, which is what the old loop
was already doing, so fetching faster was never available. One ordered, paged
`SELECT` replaced it, and the cache, the pruning pass, the three-try retry and
the flag that skipped the listing went with it.

### Three cheapenings that are deliberately not here

Each was taken out because it made the pipeline harder to reason about while the
foundation was still wrong, and each is worth having once the foundation is
boring. **They are recorded so that reintroducing one is a decision rather than
a rediscovery**, and the order below is the order they pay off in.

**A SCREEN WHOSE CUT COMES FROM THE LIST'S OWN LEADER.** 36% of a 132-hour bill
went on rows scoring under a quarter of their group's leader — rows that
cannot be published whatever they measure. A cheap probe deciding which to skip
is a second kind of number on the same board, which is why the old one had to
go: it wrote a `probe:` field, the archive then had to say "screened, not
measured", and the assembly needed a rule for it. Done again, the cut must come
from the group's OWN leader in the facts, and a screened row must produce no row
at all rather than a lesser one.

**WORK ORDERED BY COST.** The split already packs by measured cost, so the
shards are balanced; what is not ordered is which rows a BOUNDED run takes.
`--new-limit` takes them in walk order, so a run's 3,000 rows are whatever the
library's order hands it. Taking the cheapest first would publish more weapons
per run, because a weapon is published only when every one of its rows is
measured — and the tail rows that hold a weapon back are the expensive ones.

**RESUMABLE PARTIALS IN THE FACTS TABLE.** A row is indivisible today: a shard
killed part-way through a 121-minute row keeps nothing of it, and the next run
starts that row from zero. The scorer already banks partial progress WITHIN a
run (`Partial`, and `a_row_paid_for_in_sittings_is_the_row_paid_for_in_one`
pins that the sum is bit-identical); what is missing is a place to put one
between runs. The reason it is not the `scores` table is that a partial is not a
fact — it is a fact under construction, and putting the two in one table would
give the publisher something to filter out.

### The fight IS the run, measured

There is no overhead to schedule around, which is worth knowing before
optimising anything else. One run of 4,441 rows:

| | |
| --- | --- |
| whole run, wall clock | **39.2 min** |
| reading the library and the facts | 48 s |
| the shards | 37.2 min wall clock, 1,178 shard-minutes |
| publishing | 1 min, of which the assembly itself is **6 s** |

And inside a shard: 44 s of checkout, cache and build, then 2,088 s of fighting.
The rows those shards wrote record **1,177 CPU-minutes** of measured fight against
the 1,178 they were billed — so the scheduling, the reading and the carrying
together are under 2%.

**THE ONLY LEVER IS COMPUTING FEWER ROWS.** Batching saves nothing, because
nothing per-run is being wasted: a run with no work costs one job and 44 seconds.
The spread is what to aim at instead — 15.9 s average in that run, 1,434 s for
its worst row, and one row is indivisible.

**AND THE BINDING QUOTA IS WRITES, not reads or minutes.** Per run: 44,133 D1
rows read (the library once, the facts twice — the shards read the artifact, not
the database) and 4,441 written. At 24 runs a day that is 1.06M reads against a
free 5M/day, and 107k writes against a free 100k/day.

### The next wall, named in advance

1. **Reading the library is one unsharded job**, and it is now one paged query
   rather than a request per build. What grows is the number of PAGES, which is
   linear in the library and measured in seconds.
2. **Every published row is committed.** A weapon at a time, so an hour that
   moved twenty weapons writes twenty small files rather than the board — but
   the repo still grows with the community.
3. **Full rescores are O(store)**, and the shard count buys a constant factor
   against two ceilings that are both already reached — §"Two ceilings, and
   neither is the shard count". A code change is no longer assumed to change
   every number (§"When the code moved"), and what a full rescore does pay for
   is bounded by the screen below.

### Where the 132 hours go, measured

Every row records what it cost, so the bill can be read straight off the boards
rather than estimated. READ AT 7,493 ROWS A RULER, across the three the board
held then — `single_target_no_aim` has since been retired and
`single_target_demolisher` has taken its place, and neither the row counts nor
the totals below have been re-read since:

| ruler | rows | total | median row | worst row |
| --- | --- | --- | --- | --- |
| `group_clear` | 7,493 | **6,153 min** | 20.0 s | **121 min** |
| `single_target` | 7,493 | 999 min | 3.6 s | 4.2 min |
| `single_target_no_aim` | 7,493 | 759 min | 2.6 s | 1.7 min |

WHAT CARRIES IS THE SHAPE, not the figures: the bill is dominated by the ruler
with the most bodies in it, and a single-target ruler costs an order of
magnitude less however many of them there are. That is a fact about 361 bodies
against one, and it does not depend on which single-target rulers exist.

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

### Two ceilings, and neither is the shard count

Raising `SHARDS` was the answer three times and it is not available a fourth,
because the shard count is not what binds. Both ceilings are measured, and a
sizing argument that does not name them will be wrong the way the last three
were.

**FORTY JOBS RUN AT ONCE, WHATEVER THE MATRIX SAYS.** The account's concurrent
job limit is the real fan-out: across every board run in a day, concurrency sat
pinned at exactly 40 for 263 minutes and never once reached 41. A matrix of 128
is therefore several waves, not one — measured start spread across the shards of
one run, 195 minutes — and every shard past the limit adds a checkout and a
cache restore while buying no parallelism at all. **The wall clock of a full
rescore is `total work / the jobs in flight`**, and no shard count moves it.

**AND THE CEILING IS THE WHOLE REPOSITORY'S**, not the board's. A rescore that
takes all forty starves every other workflow: a board run held them for three
and a half hours with CI queued behind it, which is how a red build went two
days unseen. `max-parallel: 32` on the scoring matrix leaves eight — the board
is a background job, CI is the one a person waits on, and a rescore a quarter
longer against hours it already takes is the cheaper side of that trade.

**AND ONE ROW IS INDIVISIBLE.** The worst row is 121 minutes, so even at
infinite fan-out a full rescore cannot finish faster than that. Row-wise
splitting is within a small factor of its own floor already.

Together they say the same thing: **a full rescore cannot be made fast, so the
lever is not paying for one.** §"A row's code dependency is measured, not
assumed" is where that lever is.

### Not paying for rows almost nobody reads

A third of the bill goes on rows scoring under a quarter of their group's
leader. They are PUBLISHED and a reader can widen the depth to them, which is
why they are not simply dropped — but a reader at the default never sees one,
and they are the largest cheapening still available. **There is no screen today**: the one that was here wrote a second
kind of number onto the same board, and what it cost to keep the two apart is
§"Three cheapenings that are deliberately not here", which also says what a
second attempt has to get right.

### What this system actually is

Strip the implementation and the board is **a materialised ranking over an
expensive pure function on a growing input set**:

```
score = f(build, ruler, engine_version, data_version)
```

Six properties decide everything downstream:

1. **`f` is deterministic** — the seed is pinned, so the same inputs give the
   same number for ever.
2. **`f` is expensive** — 21.4 seconds per `(build, ruler)` on average, 8,071
   CPU minutes over 22,656 pairs, and the spread is four orders of magnitude
   wide (§"Where the 132 hours go").
3. **The input set only grows**, apart from the one-year expiry.
4. **The output is a projection** — top N per (weapon, mode, ruler).
5. **A ROW IS `(build, ruler, mode)`, and a mode is a property of the WEAPON.**
   Every melee carries seven — base, block, block_forward, forward, heavy,
   heavy_slam, slide — the Ballistica Prime four, and most guns one or two, so
   the roster's 149 weapons are 259 groups on each board. A weapon with n modes
   is n independent rankings, because the cards that win them differ.
6. **Most changes are to the CODE.** Measured over two weeks of 647 commits:
   55.6% touch `engine`/`webapi`/the scorer, 13.6% touch only `data/`. The data
   half of invalidation is already asked per row and is the cheap half; the
   expensive half is the one a single hash answers for the whole board.

Anything with the first four properties is a BUILD SYSTEM, and that is not an
analogy: Bazel and Nix exist for exactly this shape — an expensive pure function
over a versioned input set — so the answers can be taken from there rather than
invented.

#### A SCORE IS A FACT, NOT A STEP IN A PIPELINE

This is the one idea the rest follows from. `(build, ruler, mode) -> score` is
true for ever once computed. It is a fact, not an intermediate result, and a
fact should be written down THE MOMENT IT IS COMPUTED rather than when a batch
finishes.

The board is recomputed as a batch today, and every symptom traces back to that:

| | batch (today) | facts |
| --- | --- | --- |
| a run is cancelled | everything it computed is lost | at most one score |
| a code push | a 2h20m blocking full rescore | N facts are missing; they backfill |
| a new submission | one cron period | seconds |
| adding a ruler | rescore everything | the missing facts enqueue; nothing else moves |

#### A MIXTURE IS ALLOWED, AND THAT IS WHAT MAKES A BACKFILL INVISIBLE

The tempting invariant is *a board whose rows were measured by different builds
is not a board*, enforced by publishing only a COMPLETE set of one build's
measurements. It was tried, and it is the wrong trade:

- it pays a **full rescore on every code change** — 130 CPU-hours — to confirm
  numbers that almost never move, since 55.6% of commits touch the engine and
  nearly none of them can move one;
- while that runs, **the board cannot move at all**, so a six-hour backfill is a
  six-hour freeze on new builds;
- and the guarantee is not even achievable by declaration: one label over a
  store written by six different commits is a claim the data does not support.

**THE BOARD IS A RECORD OF WHAT WAS MEASURED**, and its reliability comes from
every row being a real measurement rather than from an atomicity property of the
publish:

```
   within a weapon      never mixed   — a weapon's file is written whole
   across weapons       may be mixed  — and every row in it is a real measurement
```

A row measured by an older build is not a wrong row; it is a row nothing has
DISPROVED. `measured_by` says which build wrote it, which is what makes a broken
build's rows findable afterwards — `WHERE measured_by = ?` — and that is the
whole of what an engine version is for here.

**A NEW SUBMISSION IS PENDING, NOT A GENERATION.** It has no fact yet, so it
cannot enter the ranking — but the submitter's own client already computed a
number to show them, and holding the row back entirely would be less honest than
showing it as what it is. A submitted row appears immediately, marked as
unverified, ranked provisionally by the client's number and OUTSIDE the
ranking, and is replaced by its fact when one exists. The client's number is
never a score: it is a placeholder that the board is required to overwrite, and
a placeholder that does not match the fact is a signal worth recording rather
than a row worth trusting.

#### THREE TIERS, EACH WITH ITS OWN SCALING LAW

They are one lockstep batch in Actions today, and that is the whole of the
trouble.

| tier | scales with | needs | the right thing |
| --- | --- | --- | --- |
| ingest | new submissions | cheap, always up | a Worker, and a doorbell |
| compute | missing facts | embarrassingly parallel, CPU-bound | wherever CPU is cheapest |
| serve | readers | fast, unblockable | a static file on the CDN — already right |

**THE QUEUE IS A QUERY, SO THERE IS NO QUEUE.** Once a score is a fact keyed by
its inputs, the work outstanding is `the keys with no fact yet, ordered by
priority` — derived from the store, never stored beside it. That is strictly
better than a real queue here rather than merely cheaper: nothing can be lost,
because nothing was enqueued; a worker that dies leaves the key missing and the
next one takes it; scoring twice is harmless because `f` is deterministic, so
at-least-once delivery costs nothing to tolerate; and a push that reorders every
priority at once is a different `ORDER BY` rather than a re-enqueue of the
backlog. A queue would add one more piece of state that can disagree with
reality, which is the failure this section exists to remove. What ingest needs
is not a queue but one bit — *there is work* — and a `repository_dispatch` from
the Worker carries it.

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

**THE FREE TIER IS NOT SHORT OF COMPUTE.** Actions minutes are unmetered on a
public repo, so the budget is the concurrency ceiling times the clock: 40 jobs
times 24 hours is **960 CPU hours a day, free**. Steady state is nowhere near
it — about 365 new builds a day across three rulers is ~160 CPU minutes at the
median row, a quarter of one percent of the budget.

**WHAT EXCEEDS IT IS RESCORES, AND THEY ARE NOT RARE.** A full rescore is 134
CPU hours, and every push touching `engine`, `webapi` or `cli` asks for one. A
working day of thirteen such pushes asks for **1,742 CPU hours against 960
available** — nearly twice what exists, which no scheduling policy can absorb
and no shard count can compress. A board hours behind on such a day is not a
starved queue; it is an oversubscribed one.

The bill is also almost entirely for work that could not have mattered: of the
thirteen, most touch one mechanic, and a melee change re-derives 7,388 gun rows
that never execute a line of it. Under §"A row's code dependency is measured"
the same day asks for well under the budget. **Sizing the compute is downstream
of not asking for it.**

The two kinds of compute are good at opposite things, so use both against the
same missing-fact query: GitHub Actions is free and unmetered and absorbs a
burst 40 ways, while a small always-on box gives SECOND-level latency for a new
submission — which is a product difference, not an ops one, for a tool whose
board is the reason people come back.

A box in Germany is the right place for scoring and the wrong place for anything
a player waits on: the players are in China, and what makes wfsim.app fast and
reachable there is that it is static and on Cloudflare. User-facing work stays at
the edge; CPU-bound work goes on the box.

### The order to move in, and the one rule under all of it

> **EVERY LAYER'S FAILURE MUST BE SLOWNESS, NOT A WRONG ANSWER.**

That is what the faults this section replaces had in common. Each was a
hand-kept list — which paths wake the board, which families are attributed,
which files affect no number, which fields a sample carries — and each, left
incomplete, published a number the engine does not compute rather than costing
time. `AFFECTS_NO_NUMBER` states the correct direction for its own list and is
the model: *forgetting an entry is slow and never wrong.*

**1. THE LIBRARY IS PERMANENT AND CHEAP.** Builds are configurations; storing
every one for ever costs almost nothing, and it is the only thing here that
cannot be regenerated. Deduplicated by `builds::identity`, which keeps mod
ORDER because elements pair in first-placement order — the same cards in two
arrangements are two builds with two scores.

**2. INVALIDATION, AND ITS TWO HALVES ARE DIFFERENT PROBLEMS.**

The DATA half is already asked per row, from the entities the row names, and it
is the cheap half: 13.6% of commits. It stays. What it needs is the silent gaps
closed, not more precision.

The CODE half is 55.6% of commits and a single hash for the whole board, so
every one of them nominally marks everything unverified. The answer is not a
finer declaration: **unverified is not wrong**, so the slice repairs it at a
bounded rate and nothing declares that changing one thing affects another.

**WHAT NO SAMPLE CAN CLOSE.** Any probe that reads one row per group misses a
change that moves some builds of a group and not the sampled one. That is why
the check on a published number is the AUDIT, which reads every row in turn
rather than one per group — §"When the code moved", the two backstops.

**3. PROGRESS IS MONOTONIC.** A score keyed by its inputs is a fact, written the
moment it is computed rather than when a batch ends (§"A score is a fact").
`worker/schema.sql` already holds the table. A cancelled run then loses one row
instead of an afternoon, which does not make a long run shorter — it stops the
length of a run from being a question anyone has to answer.

**4. PUBLISHING IS A PROJECTION.** Read the facts, rank, write the files:
seconds, and independent of whether any scoring is in flight. A weapon whose
every row is measured ships; one with a gap keeps what it has.

**TRIGGERING AND SCHEDULING**, which is where the failure direction was
inverted: the trigger is an EXCLUSION list naming only what the board itself
generates, a run that finds nothing to do costs one job rather than a hundred
and twenty-eight, and a submission rings a doorbell rather than being waited for
by a schedule.

### What was refused, and why that is written down

Each of these is a plausible answer that measurement turned down. They are here
so the next reading does not have to re-derive the refusal.

**Per-unit code fingerprints, and the refactor under them.** Attributing code
units to weapon classes so a melee change retires melee rows: a hash of what a
row READ says the bytes moved, which is a different question from whether a
number did. And the refactor it needs is real — every melee commit touches
`engine/src/dummy.rs`, 32,146 lines of the engine's 78,003, which holds the gun
logic too, so a file-level attribution buys nothing until melee is moved out of
it.

**A message queue.** The pipeline wears every sign of one — durable work,
stateless workers scaled sideways, at-least-once semantics, backpressure, a
bounded batch per cycle — and it is not a queue and must not become one.

THERE IS NO QUEUE, THERE IS A SET DIFFERENCE: what the library holds, minus what
the score store holds, recomputed from scratch every run. No pending list, no
head, no ack, no redelivery, no dead letter. That is the RECONCILIATION LOOP a
controller runs — desired state against observed state, closing the gap a little
each cycle — and it is why a run that dies loses nothing. There was never a
message to lose.

THE LICENCE FOR IT IS THAT THE WORK IS DERIVABLE. A queue earns its complexity
where the work item is the ONLY record of itself: an event nobody wrote down is
gone. Here the BUILD is the record and it is permanent, so what remains to be
done can always be derived again. At-least-once needs no thought either — a
score is a pure function, so computing one twice costs time and nothing else.

WHAT WOULD CHANGE IT is the walk. The difference is taken by reading the whole
library each run, which is O(the library) and today is seconds. Grow it a
hundredfold and that walk becomes the cost, and an index of what is missing
starts to earn its keep — most likely a query against the store rather than a
queue, but that is the first moment the question is worth asking again.

THE ONE THING THAT IS NOT DERIVABLE IS A SUBMISSION, and that is the one thing
with a queue: `inbox` holds what a player sent until `wfsim-intake` has made a
build of it. Everything downstream of the library is a set difference.

**Adaptive precision — fewer runs for rows far from a boundary.** The run count
is the RULER'S OWN TERM and is where a published number's authority comes from.
Spending less of it is not an optimisation of the board, it is a trade against
the thing the board is for.

**Recording which data files a fight read.** Exact and safe, and it would retire
four hand lists at once — but it improves the half that is already cheap and
already per row. 13.6% of commits.

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
- **Nobody submits a number.** A client's figure may stand in front of a reader
  as an unverified placeholder, and may never be stored as a score or ranked
  against one. Every published row is reproducible from the repo by anyone.
- **No work is enqueued anywhere.** What is outstanding is derived from the
  facts that exist, so there is no second copy of it to fall out of step with
  the first — §"The queue is a query".
- **THE RUN COUNT IS THE RULER'S OWN TERM.** A published row is measured at the
  count its benchmark names, and that is where its authority comes from. Every
  cheaper answer this pipeline finds has to come from computing FEWER ROWS, and
  never from computing a row less well.
- **A MODE IS AN INDEPENDENT RANKING.** It is a property of the weapon, not of
  the build — seven on every melee — and the cards that win one do not win
  another, so `(weapon, mode)` is the group and its own leader sets its own
  floor. Nothing may rank two modes against each other.
- **MOD ORDER IS PART OF THE BUILD.** Elements pair in first-placement order, so
  one card set in two arrangements is two builds; measured on the boards, 338
  rows differ from a sibling by order alone and NOT ONE of them scores the same.
  An identity that sorted them would publish one and lose the other.

---

## The publisher publishes, and computes nothing

**IT TRUSTS THE DATABASE UNCONDITIONALLY.** It reads the facts and the library,
joins them, ranks, projects, and writes the files. It has no opinion about whether a number is right. If a
number is wrong, the SCORER is wrong, and that is where it is fixed.

```
  builds  ─┐
           ├─►  PUBLISHER  ─►  site/board/<weapon>.json
  facts(g) ┘   join, rank,     site/board/index.json
               project, write  data/board_state.yaml
```

That is the whole of it. No prior board, no store, no artifacts, no forced
list, no submissions to score, no fingerprint to check, no reuse to decide.

**WHY IT IS ONE SOURCE AND NOT FIVE.** A publisher handed a prior board, a
merged store, a run's artifacts, the facts and the library has five sources
that can disagree, so it needs a rule for which one wins. Every defect
this pipeline has produced lives in that rule: a merge decided by filename
order, an artifact whose absence skipped the assembly, a clock that truncated a
forced set. **A rule about which source wins is only needed because there is
more than one source.**

### The scorer's side is a list somebody wrote

```
scores  what has been measured — one row per (build, ruler, mode), the latest,
        no history and no state. The site is built from this and nothing else.
queue   what somebody asked for. Empty at rest. Ordered by its batch.
```

**A RESCORE IS "MEASURE THESE ROWS AGAIN", AND THERE IS NO SECOND KIND** — but
it is now an INSERT rather than a DELETE, and that is what stops a rescore from
putting a hole in the board: the old number stays published until the new one
replaces it. `--queue-in` is what makes a stored score stop being a reason to
skip a row.

**THE ORDER IS A COLUMN.** `batches.at` decides which group goes first, so
jumping the line is one UPDATE — no flag, no deploy, and the run still takes no
input. Progress is `total` minus what is still owed; nothing stores it, so
nothing can be wrong about it.

**THE ONE RULE THAT KEEPS TWO TABLES FROM DISAGREEING:** the queue may only ever
CAUSE work. It may not prevent work and it may not decide a number. So losing it
costs an ordering and never a fact, a stale row costs one recomputation that
returns the same number, and the publisher never reads it at all.

**AND A HAND-WRITTEN LIST CANNOT QUIETLY LOSE A ROW.** `builds × rulers × modes`
is the DEFINITION of what should exist; `--queue-missing` names everything with
neither a score nor a queue row and the run puts it in a batch, in seconds. That
is not a second source — it only ever ADDS, and only what has no score.

That is also what makes a deadline harmless. Truncating a run leaves rows
unmeasured, the board keeps publishing the weapons that are complete, and the
next run continues from what is missing rather than from the top of the same
forced list.

---

## The pipeline, designed around one rule

> **A FACT IS DURABLE THE INSTANT IT IS COMPUTED, AND NOTHING DOWNSTREAM MAY
> DESTROY IT.**

Both halves were violated, and the two together lost a full rescore — 8,008 CPU
minutes, computed correctly and then deleted:

1. A shard's scores became durable only when the shard FINISHED and then
   UPLOADED. GitHub's artifact service timed out on one of 128, the assembly
   was skipped, and nothing was published.
2. The next assembly read the banked scores, silently preferred the older
   merged set — deltas sorted before it by name — wrote that older set back,
   and SWEPT the deltas it had just discarded.

Neither step was wrong about anything it could see. The design let a merge whose
correctness nothing checked gate a delete.

### The four pieces, and what each is allowed to do

**INGEST — the worker.** `POST /api/board/submit` writes one row:
`INSERT OR REPLACE INTO builds`. The key is the build, so a resubmission is the
same row. `GET /api/board/pending` is `SELECT COUNT(*)`. It knows nothing about
scores and writes nothing else.

**COMPUTE — a shard.** Reads the library and the facts, takes its share of
the difference, and after each row **writes the fact immediately** — appended to a log and flushed per row, shipped by a process
running beside it. It produces NO artifact and NO blob: its only output is rows. Killed at any point, it keeps everything it
wrote, and what it did not write is simply missing — which is the same thing as
never having started it.

**PUBLISH — a projection, and it depends on no run.** One read of the facts,
rank, keep everything within half of each group's own leader, write
`site/board/<weapon>.json` and the index beside it, commit. It can run at any
moment, needs nothing from any scoring run, and CANNOT DESTROY ANYTHING because
the only thing it reads that it also writes is a weapon's own carried rows.

### An older engine is a REFERENCE, never a verdict

> **A SCORE DOES NOT EXPIRE, AND THE BUILD THAT MEASURED IT DOES NOT DECIDE
> WHETHER IT IS RIGHT.**

A fact's validity rests on nothing this code can check. What a row READS is
enumerable from the row; what it EXECUTES is not, and **no hash can stand in for
it.** 55.6% of commits touch the engine and almost none of them can move a
number, so treating "measured by an older build" as "wrong" means spending 130
CPU-hours to confirm numbers that were already right. `measured_by` is therefore
FORENSICS: it is what says which rows a build wrote, once a build is found to
have been broken.

**SO INVALIDATION IS A JUDGEMENT, AND A PERSON MAKES IT.** Deleting a row is the
only operation in this pipeline that destroys a fact, and it is SQL — which is
also what makes it precise: one row, one weapon, one ruler, whatever the case
actually is.

### The board is a record, and a mixture is allowed

The cross-weapon ranking can hold rows measured by different builds at the same
time, and that is a property rather than a defect:

- **A WEAPON IS NEVER INTERNALLY MIXED.** A weapon's file is written whole or not
  at all, so every row in it was measured against the data the build reads today.
- **THE ALTERNATIVE IS WITHHOLDING CORRECT ROWS.** Guaranteeing one engine across
  the whole board means publishing nothing until every row has been re-measured
  — a full rescore before a single new build can appear.
- **EVERY ROW IS A REAL MEASUREMENT**, which is where the board's authority
  comes from. The atomic-swap alternative — one engine across the whole board at
  every moment — costs 130 CPU-hours per code change to buy a property no reader
  asked for.

### What becomes impossible

| | why |
| --- | --- |
| a shard's work lost because a service timed out | the fact is in the table before the shard ends |
| a correct fact DELETED downstream | nothing deletes facts — no sweep, no merge, no delta |
| "is this board current" taking days to answer | it is one comparison |


---

## The shape, end to end

```
  READ PATH — nothing on it can fail
     reader ──► Cloudflare CDN ──► static files, committed to the repo
                                   site/board/<weapon>.json
                                   site/board/index.json
                                   site/app/*.wasm
     No service, no database, no query. The board's availability is not
     coupled to anything behind it.

  WRITE PATH — one submission is one row
     player ──► Worker ──► D1.builds
                     └──► answers "how many" and "already held" with a query

  COMPUTE — the queue is a query
     GitHub Actions
       what is outstanding = builds MINUS scores, joined on what a row reads
       32-128 shards, each fighting rows
       every fact written the MOMENT it is computed

  PUBLISH — a weapon completes, not a clock strikes
     one read, rank, write a file per weapon that has every row, commit
       Cloudflare deploys the push

  PROTECT — a gate in front, a copy at another vendor behind
     nightly   D1.builds ──► library-backups branch (GitHub)
               two-way count ──► red if the database holds fewer
     always    guard_shrink ──► refuses to publish from a short library
```

**THE READ PATH TOUCHES NOTHING THAT CAN FAIL.** The database, the worker and
the runners can all be down and a reader still sees the board. That is what
makes it fast where the readers are and free at any traffic, and it is the
property every other choice here is arranged around.

**THE QUEUE IS A QUERY, SO THERE IS NO SECOND COPY OF THE WORK.** Nothing is
enqueued, so nothing can be lost and nothing can disagree with reality: a worker
that dies leaves the row missing and the next run takes it, and computing a
score twice costs time and nothing else because `f` is deterministic.

**PROGRESS IS MONOTONIC.** A fact is written when it is computed rather than
when a batch ends, so a run cancelled at 95% has kept 95%.

**A WEAPON IS NEVER INTERNALLY MIXED, AND ACROSS WEAPONS IT MAY BE.** A file is
written whole, so every row in it was measured against the data the build reads
today; the cross-weapon ranking can hold rows measured by different builds, and
every one of them is a real measurement — §"A mixture is allowed".

### What it costs, at each tier

| tier | runs on | free ceiling | steady state |
| --- | --- | --- | --- |
| serve | Cloudflare static assets | unmetered | — |
| write | Worker + D1 | 100k requests/day; 100k rows written/day | tens of submissions |
| compute | GitHub Actions | unmetered minutes, 40 jobs = 960 CPU hours/day | ~160 CPU minutes |
| store | D1 | 500 MB per database; the library is 4.4 MB and the facts about 5 | — |
| backup | a git branch | ~22 KB a night | — |

**The free tier carries all of this**, and the one thing that would not fit it
is gone: KV metered LIST and WRITE at a thousand a DAY, where D1 counts rows at
a hundred thousand and has no listing operation at all. The paid plan is taken
for Workers Builds concurrency rather than for any of these numbers.

### The rule efficiency is judged by

> **Every step's cost should be proportional to what CHANGED, not to what
> EXISTS.**

Finding the work becomes an indexed query, publishing becomes one query, and
and finding the rows a broken build wrote becomes `WHERE measured_by = ?`.

**THE ONE STEP THAT STAYS O(EXISTS) IS A FULL RESCORE**, and no store changes
that: forty jobs is the account's ceiling and one 121-minute row is a floor no
split goes under. The lever is not paying for one — a person deletes the rows a
change actually reached, and only those are rescored.


---

## One database, and the two things that are deliberately not in it

**ONE D1 DATABASE IS THE SYSTEM OF RECORD**, and the board holds three tables
in it: `inbox`, what players sent, verbatim, until intake has made a build of
it; `builds`, the library; and `scores`, the facts computed from it. `batches`
and `queue` are the work list beside them, and they may only ever CAUSE work.
Nothing else is a live store — there is no KV namespace and no R2 bucket.

The division is decided by two questions, asked of each piece of data:

> **Do you ever need to ask a QUESTION about the set?** — if so it needs a
> database, and nothing else will do.
> **What happens if it is lost?** — that decides how many copies it gets, and
> where they are.

| | question asked of it | if lost | where |
| --- | --- | --- | --- |
| the library | constantly | **gone for ever** | D1 `builds` |
| the facts | every operational one | recomputed, at 134 CPU hours | D1 `scores` |
| the published board | none — it is read | regenerated from the facts | git, served from the CDN |
| the snapshots | none | it IS the last copy | a git branch, at another vendor |

**WHY KV COULD NOT KEEP THE LIBRARY.** It has no queries, so every question
about the set had to be faked: "how many are there" became a counter key with
an hourly corrector, "is this build already held" became a read before every
write, and "which weapons are under-covered" could not be asked at all. Its free
plan meters LIST and WRITE at a thousand a DAY against D1's hundred thousand
rows, and it was the listing that took the board down.

**WHY R2 HAS NOTHING LEFT.** It held the score blobs, and a score is a row now.
The one job it might have inherited is the off-vendor copy, and it is the wrong
vendor for that (below).

**ONE STORE MEANS ONE POINT OF FAILURE FOR A WRITE**, and that is accepted: a
submission that fails is a retry, not a loss, and it failed the same way when KV
was down. What protects the library is not a second live store — it is a copy
somewhere else, and a gate in front of the damage.

### The library is protected in three layers, and they fail differently

1. **THE LIVE COPY** — D1. Where it is read and written.
2. **THE GATE** — `guard_shrink`. A run that comes back with materially fewer
   builds than the last board was built from REFUSES to publish. A backup
   restores after the damage; this declines to do it, which is the only one of
   the two that works while nobody is watching.
3. **THE OFF-VENDOR COPY** — `backup.yml` writes the whole library, one sorted
   record a line, to the `library-backups` branch every night: for ever,
   versioned, and about 22 KB a night because the file is sorted and git stores
   the delta.

**THE COPY MAY NOT LIVE AT THE SAME VENDOR AS THE ORIGINAL.** R2 is Cloudflare
and so is D1, so one account-level problem — a suspension, a mistaken delete, a
regional fault — takes both. The branch is at GitHub: another vendor, another
credential, another failure domain. That is the whole reason the snapshot is
where it is, and it is why R2 does not inherit the job.

**AND THE SNAPSHOT IS PUBLIC, WHICH IS WHAT MAKES IT FREE.** A record carries
the build and a DAY, and nothing about whoever sent it — no address, no token,
no time finer than the date. So the cheapest possible backup is also a legal
one. The `builds` table inherits that constraint: `at` is a date, and a column
recording anything finer would quietly retire this whole arrangement.

### A restore that points at a retired store is worse than none

`scripts/restore_library.sh` puts the snapshot back. It is the half that makes
the branch a backup rather than a hope, and CI runs its self-test — but a
self-test proves the SHAPE, not the destination.

So the migration is three changes and not two, and the third is not the tidying
up:

**THE RESTORE POINTS AT THE SAME PLACE THE WORKER WRITES.** A backup that runs,
commits and passes its self-test while restoring into a store nothing uses is
discovered by somebody who has just lost the library.
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

## A rescore costs the rows somebody deleted

**AND NOTHING ELSE.** A stored score is reused because it exists, so a run's
bill is the builds with no fact plus whatever a person retired — `WHERE weapon =
?`, `WHERE ruler = ?`, one row. `scripts/board_select.py` says which builds carry
the thing that was fixed, so the DELETE can be written by hand and priced before
it is run.


**THE BOARD STAYS A STATIC FILE, AND SAYS HOW FAR BEHIND IT IS.** Committed to
the repo and served from the CDN, which is what makes it fast and unblockable.
`GET /api/board/pending` answers the one fact the file cannot carry about itself:
how many builds the library holds. `SELECT COUNT(*) FROM builds`, and nothing
else — no build, no weapon, no day. The scorer records `submissions:` per board
and the difference is a footnote, SILENT when the board is current.

**ONE QUERY, WHERE IT WAS A WALK AND THEN A COUNTER.** Listing a KV namespace
took seven requests at the library's size against a free plan metering LIST at a
thousand a DAY, so a hundred and forty-three readers spent the day's allowance
and every board run afterwards died at its first step with `10048` until UTC
midnight — on the day a video landed. Avoiding the walk took a counter key and
an hourly corrector to keep it true. `COUNT(*)` replaces all of it, which is
most of why the library lives in a database at all.

Absent or unreachable, the endpoint answers `count: null` and the page draws
nothing: falling back to a walk would put the outage back where it was found,
invisibly.

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
