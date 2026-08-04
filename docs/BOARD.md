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
| consent + submit | `web/src/static/app.js` (`offerBoardSubmit`) | the player's browser |
| the submissions | a Cloudflare KV namespace (binding `SUBMISSIONS`) | written by the endpoint |
| the endpoint | `functions/api/board/submit.js` | Cloudflare Pages, same origin |
| the scorer | `cli/src/bin/wfsim-board.rs` | the scheduled job |
| the automation | `.github/workflows/board.yml` | GitHub Actions |

**The board is in the repo, not in a database.** That is what turns
"reproducible" from a claim into a property, and it means the LOCAL build has
the board with no network at all — `data/` is embedded at compile time. Only
submitting needs a network.

**The endpoint is on the site's own origin** (`wfsim.app/api/board/submit`). A
separate api domain would be a second DNS name and a second thing that can be
blocked, which is the failure the same-origin art rule was written about.

## When it updates

| trigger | scope | cost |
| --- | --- | --- |
| hourly | score what is new | sub-second per build; no commit when nothing changed |
| a push touching `engine/` or `data/` | **everything** | ~570 ms per build, minutes for a few thousand |
| a new benchmark version | everything, under the new ruler | the old board is void by id |

The second row is the point: the maintainer's ordinary work — fixing a bug,
correcting a number, changing the benchmark to 480 s — IS the trigger. There is
no dependency analysis deciding which rows were affected, because that question
can be answered wrong silently and re-scoring everything is cheap.

## Consent

Asked ONCE, inline, the first time a run finishes under the official scenario —
never on load, never as a native dialog (they are blocked in this project), and
never blocking the result. Running your own scenario neither asks nor sends.

What travels: the weapon, its mods, evolutions and arcanes, and which
benchmark. No account, no identifier, no riven, none of the names you chose,
and no score. `scripts/check_official.mjs` asserts on the WIRE that nothing
leaves before consent and nothing leaves after declining.

The endpoint stores no IP, no token and no timestamp finer than the day.

## What is not on the board

- **Rivens** (user, 2026-08-04). They are personal random items, so a board
  that counted them would rank luck. It also removes the only free-text field a
  player authors from anything uploaded.
- **The exilus slot** (user, 2026-08-04). Exilus mods are handling and
  mobility with no single-target damage model — the optimizer already excludes
  them — and the slot costs a separate adapter, so counting it would price a
  build against a resource the ranking cannot value. A benchmark build is 8
  slots.

## Setup, once (repo owner)

1. **KV namespace** — create one, then bind it to the Pages project as
   `SUBMISSIONS` (Settings → Bindings → KV namespace).

   Named for what it HOLDS, which is not the board: the board is the generated
   YAML in `data/benchmarks/boards/`, and this namespace holds the builds people
   sent, waiting to be scored. The binding was briefly called `BOARD`, which is
   a debugging trap — "the board is empty but the BOARD binding looks fine" is a
   sentence that sends you looking in the wrong place.
2. **Repo secrets** — `CF_ACCOUNT_ID`, `CF_BOARD_NAMESPACE_ID`, `CF_API_TOKEN`
   (a token with *Workers KV Storage: Read* on that namespace only).

The token only ever READS. What the board says is computed in the repo from
data in the repo; nothing secret decides a rank.

Until step 1 is done the endpoint answers 503 and the page says "could not
reach the board — nothing was sent", which is the honest state rather than a
silent failure.
