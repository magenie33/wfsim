# Native status maintenance optimization

Calculation `94b62a5d0ceda4832206521ac28315c031e3d748`, compared with preserved `1c2647db21f248c197a05f352511daa6015f3660`.

The optimization preserves the existing native calculation steps. Complete game-native equivalence remains bounded by the [binding ledger](../NATIVE_BINDINGS.md).

## Measured effects

Same frozen 180-second, 25-body requests; positive reduction means less elapsed time. Rows below are API request medians, not UI frame times.

| Workload | Native before ms | Native after ms | Native reduction | Chrome before ms | Chrome after ms | Chrome reduction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Gas/Electricity body | 213.357 | 160.844 | 24.61% | 243.200 | 178.600 | 26.56% |
| Gas/Electricity head | 167.548 | 132.538 | 20.90% | 186.200 | 148.500 | 20.25% |
| Gas/Electricity body, held out | 658.701 | 500.533 | 24.01% | 743.550 | 542.050 | 27.10% |
| Gas/Electricity head, held out | 504.787 | 402.457 | 20.27% | 562.450 | 444.200 | 21.02% |

The held-out requests contain three run states per API call. Native Torid, Gotva Prime, Scourge and Braton Prime 180-second/1,000-run controls remain within about 1% of baseline. High-repeat Chrome short controls show unchanged medians in three cases and about 1% extra elapsed time in the 128-run no-arcane control.

## Exactness

All 2,333 modes retain identical combat answers. Native/WASM responses match strictly for those modes, 38 focused requests and 14 dense/held-out requests. Chrome verifies 144 full responses, with another 32 from high-repeat controls.

Only explicitly named source-accounting counters may differ between code versions. Every raw response and strict difference is retained. A one-ULP edit to real body damage fails the gate. RNG/run fields, event order and native trace bits are compared without tolerance.

Workspace: 1,913 passed, six ignored. All-target Clippy, gas/work/score cross-platform checks and both truth scopes pass; golden expectations are unchanged. The qualified native binaries have the same hashes as the measured prototype.

## Memory and preservation

Chrome workload peak linear memory is unchanged at 22,675,456 bytes. The WASM module grows by 593 bytes.

[Machine-readable results](native-opt-94b62a5d.json) contain all controls, commands and limits. Frozen raw evidence is under `target/native-opt`; the new Drive backup keeps this candidate separate from the original and prior native snapshots. Mounted-file readback does not certify remote sync.

The three existing 40m miss-fixture browser failures remain documented in [the baseline receipt](native-engine-1c2647db.md). They are not counted as passing checks. Full combined release CI remains pending.

## Preview

The optimized candidate is retained at http://127.0.0.1:8831/weapons/Kuva_Nukor/simulator. All 39 actual HTTP responses, including native traces and the existing 40m fixture, match the frozen optimized native binary exactly. The 40m fixture still produces zero misses.
