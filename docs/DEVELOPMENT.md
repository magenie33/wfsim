# wfsim — Development Setup

This project is developed on both **Windows** and **macOS**. Toolchains are
managed with [mise](https://mise.jdx.dev/) so every machine uses the same
pinned Rust version. **We do not use Docker.**

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

CI selection, forced full runs, batching and reuse of a validated commit are in
[`CI.md`](CI.md). Quick checks run on every change; core and unknown changes
require full validation.

Feature work uses affected local checks and an explicit ready-for-integration
handoff. Finish those checks and review before dispatching CI. The merger
validates the final release batch; full CI is not a per-subfeature requirement.

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
cargo run -p wfsim-web                                        # the dev server
WFSIM_BASE=http://127.0.0.1:8787 node scripts/check_parity.mjs  # a check against it
```

A check serves `site/` itself unless `WFSIM_BASE` names a server, and CI runs
the selected browser checks on the `site/` it builds — so a change is tested against the dev
server and `site/` is never built for it.

### THE SLOW STEP GOES LAST

`build_site_app.py` is minutes — wasm, wasm-opt, one prerendered page per
weapon, the whole image set — and every browser check on top is another one or
two. Using that loop to find out what to fix next is the expensive mistake
available here, and it compounds: a change that replaces an assumption running
through the page has a dozen call sites, and discovering them one slow round
trip at a time costs an afternoon.

- **Enumerate before editing.** Grep out every call site and fix them in one
  pass. A riven's identity moving from its name to its own id touched fifteen —
  the autosave, the undo restore, the board card, the open pointer, the share
  encode and its import — and each one found the slow way cost a full cycle.
- **Debug and check against the dev server.** `cargo build -p wfsim-web` is a
  couple of seconds, and every check reaches it through `WFSIM_BASE`.
- **Leave `site/` to CI.** It builds product changes and runs the selected checks
  on that build; local checks use the dev server.

On Windows the site build sometimes fails with `OSError: Errno 22` writing one
of the prerendered pages. It is environment flake rather than the code — the
same write succeeds on its own a second later — so retry it.

**`check_parity.mjs` — the builder and the optimizer must offer the same
thing.** They are the same question asked twice: the builder fills a weapon's
slots, the optimizer searches them. The script serves `site/` itself, walks
every weapon, and compares each AXIS — mods, exilus, arcanes per pool,
evolutions per tier — option set against option set, plus what each module
decides to SHOW. Exits non-zero on any mismatch, so it can gate a push.

Run it after adding a weapon, a mod pool, or anything a weapon can carry. It
is the check that makes `weaponAxes()` in `app.js` worth having:
that function is one description of a weapon's axes so a special case is a
one-place change, and this is what notices when a second place appears
anyway.

## 5. Making the engine FASTER, without making it wrong

`one_fight` is the harness for it. It exists because the repo could already
grade the search's ACCURACY (`wfsim-truth`) and had no way to state its COST —
and "it feels faster" and "it got dumber" are indistinguishable without both.

```bash
cargo run --release --bin one_fight -- save    # remember where you started
#   …edit the engine…
cargo run --release --bin one_fight            # what it cost, and what it changed
```

The second command prints a delta against your baseline **and whether the
answer moved**. A moved answer exits non-zero: an optimisation that changes a
number is not an optimisation, it is a bug, and that is the one thing this must
never let you scroll past. It catches a change of one part in 10¹².

**THE COST COLUMN COMPARES TWO BINARIES IN ONE SESSION, NOT TWO MOMENTS.**
`save` keeps the executable beside its numbers, and a comparison runs the saved
one, then the new one, then the saved one again, against the mean of the two
outer readings: a machine holds a performance state for minutes, and the SAME
binary measures far apart between states — quickest straight after a full build.
**A delta under about 10% taken immediately after a build is still not a
result** — let it settle, or run it twice. A machine reference is timed in the
same process and printed when it moves; it is evidence that the moment was
bad, never a scale factor.

**IT ALSO GRADES ITS OWN COVERAGE**, because the answer column can only catch a
change in something the suite actually does. The default build combines every
element away — Hellfire + Cryo Rounds is Blast, Infected Clip + Stormbringer is
Corrosive — so a fourth shape, the **Braton Prime**, 60% of whose base is Slash
(a PHYSICAL type is the one thing an elemental mod cannot combine away), burns
under it. The tool FAILS when the whole suite ticks nothing, so editing the mod
list or the weapon list cannot silently drop status DoT from the suite. `-v`
prints the burn-tick count per shape. The mod list is what every saved baseline
was measured under: leave it.

Every knob is a `key=value` argument — `weapon=`, `mods=`, `runs=`,
`duration=`, `enemy=`, `level=`, `steel_path=`, `seed=`, `repeats=`, and `-v`.
`--help` lists them.

### Which module does this measure?

All three, because all three run the same primitive — `monte_carlo(params,
runs, seed)` — and two measurements say the per-run number transfers between
them unchanged:

- **per-run cost is flat from `runs=1` to `runs=1000`** (0.92 → 0.98 ms on the
  Gotva Prime), so the optimizer's first round at one or two runs a candidate
  pays the same unit price as the simulator's thousand;
- **the setup the optimizer pays per candidate** — `resolve` + `from_panel` —
  is **0.0007 ms**, a thousandth of one run.

So one number multiplies out:

| module | cost |
| --- | --- |
| Simulator | `runs × ms/run` |
| Board / benchmark | `runs × ms/run`, on a runner |
| Optimizer | **`sims × ms/run`**, and `sims` is what `wfsim-truth` reports |
| Quick calc | `candidates × (setup + runs × ms/run)` — see below, the setup is NOT 0.0007 ms |

That last row is the split worth remembering. `one_fight` answers **what one
simulation costs**; it cannot see the funnel — enumeration, culling, sharding —
and does not try. `wfsim-truth` answers **how many simulations the search
spends and what accuracy that buys** ("5558 sims, 0.8% of the reference").
Multiply the two and you have the optimizer. Neither tool alone is a
performance claim about it.

**Which SCENARIO** is a `key=value` away, and the default is the ruler's fight
(Thrax Centurion 9999 Steel Path, 180 s) for one reason: it is the only fight
in the product that is the same for everyone. The optimizer runs the
*simulator's* scenario, whatever the reader set, so there is no "the optimizer's
scenario" to benchmark — pass your own with `enemy=` `level=` `duration=`.

**Read the table across, not down.** The default is three shapes that stress
different parts of the engine, because a change to the inner loop rarely moves
them together: `-C target-cpu=native` measured −23% on the Torid, −36% on the
Scourge and **+31% on the Gotva Prime**. One weapon would have said "ship it"
and one "revert", both truthfully.

**Where wins come from.** Build flags (LTO, `codegen-units`,
`target-cpu=native`) are noise or a lottery across shapes. Read the
per-instance path for ALLOCATIONS rather than guessing at arithmetic, and read
once per instant what is the same for every stack at that instant. Beyond that
the room is in **how many runs get spent** — see docs/OPTIMIZER.md.

### The quick calc pays a different fixed cost

`gainScan` measures every candidate for one slot, and unlike the optimizer —
which resolves a candidate against an already-parsed arena — it pays a whole
`parse_fight` per candidate, 361 bodies and all.

- **A scan that takes minutes is broken, not slow.** The heaviest fight in the
  product scans in seconds. Every report of the quick calc hanging has been a
  POOL fault; `scripts/check_calc_recovers.mjs` holds that line.
- **Batching the candidates into one call buys about a tenth** — the setup
  cost against a ten-run candidate — and costs splitting `simulate_from`,
  which the simulator and the board share. The lane count buys more of it for
  nothing: the default share is half the machine's cores.

### What a gain propagates to

ONE COPY, so an improvement reaches every module: `monte_carlo` (the fight),
`FightParams::*_from_panel[s]` (its parameters), `build::loadout::resolve_for` (mods to
a panel), `webapi::fight::parse_fight` (json to a fight — five call sites, including
`optimize_json`), `board::builds::BUILD_AXES` (what a build is).

WRITTEN TWICE, so a change has to be made twice:

| | Rust | the page |
| --- | --- | --- |
| reading a score off a `Summary` | `by_kills` in `optimizer::evaluate_batch` | `readGain` |
| the funnel — screen cheap, refine the top | `run_funnel`'s rounds | `GAIN_REFINE_TOP` |
| giving work to the next free worker | `evaluate_batch`'s chunking | the lane pool's cursor |

The first of those is one quantity under three names — `mean_kill_progress` in
the engine, `score` on the wire, `r.score` on the page — and it is the
one that has already cost a bug.

ENUMERATION IS NOT ON THAT LIST AND MUST NOT JOIN IT. The optimizer samples a
space too large to enumerate; the quick calc enumerates a small one exactly and
needs a number for every member. A sampler pointed at 81 candidates drops
answers for no reason. The paired estimator the page needs for that
(`gainOver`, a ratio SE over the run series) has no counterpart in the search,
which ranks but never says what a candidate is worth.

### …and the two tools that close its gaps

**`node scripts/one_fight_wasm.mjs` — the cost in the thing that ships.**
`one_fight` measures native Rust, which is a proxy: a change can be faster there
and slower in the browser and the native number would never say so. This runs
the identical fight through the shipping wasm build in a real browser and
prints the RATIO against your native baseline, which is what makes a native
measurement mean anything for a player. **wasm is 1.3–1.8× native**, and not
a constant — so the proxy needs calibrating per shape. Regenerate `site/` first
or you are timing yesterday's engine. It refuses to print a ratio when the two
tools ran different fights.

**`cargo run --release --bin one_fight -- ablate` — WHERE the time goes.**
The harness above validates a candidate; this one helps you find one, without a
platform profiler (`perf` is Linux, dtrace is macOS, and Windows has neither by
default). Each row disables one subsystem and reports what the fight then
costs, so the share it names is a CEILING on what optimising that subsystem can
return.

```text
torid · 180 s · fixed-length fight — where the time goes
  whole fight                           0.898 ms/run
  without status and everything it starts     0.900   at most  -0.2% is here
  without the lingering field                 0.101   at most  88.8% is here
```

That Torid line is the tool doing its job: the weapon's entire cost is its
lingering field, and the status axis — the obvious suspect, and 12.5% on the
Gotva Prime — sees none of it.

**`bodies=25 spacing=3` STANDS THE RULER'S CROWD THERE**, unkillable, and
`arcanes=` seats what the row wears. The rows that set this board's makespan are
all multi-target ones and a build paid 141x on a 361-body crowd what it pays against one body,
so a table taken against the fixture alone profiles the cheap 0.7% of the
weapon. It is off by default: every saved baseline and every golden value was
measured against one body.

**AND THE CROWD BRINGS ITS OWN LIMIT, which is the same one from the other
side.** The fixture cannot kill, so nothing resets — and a build that kills 537
bodies a run in the real fight spends most of it against targets whose status
has just been cleared. Anything whose cost GROWS with untouched state reads far
too high here: Primary Debilitate profiles at 96.5% of a Phantasma row against
1.7x measured on the ruler itself. Read the coarse answer (which subsystem) and
take the shares of anything state-shaped back to the real scenario.

**Read it with its two refusals in mind.**
A row whose SHOT COUNT moved is not an ablation, it is a different fight.
A row whose share is NEGATIVE is not an ablation either — it did not remove
work, it changed which work happens: truncating the body-part list reported
−77% because every hit became a headshot. The fight is fixed-length
(`Arena::training`) for the same reason, since against a killable target
removing work also removes damage and the two cannot be separated.

Shares OVERLAP and must never be summed.

It measures NATIVE, and the product ships as wasm — use `one_fight_wasm.mjs`
for the number a player waits for.

## 6. Repo layout at a glance

See [`CORE.md`](CORE.md) §4 for the full architecture. In short:

- `engine/src/` is six layers, bottom up: `rules/`, `model/`, `data/`,
  `build/`, the fight (`target`, `formation`, `arena`, `fight/`, `record`) and
  `board/`; `scripts/check_engine_layers.mjs` holds them apart.
- `engine/`, `optimizer/`, `cli/`, `web/`, `webapi/`, `wasm/` — the Rust
  crates (Cargo workspace). `web` is the native dev server (UI in
  `web/src/static/`, where `app.js` is joined from `app/NN-name.js` in
  filename order — `docs/CHECKS.md` §`check_app_parts`); `webapi` holds endpoint logic shared with the
  `wasm` build; `site/` is generated from it by
  `scripts/build_site_app.py`.
- `data/` — versioned game data (weapons, mods, enemies, factions, arcanes).
- `docs/` — `CORE.md` (design), `MECHANICS.md` (how numbers are computed),
  this file.
- `tests/golden/` — golden tests calibrated against in-game measurements.
- `AGENTS.md` (repo root) — the condensed rulebook for every coding agent,
  whatever tool runs it; no tool gets a file of its own.

## 7. Sharing the checkout

The multi-session working process (worktrees, staging, shipping from the main
checkout) is in `private/agents` — read its README first.

For opt-in local coordination across coding tools, see
[`COORDINATION.md`](COORDINATION.md): one shared task store per clone,
explicit handoffs and advisory resource reservations. Existing `.handoff.md`
claims stay in force until their owners explicitly migrate them.

**NO LINKS IN A CHECKOUT.** A worktree that needs `private/`, `vendor/` or
`web/cache` gets a COPY: `git worktree remove --force` and `rm -rf` on Windows
follow a junction and delete what it points to. `node
scripts/remove_worktree.mjs <worktree>` refuses while a link is inside, and
`--scan <dir>...` lists them. `private/` is its own repo, `wfsim-private`, and
the private worker is its own checkout beside this one, `../wfsim-cloud`;
`private/README.md` states how both are backed up and restored.

**A BUILD HOLDS THE CHECKOUT.** `build_site_app.py` and `ship.py` hold
`.build.lock` while they run (`scripts/checkout_lock.py`), and `ship_bot.py` run
by hand refuses while another process holds it, while `site/` or
`mcp/engine.js` carries uncommitted files, or while its engine is not the one
wfsim.app serves: it copies whatever `site/` holds, and a half-written build is
an engine the site does not serve. `node scripts/fleet_pulse.mjs` reads facts per
five minutes, the waited-on questions and the fleet, on the database's clock —
run it before and after any change to what the factory hands out.

### Shell traps on Windows

- Git Bash rewrites an argument starting with `/`, so a `/Lotus/...` path
  reaches Python as `C:/Program Files/Git/Lotus/...` and `de_export.py show`
  answers "not in the export". Prefix `MSYS_NO_PATHCONV=1`.
- `subprocess.run(..., text=True)` without `encoding=` decodes as the console
  codepage (GBK on a Chinese-locale machine); UTF-8 output then fails in the
  reader thread and `stdout` comes back `None`. Pass
  `encoding="utf-8", errors="replace"`.
- A heredoc mangles `\n`, `\\`, backticks and `$` even when quoted. Write code
  files directly, and run the file's parser (`node --check`, `cargo build`)
  after generating one.
- A long check piped through `tail` shows nothing until it exits, including the
  progress lines that tell "working" from "hung". Run it bare and filter where
  you read the output.

---

## Engine cost: `one_fight`

**THE GAS METER RUNS IN EVERY BUILD** (docs/WASM.md §"Gas"), so a cost
measured here includes it. `WFSIM_GAS_OFF=1` at BUILD time (with its own
`CARGO_TARGET_DIR`) compiles it out, to time the engine without it.

**Engine COST: `cargo run --release --bin one_fight`**, and `-- save` first.
It diffs a saved baseline and says whether the ANSWER moved — a moved answer
is a non-zero exit, because an optimisation that changes a number is a bug.
Read its table ACROSS: the default is four shapes and a change to the inner
loop rarely moves them together (`target-cpu=native` is −23% / −36% / **+31%**).

IT GRADES ITS OWN COVERAGE — the fourth shape is a Braton Prime, whose 60%
SLASH is the one thing an elemental mod cannot combine away, and the tool
FAILS when the whole suite burns nothing. `docs/DEVELOPMENT.md` §5 says where
wins have come from.

## `one_fight` compares two binaries, not two moments

**`one_fight` COMPARES TWO BINARIES, NOT TWO MOMENTS.** Its baseline is a
property of the machine on the day, and a day of driving headless browsers
moves that machine. When a delta matters: `cargo build --release --bin
one_fight`, copy the exe, `git stash`, build again, run them alternately
against one baseline. The tool's noise column is measured in seconds and
cannot see hours.

## Optimizer verification: `wfsim-truth`

**Optimizer verification: `cargo run --release --bin wfsim-truth -- pool=<ids>
…`**. A search cannot vouch for itself: the tool exhausts the scope, evaluates
every job flat, and reports where the production search landed in that
reference ranking (rank / regret / recall / cost, and whether the reference
reproduces itself under a second seed). It goes through `parse_optimize`, so
it grades the app's own fight, and REFUSES a scope it cannot exhaust. Run it
after ANY change to enumeration, scheduling or scoring. The cheap CI form is
`optimizer/tests/search_accuracy.rs`. See `docs/OPTIMIZER.md` §Accuracy.

## Reading a posted build: `decode_share`

**`node scripts/decode_share.mjs <link | short id | code>`** prints the build
a share link names: weapon, every slot with its polarity, each riven's stats
and rolls, arcanes, evolutions (`--json` for the decoded object). A short id
is read from the live store; the grammar is the worker's copy of the page's
codec and the id table is `data/share_order.yaml`, so it reads a link as the
page does. A link carries the build and nothing else — the enemy, the
duration and any fight-wide stat (`extra_stats`) the poster ran it under are
not in it, so ask for them.
