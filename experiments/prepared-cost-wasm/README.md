# Preparation reuse cost experiment

Research only: this module is not a supported prepared-data format or interface.
It compares fresh preparation per chunk with an already-resident prepared
object. Transfer, decoding and initial preparation are free in the retained
variant. It measures a best-case preparation path, not candidate warm starts,
shared scores, a serialized upstream cache or Wasmtime AOT artifacts.

The native example and research wasm share the same fold helper. Each run uses
the ordinary simulator, retains JSON value validation and merges in run order.
Every result is checked against the ordinary board accumulator; the wasm driver
also checks the actual shipping wasm. The wrong-index control must fail.

From the repository root:

```sh
python3 scripts/prepared_cost.py inputs --library <official-native-library> --pack <data-pack> --output target/bake/cases.json
cargo build --release --locked -p wfsim-webapi --example prepared_cost
target/release/examples/prepared_cost target/bake/cases.json target/bake/native.json 2
python3 scripts/prepared_cost.py summary --input target/bake/native.json --output target/bake/native-summary.json
```

For wasm, build this independent manifest for `wasm32-unknown-unknown` with
`WFSIM_DATA_APART=1`. Run the pinned wasm-bindgen with `--target no-modules
--out-name prepared_cost --out-dir target/bake/wasm`, then the release's
`wasm-opt -Oz`. `node scripts/prepared_cost_wasm.mjs` times it in Node/V8 and
checks complete results against `site/release.json`'s wasm. The cold driver
creates fresh instances of one compiled module to separate first-use cost from
warm preparation; it does not attribute the difference to one specific loader.

The cases include frozen API and real board requests, casting, riven rolls,
cycles, melee, combatants and dense formations. Each uses 40 runs and its full
engagement duration; widths are 1, 7 and 40. Warm preparation excludes initial
catalog loading. Native timings use mimalloc and release optimization without
PGO. These measurements do not establish fleet savings or the value of a
precomputed search-candidate package.
