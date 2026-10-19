# Volunteer compute clients

A client takes compute orders from wfsim.app, fights them on the engine the
site serves and sends back what it measured. The page is the reference client
(`web/src/static/app/69-board-work.js`); any other speaks the same protocol to
the same worker (`worker/verify.js`, `worker/tasks.js`) and is held to the same
bits: a result counts only when an independent client, or the official
machines, produce the same bits (docs/BOARD.md §"The fact factory").

## Values

Read from the code; a check compares this block with it, and the code wins.

```text
protocol: 8
pack_protocol: 7
protocols_served: 8, 7, 6
sync_body: 262144
sync_results_max: 400
recognised_client_id: ^[0-9a-f]{24}$
verifier_id_shape: ^[a-z0-9]{16,40}$
engine_id_shape: ^[A-Za-z0-9._-]{1,40}$
gas_price_per_million: 50000
run_price: 400000
work_per_point: 1000000000
score_near_relative: 1e-12
lease_ms: 1800000
pack_ms: 60000
pack_first: 3
pack_max: 20
pack_lease_ms: 300000
packs_held: 2
renew_ms: 300000
row_ceiling_ms: 7200000
pack8_ms: 600000
pack8_max: 200
sync_lease_ms: 1500000
sync_low_ms: 120000
sync_floor_ms: 60000
sync_idle_ms: 20000
flush_ms: 600000
poll_ms: 10000
jitter_ms: 5000
urgent_jitter_ms: 2000
report_ms: 3600000
clients_per_fact: 2
facts_per_refusal_forgiven: 1000
engine_grace_days: 3
```

Each key is its constant in snake case, read by `scripts/check_clients_doc.mjs`
from worker/verify.js, worker/tasks.js, worker/appraise.js,
worker/contribution.js, worker/factory.js and engine/src/fight/gas.rs; a point
is `work_per_point` work.

**A DIAL IS STATED AS ITS DEFAULT, AND THE SERVER MAY MOVE IT.** Every key
from `lease_ms` to `report_ms` above is a dial (worker/dials.js): the owner sets
it within fixed bounds, and a change is in force within a minute with no
release. Every `/api/board/sync` reply carries the protocol 8 ones in force as
`dials`, every `/api/board/work` reply protocol 7's (and `/api/board/renew` its
`renew_ms`), and `work.json` the ones an idle client needs; a client reads them
from there and must never hard-code one. Every `next_ms` a reply gives is
already multiplied by the owner's `sync_scale_pct`.

## Terms the server holds a client to

- **The server does not tell a compliant client from the page.** Any id of the
  recognised shape is served by the same code, and its answers judged the same.
- **Refusals are paused while clients settle into a new engine** (`REFUSING`
  in worker/verify.js): a disproved answer is unpaid and nothing more. When
  they resume, the rule below applies.
- **A penalty is a cool-down and credit taken back, never a permanent ban.**
  `coolDownMs(k)` is `86_400_000 · 3^k` for a client with `k` refusals before
  this one, with no ceiling. When it ends, the next ask lifts it
  (`banned = 0` where `refused_until <= now`) and the client earns again
  without asking anyone. Each `FACTS_PER_REFUSAL_FORGIVEN · k` facts it is part
  of afterwards take one refusal off its record.
- **The previous protocols are served beside the current one** (`PROTOCOLS_SERVED`)
  and each is dropped only once nobody has asked with it for a week; a change to
  what a client must do changes this page in the same commit and adds a line
  to §Changelog. `/release.json` and every hand-out reply carry `protocol` (the
  current one) and `protocols_served`: a client that sees a `protocol` newer than
  its own may keep working and should tell its author.
- **A sync before its time is handed no work.** A protocol 8 reply's `next_ms`
  is when the client may next be handed work, and the server holds it to that
  (`early: true`): asking more often never gets more rows. Results, renewals and
  refusals in an early sync are served all the same.
- **One account's devices share one fair share** (§"Checks on a client").
- **What each answer means** is in §"The cycle, protocol 8" and §"The cycle,
  protocol 7": `unrecognised_client`
  carries a message naming whom to contact, `banned` the instant it ends,
  `stale` that the engine or the protocol asked with is not served; a missing
  consent gets nothing, a malformed body `"bad request"`, and an answer is
  always `{ ok: true }`.

## Identity

