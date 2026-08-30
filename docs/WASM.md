# WASM plan: run the engine in the player's browser

**Status: phases 1–4 implemented.** `scripts/build_site_app.py`
rebuilds `site/app/` (needs `rustup target add wasm32-unknown-unknown` +
`cargo install wasm-bindgen-cli` at the Cargo.lock version); wrangler deploys
`site/` as before. Phase 5 (worker pool over all cores) remains open.

**Goal.** wfsim.app serves static files only; every simulation and optimizer
run executes on the visitor's own CPU, inside the browser, via WebAssembly.
No server compute, no install. The native local server stays fully working —
it is the dev harness and shares all code with the wasm build.

**Verified.** `cargo check -p wfsim-engine --target
wasm32-unknown-unknown` passes as-is — every dependency (serde, serde_norway,
…) is wasm-compatible. The architecture was designed for this seam: the
frontend talks to the engine exclusively through a handful of JSON endpoints
(`/api/meta`, `/api/panel`, `/api/simulate`, `/api/opt-buffs`,
`/api/optimize` + status/cancel), so "port to wasm" ≈ "swap fetch for a wasm
call".

---

## Phase 1 — embed the data files (no behavior change)

The engine loads `data/*` YAML from disk at runtime; the browser has no
filesystem. Replace each directory scan with a compile-time-embedded file
list behind ONE shared provider, cfg-split native/wasm (or embedded on both —
simpler and removes the cwd assumption entirely; preferred).

Loaders to convert (each is a `read_dir` + `read_to_string` pair):

- `engine/src/mods_data.rs:267` (`data/mods/<class>/*.yaml`)
- `engine/src/arcanes_data.rs:654,685` (`data/arcanes/*`)
- `engine/src/evolutions_data.rs:235-251` (`data/evolutions/*.yaml`)
- `engine/src/enemy_data.rs:152,230,249` (`data/enemies/*.yaml`, incl.
  `custom/`; note `EnemySpec::load(path)` single-file API also used by the
  CLI — keep it, backed by the embedded set + a native fs fallback)
- `engine/src/dummy.rs:3010` (a test path — tests may keep fs)

Approach: `include_dir` crate, or a zero-dep `build.rs` that generates
`embedded_data.rs` with `include_str!` entries (repo prefers few deps).
`web/src/main.rs` already embeds enemies/assets via `include_str!` — after
this phase that duplication can collapse onto the engine's provider.

**Done when:** full test suite green (179 engine + 4 optimizer), the web
server behaves identically, and `cargo check` for the engine passes on
wasm32 with data access compiled in.

## Phase 2 — extract the API layer, add the wasm crate

`web/src/main.rs` (~1.9k lines) mixes the HTTP server with the JSON API
functions. Split:

- New lib crate `webapi/` (name: `wfsim-webapi`): move `meta_json`,
  `panel_json`, `simulate_json`, `opt_buffs_json`, plus the optimize
  request parsing / enumeration / result-building (everything
  `optimize_start`'s worker does, refactored into a callable
  `run_optimize(req, progress_callback, cancel_flag) -> Value`). The
  weapon registry, pools, helpers move too. The HTTP server keeps only:
  sockets, routing, static assets, the background-job registry (jobs +
  status/cancel endpoints wrap `run_optimize`).
- New crate `wasm/` (`wfsim-wasm`, `crate-type = ["cdylib"]`,
  `wasm-bindgen`): expose
  `api(endpoint: &str, body: &str) -> String` dispatching to the webapi
  functions, and `optimize(body, on_progress: js_sys::Function) -> String`
  for the long-running path. Build with `wasm-bindgen-cli` (or wasm-pack);
  release + `wasm-opt -Oz` if size matters (YAML payload is small; expect
  a low-single-digit-MB .wasm).

**Done when:** native server compiles against the webapi crate with zero
behavior change; `wfsim-wasm` builds for wasm32.

## Phase 3 — optimizer under wasm (single-threaded v1)

- `optimizer/src/lib.rs` `evaluate_batch` uses `std::thread::scope` +
  `available_parallelism`: add a `#[cfg(target_arch = "wasm32")]` branch
  that evaluates the chunk loop sequentially (identical seeds → identical
  results, just serial).
- `std::time::Instant` (`run_funnel` round timing) does not exist on
  wasm32-unknown-unknown: cfg the timing out (RoundNote.ms = 0) or use the
  `web-time` crate.
- Progress: `FunnelState` atomics work single-threaded but cannot be
  polled from outside a busy worker — thread an optional per-round
  callback through `run_funnel` (native passes None; wasm posts progress).
  Cancel: a JS-set flag checked between rounds (same `cancel` atomic,
  flipped via a wasm-exported setter before… note: a busy worker cannot
  receive messages; realistic v1 cancel = terminate the Worker from the
  page, which is clean since all state is inside it).

**Done when:** a headless browser test runs a small optimize inside a
Worker and the per-round progress arrives.

## Phase 4 — frontend transport shim + deployment

