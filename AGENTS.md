# WFSim — agent guide

The ultimate Warframe **calculator**. Core goal: **match in-game
measurements**. Rust workspace + YAML game data + a dependency-free web UI,
deployed as WASM on Cloudflare (wfsim.app).

Three verbs — **build, simulate, solve**. Nothing else is a peer of them;
anything new either feeds one or reports from one (`docs/CORE.md`).

This file is what every session needs before it knows its task: how to start,
the commands, how a change ships, and the tripwires — rules whose breaking
fails **silently**. Everything else is routed to `docs/` (§"Read first").
§"Maintaining this file" says what may be added, and a check enforces it.

## Before you start

- Read `.handoff.md` and `private/agents/README.md`, again after a context
  reset. Then **claim your task** there: a block naming what you will touch,
  live servers included, kept current and deleted once live. Ask a block's
  session before touching what it names; one session on a live thing at a time
  (`private/agents/notes/sessions-claim-their-work.md`).
- **One worktree per task**, from `origin/main`: several sessions share the
  main checkout, so never build, stage or commit there (`docs/DEVELOPMENT.md`
  §"Sharing the checkout").
- **The machine is shared.** Stop your server with `scripts/stop_server.mjs
  <port>`, never by process name: that stops every session's server. A bare
  `python -` hangs as a REPL; write a file. Shell traps: `docs/DEVELOPMENT.md`.
- Once per clone, `git config core.hooksPath .githooks`: the hooks refuse a
  commit carrying `site/` and strip AI credit from a message.
- **Never link into a checkout.** Removing a worktree, or `rm -rf` on Windows,
  follows a junction and deletes its target. Copy; remove a worktree with
  `scripts/remove_worktree.mjs`.

## Commands