- The client id (`verifier`) is 12 random bytes as 24 lowercase hex, made once
  and kept (the page: `verifierId`). It is a secret: it is how the client asks
  what it earned and how an account claims it. A client that cannot keep it
  must not work, since a new id per start is a refusal nobody can apply.
- An id outside `^[a-z0-9]{16,40}$` is `400 bad request`. An id of that shape but
  not 24 hex gets `403 { ok: false, error: "unrecognised_client", message }` on
  `/sync`, `/work`, `/answers`, `/verify`, `/renew` and riven answers; what it earned
  stays. The message asks the author of a third-party client to make contact
  before running it.
- An account claims an id with `POST /api/account/devices/claim { verifier,
  label }` under its session; from the next hour that account holds what it
  earns (docs/BOARD.md §"Contribution" rule 5). Several ids on one machine are
  accepted; each is one client for `PACKS_HELD`, and ids of one owner or one
  network never confirm each other.

## Consent

- Nothing is computed until the person running the machine says yes to a
  statement shown to them. The page's statement is `computeAskHtml`
  (web/src/static/app/73-compute.js), version `COMPUTE_CONSENT_V`: what it runs
  (simulation and optimization tasks for every player), that a result counts
  only when two volunteers compute it separately and agree, the share of the
  device it takes and when, that it pauses on battery and costs power and heat,
  how to stop it, and that it is voluntary, unpaid, free for every player and
  never a source of income for WFSim. A client shows a statement saying no less.
- Every `/sync` and `/work` carries `consent: { v, at }`: `v` an integer ≥ 1, `at` an ISO
  instant matching `^\d{4}-\d\d-\d\dT[\d:.]+Z$`. Without it nothing is handed
  out (`pack: null, work: null`); a sync's results are still taken. The server
  keeps both on the client's row whenever they change.
- A statement that asks for more — more of the device, a new kind of task, more
  sent, another use — is a new version, asked again; a rewording keeps the yes.
  The server keeps `v` as the client states it.

## The engine

- `GET /release.json` → `{ release, commit, wasm, data, data_format, engine,
  accepted, protocol, protocols_served }`. `engine` is the id a
  client sends; `wasm` names the module: `/pkg/wfsim_wasm.<wasm>.js` and
  `/pkg/wfsim_wasm_bg.<wasm>.wasm`, `<wasm>` the first 12 hex of the module's
  SHA-256; `data` names its data (§"The official data pack"), installed with
  `install_data(text)` before the first call. The module exports
  `api(path, body)`, JSON text in and out.
- `engine` is `engine_id()` in scripts/build_site_app.py: SHA-256 over
  `engine/src`, `webapi/src`, `wasm/src`, `native/src`, `tools/gas-macro`, the
  `data` pack's files with their full-line comments and blank lines removed, and
  `Cargo.lock`, tests left out, CRLF normalised, first 12 hex. A
  native build is of the commit `/release.json` names (`commit`), unmodified,
  built without `WFSIM_GAS_OFF` (which turns the meter off), and its engine id
  equals the served one.
- The served engine works, and so does an older one while `accepted` names it:
  each is an engine replaced less than `engine_grace_days` ago, and until
  `until` a `/work` ask naming it is handed packs, with `engine` (the served
  one) and `accepted_until` beside the pack.
  - `{ engine, until, changed }` — only weapon data moved: rows of every weapon
    but those in `changed` (each moved weapon and every one linked to it), and
    its `/answers` count as the served engine's.
  - `{ engine, until, join: true }` — anything else moved: only rows a served
    engine's answer of another owner and network already leads, of every
    weapon, the pack marked `join`. Its answers count only beside the served
    engine's: one equal to a served engine's answer in all but `work` (which
    moves with the code) is a witness to it and paid as one;
    one that differs is paid too (`GRACE_SHARE` of its work, worker/tasks.js)
    when an independent answer of the same engine agrees with it, and nothing
    when none does. Such answers alone never make a fact.
  Any other engine gets `stale: true` and nothing, and `/answers` naming it
  releases the lease and drops the answers. Every `/work`
  answer carries `release`; the page reloads into a newer one when idle.
- WHEN ENGINES MOVE: whenever a change is ready, any day and as often as it
  comes — there is no release schedule. A client on the official library
  (§"The official engine library") follows each one by itself; one built from a
  commit keeps working through its `accepted` days, as the served engine's
  equal when only weapon data moved and beside it when anything else did.

