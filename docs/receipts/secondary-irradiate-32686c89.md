# Secondary Irradiate evaluation

Calculation `32686c89efa` compared with optimized baseline `94b62a5d0ced`.
The feature is ready for integration; official channels are unchanged.
[Machine receipt](secondary-irradiate-32686c89.json),
[binding scope](../SECONDARY_IRRADIATE.md).

The engine preserves NumHits independently of status counters and uses the
recovered activation/collector/consume state machine. Echoes can originate from
secondary direct-hit bodies. Two-slot arcane resolution retains the entire
echo payload. Original script keys are retained. Native producer, owner/type,
charge, callback/Sleep(0) scheduling and full recipient dispatch remain open;
the live scene uses the documented immediate adapter and M59 calibration.

The fixed comparison uses 20 seconds, four runs and four supplied targets at
level 300 Steel Path. Recipes are in
`tests/fixtures/engine-compare/irradiate-calls.json`. These are regression
scenarios, separate from the M59 in-game measurements.

| case | baseline total DPS | candidate total DPS | change |
| --- | ---: | ---: | ---: |
| Cortege, Irradiate | 1689.94 | 3383.02 | +100.19% |
| Cortege, Merciless + Irradiate | 648.53 | 3644.27 | +461.92% |
| Dual Toxocyst Incarnon, Irradiate | 12196.25 | 20728.58 | +69.96% |
| Tenet Cycron, Irradiate | 3664.98 | 12279.26 | +235.04% |

The 34-case comparison preserves 26 answers and changes exactly eight: the
simulation and full record for each row above. New records include echoes
landing on the aimed body from other victims: 72, 73, 319 and 1622 events,
respectively. No echo can recursively activate itself. Laetum base/incarnon
answers and complete records are preserved. Runtime tests reproduce M59's
1536 → 2764.8, 320 → 1152 and 960 → 3456 supplied-seed readings.

Validation passes 1928 Rust tests, with six ignored, and workspace/all-target
Clippy with warnings denied. Fourteen imported native count vectors pass the
count projection with supplied compatibility; the original replay preserves
816 executed addresses. This does not claim Rust executes the native queue
comparator or Luau scheduler. Constructor-count, dropped-payload and missing
secondary-route mutations fail; an altered final damage record also fails.

Native versus production-configured WASM is strict-exact across 2419 cases and
9676 retained responses, including work/gas. Baseline comparisons preserve
2333 unmodded weapon/form simulations and 38 focused cases, with only explicitly
paired source work/gas fields excluded. Chrome 157 verifies 52 cases across
eight observations; 44 are preserved and the same eight change. A separate
44-case control comparison preserves all gameplay/record fields. Optimizer
truth exhausts 12 Irradiate/no-arcane jobs: rank 1, regret 0%, within noise.
The rebuilt preview's 72 actual HTTP responses match the frozen final native
outputs exactly at `http://127.0.0.1:8831`.

Native timing uses 30 samples per side per case. Positive values mean slower:

| workload | elapsed-time change |
| --- | ---: |
| eight no-Irradiate scenarios | −1.03% to +2.31% |
| fourteen dense-status scenarios | −2.24% to +0.20% |
| five preserved Irradiate scenarios | +3.47% to +4.93% |
| four corrected Irradiate scenarios | +29.70% to +128.51% |

Corrected scenarios settle additional damage, procs and records. The matched
WASM module grows 4626 bytes, from 7044528 to 7049154; data packs match the
baseline. Native simulation peak process RSS is 41.7 → 41.2 MiB. The 52-case
Chrome sequence with additional echo records reaches 51.6 → 89.4 MiB WASM linear
memory. A fresh 44-case sequence with preserved answers/records reaches the same
46.7 MiB on both versions. These are workload high-water measurements, not a
leak test. Short Chrome timings are sensitive to timer resolution and JIT state;
the JSON receipt retains each case's sample counts and medians.

Artifacts are retained under `target/irradiate/`, including immutable native,
embedded-data WASM and matching production-data-apart WASM snapshots, raw
responses, timings, fault controls and source qualification. Snapshot metadata
names the dirty parent used during compilation; every compiled source hash
matches calculation commit `32686c89efa`. Original, retained and optimized
baselines remain unchanged. The backup destination is
`WFSim/secondary-irradiate-20261115-32686c89`.

The three pre-existing `check_combat_record.mjs` 40m-miss failures remain
documented in the retained baseline evidence; this slice does not claim that
check passes. Integration still requires final combined-candidate CI, a new Mac
engine and coordinated channel/scoring publication. No main push, release,
site build or local board rescore is part of this qualification.
