# wfsim

[![CI](https://github.com/magenie33/wfsim/actions/workflows/ci.yml/badge.svg)](https://github.com/magenie33/wfsim/actions/workflows/ci.yml)

A hardcore Warframe damage simulator. The goal is not "compute a DPS number"
but to **physically reproduce a real fight** — fire cadence, magazine and
reload, combo, status stacking and decay, buff uptime — so that the damage
this tool computes matches in-game (Simulacrum) measurements item by item,
and so the optimal mod build can be solved on top of that.

<!-- TODO: hero screenshot of the web UI here -->

## Why another calculator?

Steady-state DPS calculators answer "what does the spreadsheet say". wfsim
answers "what actually happens when you pull the trigger":

- **Simulation, not formulas.** The fight advances through time; DPS is a
  derived statistic, never the core of the computation.
- **Conditional buffs are simulated, not assumed.** On-kill / on-headshot /
  on-reload buffs have real stacks, durations, and uptime — or lock any of
  them at N stacks if you want the community's "assume max" convention.
- **Calibrated against the game.** Every mechanic ships with golden tests
  matched to in-game Simulacrum measurements. If it doesn't match, it isn't
  done. See [`docs/MEASUREMENTS.md`](docs/MEASUREMENTS.md).
- **A real optimizer on a real engine.** The build search only ever calls the
  simulation engine — it has no simplified side-formula, so its "best build"
  means best in the simulated fight, not best on paper.

Full design: [`docs/CORE.md`](docs/CORE.md).

## What works today

- The full damage pipeline: mod resolution (correct bucket math), elemental
  combination order, tiered crits, status procs and DoTs, shields vs health
  (incl. Toxin bypass), armor mitigation.
- Condition Overload-family mechanics modeled per weapon (independent
  multiplier vs base-damage fold vs no benefit).
- Incarnon weapons: charge-backed alternate form (charges are not a
  magazine), transmute timings, and per-tier **evolution search** — which
  already exposes evolutions that are mathematically dead.
- A Monte-Carlo build **optimizer**: mod pool + constraints in, successive-
  elimination racing across all cores, ranked top-10 out — each result
  loadable back into the simulator.
- A local **web UI** (`cargo run -p wfsim-web`, then open
  `http://localhost:8787`): build configurator, per-buff stack/lock controls,
  sim panel, optimizer with savable scope presets.

## Status: early, honest edition

- **One weapon is fully modeled and calibrated: Dual Toxocyst (incl. its
  Incarnon form)**, plus Verglas Prime as a second test subject.
- The sim currently runs against a single enemy; multiple enemies in a 2D
  arena (so range/AoE/punch-through payoffs are modeled properly) is the
  headline item on the roadmap.
- No Riven support yet. No public binary release yet.

## Getting started

```sh
mise install                # installs the pinned Rust toolchain
cargo test --workspace      # golden tests vs in-game measurements
cargo run -p wfsim-web      # web UI on http://localhost:8787
cargo run -p wfsim-cli      # CLI entry point
```

Toolchains are pinned via [mise](https://mise.jdx.dev/); see
[`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for platform setup.

## Layout

- `engine/` — the damage pipeline; pure functions, one testable layer each.
- `optimizer/` — best-build search; only ever calls the engine.
- `web/` — the local web UI. `cli/` — the `wfsim` command line.
- `data/` — versioned weapon / mod / arcane / enemy data (normalized YAML,
  see [`data/README.md`](data/README.md)).
- `docs/` — design docs: [mechanics](docs/MECHANICS.md),
  [measurements](docs/MEASUREMENTS.md), [optimizer](docs/OPTIMIZER.md),
  [buffs](docs/BUFFS.md), [glossary](docs/GLOSSARY.md).

## Contributing

The most valuable contributions are **in-game measurements and game data**,
not just code — see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Data sources & attribution

Game data is derived from the community-maintained
[Warframe Wiki](https://wiki.warframe.com/) (CC BY-SA); sourcing rules are in
[`docs/DATA_SOURCES.md`](docs/DATA_SOURCES.md). wfsim is an unofficial fan
project, not affiliated with or endorsed by Digital Extremes. Warframe and
all related properties are trademarks of Digital Extremes Ltd.

## License

MIT