## The official engine library

The engine the site serves is published as a native library, so a client loads
it instead of building one: the engine moves whenever a weapon is measured,
and a download is all that costs.

- `GET https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com/engine/latest.json`
  (or `engine/<engine>.json` for one engine) → `{ sig, signed }`. `signed` is
  JSON text `{ engine, commit, abi, files, data }`, `files` by target
  (`x86_64-windows`, `x86_64-linux`, `aarch64-macos`, `x86_64-macos`) each
  `{ path, sha256, size }`, the library
  at `<bucket>/<path>`, and `data` the official data pack the same way. `sig` is the hex Ed25519 signature of the bytes of
  `signed` under the key the desktop client verifies
  (`b488481177cb0a459a685c5d66f646768e7f66c405b92ecde81025581419a95e`). Verify
  the signature, then the SHA-256, before loading; refuse either mismatch.
- The Linux library needs glibc 2.31 or newer; the macOS ones, macOS 11. A
  target is published only when its build matched the wasm bit for bit, so an
  engine may lack one: compute that engine on the wasm.
- `abi: 2` is three C functions: `wfsim_install_data(body, body_len, &len) ->
  ptr`, the wasm module's `install_data`, called once before the first call;
  `wfsim_api(path, path_len, body, body_len, &len) -> ptr`, UTF-8 in and out,
  the wasm module's `api(path, body)` exactly; and `wfsim_free(ptr, len)` to
  give each answer of either back. `abi: 1` was the last two alone. Calls are
  independent; one library serves any number of threads, and one install
  serves them all.
- To follow the served engine: when `/release.json` names an engine other than
  the loaded one, fetch its `engine/<engine>.json`, verify, download the
  library and its `data`, load, install, and compute the next pack on it. The old one keeps working meanwhile within its
  `accepted` days. A library is published before the site serves its engine.
- A client may still build the engine itself from `commit`; the terms of
  §"The engine" are the same either way.

## The official data pack

The engine is code; the game data it computes from is the official data pack,
published beside it. A host installs the pack once, after loading the engine
and before its first call. Until 2026-10-25 01:00 UTC every engine still
carries its data as well, and a pack installed replaces it; from then on an
engine with no pack answers every call with `{ error: "engine data not
installed: data" }`.

- **Which pack.** `/release.json` names it: `data` is its version, the first 12
  hex of the file's SHA-256, and `data_format` its format. It is
  `/pkg/pack.data.<data>.json` on the site, and `data` in the engine library's
  signed body (§"The official engine library").
- **Its format** (`data_format: 1`): one JSON object `{ "pack": "data",
  "format": 1, "files": { "<path>": "<text>" } }`. Each path is a file of the
  repository's `data/` at `commit`, relative to `data/`; each text is that
  file with its full-line comments and blank lines removed. An engine refuses
  a pack of a format it does not read.
- **Verify it** before installing: its SHA-256 begins with `data` (and equals
  the signed `sha256` for the library). Refuse a mismatch, as for the engine.
- **Install it**: wasm `install_data(text)`, or `install_data_bytes(bytes)` with
  the file's bytes, which a lane holds at the pack's own size where a string is
  copied in at up to three bytes a character; native `wfsim_install_data(body,
  body_len, &len) -> ptr`. The answer is `""` on success and the reason
  otherwise. Once per engine instance, before the first call: an instance that
  has answered a call refuses it. A wasm host makes one instance per thread, so
  it installs once per instance.
- **It moves with the engine.** `engine` covers the pack's files, so a new
  `data` is always a new `engine`; follow `engine` as before and take the pack
  named beside it. Never pair an engine with another release's pack.
- **Anyone can rebuild it**: `engine/build.rs` writes it from `data/` at
  `commit` (`cargo build --release -p wfsim-wasm --target
  wasm32-unknown-unknown`, then `python scripts/data_packs.py <dir>`), and CI
  holds the published one to the source.
- The site also publishes `text` and `site` packs (`/release.json` `packs`).
  They are its own catalogs' data, not part of this contract, and no
  computation reads them.

## Migrating to the data pack

A checklist for a client's author, or an agent working for one. Nothing here
touches the protocol: 8 and 7 are unchanged.

1. **Read the release as before**, and also `data`. Keep `wasm` and `engine` as
   they were.