| to | run |
| --- | --- |
| serve the page | `cargo run -p wfsim-web` (`WFSIM_PORT` picks the port) |
| stop it | `node scripts/stop_server.mjs <port>` |
| match CI | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` |
| the cheap checks | `node scripts/check_page_bodies.mjs`, `check_comment_style.mjs` |
| a page check | `WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_<name>.mjs` |
| CI on a branch | `gh workflow run ci.yml --ref <branch>` |
| engine cost | `cargo run --release --bin one_fight` (`-- save` first) |
| optimizer accuracy | `cargo run --release --bin wfsim-truth -- pool=<ids>` |
| are all channels on one release | `python scripts/ship.py --verify` |
| read a build somebody posted | `node scripts/decode_share.mjs <link>` |

## Shipping

1. Commit on a branch: an all-lowercase subject, `area: what changed and why it
   is right`, no AI credit (`check_commit_trailers`).
2. Run CI on the branch. Green, it is ready for `main`: whoever ships
   (`private/agents`) rebases and pushes it, and the branch is deleted only
   after that push lands.
3. `ship.yml` builds `site/` and publishes it with the desktop channel, the MCP
   server and the bot as one. An engine change also builds the mac engine
   first. Wait in the background, and confirm the live `app.<hash>.js`.

- **Never commit `site/`.** A hand-committed `site/` is a release no channel
  follows. Never build it to test a change either: it takes minutes and fails
  in a worktree. Run the checks against the dev server; CI runs them all.
- **Never rescore the board locally.** `scores.yml` does it from the `scores`
  table; running `wfsim-board` here rewrites `data/board_state.yaml` and the
  published files.
- **Static files and `data/` are compiled in** (`include_str!`,
  `engine::data::files_under`). After editing either, stop the server (it holds
  the exe), `cargo build -p wfsim-web`, restart — and a test run reads the data
  of the previous build until a `.rs` in that crate is touched.
- **Images are same-origin.** `site/img/` holds every file `data/assets.yaml`
  names and the build fails on a missing one. Never hotlink a CDN: it is
  unreliable to blocked from mainland China.

## Read first

Read the row for what you are about to touch before the code, not after.

| you are touching | read |
| --- | --- |
| damage, crit, status, armour, geometry, punch through | `docs/MECHANICS.md` |
| a number that has to match the game | `docs/MEASUREMENTS.md` |
| the page — presets, customs, share links, the arena, /support | `docs/UI.md` |
| the combat record, damage pops, the ledger | `docs/RECORD.md` |
| the leaderboard, a ruler, a scenario's legality | `docs/BOARD.md` · clients `docs/CLIENTS.md` |
| the search | `docs/OPTIMIZER.md` |
| adding or fixing a weapon | `docs/WEAPON_INTAKE.md` |
| a per-weapon published table (Condition Overload, Primary Compression) | `docs/CATALOGS.md` |
| melee | `docs/MELEE.md` · kitguns `docs/KITGUNS.md` · incarnons `docs/INCARNON.md` |
| buffs, Warframe abilities, auras, archon shards | `docs/BUFFS.md` |
| a Warframe build — its mods, abilities, the Helminth | `docs/WARFRAMES.md` |
| a second damage instance beside a hit | `docs/EXTRA_HIT.md` |
| what the app does not model | `docs/UNMODELLED.md` |
| where a value came from, and how to source the next one | `docs/DATA_SOURCES.md` |
| a browser or node check | `docs/CHECKS.md` |
| the agent door, `window.wfsim`, the MCP server | `docs/AGENT.md` |
| naming a field, or a cloud resource | `docs/NAMING.md` · a term you do not know `docs/GLOSSARY.md` |
| what a form is CALLED on the page | `docs/FORM_NAMES.md` |
| the worker fleet, sharding, the wasm build | `docs/WASM.md` |
| capacity and Forma | `docs/INVESTMENT.md` |
| the update channel, mirrors, shells | `docs/DISTRIBUTION.md` |
| the desktop client | `docs/DESKTOP.md` |
| setup, profiling, sharing the checkout | `docs/DEVELOPMENT.md` |
| cross-tool tasks, handoffs, resource reservations | `docs/COORDINATION.md` |
| a server — what runs where, moving a service | `docs/SERVERS.md` |
| how usage is counted | `docs/ANALYTICS.md` |
| accounts, privacy, billing | `docs/ACCOUNTS.md` |

## Map

- `engine/` — all game mechanics, and it knows no weapon names: weapons, mods
  and the rest load from `data/`. Layered `rules/ model/ data/ build/` fight
  `board/`, each naming only those below (`docs/CORE.md` §4).
  `engine::arena::Arena` is both actors of a fight, built by the web api and
  the optimizer through one constructor. Every formula cites its source.
- `optimizer/` — build search. It only ever calls the engine — never add a
  simplified damage formula here.
- `webapi/` — endpoint logic shared by `web/` (native) and `wasm/`.
- `web/src/static/` — the UI: vanilla JS/CSS, no framework, no deps.
- `wasm/` + `site/` — the static deployment; `site/` is generated
  (`scripts/build_site_app.py`).
- `desktop/` — the desktop client, an independent cargo workspace depending on
  nothing in `engine/`. The updater lives in `app.js`, not in the shell.
- `data/` — versioned game data, each thing defined once and referenced by its
  `id`; `data/README.md` is the reference graph.
- `tests/golden/` — golden tests calibrated against in-game measurements.
- `private/` — its own repo (`wfsim-private`); nothing this repo needs may live
  only there.
- **Anything about money lives in the private worker** (`../wfsim-cloud`):
  here is the calculator, reaching it via `worker/cloud.js` alone.

## Tripwires

### Correctness

- **The simulator is the truth; the optimizer obeys it.** A search's winner is
  replayed under the simulator's fight, so the optimizer calls the simulator's
  code and adds only its own scope and budget. `parse_fight` is the shared
  parse; anything that is a property of the fight goes in it. A shared helper
  is not enough — the decisions around it have to be shared too.
- **Every fact has one authoritative source.** A second copy — a cache, a prior
  output read back as input, an artifact carrying data — needs a rule for which
  wins, and that rule is where this repo's defects have lived (`docs/BOARD.md`).
- **Golden values change only with an in-game measurement.** New mechanics need
  golden tests; a faithful-looking implementation without one is not correct.
- **Measurement > the game's package > wiki > DE's export**, on any conflict.
  The export (`scripts/de_export.py`) fills what the wiki does not state; its
  `levelStats` is rounded display, never a number's source. Join by
  `internal_name` == `uniqueName`, **never by name**. WFCD is not a source.
- **A data file states only rules the engine applies.** One it does not apply
  reads as applied to anyone auditing it; `docs/UNMODELLED.md` is the register.
- **A build's axes are declared once**, in `engine::board::builds::BUILD_AXES`
  (`/api/meta.build_axes`). Every surface declares which axis its fields cover;
  `buildState()` requires a value for every state key.

### The page

- **No native dialogs** — `prompt`/`alert`/`confirm` are blocked in the owner's
  browser. Use inline inputs and inline feedback.
- **Absolute asset paths** (`/img/…`, `/logo.svg`): the SPA also loads at
  `/weapons/<Wiki_Name>`, where a relative path resolves into the fallback HTML.
- **`api()` transport**: `/api/meta` and `/api/i18n` are GET, everything else is
  POST — the native server matches on exact (method, path).
- **A check's page-side body is a template literal.** A backtick in it, even in
  a comment, ends it early, and `\b` in it is a backspace.

### Language and prose

- **A string is transcribed, never translated.** English is the source
  everywhere and ids are never translated; i18n is an overlay. DE's Chinese is
  routinely non-literal, so a name derived from the English is wrong more often
  than not: if a source cannot be reached, leave it empty and say so. Wiki URLs
  are always built from the English name.
- **What is frozen is the wire.** A field inside a saved preset, a share link
  or a board record keeps its name (`naming::FROZEN`, which may only shrink).
  New names: `docs/NAMING.md`.
- The product name is **WFSim**, wordmark never translated; in Chinese prose
  **WF模拟**. 沃肥模拟 is the community nickname and stays out of the product.
- **Prose states the current rule and nothing else** — no history, no
  attributions, no dated decisions (`docs/MEASUREMENTS.md` excepted, where the
  provenance is the data). A comment is a constraint, evidence or a design
  rule, twelve lines at most; a subject goes to `docs/`. A sourcing note that
  repeats is an id in `data/notes.yaml`. `check_comment_style` enforces it.
- No marketing copy; the home hero alone may make a checkable bold claim.
  Match the surrounding code's comment density and idiom.

### Checks

- **A ratchet that cannot fail is not a ratchet.** A new check is trusted only
  once it has failed on the broken input — on data that can make it fail; for a
  data-driven one, after forcing the rebuild above.
- Run the checks a change touches; `docs/CHECKS.md` lists what each asserts.
  Run `wfsim-truth` after any change to the search's enumeration, scheduling
  or scoring: a search cannot vouch for itself.

## Maintaining this file

**The test for a sentence is not "is it true".** It is:

> **Would an agent that never read this sentence do the wrong thing, on a task
> where it never opens the relevant file?**

- **Yes** → a tripwire: an imperative plus the consequence, three lines or so.
- **No — it would open that file anyway** → it belongs in that document, and
  here only as a row in §"Read first".
- **It explains one constant** → a comment beside that constant.
- **A check already fails on it** → one line naming the check, at most.
- **It can be enforced by a check** → write the check instead.

**Emphasis is a budget too.** Bold or capitals mark the one phrase a reader must
not miss; on every line they mark nothing.

**A number, an identifier or a measurement is a symptom**: state that goes
stale. Name the document that owns it. **If a normal feature commit would need
to edit this file, the content is in the wrong place.**

The size is a ratchet in `scripts/check_comment_style.mjs`, set close to the
file on purpose: adding a rule costs removing one. Lower it when a pass removes
some; never raise it to make a red run green.
