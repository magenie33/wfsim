# The board

The official leaderboard: **builds players submit, scored here**.

One sentence carries the whole design — **a submission is a BUILD and never a
number**. Everything else follows from it:

- a forged score is impossible, because no score is ever accepted;
- a change RE-SCORES stored builds instead of invalidating them, and nobody is
  ever asked to resubmit. What retires a stored score is a person deleting its
  row — §"A stored score is reused because it EXISTS";
- **THE STORE IS A LIBRARY OF BUILDS AND EVERY RULER CROSSES THE WHOLE OF IT**. A submission carries no score, so the ruler it happened to be
  measured under was never a property of the record. ANY fight can upload, and
  a new ruler is scored from the library the day it lands rather than waiting
  for anyone to resubmit;
- **…AND EVERY MODE OF THE WEAPON IS SCORED FROM IT.** The mode a build was
  tuned for was never a property of the record either — mods are equipped on the
  WEAPON and a mode is how it is fired, so nothing about a build can become
  illegal by being played differently. One submission is one row per
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
  it, because a second implementation is a second answer. There is one
  rule, it is the engine's, and the page ASKS it (`boardDoor`). The panel reports the run
  in aggregate ("7 of 10 uploaded · 3 × needs 8 mods"), since twenty rows off
  one search differ in their mods and not in why the board would not take them;
- every row is reproducible by anyone with the repo, since the score was
  computed by the engine that ships to their browser under the benchmark's own
  pinned seed. Wasm, Windows native and Linux native agree to the last bit
  (docs/WASM.md). What can make that untrue is not the engine but the CARRY: a
  score read back through a number parser that is not correctly rounding
  publishes `1.1070976928071057` where the engine computes `...055`, which is
  why serde_json runs with `float_roundtrip` and the scorer reads a score's
  literal (`exact_score`).

## Who computes the board

**EVERY NUMBER THE BOARD PUBLISHES IS A COMPUTE ORDER, FILLED BY THE COMMUNITY'S
MACHINES FIRST AND THE OFFICIAL ONES SECOND.** Five rules, and everything below
is how they are kept:

1. **ONE WAY IN.** Anything that needs a score — a submission, a rescore after
   the code moved, a refresh, a new ruler — asks the fact factory
   (§"Compute orders"). Nothing computes a board number another way.
2. **ONE WAY OUT.** A row becomes a fact in `scores` when `CLIENTS_PER_FACT`
   clients sent the same bits; nothing short of that reaches the public board.
3. **TWO KINDS OF COMPUTE, WITH FIXED JOBS.** The clients — browsers with the
   site open — are the first choice for every row. The official machines (the
   board server, `scores.yml`) do three things only: the FALLBACK for a row the
   clients did not finish by its deadline, the VERDICT when clients disagree, and
   the SPOT CHECK that keeps colluding clients a gamble.
4. **FOUR DIALS.** `CLIENTS_PER_FACT` (how many clients must agree),
   `RECHECK_SHARE` (how many agreements one more volunteer measures first),
   `SPOT_SHARE` (how many facts the official machines recompute anyway) and the deadlines
   (`DEADLINE_MS` in `worker/factory.js`, how long a row is the clients' alone
   before the fallback may take it). Changing how the board runs is turning one
   of them.
5. **ONE ENGINE.** A client works only while its page runs the engine the site
   serves (`release.json`'s `engine`), and a fact is made by that engine's
   bits; an older engine's result is carried to be confirmed, never taken as
   one.

So the compute grows with the time people keep the site open, and the official
compute is what the clients leave over.

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
| a build is legal | `engine::board::builds::validate_for_board` | the page's door, the worker's door — both ASK it |
| a build's axes | `engine::board::builds::BUILD_AXES` | every surface's fields, `/api/meta` |
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
ranks; `engine::board::benchmarks::core_metric` is where the rule is applied, at
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
| the board the site reads | R2 bucket `wfsim-public`, served at `cdn.wfsim.app/board/<weapon>.json` | `scripts/live_board.sh`, the moment `scores` moves |
| the board's daily snapshot | `site/board/<weapon>.json` | `publish.yml`, committed, read by people |
| ranked across weapons | `site/board/index.json` | derived from the files beside it |
| which board this is | `site/board.meta.json` | a digest per published file |
| consent + submit | `web/src/static/app/68-board-submit.js` (`offerBoardSubmit`) | the player's browser |
| the library | the Postgres database `wfsim` (binding `LIBRARY`, §"Postgres") | written by the endpoint |
| the facts | the `scores` table in it | written by `ship_facts.sh` |
| the deploy | `wrangler.jsonc` | `scripts/deploy.sh`, from a git push |
| the endpoint | `worker/index.js` | the Cloudflare Worker, same origin |
| the scorer | `cli/src/board/` (run by `cli/src/bin/wfsim-board.rs`) | the scheduled job |
| the automation | `.github/workflows/scores.yml` | GitHub Actions |

**THE ENDPOINT STORES THE WHOLE BUILD.** Every axis the page sends is written,
and every axis is part of the identity hash, or two builds differing only in a
dropped axis collapse onto one key. `scripts/check_board_submit.mjs` asserts
both against every axis, derived from a real payload rather than listed.

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
| `publish.yml` | the clock, once a day | `scores` is ranked into the `site/board` SNAPSHOT, which the site does not read | one runner, seconds |
| `live_board.sh` | every 20 s, on the board server | `scores` is ranked into the LIVE board in R2, which the site reads | two cores: seconds for the weapons a cycle moved; minutes to start, deriving the library once |

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
`SCORE_DEADLINE_MINUTES` whatever the depth — shared out across the rulers by
WHERE THE WORK IS, because the binary runs once each — and an hour with nothing
owed costs ONE job, because the gate answers `todo=0` and the fan-out never
happens.

**AND NOTHING ELSE BOUNDS THE CADENCE.** The repository is public, so Actions
minutes are unlimited.

**A RUN'S WORK IS DECIDED AT ITS START AND NEVER CHANGES.** The queue is read
once, in the first job, and handed to every shard as an artifact — which is what
lets them agree on who owns which row without talking. Builds that arrive while
a run is fighting are picked up by the next hour's reconciliation.

**A ROW NO BUILD CAN PAY IS FORGOTTEN, AND A ZERO IS PAID.** A queue row is
deleted beside the fact that settles it, so the reconciliation's dry run names
every queued (build, mode) that a full walk of the library never produced —
a build the ruler refuses, a mode the weapon lost, an id the identity rule has
since moved — and `purge_orphans.sh` forgets them; the page's "rows still
measuring" is otherwise a floor no run can lower. A fight that RAN and dealt
nothing is a measurement and is banked as 0 (the entry line keeps it off the
page); only a fight that did not run is refused.

**AN ORDER IS CANCELLED WITH `cancel_queue.sh`, NEVER A BARE DELETE.**
`--all`, `--batch <prefix>` and/or `--ruler <id>` name the queue rows
(`--dry-run` counts them); `queue.yml`'s `cancel` input runs it for one batch,
which is how the order center (wfsim-cloud) cancels. Each row was asked of the factory, so the cancel
deletes the rows, withdraws each board demand whose row no batch owes any more
(`board_owed`), and withdraws each question left with no live demand. It
works in slices of `CANCEL_SLICE` (20 000) demands, the head of the factory's
order first, each write its own transaction taking rows with SKIP LOCKED so a
sync is never waited on; a final pass takes what a lock skipped and the queue
rows no open question asks for. A queue row deleted alone leaves its question
open at the head of the order, where every pick withdraws it at a pack's worth
an ask. A row the library still owes is queued again by the next
reconciliation, so a cancel that should hold stops that writer first.

**NOTHING ASKS FOR A MEASURED ROW AGAIN ON THE CLOCK.** A row is measured again
only when a person asks — the rescore button, or a batch by hand. The engine id
moves on edits that move no number, so a re-ask driven by age or by the engine
id would rescore the whole board after a wording change. `queue.yml` holds no
such step, and `check_rescore_paths` fails on one.

**THE PUBLISH DOES NOT CARE WHAT IS BEING COMPUTED.** It reads the table, ranks
it, writes the files and commits if they moved. A scoring run may be halfway
through a batch when it fires; it publishes what is there, and the next one
publishes what is there then. It has no `needs:` and waits for nothing.

**NOTHING IS LOST BY PUBLISHING EARLY**, and that is what makes the indifference
safe. Asking for a row again is a QUEUE row and `scores` only ever grows — a
build being re-measured keeps its old number until the new one replaces it, and
a build with no number was never on the board.

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

**A PUSH DOES NOT START A RUN.** It would do exactly what the next scheduled
one does, while competing for the same slots.

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

**NO HASH OF THE INPUTS DECIDES REUSE.** A hash fires on every edit to a file
no entity owns — comments, tests, validation rules, fields only the page reads —
and stays silent on the one case that matters, a code change that moves a
number. A hash of `engine/` says the BYTES moved, which is a different question
from whether any NUMBER did.

TIME IS NOT AN INPUT, which is why there is no cooldown. An untouched row is
valid forever; a row whose engine moved is wrong immediately, not in an hour. A
cooldown would be both too slow and too fast at once.

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

**AND THE WORK IS PRICED PER RULER.** A row with a fact costs what it last cost;
a row with none is charged what THIS RULER'S measured rows averaged, which is
`unmeasured_row_seconds`. One figure across all of them is the same error as a
count: the mean row is 5.8 s under `demolisher` and 25.3 s under
`standard_multi_target`, so a backlog tilted onto the expensive ruler is
provisioned a quarter of the shards it needs and every one of them runs out of
clock with the queue still full. **The mean and not the median**, because what
is being sized is a sum and these distributions are skewed — 3.8 s median
against 6.1 s mean — so summing medians under-counts a slice by a third before
any ruler is mispriced at all.

**THE SHARD'S CLOCK IS SPLIT THE SAME WAY.** The binary runs once per ruler, so
each gets its share of `SCORE_DEADLINE_MINUTES` by that same measured work,
floored at a minute. Split EVENLY instead and a backlog sitting on one ruler —
which is the usual shape, since a rescore names one and a new ruler starts empty
— gets a third of the clock while the other two return in seconds. The floor is
what may carry a shard a minute per idle ruler past the budget, and it stays:
a ruler whose dry-run gives no answer reports no work, and a share with no floor
under it would hand that one zero seconds and stop scoring it silently.

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
0.4% of the rows are 20% of the multi-target bill.

A `--deadline` asked only before a row is dealt says when to stop TAKING rows
and cannot touch one already in flight. **One row then sets the makespan**, and
the worst of them sets it at ninety-five minutes.

So it reaches inside. The clock is asked between every RUN of a row, not only
between rows, and what a row has finished is banked: each sub-measurement under
its own label and the one that is partway as a cursor. The next run resumes
there.

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

### The assembly

**PUBLISH ASSEMBLES; IT DOES NOT FIGHT.** `wfsim-board --project <dir>` writes
every ruler's board in one pass (`cli/src/board/assemble.rs`); `--serve` is the
same assembly held in memory for the live board (§"The live board"). A row is
published from its fact and from nothing else, and a row nobody has banked is
absent from this board and lands on the next one, which is the convergence rule
applied to the assembly.

**A BUILD IS READ ONCE, NOT ONCE PER RULER.** The door's legal half
(`board_rows::board_build`) is the same for every ruler and is most of what the
door costs; each record goes through it once, and each ruler adds only its own
admission (`board_rows::admitted`, `admit_for_board`).

**A WEAPON IS RANKED ALONE.** Its rows under one ruler are ranked, cut to one
per riven shape and to the entry line, and accounted for, with no other
weapon's rows in sight — so re-ranking only the weapons a fact moved writes the
same files as assembling everything, which
`a_board_moved_a_fact_at_a_time_is_the_board_from_scratch` holds it to. A
weapon's file in a directory (the base board, or a scene's) holds the rows of
every ruler of that directory, in roster order, and the directory's index holds
each group's leader. A weapon with no row has no file.

**A DIRECTORY OPENED WITH FILES IN IT** (the committed snapshot) keeps the rows
no live ruler rewrites: a row of a ruler writing that directory is replaced, by
family, so a `_vN` row goes with it; a retired ruler's row is dropped, or
nothing could ever clear it; a row it cannot read is kept, and so is its file.

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
python scripts/board_select.py --mod 'galvanized_*' --board standard_single_target --plain
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
produce the SAME key with different numbers. The `scores` table keys on
`(build, ruler, mode)`, and `load_facts` filters on the ruler as it reads, so
the two cannot meet. A value identified by less than what determines it is the
general shape of a defect in this pipeline.

The generated files are NEVER rebased. There is no sense in which two versions
of a computed board each hold something worth keeping, so a three-way merge can
only produce a conflict. The run that just scored takes whatever base is
current and writes its numbers on top.

## The live board

**WHAT THE SITE SHOWS IS `scores`, RANKED THE MOMENT IT MOVES.** Every 20 s
`scripts/live_board.sh` on the board server reads the rows `scores` gained
(`live_publish.mjs delta` — a verification, the scorer, the server's own
settling), hands them to the ranker, stamps the result with `board_meta.py`
and pushes the files that changed through
`PUT /api/board/live/<file>` (bearer `BOARD_PUSH_TOKEN`), the stamp last, into
the bucket `wfsim-public` under their public paths, each kept thirty seconds.

**THE PAGE READS IT WITH NO WORKER IN THE WAY.** `cdn.wfsim.app` is that
bucket on its own domain, read through the CDN with `Access-Control-Allow-Origin:
*`, and the page on wfsim.app reads every board file there (`BOARD_ORIGIN`,
`boardOffOrigin`). `worker/live_board.js` still answers `/board/<file>` and
`/board.meta.json` on wfsim.app from the same bucket, for every reader that
names that address — a desktop shell in the field, the bots, the MCP server,
the relay. Everything in that bucket is public, so nothing private is ever
written to it.

**THE RANKER HOLDS THE BOARD; A CYCLE COSTS WHAT IT MOVED.** `wfsim-board
--serve` (§"The assembly") reads the library and every fact once when it
starts, writes the whole board, and then takes one command a line on stdin:
the builds the cycle took in, the facts it read, `publish`. A publish re-ranks
only the (ruler, weapon) pairs those moved and writes only the files whose
bytes changed, so a cycle's cost follows the facts that arrived and never the
size of the library. A new build moves a row only once it has a fact.

