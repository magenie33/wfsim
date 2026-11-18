# Crowd search: Synoid Gammacor and Torid

Baseline: `b4f94e09606a2d22925da19614ef6c33ebe383f7` (includes the regular-strategy default and stalled score-batch guard). The reported search explicitly selected `four_elements`. That retains the blank start and adds four unpinned 60/60 starts; it does not require all four damage types in the winning build.

The supplied progress snapshot showed 3:08:24 elapsed, 26,705 builds, 134,351 completed fights and 27 finalists waiting for verification with one worker computing. Progress and computation feedback were both current. This is evidence of expensive work, not a time breakdown or a proof that every possible search terminates. There is no original exported request, machine profile or saved leaderboard. Torid's evolution selection in the fixtures is representative, not recovered from a screenshot.

## Browser end to end

Windows, Ryzen 7 9700X, Chrome 157.0.8103.79, four unthrottled Web Workers; Rust 1.97.1, release, wasm-bindgen 0.2.126. The harness runs the real page-side quick fleet, worker handler and WASM engine on loopback, with no upload. `standard_multi_target__aimed_half`: 25 Thrax Centurions, Steel Path level 9999, 3 m spacing, target at 0.5 m, 180 s, original seed. Both versions get the same six candidate cards, three main slots, no arcane, four-elements strategy, ten runs per candidate, ten results and 1,000 runs per finalist. The restricted card pool makes an end-to-end exact comparison reproducible; these times do not predict an unrestricted player's search.

| Weapon | Baseline total | Patched total (two trials) | Baseline final | Patched final | Finalists actually measured |
| --- | ---: | ---: | ---: | ---: | ---: |
| Synoid Gammacor | 23.29 s | 6.91–7.07 s | 22.61 s | 6.23–6.40 s | 11 |
| Torid | 127.18 s | 41.66–42.13 s | 125.09 s | 39.81–40.04 s | 10 |

The baseline used one finalist worker; the patch used four. An initial baseline during compilation measured 46.93/143.64 s and is not used for speedups. A final rebuilt candidate measured 7.14/41.85 s (3.26x/3.04x), with 6.50/39.88 s in verification. All trials produced the same complete search response except `cpu_ms`: all ordered rows, replays, start/branch attribution, search counts, final work and total work. The screening phases ran 240 and 298 fights respectively. Some close contenders qualify beyond the requested ten; all count toward final work (23,067,745,118 and 109,148,990,626 respectively).

## Cost per fight

Fixed seven-card Viral + Electricity builds and a six-card control omitting the Electricity 60/60 card. This is an actual loadout comparison, not a pure ablation: the card changes status chance as well as elemental damage. On the 50% ruler the upstream Torid averaged 6,414 DoT ticks per fight versus zero for the control; Synoid Gammacor averaged 5,803 versus 24. Neither showed a pathological step at 50%. In particular Synoid Gammacor does not enter Incarnon at 0% in these samples, so that control changes its shot count as well.

Median upstream WASM times in milliseconds per three indexed 180 s fights (Node/V8; not UI wall time):

| Weapon | Electric 0% | Electric 50% | Electric 100% | No electric card 50% | Single target electric 50% |
| --- | ---: | ---: | ---: | ---: | ---: |
| Synoid Gammacor | 21.11 | 21.43 | 20.08 | 13.24 | 7.59 |
| Torid | 43.61 | 43.75 | 41.17 | 22.08 | 9.04 |

The profiles also put `libm::pow` under enemy armor and maximum health. A 32-entry thread-local cache keys just the immutable level-curve power by both operands' bits. A miss calls the same `libm` operation. No status handling, seed, RNG, charged work, search depth, or tie-break changes. Alternating `ABBAABBA` on the 50% electric rows: Synoid Gammacor 21.43 to 18.15 ms in WASM (-15.3%), 14.20 to 13.28 ms native (-6.5%); Torid 43.75 to 40.32 ms in WASM (-7.8%), 29.93 to 27.77 ms native (-7.2%). One native non-electric control was effectively unchanged. These gains are separate from distributing the finalists.

The fourteen indexed fight responses and two small full-search responses matched the baseline WASM JSON byte-for-byte. The fourteen per-run results also matched native. All 29 existing simulation-cost fixture responses matched before and after in WASM. In the unrestricted scope the first four complete scoring batches per weapon were byte-identical, including progress, order, scores, shards, and build submissions (556 measurements / 1,272 fights for Synoid Gammacor; 549 / 1,228 for Torid). The open-ended search was not replayed to the end. Core, optimizer, web API, and WASM library suites: 1,836 passed, 4 ignored. The native one-versus-four worker test compares the entire report with an identical wire request.

## Reproduce

Freeze a `wasm-bindgen --target no-modules` package for each release build in separate directories. Build without `WFSIM_DATA_APART` for this self-contained harness. Run from the checkout containing both benchmark scripts:

```sh
node scripts/crowd_search_cases.mjs target/finals.json finals
node scripts/bench_crowd_fleet.mjs target/browser-base target/finals.json target/baseline.json b4f94e0960 4
node scripts/bench_crowd_fleet.mjs target/browser-new target/finals.json target/candidate.json working 4 target/baseline.json
node scripts/crowd_search_cases.mjs target/fights.json fight
cargo run --release --example simulation_cost -- target/fights.json target/fight-results.json 3
```

The `search` fixture is a short full descent; `lead` uses the unrestricted scope for inspecting the early real batches. On the short fixture an independent exhaustive `wfsim-truth` check found rank one for each weapon (49 legal jobs each, 24 reference runs per job). It does not establish global optimality for the open mod pool.
