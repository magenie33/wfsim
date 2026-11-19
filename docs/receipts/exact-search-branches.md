# Exact search branches, Riven stats and slot gains

The incremental baseline is `46d8e72e19`, **after** the [popular-weapon and native changes](popular-search-20261118.md). Results below are not additive with earlier speedups. The aim is fewer redundant branches, not fewer distinct contenders or fewer runs per measurement.

## Search

- Ordinary element cards can share a partition only when the search proves the primary damage vector is the same. The first representative in the original DFS order still supplies the complete panel, even when other parts of a discarded panel could differ. Across 0/1/2/3/4 distinct element groups, primary panel resolutions per legal Exilus choice are `1/1/2/6/24` before, `1/1/1/3/3` after. Four ordinary groups took 8.04 to 2.46 ms per 128 expansions in the focused local measurement. This is not an end-to-end solver ratio. Pinned physical slots, multi-element Rivens, out-of-contract modifiers and exceptional values use the existing path. Second-form resolutions, logical enumeration counters and candidates stay the same.
- The descent now passes the same first-occurrence keys through legality, screening, scoring and the finalist pool. It no longer scans an already unique batch quadratically to discover duplicates. An ordinary unpinned result's identity does not require rebuilding a resolved panel; pinned results still consult the selected physical layout.
- The frozen top-40 matrix (single and 25-body targets, 50% headshots) returned identical ordered batches, submissions, work and full search results for all **80** restricted searches, plus identical measured openings for 80 unrestricted requests. The matched-compact run executed 18,642 and 18,506 screening fights in the two geometries in both versions. Summed leader/scorer calls on this run: single 5.72 to 5.51 s, crowd 9.90 to 9.78 s. These are one paired traversal per job; a fight-heavy scenario need not show a measurable wall-time gain. The 240 fixed-fight DLL calls matched the frozen raw WASM reference.
- The complete open-scope controls retained the previous response hashes and 19,519 / 19,543 candidates. Akarius Prime single took 7.80 to 7.27 s. Phantasma Prime crowd showed no repeatable *incremental* speedup: the first two-trial medians were 52.88 to 56.22 s and a second, reversed-order set was 63.61 to 52.72 s. Its final WASM memory was 202.6 MiB, down from 204.3 MiB. Earlier memory savings against upstream are measured in the separate receipt.

## Rivens

- The live stat editor computes the entered roll or the inverse of the displayed value. It does not search rolls. Illegal and unconfirmed diagnostics now share one derivation of the family's available stats instead of two. For 9,360 card responses, including forward values and inversions at five rolls, the entire WASM responses were byte-identical. Four frozen appraisal requests also matched. Nine complete, fixed-seed four-element appraisals across Braton Prime, Synoid Gammacor and Torid matched byte-for-byte, including pinned physical layouts, reversed input stats, ranking and work. Their card pools and durations were restricted for repeatability.
- The published Riven Analyst view uses its existing rankings when only the selected row changes. The reference and candidate produced identical HTML on 30 renders of three real board lists; 15 depth traversals and 15 ranking calls were avoided. New data or depth invalidates the cache. These are UI operations, not fights saved.
- The offline `riven_shapes` survey rejects a second spliced positive before constructing its descendants. It renders the legal positive tuple once for all allowed negatives. This changes only card generation: both sizes, every admitted card, the original order, no-negative options and all five status-duration negative rolls remain. No change to the number of independent appraisals a full survey ultimately asks for.
- Default Torid positive leaves: `2,600 -> 1,610`, with the same 10,045 emitted cards. Ocucor: `3,654 -> 2,499`, same 15,878 cards. The positive JSON fragment is formatted 1,610 / 2,499 times rather than once per emitted card. All 13 CLI checks matched stdout, stderr and exit status, including refused/duplicate arguments and empty results.

## MOD gains

The cold scan still measures every **distinct** candidate and uses the selected run count. Only identical ordered override payloads on the same full baseline can share a measurement; no sorting of slots, rank stripping or merging of distinct generated Rivens. The candidate list and the results have separate page-local bounds. Refusals are reported but not cached; after cancellation a manual retry reuses only completed successful work. Restart drops both caches, and a repaint cannot reverse a cancel.

Twelve visits to real mod slots on Torid, Ocucor and Magistar yielded the same 1,044 ordered rows, gain values and error bands. Comparing the old UI **and** WASM against the new UI **and** WASM with the same data: candidate list requests `12 -> 9`, simulation calls `1,056 -> 525`, individual runs `10,560 -> 5,250`. This workload includes revisits and equivalent empty slots. A changed element order correctly required new candidate fights. Separate Rust differential tests compared the complete candidate lists on ten slots for each of five weapons; browser checks exercise scenario changes, cancellation, supersession and a different waiting axis.

## Reproduce and extend

Build each revision with the same compiler/data, saving nodejs (`OLD_NODE` / `NEW_NODE`) and no-modules (`OLD_BROWSER` / `NEW_BROWSER`) bindgen packages to distinct directories. Set `BENCH_MATCHED_COMPACT=1` for two revisions which both support compact worker receipts. No hosted jobs are required.

```text
cargo test -p wfsim-optimizer time_legacy_and_classes -- --ignored --nocapture
node scripts/check_search_branch_parity.mjs OLD_NODE NEW_NODE
node scripts/check_riven_branch_wasm.mjs OLD_NODE NEW_NODE
python scripts/check_riven_branch_native.py OLD_EMBEDDED_DLL NEW_EMBEDDED_DLL
node scripts/check_riven_branch_analyst.mjs
node scripts/check_riven_branch_shapes.mjs
node scripts/check_gain_branch.mjs --before 46d8e72e19 --baseline-wasm OLD_BROWSER --wasm NEW_BROWSER --data-pack DATA_PACK
```

Profile cold candidate fights and status-heavy crowds separately. Extend an element equivalence class only with a proof for the shared vector and an ordered differential test; never drop an evolution, a Riven stat, a negative roll or a candidate because its score happened to match on a sample. Cross-session gain reuse would additionally need an engine and data generation identity.