**EVERYTHING IT WAS SENT IS ON DISK** (`library.json`, `new-builds.ndjson`,
`facts-known.ndjson`), so a ranker that stops — a reply other than `ok`, a
crash, a deploy — is started again from them on the next cycle and loses
nothing.

**THE HOURLY READ OF THE STORE RUNS BESIDE THE BOARD**, not in front of it: the
read takes most of an hour, into spares, and once both are whole the ranker
rebuilds from them (`reload`), which is how a row `scores` lost leaves the
board. The hour is counted from the end of the last read (`fetch-at.txt`), so a
restart starts no read the hour does not owe.

**A STALLED BOARD IS ASKED ABOUT EVERY HOUR, off the board server**
(`board-age.yml`, `scripts/check_board_age.py`). The ranker stamps every ruler
on each publish and each read of the store, so the newest
`scored_at_epoch_seconds` in the public stamp older than four hours means the
board stopped reaching readers, and the run fails, which mails the owner.
**A STARVED FLEET IS ASKED ABOUT EVERY HALF HOUR** the same way
(`dispatch.yml`, `scripts/check_dispatch.py`): it fails on `demand.json`'s
`now.starved`, and on a picture a quarter hour old, a daemon that stopped.

**UNVERIFIED NUMBERS NEVER REACH IT.** A client result is on no board until it
is a fact (§"Compute orders").

**THE COMMITTED BOARD IS A SNAPSHOT.** `publish.yml` writes `site/board/` once a
day for people and for the record; the worker reads it only until the first
live stamp exists, and never again after.

## The heads

