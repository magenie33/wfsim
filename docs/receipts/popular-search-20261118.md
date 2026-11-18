# Popular weapons: solving, combat and the native engine

Upstream `b4f94e0960`, incremental baseline `fa816d569d`. The earlier [Torid / Synoid Gammacor receipt](crowd-quick-finals.md) measures distributing final contenders across four workers (3.04x / 3.26x). Every comparison here starts **after** that change; do not multiply the ratios. No online jobs or submissions, game rules, score adjustments, seed changes, search cuts or golden changes.

## Scope

The public `/api/popularity` `tested` snapshot covers 30 complete UTC days (2026-10-19 through 2026-11-17), not in-game usage. We froze the homepage's top 40: 21 primaries, 11 secondaries, five melee, two archguns and one sentinel, from Torid (396) to Tenet Cycron (25). For each weapon we admitted a published board build through `/api/board/order` and held its identity, seed and equipment fixed across aim and formation. Thirty-nine were non-Riven; Bo Prime retained its published Riven. See the [per-weapon results and hashes](popular-search-20261118.json).

| Test | What is held constant | What it establishes |
| --- | --- | --- |
| Indexed combat | One or 25 Steel Path level-9999 Thrax, 180 s, 0/50/100% aim, three indexed seeds each | 240 full-response comparisons, 720 engagements per pass; timings are ms **per fight**, not per request. A headshot setting need not affect every weapon. |
| Four-element descent | Both geometries at 50%, three open slots, four elemental carriers and up to three legal cards from the published build, 12 s, 10 candidate runs, 12 final runs | 80 completed searches through the real leader/scorer path. All batches, orders, scores, reported builds, finalists, work and gas matched; 18,642 single and 18,506 crowd screening fights. |
| Open scope, real opening | No mod pool restriction; 180 s; first four actual candidates in each of 80 jobs | Openings range from 5 to 425 pending builds. Compared 320 submitted measurements, 704 actual fights, not 80 completed open searches. |
| Open scope, complete | Akarius Prime single and Phantasma Prime crowd; no mod pool limit; five branches, 6 s, two candidate runs, 12 final runs | Same 19,519 / 19,543 candidates and nine finalist jobs each, entire response SHA-256 unchanged. These finish naturally. They do not time the user's original three-hour job or certify a global optimum. |
| Chromium end to end | Four actual workers on loopback, four representative searches, both builds, 180 s, 10 candidate runs, 100 final runs, restricted cards | Includes worker boot, messages and finals, unlike the engine-call benchmark. The only field omitted from the answer comparison is `cpu_ms`. |

## Diagnosis

The original screenshot showed fresh progress at 134,351 fights and 27 finalists. Electricity can increase actual work through status ticks on multiple bodies; an invariant 50% headshot cliff does not explain this matrix. Examples from the fixed-input screening profiles: Dual Toxocyst single fired much more expensively at **both** 50% and 100% than at 0%; Vermisplicer crowd recorded about 144,062 DoT ticks and Bo Prime about 43,228 per fight at 50%. The expensive crowd case is not necessarily the one with the most ticks. Single-target profiles put much of their time in landing projectiles and settling hits. In crowds, scheduled status processing and queue setup also matter. No observed search stalled in a cycle; the exact exported request and hardware from the screenshot were unavailable.

The other bottleneck is search memory. Legality resolved each build's possible elemental orders and the unbounded table held on to every complete panel even after the build lost the screen. A first shared-panel prototype improved restricted tests but made an unrestricted single-target search worse. The final implementation retains small legality/score records, holds at most two generations of 512 panel slots in the order cache, and regenerates evicted orders deterministically from the same sorted subset and pinned slots. The limit applies to the cache, not live jobs, the number of builds searched, or the number of fights. Best element indices refer to this stable order.

