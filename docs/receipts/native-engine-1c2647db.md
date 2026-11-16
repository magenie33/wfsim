# Native calculation candidate effects

Calculation: `1c2647db21f248c197a05f352511daa6015f3660`. Original: `9a8c30a2f793cc6d449425d1e57deaae2cae2d48`.

The candidate is an evaluated native arithmetic migration. Full native fight equivalence remains uncertified; the binding ledger states the remaining producers and consumers.

Workspace: 1913 passed, 6 ignored. Clippy and original instruction vectors passed. Measured integer expectations are unchanged.

All 2,333 modes of 665 weapons produce exactly equal Native/WASM responses. Four optional native trace requests and two additional Elementalist/faction requests also match.

## Matched-request performance

Elapsed change versus the preserved original; positive means slower. Nineteen simulation requests, ABBAABBA order, three executions per request per invocation.

| Runtime | Geometric mean elapsed change |
| --- | ---: |
| Native | +2.366% |
| Node WASM | +2.293% |
| Chrome WASM | -0.039% |

## Long fights

180 simulated seconds, 1,000 runs, three repeats per invocation, ABBAABBA order.

| Weapon | Original ms/fight | Candidate ms/fight | Elapsed change |
| --- | ---: | ---: | ---: |
| torid | 0.926993 | 1.020812 | +10.121% |
| gotva_prime | 1.546782 | 1.612868 | +4.272% |
| scourge | 0.543842 | 0.567353 | +4.323% |
| braton_prime | 1.316176 | 1.376915 | +4.615% |

## Numeric changes with the RNG migration held fixed

Previous checkpoint 540eb587 versus the candidate. Same saved requests; these short seeded samples describe this workload.

| Request | Previous DPS | Candidate DPS | Change |
| --- | ---: | ---: | ---: |
| torid_simulation | 36195.236485 | 36195.236485 | +0.000000% |
| gotva_prime_simulation | 14760.865522 | 14760.865522 | +0.000000% |
| scourge_simulation | 3237.339192 | 3237.339192 | +0.000000% |
| braton_prime_simulation | 16186.700569 | 16186.700569 | +0.000000% |
| amprex_gas_electric_0_simulation | 352.790157 | 352.790164 | +0.000002% |
| amprex_gas_electric_100_simulation | 849.708300 | 849.708309 | +0.000001% |
| kuva_nukor_shield_simulation | 31070.135418 | 31070.135400 | -0.000000% |
| coda_bubonico_battery_simulation | 377.159473 | 377.159473 | +0.000000% |
| torid_transformed_chain_simulation | 973.248707 | 973.248707 | +0.000000% |
| multiple_actors_simulation | 244.641459 | 244.641459 | +0.000000% |
| primary_debilitate_callbacks_simulation | 481.498061 | 481.498068 | +0.000002% |
| secondary_encumber_window_simulation | 39213.767969 | 39213.767958 | -0.000000% |
| braton_replay | 16186.700569 | 16186.700569 | +0.000000% |
| primary_debilitate_callbacks_simulation_no_arcane | 352.790157 | 352.790164 | +0.000002% |
| primary_debilitate_callbacks_simulation_mc128 | 571.227106 | 571.227115 | +0.000002% |
| primary_debilitate_callbacks_simulation_mc128_no_arcane | 398.768940 | 398.768946 | +0.000001% |
| secondary_encumber_window_simulation_no_arcane | 31070.135418 | 31070.135400 | -0.000000% |
| secondary_encumber_window_simulation_mc128 | 36982.006550 | 36982.006542 | -0.000000% |
| secondary_encumber_window_simulation_mc128_no_arcane | 31599.383248 | 31599.383229 | -0.000000% |
| kuva_nukor_shield_simulation:elementalist_faction | 55691.320590 | 55689.875949 | -0.002594% |

## Memory

WASM module: 6,965,180 → 7,043,935 bytes (+78,755).

| Runtime | Original peak linear memory | Candidate peak linear memory |
| --- | ---: | ---: |
| Node WASM | 28,639,232 | 29,360,128 |
| Chrome WASM | 28,639,232 | 29,360,128 |

Allocation counts and process RSS are in the JSON receipt. RNG changes alter event mixes; these are matched-workload observations, not a structural memory-improvement claim.

## Inspect the calculation

Send `native_trace: true` to `/api/log`. Each captured step exposes selected original upgrade entries, input/output binary32 bits, proc/part tags, phase and explicit context binding. Optional `native_trace_limit` bounds storage and reports dropped steps.

A bounded production-path probe with source 5,000 and unrestricted status bonus 0.5 records parent 7,500, child retained seed 11,250, accumulator 11,251 and prepared tick 5,625.5 before non-unit recipient factors. This is a synthetic source-vector check, not a new in-game measurement.

[Machine-readable receipt](native-engine-1c2647db.json) · [Binding ledger](../NATIVE_BINDINGS.md).

Preserved source, binaries, requests, raw outputs and scripts: `target/native-order/qualified-retained`; Drive backup `engine-native-candidate-20261115-1c2647db`. Mounted-file readback does not certify remote sync.

## Preview and browser coverage

The candidate preview at http://127.0.0.1:8831 uses this calculation source. Six actual HTTP answers, including optional traces, match the frozen native binary exactly.

The combat-record browser check passes its arithmetic, status decomposition, pool split and display checks. Three assertions expecting misses at 40m fail: the captured request yields zero misses on the original9a8c, previous540 and current1c2647db engines. This is an existing fixture limitation, retained explicitly rather than counted as a pass. Full release CI remains pending.
