# WASM: the engine runs in the player's browser

`scripts/build_site_app.py` rebuilds `site/app/` (needs `rustup target add
wasm32-unknown-unknown` + `cargo install wasm-bindgen-cli` at the Cargo.lock
version); wrangler deploys `site/`.

**THE DEPLOY RETRIES CLOUDFLARE, NOT ITSELF.** The deploy command is `bash
scripts/deploy.sh`. It runs `npx wrangler deploy` and retries up to four
times with a 10/30/60 s backoff, but ONLY when the output carries one of the
phrases Cloudflare's own API produces when it is unwell:

```
GET /accounts/<id>/workers/scripts/wfsim/secrets -> 503 Service Unavailable
upstream connect error or disconnect/reset before headers
[ERROR] Received a malformed response from the API
```

Anything else — a wrong binding, a missing `site/`, a bad `wrangler.jsonc` —
fails on the FIRST attempt and says so, because retrying a real error three
times buys nothing and costs the reader three copies of one message. A build
that fails leaves the site on the previous commit with nothing in the repo to
show for it, which is the whole reason this is worth a script: a push that
looks green in git can be a site that never moved.

wfsim.app serves static files only; every simulation and optimizer run executes
on the visitor's own CPU, inside the browser, via WebAssembly. The native local
server stays fully working — it is the dev harness and shares all code with the
wasm build. The frontend talks to the engine only through JSON endpoints
(`/api/meta`, `/api/panel`, `/api/simulate`, `/api/opt-buffs`, `/api/optimize`
+ status/cancel), so the wasm build swaps fetch for a wasm call.

---

## The layers

Code cites these by phase number.

**Phase 1 — the data is embedded.** `engine/build.rs` compiles the `data/` tree
into the engine behind one provider, on native and wasm alike, so nothing reads
`data/` from disk at runtime and nothing assumes a working directory.

**Phase 2 — the API layer.** `webapi/` (`wfsim-webapi`) holds every endpoint
function; the native server keeps only sockets, routing, static assets and the
background-job registry. `wasm/` (`wfsim-wasm`, a `cdylib` over `wasm-bindgen`)
exposes `api(endpoint, body)` dispatching to the same functions, plus the
long-running entries (`optimize`, `simulate_progress`, the shard calls).

**Phase 3 — the optimizer is serial under wasm.** A Web Worker has no threads,
so `evaluate_batch` has a wasm32 branch that evaluates the jobs in order with
the identical per-job seeds — the same results as native, just serial.

**Phase 4 — the transport shim.** The static deployment's `index.html` sets
`window.WFSIM_WASM = true`; a Web Worker (`worker.js`) then owns the wasm
module and `api()` is worker RPC, with optimize start/status/cancel emulated
against it so the progress UI is the same. Unset (the native server), `api()`
is plain fetch. Cancel terminates the worker and re-initialises it.

## Checkpoint / resume (a reload kills the run)

A page reload terminates the dedicated worker, and no browser mechanism
avoids it: a SharedWorker is torn down the moment its last client disconnects,
and a nested `new Worker()` inside one crashes it outright. So the run cannot
be made to survive; instead losing it is made cheap.

- `run_funnel(…, start_round, on_checkpoint)` fires after every COMPLETED
  round with the surviving field, and `start_round` skips straight to a
  saved round taking `alive` as its input. Seeds key off the ABSOLUTE
  round index, so a resumed run draws exactly the numbers an
  uninterrupted one would — pinned by
  `a_resumed_funnel_lands_on_the_same_leaderboard`.
- A checkpoint holds IDENTITIES only — `(ordered pool indices,
  evolution-set index, exilus choice, arcane index)` — so it fits
  localStorage and cannot drift from what the enumerator would produce.
  `webapi::run_optimize_resumable` rebuilds the candidates from them
  (`optimizer::rebuild_candidate`); a checkpoint that no longer resolves
  to any build under the current scope is refused, not silently emptied.
- `jobs_at_start` travels with it: the round schedule is a function of
  the ORIGINAL field size, so deriving it from the (already narrowed)
  survivor list would shorten the schedule and change what round N means.
- The page stores one checkpoint (`wfsim-optimize-checkpoint`) together
  with the REQUEST that produced it, and a resume replays that stored
  request — never one re-derived from the form, which may have been
  edited since. It is dropped on completion, on cancel, when a fresh run
  starts, and after 24 h. Resuming takes a click; it costs minutes of the
  visitor's CPU, so it is never automatic.
