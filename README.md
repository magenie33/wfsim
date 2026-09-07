<div align="center">

<img src="web/src/static/logo.svg" width="140" alt="WFSim" />

# WFSim

**The real Simulacrum Prime.**

A Warframe calculator that simulates the fight instead of estimating it —
builder, simulator, optimizer.

[**wfsim.app**](https://wfsim.app) ·
[Discord](https://discord.gg/H4BMqvYFVg)

[![CI](https://github.com/magenie33/wfsim/actions/workflows/ci.yml/badge.svg)](https://github.com/magenie33/wfsim/actions/workflows/ci.yml)

</div>

**Status:** usable, still moving. 387 weapons — every primary, secondary and
archgun, plus sentinel weapons and 69 Incarnon forms; mods, arcanes,
evolutions, rivens, custom enemies and the [leaderboard](docs/BOARD.md) are
live. Melee is being imported now: three weapons so far, seven attack forms
each. Fights are single-target or a 19x19 formation. Gaps are expected —
issues welcome.

**On AI:** I use AI assistance while writing this code (see
[AGENTS.md](AGENTS.md) / [CLAUDE.md](CLAUDE.md)). The simulator itself contains
no AI: it is a deterministic damage model plus a Monte Carlo search over mod
combinations. Every formula cites a wiki page, a datamine, or a measurement,
and each in-game measurement is written up as its own file under
[`docs/measurements/`](docs/measurements) — the engine tests that pin them cite
the measurement by number.

What the sim does **not** model — movement, holstering, nobody shooting back,
the Warframe's own actions beyond the buffs it puts on the weapon — is listed
in [`docs/UNMODELLED.md`](docs/UNMODELLED.md) and stated on the page itself,
per weapon.

## Contributing

The most valuable contribution here is **an in-game measurement, not code** —
the engine is only as trustworthy as its baseline, and one maintainer cannot
run every test in game. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Run

```sh
mise install             # pinned Rust toolchain
cargo test --workspace
cargo run -p wfsim-web   # web UI → http://localhost:8787

cargo run --release --bin one_fight -- save   # engine cost, before a change
cargo run --release --bin one_fight           # …and after: delta + did the answer move
```

## Docs

[Design](docs/CORE.md) · [Mechanics](docs/MECHANICS.md) ·
[Measurements](docs/MEASUREMENTS.md) · [Data](data/README.md) ·
[Contributing](CONTRIBUTING.md)

## License

[AGPL-3.0-or-later](LICENSE). If you use this code in a product or
network service, you must release your modifications under the same
license. The "WFSim" name and logo are not covered by the license and
may not be used to brand derived products or services.

Game data derived from the community
[Warframe Wiki](https://wiki.warframe.com/) (CC BY-SA). Vendored
[WFCD/warframe-items](vendor/warframe-items/LICENSE) data remains MIT.
Unofficial fan project, not affiliated with Digital Extremes; Warframe
is a trademark of Digital Extremes Ltd.
