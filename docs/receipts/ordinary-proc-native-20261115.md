# Ordinary proc native qualification and impact

The N07A ordinary-generation kernel is integrated into real fight consumers.
Implementation is `cf84a1b1b6e`; the retained optimization is `014e2ee3c7f`.
Golden measurements and game-data YAML are unchanged. This qualification
closes the supplied kernel and bounded target117 query, rather than every
native runtime producer or status lifecycle.

## Source and exact checks

Batch `20261115T065558Z-ordinary-proc-generation`, manifest
`ea4b3815f48d45b79fb63f5fc1af573ee008d2532049778db15852439382a1ec`,
pins client build `2026.10.28.09.54`, executable SHA-256
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.
All eight outer files and 223 archive members pass size/SHA-256 checks.
The durable fixture and importer retain nine weight vectors, 1895 sampling
vectors, 60 selector predicates and the enum-initializer control.

Tests compare counters, U24 values, original RNG state writes, inclusive
chance/CDF boundaries, lazy handler evaluation and target117 folding. They
include zero weights, saturation, preserved counters, NumHits 1/2/20, nine
late-query controls and four all-immunity complete-return controls. Another
645 controls run the actual scene sampler against the supplied default graph.
Changing the CDF comparison to strict greater-than fails native case1875;
changing a fixture counter fails the pinned importer check.

Both implementation and optimization pass `cargo test --workspace --jobs 2`:
1933 passed, six existing ignored. Workspace/all-target Clippy with warnings
denied, comment/page-body checks and commit-trailer checks pass. No golden
value was changed to fit a new result.

The running dev preview at `http://127.0.0.1:8831` matches the final frozen
Native binary on all 112 focused HTTP requests. The combat-record page check
passes36 and retains three existing 40m-miss assertions failing with zero
pellets; the same three failures are recorded in the earlier native-order
baseline. They remain visible and prevent describing browser validation as
fully green. Release integration must account for those failures.

## Gameplay and consumers

The frozen pre-N07A baseline has native source SHA-256
`4e6c28f73ffeecd725e512f11a1cd5864b03d4b4571c4ee4fa2c053c6c1706b7`.
It retains the already-corrected Primary and Irradiate calculation lineage.
It is separate from the original production algorithm and remains immutable.

The 2459-request suite includes 2333 bare weapon modes, focused simulations
and records, dense fields, Irradiate and 40 Phantasma audit requests. Against
the frozen baseline, 77 cases change combat fields and 2382 preserve them.
The six changed bare modes are Phage, Phantasma, Phantasma Prime, Simulor,
Synoid Simulor and secondary Vermisplicer. Raw reports also retain every work
counter difference; none is presented as a gameplay difference or hidden
from the strict report. Both versions reproduce their own responses.

Final Native versus WASM: 2459 requests, 9836 retained responses, zero strict
differences. Optimization versus the first conforming implementation: all
9836 combat responses remain exact; 727 cases change only explicitly checked
integer work accounting. The 16 high-sample Phantasma responses also preserve
every combat field after optimization. Chrome contributes another 368 final
responses exact against the same Native reference, including work accounting.

Three exhaustive optimizer reference pools cover Irradiate (12 jobs),
Encumber (43) and Primary Debilitate (22). Each production search selects
reference rank1 with zero regret at 128 reference runs. These are scoped
reference pools, rather than a full-catalog optimizer guarantee.

## Phantasma Prime calculation

Captured builds use Primary Debilitate, level300 Steel Path Corrupted Heavy
Gunner, body aim, 12 seconds, infinite ammunition, seed24301 and 1024 runs.
The following rows include Hell's Chamber. These are simulation predictions,
not new in-game measurements.
The 16 captured requests are committed in
`tests/fixtures/engine-compare/ordinary-proc-calls.json`; they contain inputs,
without turning these simulated DPS values into measurement goldens.

| composition | pre-N07A mean DPS | final mean DPS | change |
| --- | ---: | ---: | ---: |
| Radiation | 3691.79 | 3693.73 | +0.052% |
| Gas + Radiation | 10809.87 | 10811.14 | +0.012% |
| Corrosive + Radiation | 19534.50 | 19536.12 | +0.008% |
| Gas + Magnetic + Radiation | 16507.43 | 16518.68 | +0.068% |

Across all 16 with/without-Arcane and multishot controls, mean DPS changes
range from -0.103% to +0.068%. Fixed four-run traces can change substantially
more because the native draw order changes which random values each type
receives. High-sample mean agreement does not determine live callback cadence.

## Performance and size

Timings use frozen release executables after builds finish, two ABBA rounds,
and no concurrent comparison/build jobs from this task. The machine is shared.
The four standard shapes use 300 fights and three internal repeats per phase.

| Native shape | pre-N07A ms/fight | final ms/fight | change |
| --- | ---: | ---: | ---: |
| Torid | 0.9962 | 1.0921 | +9.63% |
| Gotva Prime | 1.5507 | 1.6226 | +4.64% |
| Scourge | 0.5433 | 0.5782 | +6.44% |
| Braton Prime | 1.3270 | 1.3700 | +3.24% |

The sum of warmed per-request medians across 31 actual Native API workloads
is 2108.28→2109.79ms (+0.072%). Chrome's corresponding sum across 92 fixed
requests is 2275.15→2215.60ms (-2.62%). These workload observations do not
establish a universal speedup. Native integer-chance attempts consume the
additional RNG draws required by the source; timings measure the whole fight.

The production-style data-apart WASM module grows 4784 bytes, from7049154 to
7053938 bytes. Observed module linear-memory high water across the 92-request
Chrome sequence is 89.5625→89.8125MiB. This is module memory, not total browser
heap or transfer/compression size.

## Boundaries and retained evidence

Handler selection is independent of final immunity. Unbound enemies retain
the explicit wiki scene-eligibility fallback; default13 is not universally
installed as a recovered native fact. Native handler installation, upstream
source365/scanned366 branches, later acceptance, complete RNG stream ownership
and actual Phantasma beam-to-Primary notification multiplicity remain open.
Void/Tau scene alternatives do not invent native proc ids. See
[ENGINE_MIGRATION.md](../ENGINE_MIGRATION.md) and
[UNMODELLED.md](../UNMODELLED.md).

Frozen manifests, full responses, timings, source/fixture negative controls,
optimizer outputs and HTTP/browser receipts are under `target/n07a/`.
Final Native source SHA-256:
`49ae2d100b4660c26baa8f6750cca78d0ea7238960f763732140eba172f7333c`.
Final WASM source SHA-256:
`f642590180183f9789dbc1820e7f56cecbc0c170cb0cb8529439ee0b2508341b`.
Original9a8c, intermediate94/1c and pre-N07A snapshots are retained.
Full release CI, integration with current main, Mac/channel publication and
board rescoring belong to the release owner; none ran as part of this task.
