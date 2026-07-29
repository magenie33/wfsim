<div align="center">

<img src="web/src/static/logo.svg" width="110" alt="WFSim" />

# Contributing to WFSim

[**wfsim.app**](https://wfsim.app) ·
QQ group [995078378](https://qm.qq.com/q/uiXrMSTs8S) ·
[README](README.md)

</div>

Thanks for your interest! WFSim's core promise is "matches in-game
measurements", which shapes what contributions look like — **you don't need
to write Rust to make the most valuable contributions here.**

## Where contributions land

### 1. In-game measurements (most valuable, no code)

The engine is only as trustworthy as its measurement baseline. A good
measurement report contains: weapon + exact mod config, target (unit, level,
Simulacrum settings), and the observed numbers (per-hit damage, crit tiers,
proc ticks). Protocol and existing baselines:
[`docs/MEASUREMENTS.md`](docs/MEASUREMENTS.md). Open an issue with your
numbers — discrepancies against the sim are *especially* welcome.

### 2. Game data (`data/`, YAML — no code)

Weapons, mods, arcanes, enemies, evolutions. Data is normalized — anything
reusable is defined once and referenced by `id` (stable English slugs, never
translated). Read [`data/README.md`](data/README.md) for the reference graph
and [`docs/DATA_SOURCES.md`](docs/DATA_SOURCES.md) for sourcing rules: values
come from the wiki's structured Lua modules, and each entry cites its source
module URL.

### 3. Translations (`data/i18n/`, YAML — no code, lowest barrier)

Edit `data/i18n/zh.yaml` (or add a new locale file): fill `id → name` lines
per table (weapons/enemies/mods/arcanes/evolutions/damage_types). Ten lines
is a fine PR — anything untranslated just keeps showing English. Names
follow the official CN client (cross-check
https://warframe.huijiwiki.com/wiki/Project:中英名称对照). CI validates every
key against the real ids, so a typo cannot break anything. UI strings
(`ui:`) and effect-line phrases (`effect_phrases:`) live in the same file —
translations never touch code.

### 4. Engine mechanics (`engine/`, Rust)

Every formula must carry a comment pointing at its source (wiki page /
datamining / measurement). New mechanics need golden tests calibrated against
in-game measurements — an implementation without a measurement to compare
against doesn't count as correct, no matter how faithful it looks. The
mechanics catalog is documented in [`docs/MECHANICS.md`](docs/MECHANICS.md).

### 5. Optimizer / web UI (`optimizer/`, `web/`)

The optimizer only ever calls the engine — never add a simplified damage
formula to it. The web UI's static files are `include_str!`'d into the
binary: rebuild `wfsim-web` after any JS/CSS/HTML change.

## Ground rules

- **Before a PR:** `cargo test --workspace` green and
  `cargo clippy --workspace --all-targets -- -D warnings` clean (CI enforces
  both). Toolchain is pinned via `mise install`.
- **Engine changes need tests.** Golden tests for new mechanics; existing
  golden values may only change with a measurement justifying it.
- **English everywhere** (code, comments, docs, data). i18n happens later as
  overlay files — ids and source strings stay English.
- Small, focused PRs over big ones. Open an issue first for anything
  design-shaped.
- Working with an AI coding agent? The condensed repo rulebook lives in
  [`AGENTS.md`](AGENTS.md) — point your agent at it.