Other shared improvements: borrow resolved orders and candidate payloads; cache leader keys and ordering metadata within a bounded request identity; re-use up to eight immutable entrants across batches of a frozen request, never its stream, combat state or RNG; trim only trailing positive zeroes and default seat counters in negotiated worker receipts. Promotion reconstructs the original dense shard. Legacy workers and the public board interface remain dense. Native one-worker batches run inline, and finite, lossless board shards fold directly in index order with the old path retained for exceptional values. Geometry only takes a single-square-root shortcut for exactly representable grid differences, and sparse division preserves signed zeros and all other operand cases. No general fast-math flags.

## Measured results

Both open mod pools used a fresh V8 instance per traversal in alternating ABBA / BAAB order. Times are medians of two traversals per version; heap is the maximum size of the WASM linear memory, not OS process RSS. The two old and two new heap readings agree within each pair.

| Whole search, 50% aim | Before | After | Speed ratio | WASM heap before / after |
| --- | ---: | ---: | ---: | ---: |
| Akarius Prime, single | 18.60 s | 6.68 s | 2.79x | 1,638 / 202 MiB |
| Phantasma Prime, crowd | 64.38 s | 56.70 s | 1.14x | 1,545 / 204 MiB |

The unchanged full-result hashes are `7fe96fab6b0523a56b9977add1374104253edd25e76ff4e125aba6330afcc1ef` and `d8ed5f83f71fca0dbdc0ca93ab714d0c183e24a06ac53adac233bf4429eada54`. Four separate, small exhaustive `wfsim-truth` controls (24 reference runs) put the unchanged search winner first in all four. Three recovered the complete reference top four; Okina Prime recovered three of four. This is a heuristic search, not a claim of exhaustive optimality.

Across the 40 short, completed searches in **each** geometry, summed leader and scorer WASM calls fell from 6.403 to 5.106 seconds (single, 1.25x) and 10.801 to 9.010 seconds (crowd, 1.20x). These are summed API times, one paired traversal per job, not wall time in a multithreaded page. Request plus response bytes fell from 50,929,242 to 30,684,634 and from 52,642,071 to 32,903,960 respectively. Each side performed exactly the same candidate measurements and work. At 50% aim a **fixed** 180 s fight gained essentially nothing on average in single-target WASM, and 1.05x in crowd WASM (1.00x / 1.01x in the generic native DLL). In other words, it is largely preparation, communication and the size of the search that improved, not a universal per-hit multiplier.

The four-worker Chrome medians of two trials per side (ABBA, seconds including worker startup and finals) were Burston Prime single 3.613 to 3.542, Dual Toxocyst single 1.945 to 1.894, Phantasma Prime crowd 4.520 to 4.412, and Okina Prime crowd 3.422 to 2.822. All ranked contenders, replays, branch provenance, counters and work were identical. Three of the four small-scope jobs barely moved; the larger solo open-scope memory wins should not be applied to every browser job.

## Simfleet comparison

The accepted Windows Simfleet `8ce96e275dc2` CPU DLL was compared to the **matching official** DLL and `95ae3fe8a95e` data pack, not the current worktree's differently versioned shipped desktop payload. Simfleet's accepted build manifest pins `646f70e637`, Rust 1.97.1, x86-64-v3, ThinLTO, PGO and its single-worker patch. A similarly named local non-PGO DLL was excluded by its SHA-256. On this 9700X, per-case medians summed over the 40 builds at 50%:

| Same-release path, one worker | Single | Crowd |
| --- | ---: | ---: |
| Official generic vs Simfleet generic, full indexed shards | 1.33x | 1.09x |
| Official generic short row vs Simfleet prepared short row | 1.71x | 1.36x |
| Simfleet generic short row vs its own prepared short row | 1.16x | 1.06x |

