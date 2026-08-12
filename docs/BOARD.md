# The board

The official leaderboard: **builds players submit, scored here**.

One sentence carries the whole design — **a submission is a BUILD and never a
number**. Everything else follows from it:

- a forged score is impossible, because no score is ever accepted;
- an engine or benchmark change RE-SCORES every stored build instead of
  invalidating it, and nobody is asked to resubmit;
- every row is reproducible by anyone with the repo, since the score was
  computed by the engine that ships to their browser under the benchmark's own
  pinned seed. Measured 2026-08-04: wasm and native agree to the last digit
  (`0.9647804061510868` both ways).

## The pieces

| what | where | who runs it |
| --- | --- | --- |
| the ruler | `data/benchmarks/*.yaml` | — |
| the board | `data/benchmarks/boards/*.yaml` | generated, committed |
| what the page reads | `site/board.json` | fetched at runtime, not compiled in |
| consent + submit | `web/src/static/app.js` (`offerBoardSubmit`) | the player's browser |
| the submissions | a Cloudflare KV namespace (binding `SUBMISSIONS`) | written by the endpoint |
| the deploy | `wrangler.jsonc` | `npx wrangler deploy`, from a git push |
| the endpoint | `worker/index.js` | the Cloudflare Worker, same origin |
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
| a push touching `engine/`, `webapi/`, `data/` or the scorer | **everything** | ~9 min, eight ways at once |
| a change to a benchmark's terms | everything, under the new ruler | the builds carry over; only the numbers change |
| Actions → board → Run workflow, **full = true** | everything, whatever the fingerprint says | the same ~9 min |

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

### What decides "new" — the ENGINE FINGERPRINT (2026-08-11)

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
- **fingerprint moved** → everything is rescored, and the log says what moved.

TIME IS NOT AN INPUT, which is why there is no cooldown and never will be
(asked and answered, owner 2026-08-11). An untouched row is valid forever; a row
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
digit for digit, where that build actually scores **0.170** (2026-08-12). Ten
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

## One representative per build (2026-08-04)

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

**And the MODE is the other half of it** (2026-08-09). A Torid through its
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
then by DE's own English name (owner, 2026-08-04). The endpoint stores what was
submitted verbatim — it has no mod pool and cannot tell an elemental mod from
any other — and the scorer collapses spellings after `validate` has canonicalised
them.

**The rows scored before this are wrong**, and worse, unrecoverable: the
endpoint SORTED on the way in, so the order those players actually built is
gone. They re-score as "elements in alphabetical order", which is a legal build
and probably not theirs. New submissions keep what was placed.

## The pipeline, stated once (owner, 2026-08-04)

Every step below is a rule, not a description — each one is somewhere a wrong
answer could be published.

1. **One representative per build.** `builds::canonical_mods` — elementals last
   in the order that pairs them, everything else ahead by biggest drain then by
   DE's English name. Substantively identical builds are one row.
2. **We collect builds. We compute the score.** No submission carries a number
   and none would be believed.
3. **We validate legality ourselves**, including Forma: pool, families, eight
   slots, capacity.
4. **Forma, in priority order** (owner, 2026-08-04). The order is the rule, not
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
7. **Shown at four significant figures AND four decimals** (owner, 2026-08-04),
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
the 300 s ruler of the day (owner, 2026-08-04). Boar Prime scores identically either way, because it resupplies.

One term, no weapon named, right for the whole roster.

## No version numbers

A benchmark has an `id` and no `version` (owner, 2026-08-04). There is one board
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

## What is not on the board

- **Rivens** (user, 2026-08-04). They are personal random items, so a board
  that counted them would rank luck. It also removes the only free-text field a
  player authors from anything uploaded.
- **The exilus slot** (user, 2026-08-04). Exilus mods are handling and
  mobility with no single-target damage model — the optimizer already excludes
  them — and the slot costs a separate adapter, so counting it would price a
  build against a resource the ranking cannot value. A benchmark build is 8
  slots.

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

   **In the file, not in the dashboard.** wfsim.app is a WORKER (static assets),
   deployed by `npx wrangler deploy`, and a deploy REPLACES the worker's
   bindings with what the config declares — a namespace added through the
   dashboard is removed by the next push. The id is an identifier, not a
   secret; it grants nothing without a token, and Cloudflare's own docs commit
   it.

   Named for what it HOLDS, which is not the board: the board is the generated
   YAML in `data/benchmarks/boards/`, and this namespace holds the builds people
   sent, waiting to be scored. The binding was briefly called `BOARD`, which is
   a debugging trap — "the board is empty but the BOARD binding looks fine" is a
   sentence that sends you looking in the wrong place.
2. **Repo secrets** — `CF_ACCOUNT_ID`, `CF_SUBMISSIONS_NAMESPACE_ID`,
   `CF_API_TOKEN` (a token with *Workers KV Storage: Read*).

   The middle one is the SUBMISSIONS namespace's id, and it is named that way
   for the same reason the binding is: it points at the builds waiting to be
   scored, not at the board. Every name in this pipeline says what it holds —
   the board is a file in the repo and nothing in Cloudflare is called after it.

The token only ever READS. What the board says is computed in the repo from
data in the repo; nothing secret decides a rank.

Until step 1 is done the endpoint answers 503 and the page says "could not
reach the board — nothing was sent", which is the honest state rather than a
silent failure.
