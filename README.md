# wfsim

A hardcore Warframe damage simulator. The goal is not "compute a DPS number" but
to **physically reproduce a real fight** — so that the damage this tool computes
matches in-game (Simulacrum) measurements item by item, and so we can solve for
the optimal mod build on top of that.

See [`docs/CORE.md`](docs/CORE.md) for the full design (vision, damage pipeline,
architecture, roadmap).

## Why another calculator?

Unlike steady-state DPS calculators, wfsim simulates the fight over time
(fire cadence, combo, status stacking, magazine/reload, target movement) and
treats range, ballistics, hit resolution, and AoE as first-class. Its north-star
metric: match real Simulacrum measurements within rounding error.

## Stack

- **Language:** Rust (2021 edition), pinned via [mise](https://mise.jdx.dev/).
- **Layout:** a Cargo workspace with three crates:
  - `engine/` — the damage pipeline, pure functions, one testable layer each.
  - `optimizer/` — best-build search; only ever calls the engine.
  - `cli/` — the `wfsim` command-line entry point.
- `data/` — versioned weapon / mod / enemy / faction data.
- `tests/golden/` — golden tests calibrated against in-game measurements.

## Getting started

```sh
mise install      # installs the pinned Rust toolchain
cargo build
cargo test
cargo run -p wfsim-cli
```

## Status

Early scaffold (milestone M0). The engine and optimizer are not implemented yet.

## License

MIT