- `web/src/static/app.js`: introduce `api(path, body)` used by all fetch
  sites (`/api/meta`, `/api/panel`, `/api/simulate`, `/api/opt-buffs`,
  runOptimize/poll/cancel). Native mode: fetch as today. Wasm mode
  (`window.WFSIM_WASM = true`, set by the static deployment's index.html):
  a Web Worker owns the wasm module; `api()` becomes worker RPC. The
  optimize start/status/cancel triad is emulated against the worker
  (progress messages fill the same status shape the poller renders;
  cancel = worker terminate + re-init), so the progress UI is unchanged.
- Assets: `/pol/*` icons ship as static files; `/img/*` goes straight to
  the CDN (the local disk cache is a native-server nicety).
- Packaging: build step copies `web/src/static/*` + the wasm pkg into
  `site/app/`; wrangler already serves `site/` at wfsim.app. The weapon
  page's "Open in your local wfsim" button becomes "Open the builder" →
  `/app#/w/dual_toxocyst`.

**Done when:** wfsim.app/app runs a sim and an optimize with DevTools'
network tab showing zero compute requests.

## Checkpoint / resume (a reload kills the run)

A page reload terminates the dedicated worker, and there is no browser
mechanism that avoids it. A SharedWorker does not help: measured
2026-07-30 with a probe that counts in a blocking loop, the shared
instance is torn down the moment its last client disconnects — the
reloaded page reconnects to a *fresh* worker whose counter restarts. (A
nested `new Worker()` inside a SharedWorker is worse: it crashes the
SharedWorker outright, so start succeeds and every later status says "no
such job".) So the run cannot be made to survive; instead losing it is
made cheap.

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
- The enumeration budget's clock is held while a resumed screen re-walks.
  That phase does no screening (rejected candidates are skipped), so
  charging it would spend the whole 20 s catching up. A resumed screen
  therefore covers MORE of the scope than an uninterrupted one — which is
  the point of resuming, not a discrepancy.
- Best-so-far also comes from INSIDE a round, every 4096 jobs. A round is
  one blocking `evaluate_batch` call and round 1 of a materialized scope is
  millions of jobs — round boundaries alone are far too coarse a heartbeat
  to answer a cancel with.
- **Still not resumable: the inside of a round.** Rounds are the resume
  granularity, so a reload 4 minutes into a 5-minute round 1 replays that
  round. Making it finer means persisting the evaluated prefix's summaries
  (the cull needs every job's score and σ, not just the leaders) — order
  MBs, so IndexedDB rather than localStorage. Not built.

## Phase 5 (later) — use all the player's cores

Two options, decide then: (a) a JS-level Worker pool — partition jobs
across N workers each owning a wasm instance (jobs are independent; merge
per round; no shared memory needed), or (b) wasm threads
(SharedArrayBuffer + COOP/COEP headers on the wrangler worker +
`wasm-bindgen-rayon`). (a) is simpler and deployment-header-free.

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
missing one). Hotlinking `cdn.warframestat.us/img/…` answers **301 →
raw.githubusercontent.com**, which is unreliable to blocked from mainland
China. If wfsim.app loads, its art loads.

**A SIZE CLAIM IS MADE ON THE WIRE, NOT ON DISK.** Cloudflare answers `br`, so
the raw byte count is not a number about any reader: a 6.7 MB wasm is
**1,336 KB** downloaded. Judge a change by compressing both sides with the
same brotli. `wasm-opt -Oz` takes 6.74 MB to 5.89 MB, which reads as 13% and
is **-0.3% on the wire**, because it shrinks CODE and 59% of this binary is
DATA. Not shipping the 43% of `data/` that is comments (`engine/build.rs`)
moves it: 1,192 KB to 927 KB, **-22%**. wasm-opt runs anyway, for the 1.5 MB
it takes off the blob this repo COMMITS every build.
DE permits this: their Content Policy requires only that use of Warframe
assets be non-commercial, and the wiki hosts the same files on the same basis.
What it forbids is their LOGOS, so the only mark here stays ours.
A `wiki:` prefix in `assets.yaml` means the CDN lacks that file and the FETCHER
takes it from the wiki; the cached name and the page's URL are the bare name.

## A simulation runs on a worker fleet

**A SIMULATION RUNS ON A WORKER FLEET.** The runs are INDEPENDENT given their
index, so the page shards them across one worker per core (capped at eight)
and the shards merge back into exactly what one worker would have produced.
Measured on the group-clear ruler with the board's #1 Phantasma Prime build:
**85.7 s → 18.3 s**. THE ENABLER IS THE SEED — each run's dice are a pure
function of `(seed, index)`. THE MERGE IS IN RUST, so there is one
implementation of the arithmetic: the page schedules and collects,
`simulate_merged` computes every field. A `Shard` carries SUMS rather than
runs — 24 KB at a thousand runs against 8 MB — plus one
`(effective, rng_state)` per run, because the MEDIAN engagement is what the
panel shows; the merge ranks those and REPLAYS the winner.

**A JSON NUMBER IN JAVASCRIPT IS A DOUBLE**: the 64-bit RNG state travels as
two `u32` halves (`RunKey`), or it comes back ROUNDED and the merge replays a
fight that never happened — every mean matching to the last bit while `score`,
the one figure taken from the median run, disagrees. Asserted three times: on
the summary (`eight_shards_are_one_run`), on the whole response
(`a_fleet_of_shards_reports_what_one_worker_reports`), and ON THE WIRE in
`check_run_counts`, the only one that could catch the rounding.

A COMPARISON IS TO A PART IN 10^12, not bit for bit: floating-point addition
is not associative.

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
