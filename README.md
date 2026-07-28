# wfsim

[![CI](https://github.com/magenie33/wfsim/actions/workflows/ci.yml/badge.svg)](https://github.com/magenie33/wfsim/actions/workflows/ci.yml)

**The Simulacrum. The Primed One.** A Warframe damage simulator that replays
the whole fight and matches in-game numbers — then shows you everything the
game won't. Includes a Monte-Carlo build optimizer and a local web UI.
Website: [wfsim.app](https://wfsim.app)

**Status:** in development. One weapon fully modeled (Dual Toxocyst, incl.
Incarnon). Single enemy per sim; no Rivens yet.

## Run

```sh
mise install             # pinned Rust toolchain
cargo test --workspace
cargo run -p wfsim-web   # web UI → http://localhost:8787
```

## Docs

[Design](docs/CORE.md) · [Mechanics](docs/MECHANICS.md) ·
[Measurements](docs/MEASUREMENTS.md) · [Data](data/README.md) ·
[Contributing](CONTRIBUTING.md)

## License

MIT. Game data derived from the community
[Warframe Wiki](https://wiki.warframe.com/) (CC BY-SA). Unofficial fan
project, not affiliated with Digital Extremes; Warframe is a trademark of
Digital Extremes Ltd.
