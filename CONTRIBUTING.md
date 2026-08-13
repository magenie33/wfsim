<div align="center">

<img src="web/src/static/logo.svg" width="110" alt="WFSim" />

# Contributing to WFSim

[**wfsim.app**](https://wfsim.app) ·
QQ group [995078378](https://qm.qq.com/q/uiXrMSTs8S) ·
[README](README.md)

</div>

Thanks for your interest! WFSim's core promise is "matches in-game
measurements", which shapes what contributions look like — **you don't need to
write Rust to make the most valuable contributions here.**

## Read this first: the repo moves fast

One maintainer, on the order of 30 commits a day. That single fact should shape
how you contribute:

- **A branch's base rots in days, not months.** A large PR opened against last
  week's `main` will not merge cleanly. That is not the contributor's fault and
  it is not a judgement on the work — it is arithmetic.
- **Some files are rewritten constantly** — `web/src/static/app.js`,
  `webapi/src/lib.rs`, `engine/src/loadout.rs`. A PR touching them competes with
  the maintainer's own edits every day it stays open.
- **So: open an issue before writing anything over ~300 lines.** Say what you
  want to build and where you think it plugs in; the maintainer confirms the
  interface and the landing spot; then it goes in as small pieces. This is not
  bureaucracy, it is the only shape that survives the pace.

Everything in §1–§3 below is immune to all of this, which is part of why it is
ranked first: a measurement, a data fix or a translation does not have a base
to rot.

## Where contributions land

### 1. In-game measurements (most valuable, no code)

The engine is only as trustworthy as its measurement baseline, and one person
cannot run every test in game. A good measurement report contains: weapon +
exact mod config, target (unit, level, Simulacrum settings), and the observed
numbers (per-hit damage, crit tiers, proc ticks). Protocol and existing
baselines: [`docs/MEASUREMENTS.md`](docs/MEASUREMENTS.md). Open an issue with
your numbers — **discrepancies against the sim are especially welcome.**

An accepted measurement becomes a numbered `M<n>` entry in that document,
credited to whoever ran it. The number is permanent: the engine can be
rewritten around it and the entry still says who established the fact.

### 2. Game data (`data/`, YAML — no code)

Weapons, mods, arcanes, enemies, evolutions, rivens. Data is normalized —
anything reusable is defined once and referenced by `id` (stable English slugs,
never translated). Read [`data/README.md`](data/README.md) for the reference
graph and [`docs/DATA_SOURCES.md`](docs/DATA_SOURCES.md) for sourcing rules.
Two sources, always cross-checked: the wiki's structured Lua modules and WFCD's
export. Each entry cites its source.

### 3. Translations (`data/i18n/<locale>/`, YAML — no code, lowest barrier)

A locale is a directory: hand-written `names.yaml` and `ui.yaml`, generated
`descriptions.yaml`. Fill `id → name` lines per table; ten lines is a fine PR,
and anything untranslated just keeps showing English. CI validates every key
against the real ids, so a typo cannot break anything.

**A string is transcribed, never translated.** Names come from the official CN
client — DE's own Chinese is routinely non-literal, so a name derived from the
English is wrong more often than not. If you cannot reach a source, leave it
empty and say so; an empty line is honest, a guessed one is a bug that reads as
a feature.

### 4. Engine mechanics (`engine/`, Rust)

Every formula carries a comment pointing at its source (wiki page / datamine /
measurement). New mechanics need golden tests calibrated against in-game
measurements — an implementation without a measurement to compare against does
not count as correct, no matter how faithful it looks. Catalog:
[`docs/MECHANICS.md`](docs/MECHANICS.md).

### 5. Optimizer / web UI (`optimizer/`, `web/`)

The optimizer only ever calls the engine — never add a simplified damage
formula to it. The web UI's static files are `include_str!`'d into the binary,
so rebuild `wfsim-web` after any JS/CSS/HTML change.

This is the area where the pace warning above bites hardest. Issue first.

## What gets acted on

- **A reproduction** — a share link plus the number you got in game. This is
  the fastest path from "that looks wrong" to a fix, by a wide margin.
- **A measurement**, in the protocol above.
- **A data correction with its source.**
- "This number looks wrong", with nothing attached, is a starting point rather
  than a report. Expect to be asked for one of the three above; that is not a
  brush-off, it is the only thing that can be acted on.

## Roadmap

The maintainer sets the order of work. **Agreeing that something is worth doing
is not the same as agreeing it is worth doing next** — a good suggestion can sit
in the backlog for a long time, and that is not a rejection.

Conversely, what the project already admits it does not model is not a bug
report: [`docs/UNMODELLED.md`](docs/UNMODELLED.md) lists the edges by reason,
and the app states them on the page.

## Ground rules

- **Before a PR:** `cargo test --workspace` green and
  `cargo clippy --workspace --all-targets -- -D warnings` clean (CI enforces
  both). Toolchain is pinned via `mise install`.
- **Engine changes need tests.** Golden tests for new mechanics; existing golden
  values may only change with a measurement justifying it — including the
  maintainer's.
- **English everywhere** (code, comments, docs, data). i18n is an overlay; ids
  and source strings stay English. Issues and PR discussion can be in Chinese.
- **Small, focused PRs.** See the pace section — this one is load-bearing here,
  not boilerplate.
- Working with an AI coding agent? The condensed repo rulebook lives in
  [`AGENTS.md`](AGENTS.md) — point your agent at it.
