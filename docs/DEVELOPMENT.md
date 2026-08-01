# wfsim — Development Setup

This project is developed on both **Windows** and **macOS**. Toolchains are
managed with [mise](https://mise.jdx.dev/) so every machine uses the same
pinned Rust version. **We do not use Docker** during development; containerized
packaging is deferred until the project is feature-complete.

## 1. Prerequisites (all platforms)

- **git**
- **mise** — the version manager that pins our toolchain. Install per the
  [mise docs](https://mise.jdx.dev/getting-started.html):
  - macOS: `brew install mise`
  - Windows: `scoop install mise` (or `winget install jdx.mise`)
- A **C linker/toolchain** (platform-specific — see §3). Rust binaries always
  need a system linker; this is the one non-mise dependency.

After installing mise, make sure it is activated in your shell (see the mise
docs for `mise activate`). This repo also works via `mise exec -- <cmd>` without
shell activation.

## 2. Toolchain via mise

The Rust version is pinned in [`mise.toml`](../mise.toml). From the repo root:

```sh
mise trust        # first time only: trust this repo's mise.toml
mise install      # installs the pinned Rust toolchain
```

`mise install` reads `mise.toml` and installs the exact Rust version for you.
Do not install Rust separately (no system rustup/homebrew rust) — let mise own
it so Windows and macOS stay in sync.

## 3. Platform-specific: the C linker

Rust on every OS shells out to a native linker. mise installs `rustc`/`cargo`
but not the linker, so install one per platform.

### macOS

Install the Xcode Command Line Tools (provides `clang`, the linker, and system
headers). Usually already present; if not:

```sh
xcode-select --install
```

That is all — `cargo build` works afterward. No further setup expected on macOS.

### Windows

Our Rust toolchain targets the **MSVC** ABI (the Windows default and what CI
uses), so install the Microsoft C++ build tools. Recommended, via winget
(needs elevation; ~2–4 GB download):

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

After it finishes, `rustc` auto-detects the Visual Studio install and its
`link.exe`; you do **not** need a "Developer Command Prompt". Plain PowerShell
works.

> **Note (Git Bash users):** Git for Windows ships a coreutils `link.exe` on its
> `PATH` that can shadow the MSVC linker. Once the MSVC Build Tools are
> installed, rustc invokes the linker by absolute path, so this is a non-issue.
> If you ever hit a `link: extra operand` error, it means the MSVC tools are not
> installed yet — install them per above.

#### Lightweight alternative (GNU toolchain)

If you cannot install the multi-GB MSVC tools, use the GNU toolchain with MinGW
instead (~300–400 MB, no elevation):

```powershell
scoop install mingw
rustup default stable-x86_64-pc-windows-gnu   # or the pinned gnu version
```

This is a supported fallback. It produces identical numeric results but diverges
from the Windows/CI default, so prefer MSVC unless you have a reason not to.

## 4. Build, test, run

From the repo root (prefix with `mise exec --` if mise is not shell-activated):

```sh
cargo build                 # build the whole workspace
cargo test                  # run all tests, including golden tests
cargo run -p wfsim-cli      # run the CLI
```

### UI checks (headless Chrome over CDP)

`cargo test` cannot see the page. Anything that lives in the browser is
checked by driving headless Chrome — Node >= 22 has a global `WebSocket`, and
Chrome is expected at its default install path (override with `CHROME=`).

```sh
python scripts/build_site_app.py   # the checks read site/, so build it first
node scripts/check_parity.mjs      # builder vs optimizer, every weapon
```

**`check_parity.mjs` — the builder and the optimizer must offer the same
thing.** They are the same question asked twice: the builder fills a weapon's
slots, the optimizer searches them. The script serves `site/` itself, walks
every weapon, and compares each AXIS — mods, exilus, arcanes per pool,
evolutions per tier — option set against option set, plus what each module
decides to SHOW. Exits non-zero on any mismatch, so it can gate a push.

Run it after adding a weapon, a mod pool, or anything a weapon can carry. It
is the check that makes `weaponAxes()` in `web/src/static/app.js` worth having:
that function is one description of a weapon's axes so a special case is a
one-place change, and this is what notices when a second place appears
anyway. In the two hours around its own writing it caught the optimizer
offering Exilus and Arcane scopes on a sentinel weapon, an exilus slot on the
Larkspur with no mod that could enter it, and the two modules computing the
exilus pool from different sources — agreeing only by coincidence.

## 5. Repo layout at a glance

See [`CORE.md`](CORE.md) §4 for the full architecture. In short:

- `engine/`, `optimizer/`, `cli/`, `web/`, `webapi/`, `wasm/` — the Rust
  crates (Cargo workspace). `web` is the native dev server (UI in
  `web/src/static/`); `webapi` holds endpoint logic shared with the
  `wasm` build; `site/` is generated from it by
  `scripts/build_site_app.py`.
- `data/` — versioned game data (weapons, mods, enemies, factions, arcanes).
- `docs/` — `CORE.md` (design), `MECHANICS.md` (how numbers are computed),
  this file.
- `tests/golden/` — golden tests calibrated against in-game measurements.
- `AGENTS.md` (repo root) — the condensed rulebook for AI coding agents.

## 6. Docker (deferred)

Intentionally **not** used during development. We will revisit containerized
builds/packaging only once the simulator is feature-complete. Until then, mise
is the single source of truth for the toolchain.
