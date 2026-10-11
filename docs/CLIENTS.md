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
protocol: 7
protocols_served: 7, 6
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
clients_per_fact: 2
facts_per_refusal_forgiven: 1000
```

Each key is its constant in snake case, read by `scripts/check_clients_doc.mjs`
from worker/verify.js, worker/tasks.js, worker/appraise.js,
worker/contribution.js, worker/factory.js and engine/src/fight/gas.rs; a point
is `work_per_point` work.

## Terms the server holds a client to

- **The server does not tell a compliant client from the page.** Any id of the
  recognised shape is served by the same code, and its answers judged the same.
- **A penalty is a cool-down and credit taken back, never a permanent ban.**
  `coolDownMs(k)` is `86_400_000 · 3^k` for a client with `k` refusals before
  this one, with no ceiling. When it ends, the next ask lifts it
  (`banned = 0` where `refused_until <= now`) and the client earns again
  without asking anyone. Each `FACTS_PER_REFUSAL_FORGIVEN · k` facts it is part
  of afterwards take one refusal off its record.
- **The previous protocol is served beside the current one** (`PROTOCOLS_SERVED`)
  and is dropped only once nobody has asked with it for a week; a change to
  what a client must do changes this page in the same commit.
- **What each answer means** is in §"The cycle": `unrecognised_client`
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
  `/work`, `/answers`, `/verify`, `/renew` and riven answers; what it earned
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
- Every `/work` ask carries `consent: { v, at }`: `v` an integer ≥ 1, `at` an ISO
  instant matching `^\d{4}-\d\d-\d\dT[\d:.]+Z$`. Without it the answer is
  `{ ok: true, work: null, pack: null }` and nothing is handed out. The server
  keeps both on the client's row whenever they change.
- A statement that asks for more — more of the device, a new kind of task, more
  sent, another use — is a new version, asked again; a rewording keeps the yes.
  The server keeps `v` as the client states it.

## The engine

- `GET /release.json` → `{ release, commit, wasm, engine }`. `engine` is the id a
  client sends; `wasm` names the module: `/pkg/wfsim_wasm.<wasm>.js` and
  `/pkg/wfsim_wasm_bg.<wasm>.wasm`, `<wasm>` the first 12 hex of the module's
  SHA-256. The module exports `api(path, body)`, JSON text in and out.
- `engine` is `engine_id()` in scripts/build_site_app.py: SHA-256 over
  `engine/src`, `webapi/src`, `wasm/src`, `tools/gas-macro`, `data` (less
  `ENGINE_SOURCES_NOT`) and `Cargo.lock`, CRLF normalised, first 12 hex. A
  native build is of the commit `/release.json` names (`commit`), unmodified,
  built without `WFSIM_GAS_OFF` (which turns the meter off), and its engine id
  equals the served one.
- Only the served engine works: a `/work` ask naming another gets `stale: true`
  and nothing; `/answers` naming another releases the lease and drops the
  answers. Every `/work` answer carries `release`; the page reloads into a newer
  one when idle.

## The cycle

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
  holds as the one before; the reply is `{ ok: true }` either way.
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
  is what `/api/board/score` returns, and an answer whose work differs from the
  served engine's is a wrong answer.
- **NATIVE OR WASM, THE SAME BITS.** The meter's weights are counted from the
  source, so a native build of the served commit counts what the wasm counts;
  `scripts/check_gas.mjs` holds native and wasm to the same gas, work and score
  however the runs are cut.

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
   `stale` the client fetches the release again before asking.
4. A plain ask only when holding nothing; `ahead` only near a pack's end.
5. Rows are renewed while running; `held: false` stops the row.
6. Every way out gives back what is held (`done: true`, `/release`).
7. For each row of `tests/gas/rows.json`, the client's `{ score, metric, work,
   counts.gas }` equals the site's wasm's, and `work` equals
   `floor(counts.gas · 50000 / 10^6) + request.runs · 400000`.
8. The same row cut into one-run pieces, seven-run pieces and pieces asked last
   first gives the same bits.

**Reproducing the vectors.** The rows are inputs; their expected bits are
whatever the served engine gives, so they are produced, not stored:

```sh
git clone https://github.com/magenie33/wfsim && cd wfsim
mkdir -p /tmp/pkg && w=$(curl -s https://wfsim.app/release.json | node -pe 'JSON.parse(require("fs").readFileSync(0)).wasm')
curl -so /tmp/pkg/wfsim_wasm.js https://wfsim.app/pkg/wfsim_wasm.$w.js
curl -so /tmp/pkg/wfsim_wasm_bg.wasm https://wfsim.app/pkg/wfsim_wasm_bg.$w.wasm
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