A short row is three single-run spans plus merge and score, not a published 1,000-run row. The opaque prepared ABI exposes final score, metric, counts and work, not every run shard. Its generic sibling was verified separately on every field. DLL-vs-DLL results are workload-specific; other clients, batch sizes or a WASM baseline are different comparisons. The current source now supports an **opt-in** Windows v3 native build without changing WFSim's ABI or deploying an older Simfleet DLL. Default desktop builds remain compatible with generic CPUs.

The newly built v3 baseline, instrumented and PGO stages used Rust 1.97.1, static CRT, ThinLTO and one CGU. The eight-pass gate selected PGO: on nine holdout rows the median ratio was 0.7407 of the **same v3 build without PGO**. All nine were faster in this gate; 29 API fixtures and the nine rows were compared before selection. Selected DLL SHA-256 `56682dc089d9a7d78c2d3442fa7976cd3535b7bd2b024138f61832a8be47ee23`; data pack `450ebd22f51005c73b96f20a10fd7c27ea049875f081c5b8b1cb461fba03b83a`. The local archive includes both, a manifest, the admission receipt and licenses. It was not published or installed over any existing client.

In a separate three-DLL alternating run on the complete popularity matrix, the selected **new WFSim** v3 DLL delivered 1.49x single / 1.27x crowd against the pre-change generic DLL at 50% aim, or 1.52x / 1.22x against the **same-source** new generic DLL. Across all three aim settings the former ratios were 1.50x / 1.28x. The final WASM-to-tuned-DLL per-fight sums were about 2.04x / 1.67x at 50%; these two runtimes were timed separately and are not an interleaved benchmark. All three actual DLLs matched the same raw WASM reference in all 240 cells, including floats, integer fields, seeds and gas. The tested ranks and work never depended on this optional native build. This is **not** a universal 3x fight speedup.

The affected Rust library suites passed 1,850 tests (four intentionally ignored), plus six build/admission tests. The native/WASM gas gate matched across independent workers, splits and folds. The chosen PGO binary and matching pack also matched WASM on the 29 API fixtures and nine scored holdout rows. The two originally reported weapons passed the 14 focused combat and two smaller end-to-end search fixtures again after the final cache change. No whole-site release CI, ranking rewrite, online volunteer work or deployment was performed.

## Reproduction

From this worktree, keep frozen old/new WASM bindgen packages (`nodejs` for `bench_popular_search`, `no-modules` for Chrome) and DLLs in distinct directories. Use the same Rust 1.97.1 and embed the same data in both generic builds. The dry browser harness uses actual workers with no upload. The snapshot and witness can be reproduced without a live popularity query using the tracked snapshot. Never run the leaderboard writer to reproduce this test.

```powershell
node scripts/popular_search_cases.mjs docs/receipts/popular-snapshot-20261118.json OLD_WASM D 40
node scripts/bench_popular_search.mjs D OLD_WASM NEW_WASM fight
node scripts/bench_popular_search.mjs D OLD_WASM NEW_WASM fleet
node scripts/bench_popular_search.mjs D OLD_WASM NEW_WASM opening
node scripts/bench_popular_search.mjs D OLD_WASM NEW_WASM whole
node scripts/bench_crowd_fleet.mjs OLD_BROWSER D/browser-cases.json D/old.json fa816d569d 4
node scripts/bench_crowd_fleet.mjs NEW_BROWSER D/browser-cases.json D/new.json working 4 D/old.json
python -B scripts/bench_popular_dll.py --fights D/fights.json --sample D/sample.json --embedded A=OLD_DLL --embedded B=NEW_DLL --require-equal A=B --threads 1 --repeats 4 --out D/dlls.json
```

For an eligible Windows x64 CPU: `python -B scripts/native_pgo.py build --windows-v3 --output target/v3/wfsim_native.dll`. The compiler target feature check runs before training; static CRT and gas metering stay enabled. The matched baseline/profiled builds use the same CPU, optimization and data options; the gate also rejects a gain that hides excessive individual regressions. This is not an automatic update or a change to WFSim release defaults. Use the matching pack and keep the generic build on other machines.
