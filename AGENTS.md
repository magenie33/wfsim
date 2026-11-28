# WFSim — agent guide

Warframe build calculator (**build, simulate, solve**; `docs/CORE.md`) aiming
to match in-game measurements. Rust workspace + normalized YAML data + vanilla
JS/CSS UI, deployed as WASM on Cloudflare (`wfsim.app`).

## Before you start

- **Shared checkout (when present):** if `.handoff.md` and
  `private/agents/README.md` exist, read both at start and after resets; claim
  your task (scope, worktree, live ports) in `.handoff.md`, ask before touching
  another claim, and keep one session per live resource.
- **Worktrees & links:** use one worktree per task from `origin/main` (never
  build, stage or commit in main; remove via `node scripts/remove_worktree.mjs`).
  Never symlink or junction into a checkout — copy instead.
- **Shared machine & setup:** stop servers by port
  (`node scripts/stop_server.mjs <port>`), never by process name. Never pipe
  into bare `python -` (`docs/DEVELOPMENT.md`). Run `mise install` and
  `git config core.hooksPath .githooks` once per clone.

## Commands

| task | command |
| --- | --- |
| dev server / stop | `cargo run -p wfsim-web` (`WFSIM_PORT`) · `node scripts/stop_server.mjs <port>` |
| Rust CI | `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |
| fast checks | `node scripts/check_page_bodies.mjs`, `check_comment_style.mjs`, `check_commit_trailers.mjs` |
| page check | `WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_<name>.mjs` |
| engine perf / truth | `cargo run --release --bin one_fight` (`-- save` first) · `cargo run --release --bin wfsim-truth -- pool=<ids>` |
| decode build link | `node scripts/decode_share.mjs <link>` |
| CI / live verify | `gh workflow run ci.yml --ref <branch>` · `python scripts/ship.py --verify` |

## Shipping & handoff

- **Branch, commit & handoff:** commit on a task branch with a lowercase
  subject (`area: what changed and why`) and no AI trailers
  (`check_commit_trailers.mjs`; outside contributors follow `CONTRIBUTING.md`
  and `CLA.md`). Run fast and affected checks, push, and hand off as **ready
  for integration** — the merger runs combined CI before `main` (`docs/CI.md`).
  Keep branches until `main` lands.
- **Never commit or test-build `site/`:** CI builds and publishes it with the
  other release channels (`docs/DISTRIBUTION.md`); test against `wfsim-web`.
- **Never rescore the board locally:** board scoring runs through the published
  pipeline (`docs/BOARD.md`), not local `wfsim-board` runs.

## Map

- `engine/` — mechanics (no weapon names; loads from `data/`; layered
  `rules/ model/ data/ build/` → `fight` → `board/`, `Arena` shared by web API
  and optimizer; every formula cites its source).
- `optimizer/` — build search calling `engine/` only (no simplified formulas).
- `webapi/`, `web/src/static/`, `wasm/` — shared endpoints, dependency-free
  JS/CSS UI, and WASM build (`site/` is CI-generated via
  `scripts/build_site_app.py`).
- `desktop/`, `client/` — independent Cargo workspaces (desktop shell and
  official compute CLI); neither depends on `engine/`.
- `worker/`, `mcp/`, `bot/` — Cloudflare site worker, MCP/A2A worker, and
  QQ/Discord bots.
- `data/`, `tests/golden/`, `scripts/`, `docs/` — normalized YAML
  (`data/README.md`), calibrated golden tests, checks/tooling, subsystem docs.
- `private/` (gitignored, optional) — maintainer notes; public code never
  depends on it. Billing lives outside this repo behind `worker/cloud.js`.

## Read first

Read the owning doc before editing its subsystem.

| area | docs |
| --- | --- |
| mechanics, measurements, extra hits, unmodelled | `docs/MECHANICS.md` · `docs/MEASUREMENTS.md` · `docs/EXTRA_HIT.md` · `docs/UNMODELLED.md` |
| weapon intake, catalogs, data sources | `docs/WEAPON_INTAKE.md` · `docs/CATALOGS.md` · `docs/DATA_SOURCES.md` |
| melee, kitguns, zaws, incarnons | `docs/MELEE.md` · `docs/KITGUNS.md` · `docs/ZAW.md` · `docs/INCARNON.md` |
| buffs, abilities, Warframe builds | `docs/BUFFS.md` · `docs/WARFRAMES.md` |
| page UI, combat record, form names, Forma | `docs/UI.md` · `docs/RECORD.md` · `docs/FORM_NAMES.md` · `docs/INVESTMENT.md` |
| optimizer, worker fleet, WASM | `docs/OPTIMIZER.md` · `docs/WASM.md` |
| leaderboard, compute clients, tasks & queue | `docs/BOARD.md` · `docs/CLIENTS.md` · `docs/TASKS.md` · `docs/COMPUTE_TASKS.md` |
| agent door, `window.wfsim`, MCP/A2A, bots | `docs/AGENT.md` |
| checks, CI, dev setup, coordination | `docs/CHECKS.md` · `docs/CI.md` · `docs/DEVELOPMENT.md` · `docs/COORDINATION.md` |
| releases, desktop shell, servers | `docs/DISTRIBUTION.md` · `docs/DESKTOP.md` · `docs/SERVERS.md` |
| naming & glossary | `docs/NAMING.md` · `docs/GLOSSARY.md` |
| analytics, accounts & privacy | `docs/ANALYTICS.md` · `docs/ACCOUNTS.md` |

## Tripwires

- **Simulator is truth; optimizer obeys it:** both share `parse_fight` and its
  decisions; optimizer adds only scope and budget.
- **One authoritative source per fact:** any cache or projection needs an
  explicit precedence rule (`docs/BOARD.md`).
- **Golden values move only with an in-game measurement.** New mechanics need
  golden tests.
- **Source precedence:** measurement > game package > wiki > DE Public Export
  (ignore rounded `levelStats`; join by `internal_name == uniqueName`, never by
  name; never use WFCD).
- **Data declares only applied rules:** unimplemented mechanics belong in
  `docs/UNMODELLED.md`, not active data fields.
- **Build axes declared once** in `engine::board::builds::BUILD_AXES`
  (`/api/meta.build_axes`); `buildState()` requires every state key.
- **Compiled-in static/data:** stop, rebuild and restart `wfsim-web` after
  editing `web/src/static/` or `data/`; Cargo tests can reuse stale embedded
  data until a `.rs` in that crate changes.
- **Page & asset rules:** no `prompt`/`alert`/`confirm` (inline UI only); use
  absolute paths (`/img/…`, `/logo.svg`); `/api/meta` and `/api/i18n` are
  `GET`, all other `/api/*` are `POST`; `data/assets.yaml` images live
  same-origin in `site/img/` (no CDN hotlinks).
- **Checks & ratchets:** page-side check bodies are template literals (no
  backticks, even in comments; `\b` is backspace). Verify new ratchets fail on
  broken input first (`docs/CHECKS.md`). Run `wfsim-truth` after optimizer
  enumeration, scheduling or scoring changes.
- **Language, wire & brand:** English is the source; transcribe official client
  strings for i18n (leave missing ones empty, never guess); wiki URLs use
  English names. Wire names in presets, share links and board records stay
  frozen (`naming::FROZEN`). Brand is **WFSim** (Chinese prose: **WF模拟**; no
  community nicknames).
- **Prose & comments:** state the current rule only — no history, attributions,
  dates (`docs/MEASUREMENTS.md` and `docs/measurements/` excepted) or marketing
  copy outside the home hero (`check_comment_style.mjs`). Shared data notes go
  in `data/notes.yaml`.

## Maintaining this file

Keep a rule here only if an agent that never opens the relevant file would
otherwise do the wrong thing. Put details in `docs/` (with a row in §"Read
first"), constants beside their code, and enforceable rules in checks. No
volatile counts, identifiers, or release quirks. Size is ratcheted in
`scripts/check_comment_style.mjs` — lower the ceiling when shrinking; never
raise it.