2. **Download the pack** with the engine: a wasm client
   `https://wfsim.app/pkg/pack.data.<data>.json`; a library client the signed
   body's `data.path` under the bucket. Cache it by `data`, as the engine by
   its digest.
3. **Verify** its SHA-256 (above).
4. **Install** it into every engine instance, right after loading it:

   ```js
   // wasm (the glue from /pkg/wfsim_wasm.<wasm>.js, any JS host)
   wasm_bindgen.initSync({ module });
   const err = wasm_bindgen.install_data(packText);
   if (err) throw new Error(`data pack: ${err}`);
   ```

   ```c
   /* native library, abi 2 */
   size_t n = 0;
   uint8_t *r = wfsim_install_data(pack, pack_len, &n);
   int ok = (n == 0);           /* any text is the reason it was refused */
   wfsim_free(r, n);
   ```

   A Rust host loading the library with `libloading` declares it
   `unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8`.
5. **When `engine` moves**, load the new engine AND its pack, and drop the old
   pair together.
6. **Prove it** before the cutoff: install the pack, then measure the rows of
   `tests/gas/rows.json` (§"Conformance", item 7); the bits must be those the
   script there prints.
7. **Know the failure**: an engine built after the cutoff answers every call
   with `{ error: "engine data not installed: data" }` until the pack is in.

A client that has not migrated stops getting work it can do after the cutoff,
and never produces a wrong number.

## The cycle, protocol 8

One door, `POST /api/board/sync` (body ≤ `sync_body` characters,
worker/sync.js). A client syncs when its work runs low, when results have waited
`flush_ms`, and when it stops; an idle client reads the work signal instead
(§"The work signal"). Every sync states, in full, what the client holds.

**Request**

```json
{ "verifier": "0123456789abcdef01234567", "engine": "6aca511ab0f4", "protocol": 8,
  "consent": { "v": 1, "at": "2026-10-14T08:00:00.000Z" },
  "lanes": 8, "pace_ms": 42000, "want_ms": 540000,
  "held": [{ "lease": "<32 hex>", "rows": ["<question>", "<question>"] }],
  "results": [{ "lease": "<32 hex>", "question": "<question>", "score": 1.25, "metric": "kpm",
                "work": 7000000000, "compute_ms": 41000, "counts": { "gas": 123 }, "lanes": 8 }],
  "keep": { "lease": "<32 hex>", "question": "<question>" },
  "refused": [{ "lease": "<32 hex>", "question": "<question>", "step": "order" }],
  "idle": [{ "hour": "2026-10-14T07", "asks": 6, "failed": 0, "lanes": 48 }],
  "states": ["<question>"], "since": "2026-10-13T16" }
```

- `lanes` 1–256 (else 1), the cores free now; `pace_ms` 100–`sync_lease_ms`, the
  smoothed wall time of a row; `want_ms` 0–`pack8_ms`, how much work to be
  handed (0: renew and hand in only).
- `held`: EVERY row the client still holds and has not answered, under the lease
  it came with. `≤ 400` rows. Each is renewed `sync_lease_ms` from now.
  **A LIVE LEASE THE SYNC DOES NOT NAME IS GIVEN BACK** — a row in `results`,
  `refused` or `keep` counts as named.
- `results`: `≤ sync_results_max`, shaped as protocol 7's answers, each with
  the `lease` its row came under. Kept by the client until a reply lists the
  question in `taken` or `dropped`, so a result is never lost to a lost reply.
- `keep`: a row cut short that the client will resume (§"A row cut short").
- `refused`: rows its engine refused (`/api/board/order` answered `ok: false`;
  `step` `^[a-z_]{1,24}$`, the page sends `"order"`). Given back, never handed
  to this client again on this engine; refused by `refusals_to_withdraw` owners
  on the served engine, the row is withdrawn.