- A `beforeunload` guard warns while a run is in flight.
- **The screen is a resume point too**, every 8,192 candidates walked. It
  is the one phase with no rounds in it, so before that a reload during it
  cost the whole pass. A screen cut is `(candidates walked, survivors as
  (sequence number, arcane))` — POSITIONS in the walk, not builds: the walk
  is deterministic, so re-walking regenerates them, and the resumed screen
  pays only to re-evaluate the survivors, not for the scope it had already
  rejected. `a_resumed_screen_lands_on_the_same_survivors` pins the set.
  Only the SERIAL (wasm) screen emits cuts: the threaded screen's heap lags
  its producer, so a cut taken there would not be a consistent prefix.
  Honouring a cut works on both.
- Best-so-far also comes from INSIDE a round, every 4096 jobs. A round is
  one blocking `evaluate_batch` call and round 1 of a materialized scope is
  millions of jobs — round boundaries alone are far too coarse a heartbeat
  to answer a cancel with.
- **The inside of a round is not resumable.** Rounds are the resume
  granularity, so a reload 4 minutes into a 5-minute round 1 replays that
  round.

---

## Build notes

- Static assets are `include_str!`'d into the native server — rebuild
  (`cargo build --release -p wfsim-web`) and restart after any static
  edit. Serve `site/` locally with
  `python -m http.server 8000 --directory site`.
- The UI never uses native `prompt()/alert()/confirm()` dialogs — inline
  inputs only.
- Determinism: per-job seeds are fixed; serial wasm evaluation must
  reproduce native results bit-for-bit (same seed math, same order).
- The optimizer only calls the engine (CORE.md §5).

---

## A size claim is made on the wire, not on disk

**Images are SAME-ORIGIN, and the art ships with the site.** `site/img/` holds
every file `data/assets.yaml` references (`scripts/fetch_images.py` fills
`web/cache/img/`, `build_site_app.py` copies it and FAILS the build on a
missing one). A third-party image host is unreliable to blocked from mainland
China; same-origin means that if wfsim.app loads, its art loads.

**A SIZE CLAIM IS MADE ON THE WIRE, NOT ON DISK.** Cloudflare answers `br`, so
the raw byte count is not a number about any reader: a 6.7 MB wasm is
**1,336 KB** downloaded. Judge a change by compressing both sides with the
same brotli. `wasm-opt -Oz` takes 6.74 MB to 5.89 MB, which reads as 13% and
is **-0.3% on the wire**, because it shrinks CODE and 59% of this binary is
DATA. Not shipping the 43% of `data/` that is comments (`engine/build.rs`)
moves it: 1,192 KB to 927 KB, **-22%**. wasm-opt runs anyway, for the 1.5 MB
it takes off the blob this repo COMMITS every build.
The art is Digital Extremes' and this repository makes no grant in it —
`LICENSE-DATA.md` §4. No Warframe or Digital Extremes logo is used anywhere, so
the only mark here stays ours.
A `wiki:` prefix in `assets.yaml` means the CDN lacks that file and the FETCHER
takes it from the wiki; the cached name and the page's URL are the bare name.

## The quick descent across the fleet

The optimizer's search is a descent from each start (docs/OPTIMIZER.md, "The
quick descent"), and it runs on EVERY worker, whatever the number of starts. A
worker is one thread and the page is not cross-origin isolated, so workers
share no memory and none can wait for another; the descent is therefore split
into steps the PAGE drives (`woptQuickFleet`):

