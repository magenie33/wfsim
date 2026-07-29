# wfsim

[![CI](https://github.com/magenie33/wfsim/actions/workflows/ci.yml/badge.svg)](https://github.com/magenie33/wfsim/actions/workflows/ci.yml)

**The Simulacrum. The Primed One.** A Warframe builder, fight simulator,
and Monte-Carlo build optimizer — true to in-game numbers and mechanics.

Local web UI. Website: [wfsim.app](https://wfsim.app) ·
Community QQ group: [**995078378**](https://qm.qq.com/q/uiXrMSTs8S)

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