- `idle`: protocol 7's `idle` counts (§"The cycle, protocol 7"), sent until a
  reply says `kept`. `states` (≤ 100) asks where results stand; `since` adds
  `points` (`/api/board/points`' body).

**Reply** (always `200`; a malformed body is `400 bad request`, an
unrecognised id `403`)

```json
{ "ok": true, "protocol": 8, "protocols_served": [8, 7, 6], "release": "cf74c9aba47d", "kept": true,
  "taken": ["<question>"], "dropped": [{ "question": "<question>", "why": "not_held" }],
  "held": [{ "question": "<question>", "held": true }],
  "pack": { "lease": "<32 hex>", "lease_ms": 1500000,
            "tasks": [{ "question": "<question>", "identity": "<build>", "record": { "weapon": "torid" }, "ruler": "standard_single_target", "mode": "base" }] },
  "work": null, "states": [{ "question": "<question>", "state": "confirmed", "at": "2026-10-14T08:03:00.000Z" }],
  "next_ms": 420000, "dials": { "pack8_ms": 600000, "...": "every key of SYNC_CLIENT_DIALS" } }
```

- `taken`: recorded — including a result recorded before and sent again.
  `dropped`: never recorded, with `why`: `not_held` (another client took the row
  since) or `stale_engine`. Either way the client deletes the result.
- `held`: whether each named row is still the client's; `false` stops that row.
- `next_ms`: when the client may next be handed work. Sync then, or when its
  work runs low (`sync_low_ms` left), whichever is later; sync sooner only to
  hand in results or to stop.

| reply | meaning | then |
| --- | --- | --- |
| `pack: {…}` | board rows, leased `lease_ms`; `pack_lease_ms` while work is short | compute them |
| `work: { kind: "riven_gain", … }` | a question someone waits on, alone (only to a client holding nothing) | §"A riven gain" |
| `pack: null, work: null` | nothing now: no work, fair share, no consent, or `want_ms: 0` | read the work signal; sync after `next_ms` |
| `early: true` | synced before its time; results and renewals were served | wait `next_ms` |
| `banned: true, until` | in a cool-down | sync after `until` |
| `stale: true` | engine or protocol not served | fetch `/release.json`, follow the engine |
| `busy: true` / `paused: true` | the server sheds load / its tables are moving; nothing was written while paused | keep results; sync after `next_ms` |
| `fallback: 7` | the owner switched protocol 8 off | speak protocol 7 for the rest of the session |
| `withdrawn: [question]` | rows the clients' engine refused, withdrawn now | nothing |

A pack beside an older accepted engine carries `engine`, `accepted_until` and
`join` as in §"The engine". **Sizing**: `round(want_ms / pace_ms)` rows
(`pack_first` without a pace), at most `pack8_max` and never more than open rows
over computers asking — while that cap binds, work is short: the lease is
`pack_lease_ms` and an account past its fair share is handed nothing.

## The work signal

`GET https://cdn.wfsim.app/work.json` (v1), cached at the edge for seconds:

```json
{ "v": 1, "at": "2026-10-14T08:00:05.000Z", "open": 4210, "urgent": 0, "engine": "6aca511ab0f4",
  "release": "cf74c9aba47d", "protocol": 8, "protocols": [8, 7, 6], "computing": 73,
  "poll_ms": 10000, "jitter_ms": 5000, "urgent_jitter_ms": 2000, "mode": "signal" }
```

An idle client — consent given, holding nothing — reads it every `poll_ms`
(±25%) and does not sync. `urgent > 0`: sync after a random wait in
`[0, urgent_jitter_ms]`; `open > 0`: after `[0, jitter_ms]`. A different `engine`
or `release` is followed as `/release.json` would be. **`at` is aged by the
server's clock**, never the client's: the `Date` of a reply from wfsim.app (the
signal's own `Date` is not readable across origins in a browser), since a
machine's clock minutes off reads a fresh signal as stale. **Fallback**: unreachable,
`at` older than 90 s, or `mode: "ask"` — sync directly every 2 minutes with
jitter, the reply's `next_ms` holding. An idle client also syncs every
`report_ms` (0: never) with `want_ms: 0`, so the server knows it is on.

## Migrating from 7 to 8

| protocol 7 | protocol 8 |
| --- | --- |
| `/api/board/work` (a plain ask gives back everything; `ahead`) | `/api/board/sync` with `want_ms`; what is not named in `held` goes back |
| `/api/board/answers` every 30 s, `done` at the pack's end | `results` in the next sync, kept by the client until `taken`/`dropped` |
| `/api/board/renew` per row | `held` in each sync renews every named row |
| `/api/board/release`, `done: true` | a sync naming nothing; `keep` for a row to resume |
| `/api/board/points`, `points_since` | `since` → `points` |
| `/api/board/mine` | `states` |
| ask again 10 s after an empty answer | read the work signal; never sync before `next_ms` for work |
| `/api/board/order` `ok: false`: row left to the pack's end | report it in `refused` |
| `dials` of protocol 7 | `dials` of protocol 8 (`SYNC_CLIENT_DIALS`) |

Unchanged: the id, consent, the engine and its library, measuring a row, what a
correct answer is, the riven gain's own doors, independence, recheck and spot
checks. Protocol 7 stays served while anyone uses it.

## Changelog

- **2026-10-19 — macOS in the engine library.** `files` may name
  `aarch64-macos` and `x86_64-macos`, from the first engine shipped after this;
  a target is published only once it matched the wasm bit for bit.
- **2026-10-18 — the official data pack.** `/release.json` names `data` and
  `data_format`; the engine library is `abi: 2`, its signed body names `data`,
  and it gains `wfsim_install_data`, as the wasm module gains `install_data`.
  From 2026-10-25 01:00 UTC the engine carries no data and a host must install
  the pack (§"Migrating to the data pack"). A source note in a data file or a
  test no longer moves `engine`.
- **2026-10-14 — protocol 8.** `/api/board/sync` and the work signal; 7 and 6
  still served. `/release.json` and every hand-out reply carry `protocol` and
  `protocols_served`. Asking computers now count a client that holds a live
  lease, for every protocol. A row an engine refuses is reported and withdrawn
  once `refusals_to_withdraw` owners refused it.

## The cycle, protocol 7

`POST /api/board/work` (body ≤ 2048 characters)
`{ verifier, engine, protocol: 7, consent, lanes, pace_ms?, ahead? }`:
`lanes` 1–256 (else 1), the cores free now; `pace_ms` 100–`lease_ms`, the
smoothed wall time of the client's recent rows. Answers, all `200 ok: true`:

| answer | meaning |
| --- | --- |
| `pack: { lease, lease_ms, tasks: [{ question, identity, record, ruler, mode }] }` | board rows |
| `work: { kind: "riven_gain", lease, code, weapon, ruler, request, context }` | one search, alone |
| `work: null, pack: null, stale: true` | engine or protocol not served |
| `work: null, pack: null, banned: true, until` | in a cool-down |
| `work: null, pack: null` | nothing now: no work, fair share, no consent |

Every answer above also carries `dials: { lease_ms, pack_ms, pack_max,
pack_lease_ms, packs_held, renew_ms, row_ceiling_ms }`, the values in force
(§"Values").

Protocol 6 receives a pack of one as `work: { lease, identity, record, ruler,
mode }` and answers it with `POST /api/board/verify { lease, verifier, engine,
score, metric, work, compute_ms?, counts?, lanes? }`.

- **A PLAIN ASK GIVES BACK EVERYTHING THE ID HOLDS.** Ask without `ahead` only
  when holding nothing. Ask `ahead: true` as the current pack ends (the page:
  when the rows left would take under 15 s); it is served only while the id
  holds 1 pack and fewer than `PACKS_HELD`, and never gets a question someone
  waits on.
- **PACK SIZE** is `round(PACK_MS / pace_ms)` (`PACK_FIRST` without a pace),
  capped by `PACK_MAX` and by open rows over computers asking; leased for
  `PACK_LEASE_MS` whatever its size, every row kept by renewing it.
- **ANSWERS** go together, `POST /api/board/answers` (body ≤ 32 768 characters,
  ≤ `PACK_MAX` items) `{ lease, verifier, engine, done?, keep?, answers: [{ question,
  score, metric, work, compute_ms?, counts?, lanes? }] }`: `score` finite ≥ 0,
  `metric` `^[a-z_]{1,24}$`, `work` a safe integer ≥ 0, `counts` up to 32
  `^[a-z_]{1,32}$` keys of safe integers. The page sends every 30 s and as the
  pack ends with `done: true`, which gives back the rows not answered. An
  answer counts only for a row the lease still holds (`lease_until >= now`) or
  holds as the one before; the reply is `{ ok: true }` either way. Optional
  `points_since` (the `/api/board/points` `since`) adds `device_points`, that
  endpoint's body, to the reply — the page sends it with every send that
  carries results instead of asking `/api/board/points` after it.
- **A LEASE IS A HEARTBEAT.** While a row runs and its lease nears its end
  (the page: 2 min before), and before starting a row whose lease may have
  run out, `POST /api/board/renew { lease, verifier, question }` →
  `{ ok, held, renew_ms }` keeps that row `RENEW_MS` longer. `held: false`: the
  row was taken; stop it. A failed renewal is retried (the page: 30 s), the row
  computing on.
- **NOBODY IS CUT OFF.** Past `ROW_CEILING_MS` from its lease another client
  may take the row too; the holder computing longest stays as the one before,
  its renewals and its answer still counted, and the first answer settles it.
- **GIVE BACK, DON'T LAPSE.** A client that stops sends its pack's answers with
  `done: true`, or `POST /api/board/release { lease, verifier, keep? }`; a riven
  gain goes back with `POST /api/appraise/<code>/renew { lease, verifier,
  release: true }`.
- **A ROW CUT SHORT MAY RESUME.** `keep: question` on `done` or `/release`
  leaves that row under the lease while the rest go back. The client keeps its
  fold (`acc`, the runs folded) with the engine id, and before any plain ask —
  which gives back everything the id holds — renews the row and folds on from
  the next run in run order; `held: false` or another engine drops it.
- **A RIVEN GAIN** is renewed every 2 min with the same door without `release`
  (`held: false` stops it) and answered with `POST /api/appraise/<code>/result
  { build, lease, verifier, engine, score, work, compute_ms?, counts?, lanes? }`.
  Its answer is the page's quick search (`quickFleet`,
  web/src/static/app/08-checkpoint-api.js) to the bit: build, score and work.
  A riven gain released may be handed out again, to this client too.
- **STATUS**: `/api/board/mine { verifier, orders: [{ identity, ruler, mode }] }`
  (≤ 60) → `waiting | checking | confirmed | gone`; `/api/board/points`.
- **PACE**: ask 10 s after an empty answer, at once after work, as the page
  does.

## Measuring a row

The scorer's path, every step a wasm door (`webapi::board_rows`):

1. `/api/board/order { record, ruler, mode }` → `{ ok, request }`.
2. `/api/board/runs { request, from, count }` → `{ ok, shards }`, one shard per
   run. Any lanes may fight any ranges.
3. `/api/board/fold { request, acc, pieces }` → `{ ok, acc }`, `acc` `null` at
   first, pieces STRICTLY IN RUN ORDER, every run exactly once.
4. `/api/board/score { ruler, request, acc }` → `{ ok, score, metric, work,
   counts }`. Send `score`, `metric` and `work` as returned, and `counts`.

`ok: false` at any step: the row is not answered, and goes back with the pack.
JSON must round-trip without loss: a shard carries 64-bit RNG state as two
`u32` halves and floats that a lossy parser rounds (docs/WASM.md §"A
simulation runs on a worker fleet").

## A correct answer

- **EQUAL MEANS BIT-IDENTICAL CANON.** A board answer's canon is
  `canon({ score, metric, work })`, keys sorted, numbers as `JSON.stringify`
  writes them (`VERBS.simulate`, worker/factory.js); a riven gain's is
  `canon({ build, score, work })`. `CLIENTS_PER_FACT` equal answers from
  different owners on different networks make a fact.
- **A SCORE WITHIN `SCORE_NEAR` RELATIVE**, metric and work equal, is the runs
  folded in another grouping: never paid as agreeing, never refused.
- **WORK** is `Shard::work`: `floor(gas · GAS_PRICE_PER_MILLION / 10^6) + runs ·
  RUN_PRICE`, integer arithmetic, `gas` the meter's count over the row's runs
  (`counts.gas`), `runs` the request's. A client never computes it itself: it
  is what `/api/board/score` returns. An answer with the right score and other
  work agrees with no other client, so it waits for the official machines; they
  pay it the official work and refuse nobody for work alone. Until then it
  earns nothing, and it holds the row's other clients back too, so an engine
  that counts other work is a client to fix.
- **NATIVE OR WASM, THE SAME BITS.** The meter's weights are counted from the
  source, so a native build of the served commit counts what the wasm counts;
  `scripts/check_gas.mjs` holds native and wasm to the same gas, work and score
  however the runs are cut. The engine's transcendentals are the pure-Rust
  `libm` crate on every target (`rules::math`), never the platform's maths
  library, so a native build on any OS computes the wasm's last bit.

## Checks on a client

- **INDEPENDENCE.** No two answers of one owner (an account and all its ids, or
  an unclaimed id alone) or of one network (`netOf`: IPv4 whole, IPv6 by /64,
  salted hash) make a fact, and a row is not handed to an id whose owner or
  network already answered it.
- **RECHECK.** `RECHECK_SHARE` of agreements wait for one more independent
  client before anyone is credited; one who differs makes a dispute.
- **SPOT CHECK.** `SPOT_SHARE` of facts, credited at once, are fought again by
  the official machines on the site's wasm.
- **WHAT A WRONG ANSWER COSTS.** An answer a dispute or spot check disproves on
  the same engine is a refusal: a cool-down, every answer of that id voided,
  everything it was credited taken back, and every fact it witnessed that
  nothing official checked opened again. A disagreement with every other answer
  (`nondeterministic`) refuses nobody.
- **FAIR SHARE.** While open rows are fewer than the asking computers would
  take, an account over its target share of the last ten minutes' work gets
  nothing on an ask with probability `1 − target / received` (worker/fair.js).
  It looks like an empty answer; ask again later.

## A client must not

- run any engine but the served one, modified in anything that can move a bit,
  or send an engine id its code was not built from;
- send a `work` the engine did not return, or a `score` from a partial fold;
- ask from rotating addresses, proxies or several networks to appear as
  independent witnesses, or answer one row under several owners;
- compute without the yes, or hold leases it is not computing.

## Conformance

1. The id is 24 lowercase hex, made once, kept, never shared.
2. A statement is shown and a yes recorded before the first ask; `consent`
   travels with every ask; a new statement asks again.
3. `protocol` is in `PROTOCOLS_SERVED`; `engine` equals `/release.json`'s; on
   `stale` the client fetches the release again before asking. Every engine
   instance has the official data pack named beside its `engine` installed,
   verified by its SHA-256, before its first call.
4. Protocol 8: every sync names everything held; work is asked for only after
   `next_ms`; an idle client reads the work signal and syncs on `open`/`urgent`
   after its jitter; results are kept until `taken` or `dropped`; a row its
   engine refused is reported in `refused`. Protocol 7: a plain ask only when
   holding nothing; `ahead` only near a pack's end.
5. Rows are renewed while running (a sync's `held`, or protocol 7's `/renew`);
   `held: false` stops the row.
6. Every way out gives back what is held (a sync naming nothing, `done: true`,
   `/release`).
7. For each row of `tests/gas/rows.json`, the client's `{ score, metric, work,
   counts.gas }` equals the site's wasm's, and `work` equals
   `floor(counts.gas · 50000 / 10^6) + request.runs · 400000`.
8. The same row cut into one-run pieces, seven-run pieces and pieces asked last
   first gives the same bits, and so does the first row fought again after all
   the others in the same process.

**Reproducing the vectors.** The rows are inputs; their expected bits are
whatever the served engine gives, so they are produced, not stored:

```sh
git clone https://github.com/magenie33/wfsim && cd wfsim
mkdir -p /tmp/pkg && w=$(curl -s https://wfsim.app/release.json | node -pe 'JSON.parse(require("fs").readFileSync(0)).wasm')
curl -so /tmp/pkg/wfsim_wasm.js https://wfsim.app/pkg/wfsim_wasm.$w.js
curl -so /tmp/pkg/wfsim_wasm_bg.wasm https://wfsim.app/pkg/wfsim_wasm_bg.$w.wasm
d=$(curl -s https://wfsim.app/release.json | node -pe 'JSON.parse(require("fs").readFileSync(0)).data')
curl -so /tmp/pkg/pack.data.json https://wfsim.app/pkg/pack.data.$d.json
node --input-type=module -e '
import fs from "node:fs"; import { loadEngine, measureRow } from "./scripts/wasm_engine.mjs";
const e = loadEngine({ pkg: "/tmp/pkg" });
for (const row of JSON.parse(fs.readFileSync("tests/gas/rows.json", "utf8"))) {
  const s = await measureRow([e], row);
  console.log(JSON.stringify({ weapon: row.record.weapon, ruler: row.ruler, mode: row.mode,
    score: s.score, metric: s.metric, work: s.work, gas: s.counts && s.counts.gas, runs: s.runs }));
}'
```

`measureRow` takes `{ piece }` to cut the runs differently (item 8). WFSim's
own native side is held to the same pkg by `node scripts/check_gas.mjs
native=<wfsim-gas binary> pkg=/tmp/pkg`, over a tenth of each row's runs; a
client's native build is held to it by item 7.