1. Worker 0 LEADS: `quick_fleet: { lead, fresh, scores }`. It runs the whole
   descent again on every score it holds — no simulation, and its candidate
   lists and legality are cached between calls (`Fleet`, a thread-local that
   lives as long as the worker). A start that reaches builds nobody has scored
   PAUSES and the next start runs, so one call returns every start's next
   batch: `{ pending: [...] }`. An item is a build, or `{build, runs}` — a
   step screen's short measurement (docs/OPTIMIZER.md, "Each step is
   screened"), answered `rough: true` with its runs (`shards`), or `{build,
   shards}`, a full measurement that continues them, answered with its best
   order's runs (`shard`); the page passes all of it through unread.
2. The page splits the batch across all workers, the leader included:
   `quick_fleet: { score: [...] }` → each build's score and the element order
   it scored best at.
3. The scores go back to the leader. When no start pauses, the leader
   simulates the contenders — the final round is the simulator's — and
   answers with the ordinary result.

Every build is scored on the one paired stream wherever it runs, so the fleet
lands exactly where one process does
(`a_descent_split_across_workers_lands_where_one_worker_does`). There is no
time budget: the run goes until every start has settled, with a progress line
and a Cancel in front of it.

## A simulation runs on a worker fleet

**A SIMULATION RUNS ON A WORKER FLEET.** The runs are INDEPENDENT given their
index, so the page shards them across one worker per core (capped at eight)
and the shards merge back into exactly what one worker would have produced.
Measured on the multi-target ruler with the board's #1 Phantasma Prime build:
**85.7 s → 18.3 s**. THE ENABLER IS THE SEED — each run's dice are a pure
function of `(seed, index)`. THE MERGE IS IN RUST, so there is one
implementation of the arithmetic: the page schedules and collects,
`simulate_merged` computes every field. A `Shard` carries SUMS rather than
runs — 24 KB at a thousand runs against 8 MB — plus one
`(value, rng_state)` per run, because the BENCHMARK FIGHT — the middle run by
the scenario's metric — is what the replay shows; the merge ranks those and
REPLAYS the winner.

**A JSON NUMBER IN JAVASCRIPT IS A DOUBLE**: the 64-bit RNG state travels as
two `u32` halves (`RunKey`), or it comes back ROUNDED and the merge replays a
fight that never happened — every mean matching to the last bit while only the
benchmark fight's own figures (`sample`) disagree. Asserted three times: on
the summary (`eight_shards_are_one_run`), on the whole response
(`a_fleet_of_shards_reports_what_one_worker_reports`), and ON THE WIRE in
`check_run_counts`, the only one that could catch the rounding.

A COMPARISON IS TO A PART IN 10^12, not bit for bit: floating-point addition
is not associative.

**…BUT ONE GROUPING IS ONE ANSWER, ON EVERY TARGET.** Folded the same way —
one run per shard, as the board scorer does — wasm, Windows native and Linux
native produce the same bits: every run and every report field of 90 board
rows across all three rulers, and the published score of each row measured by
the current engine. The tolerance above is for a DIFFERENT grouping and for
nothing else. THE WIRE HAS TO BE LOSSLESS for that to reach the page: a shard
crosses to the merge as text, and serde_json parses a number with correct
rounding only under `float_roundtrip` (`Cargo.toml`) — without it the fleet's
report is a ULP off the scorer's on most rows.
`a_number_read_back_from_text_is_the_same_bits` holds it.

## One executor

**A PERSON'S RUN OUTRANKS BACKGROUND WORK.** The simulator and the quick calc
share the worker pool; a person's Run holds priority for its duration, and
background work yields between pieces, never mid-piece, so the foreground
waits for one piece rather than for the scan (`check_run_outranks_scan.mjs`).

**…AND THE COMMUNITY'S WORK YIELDS TO ALL OF IT.** A board order or a
volunteer riven gain runs only while the reader computes nothing: no call of
theirs on the pool (`readerInFlight`, counted on the lane — a call is the
reader's unless the community's work tagged it), no search, scan or Shapley
between calls, and no other WFSim tab saying it is busy (`readerBusy`). A board
order waits between pieces and re-asks a piece the reader's Stop took; a riven
gain, on workers of its own, stops (`check_compute.mjs`).

**THE RUN COUNT IS NOT ONE OF THE THINGS THAT ADAPT.** Adaptation is a property
of the SCHEDULER — how the work is cut up, which lane takes it, what waits for
what. How many runs an answer is measured over decides the ANSWER, and an
answer that depends on the machine reading it is not a measurement: the same
build would score differently on a phone and a workstation, and no two board
rows would be comparable. A slow machine waits longer for the same number. It
does not get a cheaper one.

So what a measured per-run cost buys is what to SAY, never what to compute: an
honest estimate of the time left (`check_calc_eta.mjs`), and an admission when
the answer will take longer than a reader will wait.

## A long sim says how far it has got

**A LONG SIM SAYS HOW FAR IT HAS GOT.** The run count is unbounded and so is
the cost per run: single-target is about a millisecond, a 361-body fight
~28 ms. `simulate_progress` is the wasm entry (its own, not a flag on `api`,
because `/api/simulate` is the one endpoint whose cost is unbounded), the
worker forwards `{done, total}`, and the panel draws a bar, THE COUNT and a
time remaining. The count, because "412 / 1000" is a number a reader can act
on. THE ANSWER IS UNCHANGED — the callback observes and never steers — and the
throttle is in the WASM layer at one message per percent. The remaining time
is hidden below a second and before 5%.