**A SHORT ANSWER READS THE HEADS, NOT THE BOARD.** A weapon's file holds every
row it kept, in every scene, which for a much-surveyed weapon is tens of
thousands of rows; a top-N answer — a bot's long image, `builder.board.read`
— needs a few. Each cycle `scripts/board_heads.mjs` cuts each weapon's rows,
every scene joined, to `boardHeads` (`87-headless.js`, the page's own code) into
`board/heads/<id>.json`, pushed with the rest of the live board: every row
holding one of the first `BOARD_HEADS_PLACES` places of its ruler, mode and
riven-ness, and each ruler's newest-measured row. Any ranking the page draws,
run on the heads, places its first `BOARD_HEADS_PLACES` as on every row, ties
and their preferred build included; `check_board_heads` holds that over the
whole snapshot. The heads are rows in the board file's own shape, so a reader
changes the file it reads and nothing else. A weapon's heads are written again
only when one of its files is newer. `heads/` is no scene (`board_meta.py`
`HEADS`), and the daily snapshot carries none.

## The fact factory

**ONE THING IS DONE WITH THE VOLUNTEERS' COMPUTERS: A QUESTION BECOMES A
FACT** (`worker/tasks.js`). A QUESTION is a verb on a canonical request, its id
the hash of both; an ANSWER is one witness's canonical result; a FACT is enough
independent witnesses with equal bits — two owners on two networks, or the
official machines alone. Whatever asks is a PRODUCER: it `demand`s a question
with a deadline and is told the fact (`on_fact`); the factory names none of
them, so a new kind of task is a new producer (`worker/factory.js`) and the
factory does not change. Its producers are the board's rows (§"Compute
orders") and riven gains, a chat's and a survey's (docs/AGENT.md §"Riven
appraisal"); the official machines are witnesses of the same factory
(`/api/tasks/official/*`, the factory's secret), taking what is past its
deadline and what only they may settle.

- **ORDER** (§"Fair share between producers"). A question a person waits
  on now first; then the PRODUCER that has used the least compute, among those
  with a question this witness may answer; inside it, earliest deadline first:
  a chat's in two minutes, a new build's row in six hours, a survey shape in a
  day, a refresh in four, a rescore's or a sweep's in a week (`DEADLINE_MS` —
  the dispatch policy, stated once). A pack takes first, in that order, the
  questions another volunteer has answered, on whatever engine
  (`led_questions`): one more agreeing answer makes each a fact and pays its
  first witness, where a batch due sooner would leave it waiting behind every
  row of that batch; an older engine's answer joins a served one that
  reproduces it, so a release leaves none of them behind.
- **ELIGIBILITY IS ONE QUERY.** What a witness may answer — not its own, its
  owner's other devices', nor its network's, with the cores a waited-on
  question needs at its age — is decided in the SQL that picks it, never by a
  filter after a `LIMIT`, so a witness is handed nothing only when nothing
  exists it may answer (`check_tasks`, LIVENESS).
- **EVERY STATE HAS AN EXIT** (TERMINATION). Answers to the verb's cap that
  never agreed are `disputed`, or `nondeterministic` when every one differs from
  every other — an engine fault, refusing nobody; both, and a `spot` check of a
  fact, wait for the official machines, who refuse a witness they disprove only
  in a dispute or a spot check. A question nobody asks for any more is
  `withdrawn`.
- **ONE COMPUTATION, CREDITED ONCE.** Two producers asking one thing share one
  question; the fact goes to both, and its witnesses are credited its work once.
  A fact the served engine already holds is told to a later ask at once; one of
  another engine opens again, its answers carried for the new engine to
  reproduce or replace.
- **THE WATCHDOG** (`watchdog`, every ten minutes): a producer whose overdue
  questions got no answer in half an hour while witnesses worked, a
  non-deterministic question, a dispute or spot nobody settled in two hours
  since its last answer, a backlog past due (a thousand questions, or one three
  hours late — a refresh past due is a row past fifteen days), and clients disagreeing with the hour's facts on
  the served engine (thirty answers and 3% of them, by owner) are found every
  round. So is a scene still
  filling, its cell rulers' rows measured, measured in six hours, and owed —
  a cell that measured none is said as needing someone, and its time left is
  worked from the last 24 hours (`PACE_HOURS`). Each is said with a
  level, what it means and what to do (`worker/watchdog_say.js`). **COMPUTERS ON
  BUT IDLE** (`worker/idle_devices.js`): an id that asked in the last half hour
  yet holds no row and answered nothing in it — on an old engine, asking more
  than twice what a page asks when handed nothing, or on the served engine and
  not computing. **A STARVED FLEET** (`starved`) is read off the daemon's
  `demand.json` (`now.starved`), the verdict the scheduled check fails on.
  **WHAT IS FOUND IS WRITTEN DOWN** in `watchdog_said` as a
  round — every finding at once, one row each under one `at` — at most hourly,
  and within ten minutes of a trouble after a quiet spell; kept thirty days.
  Nothing is sent to anyone: whoever fixes it reads the latest round,
  `SELECT cause, said FROM watchdog_said WHERE at = (SELECT max(at) FROM watchdog_said)`.
- **WHO IS ON**: a page's ask for work is recorded once a minute
  (`verifiers.asked_at`, with its engine and free cores), so a machine asking
  and holding no lease is idle, to the minute, and one holding a lease computes.
  **ASKING** — what a pack's fair cap, the fair table and the settler's
  snapshot divide by — is one definition (`askingIds`, worker/tasks.js): asked
  within the caller's window OR holding a live lease, since a protocol 8 client
  deep in a ten-minute pack asks only as it ends.
- **PROTOCOL 8** (`/api/board/sync`, worker/sync.js; docs/CLIENTS.md) hands out
  by the same `headOf`, `nextPack`, fair table and pack cap as protocol 7, in
  packs of `pack8_ms` leased `sync_lease_ms` (`pack_lease_ms` while work is
  short), and holds each client to its `next_ms` (`verifiers.next_at`). A
  question its engine refused (`engine_refusals`) is not handed to that device
  again; refused by `refusals_to_withdraw` owners on the served engine it is
  withdrawn, written to `refusal_withdrawals`, said by the watchdog
  (`refused_rows`), and stays withdrawn on that engine when asked again.

### Postgres

**TWO POSTGRES DATABASES ON THE OWNER'S SERVER, AND NOTHING ELSE**
(`worker/db.js`). The worker reaches them through Hyperdrive; scripts and CI
through a Cloudflare Access TCP tunnel to `pg.wfsim.app`.

| database | holds | worker binding | DDL |
| --- | --- | --- | --- |
| `wfsim` | the library and the fact factory, one database and one transaction — inbox, builds, scores, batches, queue, orders, shares, usage_days, usage_points, bot_inbox, appraisals, appraisal_results, fissures, donors, rates, donations; questions, demands, answers, verifiers, verifier_hours, watchdog_said, points_pool | `HYPERDRIVE` → `LIBRARY` | `worker/pg/schema.sql`, `worker/pg/library.sql`, `worker/pg/sync.sql` |
| `wfsim_accounts` | the accounts — every table the open half and the paid half name, under their own role | `HYPERDRIVE_ACCOUNTS` → `ACCOUNTS` | `worker/pg/accounts.sql` |

`withDatabases` opens each from its binding; every query is Postgres text.
Every text column is `COLLATE "C"`, so an ISO instant compares byte by byte
whatever the server's locale; the last slot's trigger is a function and a
trigger.

- **A SCRIPT REACHES A DATABASE THROUGH ONE DOOR.** `scripts/db.sh` and
  `scripts/db.mjs` take one statement and answer from whichever database
  `PG_URL` (`PG_ACCOUNTS_URL`) names; the workflows set it through
  `.github/actions/databases`, the board server through
  `deploy/wfsim-pg-tunnel.service`. The factory's tables are read through the
  official door's `report`s.
- **A NODE CHECK RUNS THE WORKER'S OWN SQL**: `scripts/pg/test_db.mjs` lays
  `worker/pg/*.sql` in an in-memory PGlite (`testDb("library")`,
  `testDb("accounts")`) behind the statement API the worker gets;
  `check_pg_scripts` runs the pipeline's scripts on Postgres.

#### The sync in the database

**WHAT A WITNESS MAY TAKE, AND IN WHAT ORDER, IS STATED ONCE, IN SQL**
(`worker/pg/sync.sql`): `factory_eligible` (what a witness may answer),
`factory_head` (the head of its order, fair between producers),
`factory_take`, `factory_take_pack`, `factory_charge` and `factory_lease_pack`
(a pack picked, leased and charged),
`factory_asking`, `factory_open`, `factory_restate`, `factory_takeable`,
`factory_supersede` and the board's `board_owed`. Every caller in
`worker/tasks.js`, `worker/verify.js` and `worker/sync.js` calls them, so no
path can hand out by a rule of its own. They are SQL functions the planner
inlines, so each is the query it replaces. The writes a sync makes are stated
there once too: `verifier_seen`, `verifier_asked`, `verifier_asks`,
`factory_held`, `factory_given`, `factory_answer`, `factory_learn`,
`factory_queue`, `verifier_worked`, `factory_renew`, `factory_release_unnamed`,
`factory_refuse`, `factory_states`. **THE WORKER NEEDS THEM**: apply
`sync.sql` before a deploy that calls a function it adds.

**A PROTOCOL 8 SYNC IS ONE CALL** with the dial `sync_proc` at 1: `CALL
board_sync(p, r)` does in one transaction what `worker/sync.js` otherwise does
statement by statement, in the same order and through the same functions — a
sync is then one wait on the database and one on the accounts (the device's
span, for its points), where statement by statement each is one. `check_sync_proc` holds the
two paths to the same replies and tables. What stays in the worker, and why:

- **the canonical answer** (`VERBS[verb].canon`): only JS writes the bytes two
  equal answers share, so each result comes with its canon for every verb;
- **the fair-share dice and table** (worker/fair.js): the table is the
  worker's, kept a minute; the call is told whether this account is throttled
  should work be short;
- **who a device is** and **its span** (the accounts database), and every
  instant and dial, worked out once before the call;
- **a question someone waits on** (`outcome: alone`): it is leased by `next`,
  whose producer may be told of the lease;
- **settling in the request** (`settle_async` 0): the answers are recorded and
  settled first, as the statements do, and the call does the rest — settled
  inside it, a pack would be chosen before the facts it waits on;
- **whatever needs a producer's JS** — an answer the settler may not take, a
  pack row a producer judges in JS (`wanted_in_sql` unset) — raises `WS001`:
  nothing the call did is kept and the statements run instead.

`sync_proc` at 0 is the statements, and the way back.

#### Setting a server up

The databases, their owners, the DDL — applied as each file's header says — and
the pipeline's roles:

```sh
sudo -u postgres psql -c "CREATE ROLE wfsim LOGIN PASSWORD '…'" -c "CREATE DATABASE wfsim OWNER wfsim"
sudo -u postgres psql -c "CREATE ROLE wfsim_accounts LOGIN PASSWORD '…'" -c "CREATE DATABASE wfsim_accounts OWNER wfsim_accounts"
sudo -u postgres psql -d wfsim -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim' -f worker/pg/schema.sql
sudo -u postgres psql -d wfsim -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim' -f worker/pg/library.sql
sudo -u postgres psql -d wfsim -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim' -f worker/pg/sync.sql
sudo -u postgres psql -d wfsim_accounts -v ON_ERROR_STOP=1 -c 'SET ROLE wfsim_accounts' -f worker/pg/accounts.sql
sudo -u postgres psql -d wfsim -c "CREATE ROLE wfsim_pipeline LOGIN PASSWORD '…'" \
  -c "GRANT SELECT, INSERT, UPDATE, DELETE ON inbox, builds, scores, batches, queue TO wfsim_pipeline" \
  -c "GRANT SELECT ON verifiers, verifier_hours, verifier_asks, adjustments, factory_events TO wfsim_pipeline" \
  -c "GRANT SELECT (code, channel, request) ON appraisals TO wfsim_pipeline"
sudo -u postgres psql -d wfsim_accounts -c "CREATE ROLE wfsim_accounts_pipeline LOGIN PASSWORD '…'" \
  -c "GRANT SELECT ON devices TO wfsim_accounts_pipeline"
```

A Hyperdrive config for each database as its owner, bound in `wrangler.jsonc`
as `HYPERDRIVE` and `HYPERDRIVE_ACCOUNTS`. Repository secrets
`CF_ACCESS_CLIENT_ID`, `CF_ACCESS_CLIENT_SECRET` (the Access service token
`pg.wfsim.app` admits), `PG_PIPELINE_URL`
(`postgres://wfsim_pipeline:…@127.0.0.1:15432/wfsim`),
`PG_ACCOUNTS_PIPELINE_URL` (`…wfsim_accounts_pipeline…/wfsim_accounts`). On the
board server: cloudflared, `/etc/wfsim-pg-tunnel.env`, the tunnel's unit, and
`PG_URL=postgres://wfsim_pipeline:…@127.0.0.1:15432/wfsim` in
`/etc/wfsim-live.env`.

### The dials

**EVERY NUMBER THAT DECIDES HOW WORK IS HANDED OUT IS A DIAL** (`worker/dials.js`):
a pack's length and size, the leases, how long a row is one computer's, how
often an ask is written down, the database's safety valve (`compute_cap`). The
code holds each one's default and bounds and nothing else; the owner sets a
value in `factory_dials`, every change lands in `factory_dial_log` with the
value before and after, and an isolate reads the table at most once a minute
(`DIALS_FRESH_MS`), so a change is in force within a minute and needs no
release. A value outside its bounds is refused when set and ignored when read;
a dial never set is its default. A client is told the dials it reads in every
reply (docs/CLIENTS.md §"Values"). `CLIENTS_PER_FACT` is not a dial: two owners
per fact is the board's promise, not a capacity knob. `check_dials` holds this.

### The settler

**WHAT THE FACTORY WORKS OUT AWAY FROM A REQUEST IS WORKED OUT BESIDE THE
DATABASE** (`worker/settler.js`, run by `scripts/factoryd.mjs` as
`deploy/wfsim-factoryd.service` on the database's own machine, over its unix
socket, so a transaction costs no round trip). It calls the worker's own code —
the fair table is `tableOf`, a witness is `independent` — so the two can never
judge by different rules, and it ships from the same commit as the worker
(`scripts/ship_factoryd.py`, which runs one pass of every tick before it swaps
the copy in). It writes:

- `factory_snapshot`, every ten seconds: what is open and waited on, who is
  asking and computing (the worker's own definitions), the queue, and its
  heartbeat with the commit it runs;
- `fair_shares`, every minute: the fair-share table for the served engine;
- an official check of another engine undone, every minute
  (`factory_moot_checks`, §"A release carries a result");
- nothing past the window, every minute (`factory_forget_past`): **THE
  LIBRARY KEEPS FOURTEEN DAYS** (`KEPT_MS`), so its size follows the board and
  the window, never the days it has run. A fact of an engine no longer served
  and a withdrawn question, settled before the window, go with their answers,
  their board demands and their refusals; the served engine's facts stay, the
  cache a row asked again is told at once, and so does a question a demand
  waits on, one a producer outside the board asked, and one its engine refused.
  What a device earned is in `verifier_hours` and stays; a ledger's per-answer
  counts and a refusal reach back the window;
- `factory_findings`: what its comparisons found, one row a kind.

ITS ROLE, `wfsim_factoryd`, reads only what its ticks read. A tick that reads
a table the role lacks fails its `--once` pass and `ship_factoryd.py` keeps the
old copy running, so a new read ships with its grant:

The grants are defined in `deploy/factoryd_grants.sql` and
`deploy/factoryd_accounts_grants.sql`, applied by
`python scripts/ship_factoryd.py --bootstrap` (docs/SERVERS.md).

**THE WORK SIGNAL** is the snapshot published: after every snapshot, and at
once when open or waited-on work appears from none (a two-second probe), the
daemon writes `work.json` to the public bucket, which `cdn.wfsim.app` serves
with `max-age=5` — written straight to R2's S3 API (`scripts/sigv4.mjs`), no
worker call. An idle computer reads it instead of asking:

```text
{ v: 1, at, open, urgent, engine, release, protocol, protocols, computing,
  poll_ms, jitter_ms, urgent_jitter_ms, mode }
```

`poll_ms`, `jitter_ms` and `urgent_jitter_ms` are dials (§"The dials"), and so
is `mode`: `signal_mode` 1 makes it `ask`, sending every computer back to the
worker. Without the R2 token the signal is skipped and `signal_unconfigured`
says so; read back through the CDN every minute, three reads in a row older
than 30 s are `signal_stale` and three not cached at the edge are
`signal_uncached`.

**IT SETTLES A VOLUNTEER'S BOARD ANSWERS** while dial `settle_async` is 1.
`recordMany` writes the answer and queues its questions in `settle_queue`,
in the same transaction, and returns; every two seconds the daemon's
`settleBatch` claims up to `settle_batch` of them (`FOR UPDATE SKIP LOCKED`), in
question order. It settles each one through `settleMany`, the request path's own
code: `decide`, the credit, and the producer's `on_fact_writes`. Then it gives
back the lease of a holder that answered. Only a packable verb whose producer
has no `on_answer` is queued; a riven's producers are told every answer, so
they and every official answer still settle in the request.

- A queued question goes to nobody for `settle_grace_ms`. After that it is
  handed out again, so a dead settler holds no work.
- An answer to a question that is already claimed clears the claim. The pass
  that claimed it then leaves the row queued for the next pass.
- A pass that dies is claimed again a minute later. Settling twice changes
  nothing: a state moves only from `open`, and a credit is one transaction
  that pays only answers still unpaid.
- What a settlement committed but never did is the watchdog's: `repay` and
  `redeliver`, each reading the facts formed since its cursor (`factory_cursors`),
  `repay` an hour of them a call. Each runs apart: one that throws keeps no
  round from being written.
- If the heartbeat is five minutes old, the watchdog empties the queue itself.
- Dial 0 queues nothing more, and the daemon empties what is already queued.

**THE REST RUNS IN SHADOW.** The worker still works out its own fair table. The
shadow checks itself against the request path two ways, and each check writes a
finding:

- `settle_shadow`: every settled question, two minutes after its fact, judged
  again from its answers as they stood — the fact's bits must be a group of
  `needs` independent witnesses, and each of them credited. A question an
  official answer settled, or one where an older engine was a peer, is the
  official path's or the grace's and is counted apart, not judged.
- `fair_shadow`: the worker's ten-minute cron (`watchdog`) compares the
  daemon's table with its own `tableOf` now, within 0.05 of a target or a
  share, since asks and answers land between the two readings.

`check_settler` holds the shadow. `check_settler_async` holds the queue: what
the request path leaves, and what one clean pass leaves after a pass that died
at any call.

### Fair share between producers

**EARLIEST DEADLINE FIRST ALONE COLLAPSES UNDER OVERLOAD**, and this work never
runs out: the batch due first takes every computer and every other producer
misses its deadline (a survey's thousands of shapes, due together, hold the
board's rows at zero for hours). So deadlines order work INSIDE a producer and
compute is shared BETWEEN producers, as Linux's CFS shares a CPU and BOINC a
volunteer's machine among projects — no amount of compute is set for any
problem, and a new kind of task is a new producer that takes its share.

- **`producer_use`**, a row a producer: `used`, the compute charged to it,
  over its `weight` (1 for all); `answer_ms`, the running mean of its answers'
  fights (`PRODUCER_MEAN_SHARE`, about the last sixty).
- **A HAND-OUT IS CHARGED** each question's `answer_ms`, in the statement
  that leases it: an expensive survey shape counts as dozens of board rows, so
  shares are of compute, not of questions. A lease nobody answers was still
  handed out, and stays charged.
- **THE PICK** (`fairSql`): each producer's head among what this witness may
  answer, one probe each on `(state, producer, waits DESC, due_at, id)`; of
  those, the waited-on first, then the least `used`. A pack is one producer's
  rows. Where no producer's row finds a question the plain order does, so
  liveness never rests on `producer_use`.
- **NO SHARE IS BANKED.** A new producer starts at the least any has used, and
  the watchdog lifts one with nothing open to the least used among those with
  open work (`liftIdle`), so one that comes back after a quiet day is level,
  not owed a day.
- `questions.producer` is the producer of its earliest-due live demand, kept
  where `due_at` is (`demandAll`, `restate`).

### Who asked

**EVERY ASK NAMES ITS INITIATOR**, so one asker cannot hold the community's
computers and so the work done for players can be told from WFSim's own.
`demands.initiator`, and `questions.initiator` taken from the earliest-due
live demand exactly as `producer` is (`demandAll`, `factory_restate`):

- `system` — WFSim's own: every board batch but a player's arrivals
  (`initiatorOf`: refresh, rescore, sweep, backfill, a scene's first pass) and
  a survey.
- `bot:<code>` — a riven gain asked in a chat, the code of its channel and asker;
  its agreed build goes to the board's door as the same asker, a survey's as
  `system` (`submitRecord`).
- `user:<code>` / `anonymous` — a player's submission signed in / signed out.
  `submit` writes it beside the record in `inbox`; `wfsim-intake` carries it
  to the fight the arrival named and to every row it owes (`--asked`,
  `--orders`; the first named one when two ask one row) and never into the
  library; `ship_queue.sh` keeps it on the `queue` row, and
  `ask` reads it there, never from the runner (`initiatorOf`). A backfill's row
  is the system's though it is asked as an arrival is.
- NULL — not recorded: every ask before this, never back-filled.

**A CODE IS A KEYED HASH** (`initiatorCode`, keyed by `AUTH_SECRET`): the
board's database cannot turn it back, the accounts' side makes it again from
the same id, and a computing browser's id is never one (§"Contribution").
**AN ASK THAT IS OVER KEEPS NO CODE**: the settler erases it from every demand
no longer live and every question no longer open (`factory_forget_codes`, dial
`initiator_forget_ms`); a retracted fact's demand comes back live without one.

**THE SHARE** (`factory_capped`, in `factory_eligible`): a `user:` or `bot:`
initiator holding more of the live leases than dial `initiator_share_pct` of
them all, and more than `initiator_floor_rows`, is passed over until it holds
less; the rest is handed out as before. `system` and `anonymous` are never
passed over. Leases are counted as rows, not compute. Off while the share is 0.

### Merit

**WHAT VERIFIED COMPUTE BUYS BACK FROM THE COMMUNITY'S COMPUTE, AND NOTHING
ELSE** (`worker/merit.js`; the accounts' `merit`, `merit_ledger`). Never bought,
never moved between accounts, never decaying, never exchanged for a membership,
a paid feature or money; it changes no number, score or witness rule.

- **MINTED** from contribution points one for one, by the worker's ten-minute
  cron (`mintMerit`). An account first seen is GRANTED all its points so far,
  up to its balance cap; after that each mint adds the points earned since
  (`points_seen`), within what the UTC day's cap and the balance cap leave
  (`merit_room`). A point past either cap stays a contribution point and mints
  nothing later.
- **THE CAPS** follow the contribution rank (`contributorRank`), linearly:
  a day's mint `merit_daily_base + merit_daily_per_rank × rank` (16,000 + 500 ×
  rank), the balance `merit_balance_days` (10) days of it. Dial `merit_on`.
- **AN ACCOUNT SEES ITS OWN** balance, caps, today's mint and ledger
  (`/api/account/merit`, its export); **THE PUBLIC SEES SUMS** by kind
  (`/api/merit`, carried in the ranking's file as `merit`), never a row.
- **SPENT ON ONE THING**: a player's own rows on the leaderboard, sooner
  (§"Merit priority"), offered while dial `merit_priority` is 1.

### Merit priority

**A SIGNED-IN PLAYER'S OWN ARRIVALS, SOONER, PAID ONLY WHEN THEY WENT AHEAD OF
SOMEONE** (`holdPriority`, `settleMerit`). Waited-on work still goes first and
is never held; a system row, an anonymous one and a row the board already
scored never move.

- **THE SWITCH** (the account panel, `/api/account/merit`) is the consent: while
  it is on, `merit_codes` maps the account's `user:` code to it, and turning it
  off deletes the mapping. Nothing else maps a code to an account.
- **AT THE ASK** (`ask`, an `arrivals-` batch): each row whose queue row names a
  switched-on account is priced — the median work of the recent paid answers on
  its ruler and mode, in points, up to the next ten, else
  `merit_price_default` — and, while the balance pays it, held: the price leaves
  the balance (`spend`), a `merit_holds` row keeps the old and the new
  deadline, and the row is due `merit_priority_pct` (30%) of its deadline
  sooner. At most `merit_rows_per_hour` an account a UTC hour, and only while the
  holds are under `merit_share_pct` of the board's live rows.
- **SETTLED** by the ten-minute cron once a computer took the row: CHARGED where
  an open row of its producer nobody paid for, opened before the hold, was due
  between the two deadlines; REFUNDED where it passed nobody, where its ask was
  withdrawn, or after `merit_hold_max_ms`. A refund is one ledger row keyed by
  the hold, so settling twice refunds once; a settled hold keeps no code.

## Compute orders

**EVERY OWED ROW IS A QUESTION, AND THE MACHINES WITH THE SITE OPEN ANSWER
THEM** (§"The fact factory"; `worker/board_tasks.js`). Nothing about a
submission is computed by the person who sent it: the build goes into the
library, and every (ruler, mode) it owes becomes a row in `queue` and is asked
of the factory — verb `simulate`, request the build's record on that ruler and
mode, the demand's `ref` its key. However many rulers and modes there are, a
machine is handed a minute of rows at a time, so the board's growth is more
questions, never a heavier client.

- **Asking.** `ship_queue.sh` writes the batch and its queue rows, then asks
  the factory for each (`/api/board/ask`, the factory's secret) — from the bot
  server the moment a submission lands (`live_board.sh`: `wfsim-intake
  --orders`, `ship_builds.sh`), from `queue.yml` hourly, and from a scorer
  run's reconciliation. THE FIGHT AN ARRIVAL NAMED IS WAITED ON: the runner
  marks that row `named` (`wfsim-intake --asked`) and the worker asks it as a
  question someone waits on (`rowAsk`), so the row the player just ran goes
  ahead of the build's other rows and every row nobody waits on; a backfill's
  re-ask of it is an ordinary row. The worker decides who asks: `new_build` for an
  `arrivals-` batch's row whose build was submitted that day or the one before
  (due in six hours) — `survey_build` when WFSim submitted it, a survey's winner
  (due in three days: a batch of them owes millions of rows) — `refresh` for a `refresh-` batch's (due when its fact is fifteen days old),
  `sweep` for a rescore's, an older build's first pass, or a `backfill-`
  batch's (due in a week).
- **The most popular weapon first.** A rescore and the reconciliation ask
  their rows in the order `/api/popularity` counts weapons, inside one by the
  row's last score (`popular_order.py`), each row naming its weapon's place
  (`order`). A placed row is due by its batch's DAY plus its producer's
  deadline, a second a place (`rowAsk`), so the factory's earliest-deadline
  order inside a producer hands the most popular weapon out first, however
  many runs asked the batch. A rescore is one batch a weapon,
  `rescore-<weapon>-<day>`, every ruler's rows in it when each ruler's run
  names the same `day`: the order center lists them most popular first, and
  each is moved or cancelled alone. The reconciliation ships `backfill-`, rows nobody
  ordered: under `arrivals-` a day's library growth queued inside `new_build`
  ahead of the submission somebody waits on. An
  arrival's row the board already scored is not asked: a resubmission the page
  did not recognise is the same build. ONLY A ROW THE BOARD ADMITS IS QUEUED:
  every writer takes its rows from `owed_rows` (`webapi/src/board_rows.rs`),
  the `validate_for_board_with` door `/api/board/order` opens a row through, so
  a build the library keeps but a ruler refuses — a melee build with an empty
  stance slot — is never a row no client can open. `scripts/sweep_admission.sh`
  names queued rows that door refuses, and deletes them only with `--delete`.
  EVERY OWED ROW IS ASKED FOR: the
  backfill (`ship_queue.sh --backfill`) asks for any queue row nobody asked
  for, since a run stopped between writing the queue and asking leaves rows
  only the scorer is ever handed.
- **Leasing, in PACKS.** `/api/board/work` (`worker/verify.js`) hands a client
  the factory's next questions it may answer — a board row as its library
  RECORD, a ruler and a mode, never a number — in the factory's order, as a
  PACK: the rows of about a minute of that computer's work (`PACK_MS`, by the
  `pace_ms` its page reports; `PACK_FIRST` before it has one, at most
  `PACK_MAX`), under one lease of a heartbeat (`PACK_LEASE_MS`). The
  DATABASE IS ASKED ONCE A PACK, not once a row: everything an ask writes and
  reads before choosing is one batch, the pack one statement
  (`worker/tasks.js` `nextPack`), and the answers come back together
  (`/api/board/answers`, `answerPack`) every half minute and as the pack ends,
  settled in a few round trips however many they are. A PACK IS A DISPATCH
  UNIT ONLY: each row in it is its own question, judged, made a fact and
  credited alone. FAIR SHARE: while the open questions are few, a pack is at
  most their number over the computers asking, so the last rows of a batch go
  to many computers, never to one fast one while others wait. AND BY ACCOUNT
  (`worker/fair.js`): while that cap binds, each account's target share of
  the hand-out is 70% by the capacity it has asking and 30% equal among
  established accounts (a day old and credited once; every unclaimed browser
  together is one, the walk-ins). An account that received more than its
  target in the last ten minutes, in `work` on the served engine, is handed
  nothing on an ask with probability 1 − target / received. CAPACITY IS
  WORK DELIVERED, so a fast machine is never held back for being fast: a
  device's best credited hour of the last day (`CAPACITY_HOURS`), or its
  credited rate over the window if that is more; only whole hours after the
  served engine's first answer count, since a new engine may price a row
  anew. A device with nothing
  credited yet counts at the median. Cores × speed counts a page that rests
  between rows as twice what it does, and both are only the client's word.
  Credited work was checked by a second witness. A big account
  still takes the most, but a small one is never starved while work is short.
  While work is plentiful none of this runs. A question
  someone waits on, and a riven gain, goes ALONE, to a computer free now. A row
  no longer owed is withdrawn where it is found, and A PICK THAT FOUND DEAD ROWS
  PICKS AGAIN (`PICK_ROUNDS`, eight picks, under the one lease) until the pack
  is full or a pick finds none: rows cancelled at the head of the order cost an
  ask a slower answer, never an empty one, and each ask withdraws a pack's
  worth of them. The official machines' claim does the same. A CLIENT THAT ASKS IS WORKING
  ON NOTHING: a browser computes in one tab (a Web Lock) and asks only between
  packs, so whatever its id still holds was lost — a crash, a reload, an old
  page — and is taken back as it asks, never waited out. UNLESS IT ASKS AHEAD:
  once the rows left would take under fifteen seconds (a search, past 80% of the
  last one's time) the page asks `ahead`, keeps what it holds and may hold
  `PACKS_HELD` packs, never a question someone waits on, so the next pack
  starts the moment this one ends. A background search takes the cores the
  reader leaves it at every round and hands its workers to the next
  (08-checkpoint-api.js `quickFleet`); its answer is the same on any number of
  cores (`check_quick_fleet`). ONLY THE SERVED ENGINE LEASES: the worker reads
  `engine` from the site's `release.json`, and a page of any other gets no task
  and its answers are dropped, so a page of another engine reloads at once,
  unless what its reader started is running. Every answer names the `release`
  too, and a page of an older one on the same engine reloads into it once left
  idle, so a machine left computing for weeks takes every fix without anyone
  touching it. A page whose
  engine lane fails to load asks `release.json` too, since two releases on, the
  worker it names is no longer served.
  A CLIENT IS ONE THE SITE KNOWS: its id is the page's 24 hex, which simfleet,
  a community client, also makes. Any other is handed no work and its answers
  are not taken (`unrecognised_client`, with whom to ask); what it earned stays.
  A RELEASE STOPS NO CLIENT: the protocol before the current one is served too
  (`PROTOCOLS_SERVED`), its answers taken as any page's, until nobody has asked
  with it for a week — a page reloads into a release, a program somebody else
  wrote runs on until its author ships.
- **A release carries a result.** An answer of an older engine stays with its
  question: a client of the new engine that reproduces its bits makes the fact
  and both are credited; one that does not starts the new engine's own count,
  and nobody is refused, since the release moved the number. **AN OFFICIAL
  CHECK OF ANOTHER ENGINE IS NONE**: the official machines run the served
  engine alone, so a dispute or a non-deterministic question no answer of it
  is in goes back to the volunteers as a fact of another engine does — open
  where a live demand waits, its answers carried, withdrawn where none does —
  and a spot of another engine's fact is that fact again
  (`factory_moot_checks`, at a re-ask and the settler's `moot` tick).
- **Fighting.** The client asks `/api/board/order` for the fight off the record
  (the scorer's `scored_build` and `row_requests`, a riven's rolls included),
  fights its runs on every community lane with `/api/board/runs` (a shard per
  run), folds them IN RUN ORDER with `/api/board/fold` and ends it with
  `/api/board/score` (`69-board-work.js`) — `a_compute_order_is_the_scorers_row`
  holds the path, lanes included. It sends what the pieces and folds took,
  summed over its lanes (`compute_ms`), how many lanes, and the engine's work
  counts — kept on the answer to fit what a point is worth, never credited.
- **`CLIENTS_PER_FACT` results** (`worker/verify.js`, 2). Equal bits — score,
  metric and work — from different owners on different networks make the fact:
  written to `scores` as `verified:<engine>` — replacing the old number of a
  row a rescore asked for again, as the scorer's fact would — and the queue row
  deleted. A `RECHECK_SHARE` of agreements wait for one more independent
  volunteer first — nothing credited meanwhile, a third who differs makes a
  dispute — and a `SPOT_SHARE` of facts, credited at once, is a `spot` the
  official machines check again. A difference is a `dispute`. The answer never says which.
- **Settling.** `settle.yml`, hourly, runs `live_orders.mjs settle` on hosted
  runners, a witness `official:settle-<shard>` each: it claims what only the
  official machines may settle — `disputed`, `spot`, `nondeterministic` —
  fights each on the volunteers' own engine — the checked-out site's wasm under
  node, along the page's path (`wasm_witness.mjs`), since only that build
  counts their work — ships the fact and tells the factory. The board server
  fights nothing. The factory credits whom the fact agrees with and refuses a client a
  dispute or a spot check disproves: its answers stop counting, what it was
  paid within the library's window (`KEPT_MS`) is taken back, and a fact it
  witnessed in that window that nobody official checked is opened again, its number taken off the board and its row owed again. A score
  within a part in 10^12 of the fact's, its work equal, is the runs folded in
  another grouping (docs/WASM.md): unpaid, never refused. A
  REFUSAL IS A COOL-DOWN, never for good: a day, then three times as long each
  time with no ceiling (`coolDownMs`, `verifiers.refusals` and
  `refused_until`), after which the client works again — and a client with k
  refusals has one taken off by k thousand facts it is part of afterwards
  (`clean`), back to none. A refusal needs the same engine; agreement does not.
- **Nobody online** costs nothing. A row past its deadline is the official
  machines' too: a `scores.yml` run CLAIMS from the factory every board
  question past its deadline that no client holds (`fetch_queue.sh`, as
  `official:scorer`) before it reads, and reads only what it claimed and the
  rows nobody asked the factory for — so a row never goes to both. Its facts are
  its answers (`order_credit.mjs`): a client whose bits it reproduced is paid
  (§"Contribution" rule 1), one it did not is not, and the run's last job
  releases what it left. A FACT WITHOUT WORK — a native build's, which counts
  none — witnesses the score alone: the row is the fact, and one a client gave
  that score is a `spot`, its work checked by `settle` before anyone is paid. **A LEASE IS GIVEN BACK, NOT LEFT TO LAPSE**: a page
  that stops — closed, reloaded, paused — sends its pack's answers so far
  with `done` (by beacon on `pagehide`), which gives back the rows it did not
  reach, so neither the rows nor the computer wait out the lease. **A LEASE IS
  A HEARTBEAT, NOT A DEADLINE**: a page still computing a row is never cut off.
  Once the row's lease nears its end the page renews it (`/api/board/renew`,
  `renewRow`, `RENEW_MS` at a time) as its pieces come in, and before starting
  a row whose lease may have run out meanwhile — kept even then, unless another
  computer took it, when the page passes it by. **NOBODY IS CUT OFF**: past
  `ROW_CEILING_MS` from its lease another computer may take the row too,
  and whichever answers first makes it. The holder computing longest is kept
  as the one before (`lease_prev`), its renewals held and its answer counted,
  so a page that beats but never answers holds nothing back. How many packs a client holds is decided in the statement that takes
  one, so two tabs asking at once cannot both win; a refused client is told
  `banned` rather than handed nothing in silence.

**THE CLIENT IS AN ID** its browser made for itself, joined to no submission,
kept in `verifiers` so a refusal has something to hold, and joined to an
account only by that account claiming it (§"Contribution"). Working is OFF
until the reader says yes (rule 0 below).

## Contribution

**WHAT THE VOLUNTEERS' DEVICES COMPUTE TOGETHER IS FREE FOR EVERY PLAYER, AND
WFSIM NEVER MAKES MONEY FROM IT**: no paid feature runs on it, and each device's
share is counted under its owner's name. The statement promises exactly that.
A CHANGE THAT ASKS FOR MORE — more of the device, a new kind of task, more
sent, another use — is a new statement (`COMPUTE_CONSENT_V`), asked again; one
that asks for no more, a rewording or a narrowing, keeps its version and every
yes, since each ask again is a reader interrupted. Eight rules:

0. **NOTHING IS COMPUTED WITHOUT THE READER'S YES.** Computing together is
   off until the reader turns it on in answer to one statement, asked once in
   a card on any device that can compute (`COMPUTE_CONSENT_V`, 73-compute.js);
   a yes or a no is kept with when, a new statement asks again, and every ask
   for work carries the yes, which the server keeps on the client's row
   (`verifiers.consent_v`, `consent_at`) and without which it hands nothing
   out. Running a stranger's device without that yes is controlling it,
   whatever it computes. While it is on, a ring of fixed size in the top bar
   says so and fills with the task.
   **SOLO AND TOGETHER** are the two words the page says, never "help" or
   "community computing": SOLO is the share the reader's own calculations
   take (10–100%, 50% unless set, `computePct`), TOGETHER the share computing
   together takes (10–100%, 30% unless set, with its own switch,
   `communityShare`) — two sliders, the same control in the settings menu and
   on `/compute`. Neither pauses for the other. A share is of the device's
   PROCESSING POWER, not its core count: together runs as lanes each resting
   between pieces for the share of the time the setting leaves
   (`communityPower`, `communityRest`, the rest kept in the worker), so 30% is
   30% on one core or thirty-two; solo runs whole lanes, since the reader is
   waiting. Within 100% together the two never touch; past it, while the
   reader computes, they share in proportion, solo counted at the lanes it
   holds. Together holds itself on battery unless the reader allows it
   (`computeOnBattery`), and with the browser's data saver on; a device that
   reports no power state counts as on power. NO RULE NAMES A KIND OF DEVICE:
   the reader decides where it runs. A page the browser freezes hands back
   what it holds at once (`freeze`), so no task sleeps on a sleeping device.
   The statement names its tasks by kind — simulation and optimization — so a
   new task of either kind needs no new statement; a third kind does.
1. **ONLY A FACT SCORES.** A client is credited a question's work when it
   becomes a fact, and every client in that agreement is credited all of it,
   once, however many producers asked. A result that never becomes a fact earns
   nothing; the official machines' work is nobody's. THE SERVER IS A WITNESS
   TOO: a client result the official machines reproduce bit for bit — a row the
   scorer claimed and measured (`order_credit.mjs`), or a disputed or spot row
   `settle` confirms —
   is credited to the clients it agrees with, exactly as a second client's yes,
   whatever engine id either side carries: the bits are the witness. A refusal
   still needs the same engine, since another one may differ honestly.
2. **THE SAME WORK IS THE SAME POINTS ON EVERY MACHINE.** The engine counts its
   own work: `Shard::work`, the gas its runs ran (the metered source's weights,
   docs/WASM.md §"Gas") at `GAS_PRICE_PER_MILLION`, plus `RUN_PRICE` a run
   (engine/src/fight/gas.rs). Native and wasm builds count the same
   (`check_gas`). A client's `compute_ms` is its machine's speed and is never
   credited. One point is 10^9 of work. A point earned is never repriced: a
   price applies to the answers of the engine that carries it.
3. **THE WORK IS PART OF THE BITS.** A result names its work beside its score,
   and a further result agrees only if the work is equal too. A recheck and the
   server's spot check hold the clients to the work as well as the score, so a claimed work
   nobody did is a refused client.
4. **A FACT NEEDS DIFFERENT OWNERS AND DIFFERENT NETWORKS.** A further result
   is never handed to a device of an owner whose device already measured the
   order, an unclaimed device being its own owner — nor to one asking from the
   network a result came from (`netOf`: a salted hash of the address, kept on
   the open order and cleared with it), so two browsers on one desk are one
   witness.
5. **A DEVICE'S WORK IS WHOSE IT WAS WHEN IT WAS EARNED.** A signed-in page
   claims its browser's id (`/api/account/devices/claim`, `devices` in the
   accounts database), and from the next hour that account holds what the
   device earns (`device_spans` over `verifier_hours`, worker/contribution.js
   §"Spans"). What it earned before anyone held it — its notebook — goes to
   its first claim at once, and the browser's own count is empty after.
   Another account signing in there holds what comes next and takes nothing
   already earned; removing a device closes its span and keeps its points.
   ONLY THAT BROWSER CAN CLAIM IT: its full id is a secret it alone keeps, and
   no answer carries one (`check_contribution`). A signed-in page claims its
   browser once a load, the server's row the only record of it, so a browser
   stops counting for an account by signing out there. A client in
   its cool-down is off the ranking until it ends.
   **CREDIT THAT LEAVES ITS HOLDER GOES TO THE OFFICIAL POOL** (`points_pool`):
   a deleted account's, once its week is over (docs/ACCOUNTS.md), and what a
   refusal takes back. Points are compute, so none is uncounted; the pool keeps
   only when and why, never whose, and the ranking states it on its own line.
   The owner sees every device of theirs at `/compute`: the name they call it
   (the browser's coarse guess until renamed), when it last answered
   (`verifiers.last_at`) and the task it holds a lease on, named by its KIND
   and public facts alone; a device is renamed or removed there, and every
   block of the page folds as the site's blocks do. This
   browser's own task list stays in this browser. Above it, for anyone, the
   whole picture (§"Demand"): browsers computing now, owed rows by why, riven
   gains waiting, each survey, the scores per hour by the volunteers and by the
   official machines, and the hours to clear — totals, never a device or a person.
   EVERY POINT HAS A LEDGER, as open as privacy allows (worker/contribution.js
   §"The ledger", `check_ledger`): this browser, by its own id, sees its
   credited hours day by day in the reader's time zone, each said with where
   it went — kept on the device, the reader's own account, or another account
   never named, an ended span said as ended — its all-time split by holder,
   and its window's results that earned nothing, by why. An owner opens the
   same for each listed device, and the account's points by day, device by
   device.
   Below it this browser's recent tasks, each with the build, the fight, the
   number it computed and where it stands, asked by the browser's own id —
   a board order of `/api/board/mine` (waiting, checking, confirmed or no
   longer owed), a riven gain of `/api/appraise/mine` (waiting, confirmed once
   its answer is one of the pair credited, or not counted) — opened
   in place and into the simulator. Each is marked by its verb: a board order
   SIMULATES one build and shows its score; a riven gain OPTIMIZES and shows
   the best it found and how many builds it tried.
6. **EVERYONE IS RANKED; A NAME IS SHOWN ONLY BY CONSENT.** `/contributors`
   lists every account with a claimed device, by all their points, this UTC
   calendar month's or this week's from Monday (`periodStarts`; the page says both
   ends in the reader's own time) (`verifier_hours`, credited with the total and
   read through the same ban), and tells each account its place on all three.
   The same table, by the UTC hour, also counts each browser's results and
   their time, so a page's "today" is whole hours from its own midnight
   (`/api/board/points` `since`), never its own fifty-task list. A row is
   anonymous until its account agrees to show its name
   (`contribution_choice`, asked once, inline, where the reader sees their
   points); then it is the display name, else the username, and nothing else.
   An anonymous row's name and tier never leave the server; the reader's own
   row is marked to them alone. A browser asks what it earned by its own id
   (`/api/board/points`), and is told it in the reply to each send of results
   (`/api/board/answers` `points_since`).
   **THE RANKING MOVES WHILE IT IS READ, AND NEVER AHEAD OF THE SERVER.** The
   page asks for the rows it has scrolled to, a hundred more as the end comes
   near, and asks again each minute, the server keeping the standings a minute
   at the edge with nothing in them an answer may not carry. Each number rolls
   from what was shown to the new answer, never past it, and the order is the
   numbers as shown, so a row slides past another when its number does. A
   row's `key`, its account's id and the UTC day hashed with the server's
   secret, is how the page follows it, and a new day is a new key.
   A RIVEN GAIN (docs/AGENT.md §"Riven appraisal") is credited the same way:
   its search's work, summed by the engine (`Summary::work`, the scorers'
   `work` and the final round's `final_work`), to the two owners' computers
   whose answers agree.
   A device that said yes and has been credited makes its account a
   **WFSim Volunteer** — an honour earned by computing, never sold — shown on
   the account page and the compute page. The ranking's named rows carry it but
   the page does not draw it there: nearly everyone on the ranking has computed,
   so it tells nobody apart.
   Every account also holds a **CONTRIBUTOR RANK**, Warframe's Mastery Rank
   curve on its all-time points: ten points are one experience, rank n is
   reached at 2,500·n² up to 30, and every 147,500 past that is one rank more,
   without end. The server computes it (`contributorRank`) beside every points
   figure it returns; the page, the share card and Nona draw it and never
   derive it, so a curve change is one edit.
7. **A MARK NEVER MOVES A PLACE.** A named row carries the mark its account
   carries on a signed share, proved by the paid half and attached by the
   server (`cloudMarks`), drawn by the extension beside the name and on the whole row
   (`contributorMark`, `contributorRow`); the order
   is the points alone, and nothing paid adds a point or puts an order in a
   device's hands sooner.

**THE TWO PRICES ARE FIT, NOT CHOSEN, AND NOT ON ANYONE'S MACHINE.**
`.github/workflows/work.yml` builds the engine contributors run from the ref it
runs on, times it over a fixed sample of the published board on four hosted
runners, and fits the price BETWEEN a million gas and a run to each row's
median time (non-negative least squares on relative error,
`scripts/work_calibrate.mjs`), reporting the error on a held-out half. The
SCALE is set on what volunteers were paid: their credited board answers of the
last seven days, priced anew, must total `total` (1.00–1.20) times what they
were, an even sample fought natively for its exact count and the rest predicted
from the counts their answers carry (`WORK_COUNTERS`, a record beside each
answer, never priced). Its report gives that total, each account's ratio (by
digest, under 0.95 flagged) and the twenty rows that move most. The prices
shipped may be round ones near the fit, judged by the same report
(`prices=<gas>,<run>`): their time error no worse than the fit's and their
total within the same 1.00–1.20, so points never shrink with a new engine.

### Demand

**THE COMPUTE PAGE'S PICTURE IS ONE OBJECT EVERY READER GETS.** Each minute
the daemon works it out (`demandOf`, worker/contribution.js) and writes
`demand.json` to the public bucket beside the signal, kept 30 s at the edge;
the page reads it from `cdn.wfsim.app`, and `/api/board/demand` serves the
same object from the bucket, never working it out. Worked out per request,
each isolate and each edge would keep its own minute, and two readers would
see two.

```text
{ ok, v: 1, at, computing, owed: { new_builds, rescores, sweeps, refreshes },
  riven_gains, orders: [{ kind, total, dispatched }],
  surveys: [{ weapon, ruler, shapes, agreed, started, clear_hours }],
  per_hour: { volunteers, official }, clear_hours,
  now: { window_seconds, asking, held, answered, starved } }
```

**THE TOTAL IS EVERY ORDER THERE IS NOW.** The page heads the picture with
it, said as what is dispatched and what waits on the server: an order
withdrawn leaves it, one the system asks for joins it, and nothing else moves
it. The countdown under it is that total's.

- `owed`: the queue's rows by why, a row in two batches counted once as the
  first of a new build's, a rescore's, a sweep's, a refresh's.
- `orders`: the same counts as one list, a kind a row (`ORDER_KINDS`:
  `rescore`, `sweep`, `new_build`, `refresh`, then `riven_gain`), a kind
  with nothing owed left out. `dispatched` is how many of its rows a device
  has taken — its question leased, by a volunteer or an official machine, now
  or before — and a riven gain's once begun; the rest wait on the server. A new
  kind is added to the list and named on the page (`ORDER_KIND_NAMES`); a
  page that does not know it shows it by its id.
- `per_hour`: scores written over the last 24 hours, per hour, by who
  computed them. One hour swings five-fold between two quiet ones and a
  stopped hour reads as never; anything under a day reads the time of day,
  since who is online follows the clock round the world.
- `clear_hours`: the open work priced in compute. Each verb's work (the
  queue's rows for `simulate`, its open questions for any other) times what
  one of its facts cost over the last day — every volunteer answer's fight
  over the facts formed since the first answer the read reaches, so the usual
  rechecks and losses are in the price and a day of more answers than the read
  takes is priced over the hours it covered — over the compute the volunteers answered per hour
  in the last six whole hours (`verifier_hours`). A day, because a few hours
  of one bad release would price every row at what that release wasted; the
  compute, because it holds within a fifth hour to hour while rows per hour
  swing from nothing to thousands as the work moves between verbs. Null where
  a verb with work formed no fact that day.
- a survey's `clear_hours`: its shapes left over its shapes agreed per hour
  in the last 24 hours; null when none were.
- `now`: the fleet over the last `window_seconds` (`NOW_WINDOW_MS`), a
  reading and not a speed: the ids that asked, the leases running, the
  volunteer answers. `starved` (`starvedOf`) is rows waiting on the server,
  ten for every asking id, while those ids answered under fifteen each — a
  healthy hour answers hundreds an id. Alarms read this, never `per_hour`:
  a day's mean hides hours of a fleet handed nothing.

EVERY SPEED THE SITE STATES IS OVER THE LAST 24 HOURS (`PACE_HOURS`,
`worker/instant.js`): the demand's pace and hours to clear, a survey's, the
home tally's per-hour count and the watchdog's time left on a filling cell.
The compute page prints the hours to clear as days, hours and minutes, to the
minute, over a note that says what it rests on.

### The public stats

**A NUMBER EVERY READER SEES ALIKE IS A FILE ON THE CDN, NOT A CALL.** Two
are written to the public bucket under `stats/` and read from
`cdn.wfsim.app`; the worker's endpoint stays, as the page's fallback.

| file | writer, period | edge life | same body as |
| --- | --- | --- | --- |
| `stats/tally.json` | the daemon, `stats_tally_ms` (60 s) | 30 s | `/api/board/tally` (`tallyOf`) |
| `stats/contributors.json` | the worker's ten-minute cron, `stats_contributors_ms` (10 min) | half a period, at most 5 min | `/api/contributors`, each period |

- **The ranking is written by the WORKER**, not the daemon: a row's key is
  hashed with `AUTH_SECRET` and a named row's mark comes from the paid half,
  and neither may reach the Hetzner box. Its rows are `everyoneOf`'s, the
  ones every answer is cut from, ordered per period by the same `ordered`:
  `{ ok, v: 1, at, day, computing, pool, rankings: { all, recent, week } }`.
- **A RANKING WITHOUT ITS MARKS IS NOT WRITTEN.** The cron waits for the paid
  half however long it takes (a reader's ask waits four seconds), and when it
  does not answer the last file stays served: a badge gone for ten minutes
  reads to everyone as a membership refunded.
- **Nothing in it names anyone who did not agree**: no account id, and a
  name, a mark or the honour only on a named row. `check_contribution`
  asserts it, and that the same test finds each one planted.
- `/api/contributors` reads the file back while it is of today and younger
  than two periods, so the worker works the ranking out once a period. A
  signed-in page still asks it, because only it can mark the reader's own row.
- **The page passes a file over** — for the endpoint — when it cannot be
  reached, is not `v: 1`, or is older by the server's clock than 10 minutes
  (the tally) or 30 (the ranking); a ranking of another day is passed over too,
  its keys being that day's. `check_stats_files`.

### Outages

When the factory cannot hand out work, the computers that were waiting are
owed what they would have earned. Three tables make that a lookup, not a guess:

- **`verifier_asks`** — per device and UTC hour: `asks` the server saw (not
  `ahead`), and the page's own count of asks that brought nothing (`idle`), the
  cores they left waiting (`idle_lanes`), and those that never reached the
  server (`failed`). The page keeps that count in `localStorage` and sends it
  with each ask until a reply says `kept` (worker/verify.js `asksOf`), so an
  outage the database itself was part of is counted once it ends. The page's
  numbers are claims: they pay only inside a window the owner declares an
  outage, and never past the device's own earlier rate.
- **`factory_events`** — `pause` (noted by the asks `FACTORY_PAUSED` turns away,
  once a minute an isolate) and `alert:<cause>` (each watchdog round a cause
  stands), each from its first sight to its last (worker/factory.js `noteEvent`).
- **`adjustments`** — points given by hand, each also added to `verifier_hours`
  in the same transaction; the ledger shows an hour's part of it as
  compensation (`given`), so it never reads as computing.

## Consent

Asked ONCE, inline, the first time a run finishes — never on load, never as a
native dialog (they are blocked in this project), and never blocking the
result. Any fight sends its build, because what the board scores is the build
under the ruler's fight and never the reader's own.

What travels: the weapon, its mods, evolutions and arcanes, its riven's SHAPE
and the fight it was run in. No account, no identifier, no score and none of
the names you chose. `scripts/check_official.mjs` asserts on the WIRE that
nothing leaves before consent and nothing leaves after declining.

The endpoint stores no IP, no token and no timestamp finer than the day.

## One representative per build

A board row is keyed by what makes it a different FIGHT, and mod ORDER is part
of that — mods combine ELEMENTS in the order they are listed. Measured on the
Torid, six mods:

| spelling | pairs to | DPS |
| --- | --- | --- |
| Heat, Cold, Toxin, Electric | Blast + Corrosive | **12,424** |
| Heat, Toxin, Cold, Electric | Gas + Magnetic | **46,583** |

**And the MODE is the other half of it**. A Torid through its Incarnon cycle
and a Torid that never transmutes are two entrants, so the key is
`identity(build)#mode`, at the endpoint and the scorer alike. A record stored
without a mode is read through the migration fallback — the cycle where there
is one — and `wfsim-board` prints how many arrived without a mode, so the
migration is visible and ends at zero.

Raw order is not the answer either: three elementals in slots 1-3, the same
three in 4-6, the same three interleaved with the rest, and the non-elementals
reshuffled all score an identical 146,707.582. Only the elementals' order
**relative to each other** is the build.

So `board::builds::canonical_mods` gives every build ONE representative: elementals
LAST in the order they arrived, everything else ahead of them by biggest drain
then by DE's own English name. The endpoint stores what was
submitted verbatim — it has no mod pool and cannot tell an elemental mod from
any other — and the scorer collapses spellings after `validate` has canonicalised
them.

## The pipeline, stated once

Every step below is a rule, not a description — each one is somewhere a wrong
answer could be published.

1. **One representative per build.** `board::builds::canonical_mods` — elementals last
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
   whole engagement, averaged over the ruler's runs, and the benchmark says
   `metric: kpm`. Publishing the raw figure under a "kill rate" label would
   overstate every row by the length of the fight.
7. **Shown at four significant figures AND four decimals**,
   by `data::boards::format_score`. Four decimals is where two builds a player is
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
reads three facts:

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
store name `standard_single_target_v1`, and they are builds like any other.

The bill still reports what is SPENT, not what earned room: a build with fewer
mods than mastery has polarizations buys all five, and the last land on empty
slots.

## Rivens: a SHAPE, not an item

A riven ITEM stays off the board: a board that counted personal random items
would rank luck. What is on the board is not the item.

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

**AND WHERE THE SIGN HAS STOPPED ANSWERING, THE OTHER END IS A BUILD OF ITS
OWN.** Intake stores every corner of the shape and asks a FIGHT for the default
alone; which corner is best is a comparison of SCORES, and the scorer is what
banks those. Three things follow, and they are the whole of the rule:

- **an alternative corner waits for its default to clear the entry line**
  (`board::entry`). A riven nobody will see on the board is a riven whose best
  corner nobody will read, and that question costs a fight to answer.
- **the corners of one shape publish as ONE ROW**, the best-scoring one, a tie
  going to the god roll (`one_row_per_shape`). The advice is the shape; two
  rows differing only in which end a stat landed on is that advice twice.
- **and the tolerance is gone until the standard error is banked.** Two corners
  inside the ruler's own noise should read as a draw; `scores` has a column for
  the number and none for its spread, so a near-tie is settled by whichever came
  out higher — the way every other near-tie on this board is.

**AND NO FIGHT IS RUN AT INTAKE**, which is the whole of why this shape and not
another. A fight there is the SCORER's fight run twice — once to pick a winner
and throw the number away, once to bank it — which is two answers to one
question with the evidence in the discarded half. It is also single-threaded
and hourly, where the scorer fans out to 32 shards: 20-182 s a riven record
against 0.01 s.

**WHICH STATS GET MORE THAN ONE CORNER AT ALL** is
`build::rivens::ambiguous_stats`. Three sources and no fourth:

- **the three physical stats, on every weapon.** A physical bonus does not add
  damage beside the rest, it changes the SHARE each damage type holds of the
  total — and a status proc is drawn in proportion to that share, so more Impact
  is fewer Viral and Corrosive procs.
- **status duration, on every weapon: as the MALUS at five rolls**
  (`DURATION_ROLLS`: 0.9, 0.95, 1.0, 1.05, 1.1), as a bonus at its two ends. It paces Heat's armour strip (the steps scale with it,
  M99) and shortens every other status with it, so a deeper malus strips faster
  while each Viral, Corrosive or Magnetic stack lives for less, and the best
  sits INSIDE the band. The Furis's standard_single_target #1 at disposition
  1.2 reads 175, 181, 197, 221 and 162 KPM across the five rolls; the two ends
  alone would publish 175. Even steps rather than a search for the peak,
  because the curve has steps of its own where a status's life crosses a tick.
- **a stat the WEAPON takes the sign off**, one row each in
  `build::rivens::SIGN_IS_NOT_THE_ANSWER`. A weapon earns a row by paying for NOT
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

**A CARD'S RANK IS A CORNER THE SAME WAY, BY NAME.** A submitted rank is
dropped and every card is stored at max rank, except the cards
`data/search/every_rank.yaml` names: each of those in the build is a corner at
every rank, crossed with every riven corner: a low-rank Hunter Track moves a
deep Status Duration malus back up the band. A card joins the list by name,
never by resembling one on it, and its rank is part of the id —
`<card>@<rank>` in `mods` (`data::mods::RANK_MARK`).

Measured over the library: 1,948 of 2,418 riven builds have no ambiguous stat
at all and are the god roll and nothing else; 470 name one worth more corners,
and all but five of those name exactly one — two builds for a sign stat, five
for a duration malus, not sixteen.

**A CORNER RULE THAT GROWS REACHES ARRIVALS ALONE**, because intake reads the
inbox and the inbox is emptied as it goes. The builds already stored get the
new corners from `backfill.yml` (`scripts/backfill_duration_corners.sh`), run
by hand: it sends each stored duration-malus record back through intake and
writes only the inside rolls the library lacks, never an end — a record
canonicalised differently since it was stored would come back under a new id.

**AND WHEN TWO RULERS DISAGREE, THAT IS TWO ROWS AND ONE LIBRARY.** One ruler
cannot speak for another and the card that wins a crowd need not win one
target — so the corners are in the library once and each ruler's own scores say
which of them leads THERE. The rolls are part of the id, so two ends of one
shape cannot be filed under one another.

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
`board::builds::canonical_mods_with` searches for a representative rather than
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

### The Riven Analyst

What each riven is worth, read off the board. GLOBAL: every riven row a weapon's
board publishes, against the #1 riven-free row of the same ruler and mode —
gain = riven row score / riven-free score. `rivenGroups` in `87-headless.js`
groups them; the tab (`/weapons/<Name>/riven-analyst`) and the headless query
`builder.rivens.read` both answer with it.

Nothing here measures anything: every figure is a published score, so it moves
only when the board does. A riven nobody has submitted is not on it, and a row
that reads below the riven-free #1 is a riven whose best build has not been
submitted yet. One reader's own card is a different question, and not this
tab's: it is answered by the optimizer, never by re-measuring here.

## What is not on the board

*(The exilus slot is on the board — see below.)*

**THE WIELDER, ON EVERY WEAPON A RULER CAN PIN A FRAME FOR.** A build names who
holds its weapon (docs/WARFRAMES.md §A weapon's wielder) and the record drops
it: a build tested in Ash's hands enters the library as the same build it is in
anybody's, and every ruler scores it in the Prototype's. The page sends it
anyway and the door drops it, because which weapons are the exception is game
data and the door is the half that has it.

## The one entrant a ruler cannot pin

**AN EXALTED WEAPON CARRIES ITS WARFRAME.** Valkyr Talons are a weapon in the
roster and their numbers are an ability's: they seat no capacity of their own
and they scale with ability strength, so no ruler can state the frame behind
them and a row without one could not be reproduced. So the frame — and the
Operator travelling in the same payload — is part of the BUILD there, recorded
on the submission, hashed into the identity, and two strengths are two builds.
`ValidBuild::with_wielder` is the whole of that rule, called by the board's
door and by `wfsim-intake` so the two cannot answer it differently.

**AND IT IS RANKED APART.** The fight is the same fight, but an Exalted
weapon's KPM and a gun's are not like terms. The ranking page's "All" leaves
the ability-scaled ones out, and its Exalted Weapon slot ranks them in a list
of their own above the Exalted weapons that do not scale (Garuda Talons); a
weapon's own page shows its rows either way, and the rank it states is counted
among its own kind. Two lists under one ruler is what keeps a number from being
compared to a number nothing measured against it.

**THE RANKING PAGE IS FILTERED BY ADDRESS.** Slot, the slot's own type, class
and tag chips (the home page's, `slotFilterRows`), the riven view, measured-only
and a name all ride `/benchmark?…`, and a rank is counted inside the list shown.

## What a row has to be

A ruler that wants a complete build wants exactly this, and the four rows are
the whole rule:

| | |
| --- | --- |
| the 8 main slots | **FULL** |
| every arcane seat | **FULL** |
| every evolution tier | **FULL**, a tier the row leaves out holding its first option |
| the stance, the exilus | **OPTIONAL**, each on its own |

Full where the game gives no reason to leave a slot empty, optional where it
does. `validate_for_board_with` is the rule and
`the_entry_standard_takes_a_full_build_with_or_without_the_optional_slots` is
the table above, asserted.

**AN UNSTATED EVOLUTION TIER IS ITS DEFAULT, NOT A HOLE.** The fight installs a
tier the row does not name at its first option (`evolutions::complete`), and the
door counts the tiers the same way, so a row is admitted as the weapon it is
scored as. A row stated before its weapon's Genesis was entered is that Genesis
at its defaults, never the base form.

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

Most exilus mods are handling and mobility, and the part that decides a fight
is not: `vile_precision` is **−36% fire rate**, which takes an Ignis Wraith
from **11.9694 to 9.3737** on the group ruler. Beam range is exilus too
(`sinister_reach`, `ruinous_extension`, `galvanized_acceleration`) and IS
modelled, though it does not bind on the current rulers: the same Ignis scores
11.9694 with and without Sinister Reach.

**Not `full`.** Requiring an exilus would force a choice worth nothing on most
weapons and publish whichever mod the dice favoured, which is what the quick
calc's `tied` marking exists to admit rather than to rank.

**It travels in a field of its own** (`exilus`), never as a ninth entry in
`mods`. An exilus-eligible mod is legal in a MAIN slot too, so a flat list
cannot say which one came out of the exilus slot — only the page has the slots.
For the same reason it is its own field in `ValidBuild`, in the worker's `AXES`,
in the board row, and in `board::builds::identity`.

## The entry line

**A BUILD IS ON THE BOARD ONLY WHERE IT REACHES A FIFTH OF ITS GROUP'S LEADER.**
`data::boards::KEEP_LEADER_SHARE`, and it decides two things at two
granularities:

| | | |
| --- | --- | --- |
| **a ROW** is published | `clears_entry` | it reaches a fifth of its own group's leader |
| **a BUILD** earns more fights | `keeps_earning` | it reaches a fifth in at LEAST ONE (ruler, mode) of that SCENE it has a fact for — or, with none there yet, of the base |

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
distribution of score-as-a-share-of-leader has no knee, so what places it is
what a player reads as the leader's class: a fifth is five times as long to kill
what the leader kills. Over the whole store it keeps 39.3% of measured rows and
84.4% of builds earning fights; a tenth kept 58.5% and 94.2%.

**WHAT IT CANNOT DO IS REFUSE THE FIRST FIGHT.** Knowing whether a build clears
the line means measuring it, so a build with no fact anywhere is owed one — and
exactly one. **`wfsim-intake` asks for it**, at the (ruler, mode) the ARRIVAL
named where the board admits the build there (`owed_rows`), because that is the only moment anything knows: a build carries no ruler
and no mode (`canonical` drops both, since mods are equipped on the WEAPON and a
mode is how it is fired), so after intake the question cannot be asked at all.
Clearing the line on that one fight earns the other eighty-odd rows.

**AND A BUILD WITH NOTHING OWED IS ASKED FOR EVERYTHING.** That is the net. An
arrival row that was spent, dropped with its batch or never written would
otherwise leave a build nothing ever measures, which is the one failure the
reconciliation exists to make impossible.

**AN ARRIVAL IS UNGATED, AND THAT IS HOW A PARKED BUILD COMES BACK.** Somebody
pressing upload is a person asking, so a build the line parked is re-measured on
the fight they just ran it in — a mechanic that changed makes it return by
itself, with nobody having to know which build to look for. The cost is one
fight per submission, which is the cost the first fight always is.

**EVERY OTHER PATH OBEYS THE LINE AND THE RESCORE BUTTON DOES NOT.** `--gate` is
passed by the arrivals reconciliation and by the refresh, which is where the
line pays for itself: a first measurement is a one-off bill and the refresh
recurs for the life of the board. A person asking for a ruler again after a model
correction is the other path that may reach a parked build, and
`check_rescore_paths.mjs` asserts every one of them.

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
wants to see, not about what is true, so the page answers it on a slider that
runs in steps of 5% from the leader alone down to the entry line, and shows
**every row the board kept** until a reader narrows it. Its last stop is `0`
rather than a copy of the line, because "no filter" stays true wherever the
line is drawn.

**EVERY BUILD SAYS WHEN AND BY WHICH ENGINE IT WAS MEASURED.** A row publishes
its fact's `finished_at` to the minute as `measured_at` (UTC, ISO) and the
ENGINE ID that measured it as `measured_by` — the volunteers' `verified:<engine>`
and the official machines' bare id alike (`measured_engine`); either is omitted
where the fact has none, and a fact that recorded a commit has none. The engine
id is the one version every row can state: it names every input of the number,
code and data, and two rows under one id are the same arithmetic. ONE FORMAT
ON A BOARD, so a row never cites a commit beside another citing an engine. It
is per BUILD, not per weapon or board: a weapon's builds are measured at
different times by different engines and a rescore replaces them one by one.
No time dates a whole board on the page: the live board re-ranks a weapon the
moment its facts move, so each row's own record is the age a reader sees. EVERY SURFACE THAT SHOWS A BOARD BUILD SHOWS ITS RECORD — the
board, the build finder, the weapon's measured-builds table and tab, the Riven
Analyst, the simulator's build under test, the long image, the prerendered page
and its markdown, and the headless rows — with one line beside a list saying an
older record may be behind (`measuredRecordHtml`, `measuredNote`). A weapon's
"updated" time is its newest row under that ruler.

**A PLACE IS A SCORE, NOT A ROW.** Builds on one exact score are tied — what
tells them apart moved no number under that ruler, an exilus or an evolution
the fight never reaches — so they share a place and the next score is the next
place: 1, 1, 2, never 1, 1, 3. Every ranking of builds counts this way through
one function (`placed`, `87-headless.js`): the board, the build bar, the
finder, the long image, the bots, the agent door, and where a submitted run or
a weapon would stand (`placeAmong`). A row's `builtin` id still counts ROWS, so
it stays unique; it was never a durable reference to a build (§UI "kept by what
it is").

**A SHORT LIST RANKS EVERY MODE TOGETHER AND COUNTS PLACES, NOT ROWS** (the
long image, the bots): it answers "the best ways to play this weapon", so a
ruler's modes are one ranking (`pooledRanking`; a riven at its best mode,
against the best riven-free build of any mode), and a tie is one answer shown
by its preferred build — the search's own order, more mod drain first
(OPTIMIZER.md §"A tie keeps the preferred build") — saying how many builds
share it (`distinctTop`, `preferredBuild`). The board and the build finder
still rank per mode.

A GROUP IS ONE WEAPON, IN ONE MODE, UNDER ONE RULER, and riven builds are a
group of their own. A riven build and a plain one compete with each other for
nothing, and one ruler's leader says nothing about another's; a shared
reference would let whichever group is stronger decide what the others may
show, which on most weapons means the builds most players can make are the ones
to disappear. There is no count limit, and a group whose builds are genuinely
close keeps all of them.

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

`wrangler.jsonc` deploys `site/` as a Worker's static assets with a script in
front of them. Two consequences:

- **Pages conventions do nothing here.** A `functions/` directory is ignored;
  the endpoint is a route inside `worker/index.js`.
- **`assets.run_worker_first` is not optional.** Assets match before the script
  runs, and `not_found_handling: single-page-application` answers every
  unmatched path with index.html — so an api path the worker does not run first
  comes back as the SPA with a 200. The page reports "could not reach the
  board" rather than assuming success.

## Setup, once (repo owner)

1. **The databases** — `wfsim` and `wfsim_accounts` in Postgres, laid with
   `worker/pg/*.sql` through psql as each file's header says
   (§"Postgres" §"Setting a server up"), then a Hyperdrive config for each,
   declared in `wrangler.jsonc`:

   ```jsonc
   "hyperdrive": [
     { "binding": "HYPERDRIVE", "id": "<the wfsim config's id>" },
     { "binding": "HYPERDRIVE_ACCOUNTS", "id": "<the wfsim_accounts config's id>" }
   ]
   ```

   **In the file, not in the dashboard.** wfsim.app is a WORKER (static assets),
   deployed by `npx wrangler deploy`, and a deploy REPLACES the worker's
   bindings with what the config declares — a binding added through the
   dashboard is removed by the next push. A Hyperdrive id is an identifier, not
   a secret; it grants nothing without the database's password.

   The worker serves the library as `LIBRARY`, named for what it HOLDS, which
   is not the board: the board is `site/board/`, and this holds the builds
   people sent and the facts computed from them. A binding called `BOARD` is a
   debugging trap — "the board is empty but the BOARD binding looks fine" is a
   sentence that sends you looking in the wrong place. The resource NAME follows `docs/NAMING.md` §8: the
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

2. **Repo secrets** — `CF_ACCESS_CLIENT_ID`, `CF_ACCESS_CLIENT_SECRET`,
   `PG_PIPELINE_URL`, `PG_ACCOUNTS_PIPELINE_URL` (§"Postgres" §"Setting a
   server up"); `.github/actions/databases` opens the tunnel with them.

   **A TUNNEL WITH NO DATABASE URL IS A MISCONFIGURATION, NOT AN ABSENCE.**
   The scripts treat "nothing configured" as a working state — a run with no
   database computes everything, which is what it did before there was one —
   but a half-configured one fails loudly, because the alternative is a shipper
   that runs green and sends no rows.

3. **The facts table is already in the schema above.** Nothing else has to be
   provisioned: the scorer writes a log, `scripts/ship_facts.sh` sends it, and
   `scripts/fetch_facts.sh` reads it back. Both carry their own self-tests,
   driven against a stub, so every hop is testable without a network.

   ASK IT A QUESTION, which is most of why it is a database. ON ONE LINE: a
   continuation is the one piece of shell syntax that differs between the two
   shells this might be pasted into, and `\` is an ARGUMENT to PowerShell, which
   then reports an unknown one and never says which.

   ```sh
   cloudflared access tcp --hostname pg.wfsim.app --url 127.0.0.1:15432
   psql "$PG_URL" -c "SELECT weapon, count(*) FROM builds GROUP BY weapon ORDER BY 2 DESC LIMIT 20"
   ```

   The first holds the tunnel open — run it in a shell of its own — and `PG_URL`
   is a role's `postgres://…@127.0.0.1:15432/wfsim`.

The publishing path only READS. What the board says is computed in
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

> **THE TABLES BELOW WERE MEASURED AT 1.5 m** and are evidence about
> SATURATION — which mechanism stops growing at which grid size. Neither the
> ruler's spacing nor its size is read off them: both are decided in §"The
> multi-target ruler". 1.5 m separates all three steps of a radius mod and 3 m
> does not, which is a cost the ruler pays on purpose.

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
yaml (the grid is expanded into positions at load) and asserts the prose
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

The table above is what a chain cost BEFORE `rules::chain::Layout`. Nothing in this
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
`rules::chain::tests::a_layout_answers_exactly_what_the_scan_does` asserts it instance
for instance over every seed of a grid, at three spacings, for both chain
shapes.

It is built PER RUN rather than held on `FightParams`: widen
`beam.damage_radius_m` after the params are built and a cached layout is
silently stale, which is the two-declarations bug wearing a cache.

### Where each mechanism stops growing, at 1.5 m

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
12 m reaches 110 bodies there and 110 at 23x23, so at 1.5 m no weapon in the
roster is clipped by a 19x19.

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

## The multi-target ruler

`data/benchmarks/standard_multi_target.yaml` — the second ruler, and the first
that is about a ROOM rather than a target.

**5 x 5 at 3 m centre to centre, Thrax Centurion Lv 9999 SP, 180 s, KPM.** The
shooter stands at CONTACT with the middle body of the front rank and fires
along the line that rank faces. Both numbers are extremes of a stated
criterion, and a body is reached when the centre-to-centre distance is within
the effect's radius plus `BODY_RADIUS_M` (0.25 m):

- **3 m** is the widest whole-metre spacing at which every elemental area
  effect still reaches a neighbour. Electricity's 3 m arc reaches 3.25 m: the
  bodies either side and behind at 3 m, and none at 4 m. It is also inside the
  band where the splash-versus-single-target ordering matches play (above).
- **5 x 5** is the smallest odd square that holds every elemental area effect
  centred on the aimed body WHOLE. Electricity and a Blast proc's 5 m fit a
  3x3; a fully grown 6 m Gas cloud reaches the bodies 6 m to either side and
  needs the fifth column.

**WHAT IT CLIPS IS CLIPPED ON PURPOSE.** A weapon's own explosion wider than
about 6 m, and punch-through past five bodies. Punch-through is the reason the
grid is not deeper: its score is EXACTLY proportional to the rank count (5x5
scores 5/19 of 19x19 to two digits on every such weapon), so a deeper grid
ranks the arena rather than the weapon. Measured over every weapon and mode's
best row, 200 runs each on the ruler's seed, 5x5 against 19x19 at 3 m:

| | rows |
|---|---|
| score unchanged | 438 of 593 |
| kept 90–100% | 47 |
| kept 50–90% | 62 |
| kept 30–50% | 11 |
| kept under 30% — punch-through, Praedos, Magistar slams | 35 |

The rows 19x19 ranked in its top 20 had earned **64%** of their score on
bodies outside the 5x5; rows ranked below 100 had earned 10.6%. The full reach
of a very large radius is a question for a ruler about reach, not this one.

### A crowd in three numbers, expanded ONCE

`formation_grid: {cols, rows, spacing_m}` becomes an ordinary `formation` list
in `board::benchmarks`, at the moment the yaml is parsed — not at simulate time.
A crowd written out is a line per body nobody can check by reading, and a ruler
whose terms cannot be argued with is not a ruler.

**Why there and not in `parse_fight`:** the PAGE has to draw the crowd. The
arena is the source — what you see is what gets simulated — and it reads
`formation`. Expanding at simulate time would have left the canvas drawing one
body for a crowd fight; expanding in both places is the two-implementations
bug this repo keeps paying for. One expansion, and every consumer downstream —
the canvas, the payload, `parse_fight`, the scorer — sees only bodies,
including the board page's own arena.

### Which ruler leads is declared

`primary: true` on `standard_single_target.yaml` declares the ruler the board
page opens on and the one the app seeds its default scenario from;
`board::benchmarks::all()` sorts on it, so both consumers inherit one answer.
Path order is not a declaration: a new ruler's file can sort first.

### An empty board is a real state

A benchmark exists before anyone has submitted to it: the page lists it, states
its twelve rules, draws its fight and reports "0 of 224 entrants measured".
`check_board_link.mjs` REPORTS a ruler with no rows rather than failing on it —
and says so on screen, because a check that quietly exercises nothing reads
exactly like one that exercised everything.

## Adding a ruler: what it costs

**A ruler is a data file.** Nothing on the path holds a list of benchmark ids:

| link | how it learns of a new ruler |
|---|---|
| the engine | `board::benchmarks::all()` globs `data/benchmarks/*.yaml` and expands each file's ladder |
| `/api/meta` | maps that list |
| the page's ruler picker, scenario bar, board page | read `META.benchmarks` |
| the worker | validates `benchmark` as an ID, holds no whitelist |
| the workflows, the reconciliation, the purge, the live board | `wfsim-board --rulers` |
| `site/board/` | a weapon's file holds every ruler's rows, so one replaces only its own |
| the site build | reads `data/benchmarks/*.yaml` for the base rulers' sentences |

**And the rules come with it.** `standard_multi_target` refuses an incomplete build with
the same words `standard_single_target` does — "0 mods, and this benchmark wants all 8
main slots", "0 of 2 arcane seats filled" — because `validate_for_board` reads the
`build:` block out of the yaml. A new ruler's admission standard is written, not
coded.

### A ruler's ladder: one fight, measured along axes

Each of the four fights spans `scene` and `shooting`: four scenes and three
shooting conditions make 48 cells. Shooting is a chosen combination of
headshot rate and aiming, not a headshot-only axis: its rungs are 100%, 50%
and 0% headshots, all aiming. Weapon-forced targeting and stance still apply.
The board offers one row per axis; changing either keeps the other, and
changing the fight keeps both. The full cell id travels in links and saved
scenarios. The first shooting rung retains every existing ruler id and score;
the other two have their own facts and enter the paced cell sweep.

**A FILE IS ONE FIGHT; ITS LADDER MAKES IT SEVERAL RULERS.** An axis
(`data/benchmarks/axes/<id>.yaml`) lists RUNGS, and a rung is a CHOSEN
combination of fight fields — `scene` names each one by its level, its one
empowerment (Steel Path, Deep Archimedea or The Icebind, docs/MECHANICS.md
§"Empowered Enemies") and the squad that sizes it, in the order the page shows
them: Lv 9999 Steel Path, Lv 1000 Icebind (six), Lv 500 Archimedea (four), Lv
200 Steel Path — never a product of them, so only the fights someone
decided to measure exist. The FIRST rung is what each spanning ruler's own file
states, and the build refuses a ruler that states otherwise; a rung added once
lines up on every board. A later rung overwrites only the fields it sets, so it
states every field in which its fight differs. A file spans axes by
`ladder: [...]`, and `all()` expands it into one cell per combination of one
rung per axis: the base keeps the file's id, so no stored score, link or record
moves, and every other cell appends `__<tag>` per axis it moves
(`standard_single_target__icebind1000`). The target stays the file's in every
scene, a theoretical one where the game would not spawn it.

**A CELL IS AN ORDINARY RULER.** It has its own facts under `(identity, ruler,
mode)`, its own groups and so its own entry line. A SCENE IS JUDGED ON ITS OWN
FACTS: a build keeps earning a scene's rows by clearing the line on one of its
rulers, and is first asked into a scene for clearing it on the base
(`park_under_entry_line`). `{<axis>}` in the name, the standard
and the rules is filled per cell — the rung's `short` in the name, its `long` in
the prose — and a file that spans an axis may not write its base's words out
(`a_spanned_axis_is_never_written_out`): it would publish them on every cell.

**THE ROSTER IS THE SCORER'S, NEVER THE DIRECTORY'S.** Every loop over rulers
reads `wfsim-board --rulers`. A loop over the files skips every cell, and the
purge built from one deletes every row owed under a cell as "retired" —
`check_rescore_paths.mjs` refuses one.

**THE GROUPS ARE DERIVED, NEVER COUNTED OFF THE SCORES.** Every (weapon,
ruler, mode, riven-ness) the roster, the rulers' ladders and each weapon's
sustainable modes make is a group (`board::groups::all`), whether or not a
build has landed in it; the modes are the ones a build owes rows in
(`board_modes`, read by `owed_rows`). `wfsim-board --groups` sums them by base
ruler and scene and says how many rows one build owes, so an axis added to
`data/benchmarks/` is priced before it ships.

**A SCENE COSTS A BOARD, AND IS PAID FOR AFTER THE BASE.** Every build owes a
row in every cell, so each rung multiplies the rows owed by the rulers it
spans, and a new rung starts with every build unmeasured. A cell's row is never
`new_build`'s: a new build is ranked first where it was always ranked, and the
other scenes are a sweep's (`isLadderCell` in `worker/board_tasks.js`).
Its first pass is asked at a pace, at most `SCENE_ROWS_PER_RUN` cell rows a
reconciliation, one weapon finished before the next and the most popular
first (`scripts/scene_rows.py`) — a scene's leading build need not be the
base's — so a
new rung never floods the factory's open questions.

**A SCENE'S ROWS LIVE IN ITS OWN DIRECTORY**, `site/board/<cell_dir>/` (a
weapon file and an `index.json`, as at the top) and the live board's
`/board/<cell_dir>/` beside it, so a reader of the base board never meets one.
The board page offers the scenes as a row of chips above the rulers, read from
that directory's index. A weapon's page reads its file in every scene beside
the base's (`loadWeaponBoard`), so the build finder, the weapon's board tab and
every other reader of `BOARD` hold a cell's rows as an ordinary ruler's; the
finder picks a scene before a ruler, and a short ruler name carries its scene
(`benchShort`). Every list reads one scene whole before the next: boot
regroups `META.benchmarks` (`benchScenesFirst`), the base first. What reads the
base board alone, today:

- the prerendered pages;
- the stamp's manifest (`board_meta.py`), and so the desktop shell's offline
  copy — a shell in the field cannot store a nested name, so a scene's files are
  reached online only.

The share-preview table names every cell (`ladder_cells` in
`build_site_app.py`, a second reading of `expand_ladder`).

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

The server backs up both databases and global roles nightly at 03:17 UTC
(`/usr/local/bin/wfsim-pg-backup`, `scripts/pg_backup.py`). Custom-format dumps
stream in 64MiB parts directly to private R2, without a full local staging file.
Every part is read back and checked by byte count and SHA-256; the manifest is
written last, after every dump succeeds. Concatenate each database's listed
parts in order and check its whole digest before restoring. The manifest states
whether an isolated restore was tested; readback alone does not claim one.
Thirty days stay in `wfsim-pg-backup`, apart from explicitly held maintenance
checkpoints. This copy includes personal data and stays in the owner's accounts.
Postgres logs statements past five seconds, rotated weekly and four kept.

**IT FETCHES COLD.** The backup reads the rows straight out of the database
with no cache of any kind, because a copy that shares its input with the thing
it is copying shares its failure mode.

**THE BRANCH IS CHEAP BECAUSE THE FILE IS SORTED.** One record per line
(`{"k": "<identity>", "v": {…}}`), keys sorted, object keys canonicalised — so a
night's change is ~35 added lines and git stores the delta. Measured on the
first real snapshot — 2,502 records, 1,598,310 bytes — **1.52 MB once and about
22 KB a night after it**, against 570 MB a year if each night were kept whole.

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
PG_URL=postgres://…@127.0.0.1:15432/wfsim  scripts/restore_library.sh library.ndjson --write
```

**IT IS ADDITIVE, NEVER DESTRUCTIVE.** It writes the records in the file and
touches nothing else, so restoring an old snapshot cannot delete newer
submissions — which is the failure a restore is most likely to cause and the one
nobody thinks about while restoring.

**THE PARAMETERS ARE BOUND** because an identity is built from ids that arrived at a public endpoint,
and a restore is the worst moment to discover one of them carried a quote.

### Three things fail differently, which is why there are three

- `guard_shrink` (the **tripwire**) refuses to publish a board from a short
  list. It is the only one that works while nobody is watching, and it cannot
  help if the server loses the database.
- The **backup** can, and cannot help if nobody notices for a month.
- **`site/board/` in git** is a partial copy: every PUBLISHED row carries its
  build. What it misses is the builds under the 50% floor.

## What it costs as it grows

The rule the whole pipeline is measured against:

> **Every step's cost should be proportional to what CHANGED, not to what
> EXISTS.**

Scoring obeys it: a build with a fact costs nothing, so a run's bill is the
builds that arrived. Reading the library is one ordered, paged query
(`scripts/fetch_library.sh`), and a library that comes back under 90% of the
last board's `submissions:` is refused (`guard_shrink`).

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

### Where the 179 hours go, measured

Every row records what it cost, so the bill can be read straight off the
`scores` table rather than estimated. Read there, over every row that carries a
cost — `single_target_no_aim` is retired and `demolisher` has taken its place,
and the retired ruler's 12,052 rows are kept because a fact is not wrong for
being old:

| ruler | rows | total | mean row | median row | p99 row | worst row |
| --- | --- | --- | --- | --- | --- | --- |
| `standard_multi_target` | 15,225 | **6,413 min** | **25.3 s** | 14.9 s | 217 s | **9.9 min** |
| `standard_single_target` | 21,778 | 2,194 min | 6.1 s | 3.8 s | 30 s | 4.2 min |
| `demolisher` | 21,767 | 2,096 min | 5.8 s | 3.7 s | 29 s | 4.7 min |
| `single_target_no_aim` | 12,052 | 1,181 min | 5.9 s | 2.9 s | 54 s | 1.8 min |

**THE MEAN IS THE COLUMN THE SCHEDULER READS**, and the gap between it and the
median is the whole reason: every one of these rulers costs half again what its
median row says, so a slice priced from medians is a slice sized at two thirds.

WHAT CARRIES IS THE SHAPE, not the figures: the bill is dominated by the ruler
with the most bodies in it, and a single-target ruler costs an order of
magnitude less however many of them there are. It was measured on a 361-body
crowd; the crowd ruler is 25 bodies now and its share has not been re-measured.

**`standard_multi_target` is 60% of the live bill** on 26% of its rows, and
inside that a handful are the tail: the top 100 rows of 15,225 are 11% of that
ruler's bill. That is not a pathology to hunt — the cost of a row is how much the
build actually DOES, so the most expensive rows are the strongest builds on the
biggest ruler. It is the makespan floor: one row is one indivisible unit, so no
row-wise fan-out goes below the biggest row.

**RIVEN ROWS ARE 57% of the `standard_multi_target` bill on 54% of its rows**, at
26.7 s against a plain row's 23.5 s. The corner search is not in that figure:
`wfsim-intake` resolves a shape into its corners before anything is queued, so
the scorer fights a corner like any other build and a riven row costs what its
build does.

### Two ceilings, and neither is the shard count

The shard count is not what binds. Both ceilings are measured, and a sizing
argument that does not name them will be wrong.

**FORTY JOBS RUN AT ONCE, WHATEVER THE MATRIX SAYS.** The account's concurrent
job limit is the real fan-out: across every board run in a day, concurrency sat
pinned at exactly 40 for 263 minutes and never once reached 41. A matrix of 128
is therefore several waves, not one — measured start spread across the shards of
one run, 195 minutes — and every shard past the limit adds a checkout and a
cache restore while buying no parallelism at all. **The wall clock of a full
rescore is `total work / the jobs in flight`**, and no shard count moves it.

**AND THE CEILING IS THE WHOLE REPOSITORY'S**, not the board's. A rescore that
takes all forty starves every other workflow, CI included. `max-parallel: 24`
on the scoring matrix leaves sixteen, room for CI's parallel checks beside the
settle — the board is a background job, CI is the one a person waits on, and a
rescore a third longer against hours it already takes is the cheaper side of
that trade.

**AND ONE ROW IS INDIVISIBLE.** The worst row is 121 minutes, so even at
infinite fan-out a full rescore cannot finish faster than that. Row-wise
splitting is within a small factor of its own floor already.

Together they say the same thing: **a full rescore cannot be made fast, so the
lever is not paying for one** — §"A rescore costs the rows somebody deleted".

### Not paying for rows almost nobody reads

A third of the bill goes on rows scoring under a quarter of their group's
leader. They are PUBLISHED and a reader can widen the depth to them, which is
why they are not simply dropped — but a reader at the default never sees one.
**There is no screen today.** One must take its cut from the group's OWN leader
in the facts, and a screened row must produce no row at all rather than a
lesser one: a probe that writes a second kind of number onto the board needs a
rule to keep the two apart.

### What must not change

- **The site reads the live board, and only it.** The page reads
  `cdn.wfsim.app` and the worker answers `/board/*` from the same bucket
  (§"The live board"); the committed `site/board/` is a
  daily snapshot for people and never a fallback, so a reader is never shown a
  board older than `scores` says.
- **The store keeps nothing about submitters.** No IP, no token, no time finer
  than the day. A working client's id is never joined to a submission
  (§"Compute orders", §"Contribution"). The one thing beside a submission is
  who asked (§"Who asked"): a signed-in account's code on the rows it owes,
  erased once they are computed, and never in `builds` or its snapshot.
- **The library is the only irreplaceable thing.** Boards are derived, the site
  is generated, the code is in git. Anything that could truncate it needs a
  tripwire before it needs a backup.
- **A client's number never becomes a score on its own.** It enters `scores`,
  and so a board, only when a second client produced the same bits or the
  server did (§"Compute orders").
  Every published row is reproducible from the repo.
- **No work is enqueued anywhere.** What is outstanding is derived from the
  facts that exist, so there is no second copy of it to fall out of step with
  the first — §"The shape, end to end".
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
- **A LAYER'S FAILURE MUST BE SLOWNESS, NEVER A WRONG ANSWER.** A hand-kept list
  left incomplete must cost time, not publish a number the engine does not
  compute.

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

**A RESCORE IS "MEASURE THESE ROWS AGAIN", AND THERE IS NO SECOND KIND** — and
it is an INSERT, never a DELETE, which is what stops a rescore from
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

### The four pieces, and what each is allowed to do

**INGEST — the worker.** `POST /api/board/submit` writes one row:
`INSERT … ON CONFLICT DO UPDATE` into `builds`. The key is the build, so a
resubmission is the same row. `GET /api/board/pending` is `SELECT COUNT(*)`. It knows nothing about
scores and writes nothing else.

**COMPUTE — a shard.** Reads the library and the facts, takes its share of
the difference, and after each row **writes the fact immediately** — appended to a log and flushed per row, shipped by a process
running beside it. It produces NO artifact and NO blob: its only output is rows. Killed at any point, it keeps everything it
wrote, and what it did not write is simply missing — which is the same thing as
never having started it.

**PUBLISH — a projection, and it depends on no run.** One read of the facts,
rank, keep everything within half of each group's own leader, write
`site/board/<weapon>.json` and the index beside it, commit (§"The assembly"). It can run at any
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
     player ──► Worker ──► Postgres builds
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
     nightly   Postgres builds ──► library-backups branch (GitHub)
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
every one of them is a real measurement — §"The board is a record, and a mixture is allowed".

### What it costs, at each tier

| tier | runs on | free ceiling |
| --- | --- | --- |
| serve | Cloudflare static assets | unmetered |
| write | Worker + Hyperdrive | 100k requests/day |
| compute | GitHub Actions | unmetered minutes, 40 jobs = 960 CPU hours/day |
| store | Postgres, on the owner's server | the server's disk |
| backup | a git branch | ~22 KB a night |

The paid plan is taken for Workers Builds concurrency rather than for any of
these numbers.

### The rule efficiency is judged by

> **Every step's cost should be proportional to what CHANGED, not to what
> EXISTS.**

Finding the work is an indexed query, publishing is one query, and finding the
rows a broken build wrote is `WHERE measured_by = ?`.

**THE ONE STEP THAT STAYS O(EXISTS) IS A FULL RESCORE**, and no store changes
that: forty jobs is the account's ceiling and one 121-minute row is a floor no
split goes under. The lever is not paying for one — a person deletes the rows a
change actually reached, and only those are rescored.


---

## One database, and the two things that are deliberately not in it

**ONE POSTGRES DATABASE IS THE SYSTEM OF RECORD**, and the board holds three tables
in it: `inbox`, what players sent, verbatim, until intake has made a build of
it; `builds`, the library; and `scores`, the facts computed from it. `batches`
and `queue` are the work list beside them, and they may only ever CAUSE work.
Nothing else is a live store of the board's — no KV namespace, and nothing of
it in the R2 bucket.

The division is decided by two questions, asked of each piece of data:

> **Do you ever need to ask a QUESTION about the set?** — if so it needs a
> database, and nothing else will do.
> **What happens if it is lost?** — that decides how many copies it gets, and
> where they are.

| | question asked of it | if lost | where |
| --- | --- | --- | --- |
| the library | constantly | **gone for ever** | Postgres `builds` |
| the facts | every operational one | recomputed, at 179 CPU hours | Postgres `scores` |
| the published board | none — it is read | regenerated from the facts | git, served from the CDN |
| the snapshots | none | it IS the last copy | a git branch, at another vendor |

**WHY KV COULD NOT KEEP THE LIBRARY.** It has no queries, so every question
about the set had to be faked: "how many are there" became a counter key with
an hourly corrector, "is this build already held" became a read before every
write, and "which weapons are under-covered" could not be asked at all.

**WHY R2 HOLDS NOTHING OF THE BOARD.** It held the score blobs, and a score is a row now.
The one job it might have inherited is the off-vendor copy, and the branch
holds that (below).

**ONE STORE MEANS ONE POINT OF FAILURE FOR A WRITE**, and that is accepted: a
submission that fails is a retry, not a loss, and it failed the same way when KV
was down. What protects the library is not a second live store — it is a copy
somewhere else, and a gate in front of the damage.

### The library is protected in three layers, and they fail differently

1. **THE LIVE COPY** — Postgres. Where it is read and written.
2. **THE GATE** — `guard_shrink`. A run that comes back with materially fewer
   builds than the last board was built from REFUSES to publish. A backup
   restores after the damage; this declines to do it, which is the only one of
   the two that works while nobody is watching.
3. **THE OFF-VENDOR COPY** — `backup.yml` writes the whole library, one sorted
   record a line, to the `library-backups` branch every night: for ever,
   versioned, and about 22 KB a night because the file is sorted and git stores
   the delta.

**THE COPY MAY NOT LIVE WHERE THE ORIGINAL DOES.** One server or one account —
a disk lost, a mistaken delete, a suspension — would take both. The branch is
at GitHub: another vendor, another credential, another failure domain. That is
the whole reason the snapshot is where it is.

**AND THE SNAPSHOT IS PUBLIC, WHICH IS WHAT MAKES IT FREE.** A record carries
the build and a DAY, and nothing about whoever sent it — no address, no token,
no time finer than the date. So the cheapest possible backup is also a legal
one. The `builds` table inherits that constraint: `at` is a date, and a column
recording anything finer would quietly retire this whole arrangement.

### A restore that points at a retired store is worse than none

`scripts/restore_library.sh` puts the snapshot back. It is the half that makes
the branch a backup rather than a hope, and CI runs its self-test — but a
self-test proves the SHAPE, not the destination.

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

**ONE QUERY, NEVER A WALK OR A COUNTER.** Listing a store on every read spends
a metered allowance, and a counter key needs a corrector to stay true.
`COUNT(*)` needs neither, which is most of why the library lives in a database
at all.

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
`build::scenario::Capability::absence()` sorts every capability into two kinds and
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
through `board::builds::board_key`, so the build on screen and every row its weapon
holds are keyed by one engine in one pass. `board::builds::board_key` is that one
spelling — `format!("{}#{}", identity(&v), mode)`, defaulting a blank mode to
`base` — and THE MODE IS PART OF THE KEY, because one build played two ways is
two entrants. The one order that IS the identity is the elemental one: Torid
Heat/Cold/Toxin/Electric is Blast+Corrosive at 12,424 DPS against
Heat/Toxin/Cold/Electric's Gas+Magnetic at 46,583.
