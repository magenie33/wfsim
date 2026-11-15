# Native integration dependencies and 40m browser evidence

Calculation tip: `014e2ee3c7fa47f5691e5cf210e4ae3f10c1e656`.
Qualified delivery tip: `00367efa4e2562a3613db70ee5b4effb05c9cb25`.
Original base: `9a8c30a2f793cc6d449425d1e57deaae2cae2d48`.

P0 `16002f6c3dea127464afcbdb1518041c93d28039` is already represented on
observed main `646f70e637d1eda1bbafaa2d080de4bfa7f5d5e7`. Its comparison
runner, runner self-check and original request fixture have byte-identical
Git blobs on that main. The existing P0 release/CI is a separate qualification;
it does not qualify this runtime or an eventual combined candidate.

## Minimum source dependency order

The following sequence contains every commit changing the calculation crates,
game-data schema or their native-oracle fixtures after P0. A blob-map replay
from P0, checking every patch's exact prior blob, reconstructs all5525 scoped
files at delivery tip with zero differences. The scope is `engine/`,
`optimizer/`, `webapi/`, `wasm/`, `cli/`, `data/`, native client fixtures and
the Cargo files. This is a source dependency proof, not a build or combined CI.

| order | full SHA | dependency supplied |
| --- | --- | --- |
| 1 | `a012391d5e305643885a4c556e4773ba762fba0e` | Native operations, counted packets, conditional upgrades and Arcane consumers |
| 2 | `de1460b406990f0baaef8124a5829d99c08851ea` | Rank inputs, native query operations, schema/data bindings and consumer propagation |
| 3 | `540eb587d0fe08c849237c8369c25acf5045dffe` | Native critical/query boundary and supplied-critical preservation |
| 4 | `1c2647db21f248c197a05f352511daa6015f3660` | Retained status input, source/recipient queries and radial reduction |
| 5 | `94b62a5d0ceda4832206521ac28315c031e3d748` | Equivalent capped-status scans and their regression gate |
| 6 | `32686c89efad3c06d1ee95fea1b2706f5ad094c4` | NumHits, Irradiate notification/merge payload and real fight routing |
| 7 | `e9480ee27419fc24d1ed651948bd87e1b75efb88` | Independent Primary source-oracle fixture and actual packet/callback regressions |
| 8 | `cf84a1b1b6eafb8e8932116c7ef1d3f838ead378` | Ordinary evaluator/chance/CDF/counter kernel and real consumers |
| 9 | `014e2ee3c7fa47f5691e5cf210e4ae3f10c1e656` | Equivalent immutable-prefix and selected-default fast paths |

Order7 is a source-regression dependency, rather than a runtime behavior
change. The eight calculation changes alone are not the complete review and
qualification delivery. Whole-commit cherry-picks also carry documentation
and tooling contexts; this core-scope proof is not a promise that pruning the
other commits yields a clean or already-qualified cherry-pick candidate.

## Complete qualification envelope

The retained complete delivery is the following ordered range after the
already-landed P0. It includes the supporting data-apart runner, allocation
probe, reproducible requests, source contracts and receipts. Integrate that
scope deliberately; a standalone00367 cherry-pick supplies only the receipt.
Preserve every dependency branch and the8831 preview until owner disposal.

```text
a012391d5e305643885a4c556e4773ba762fba0e
f6f029ee67fa8387c6056f72274b1111366e7d66
431d09f2ef8b92e743774ed73b6614a5070251c0
de1460b406990f0baaef8124a5829d99c08851ea
540eb587d0fe08c849237c8369c25acf5045dffe
9d7e418ed05da9340bdb1745743f2a1f36a74b13
1c2647db21f248c197a05f352511daa6015f3660
8499227c4093f232500bf5539630453487c1fffd
fcae701797f39f46031756346544a4db36c8899a
94b62a5d0ceda4832206521ac28315c031e3d748
66bf35238521dad46749890ea6f786fae337eaae
32686c89efad3c06d1ee95fea1b2706f5ad094c4
fdf0107364509b4e15b9ad0ef6a244ee6e66f2ae
e9480ee27419fc24d1ed651948bd87e1b75efb88
c7cec7ae845517318478b2144a5b8449579e13e1
cf84a1b1b6eafb8e8932116c7ef1d3f838ead378
014e2ee3c7fa47f5691e5cf210e4ae3f10c1e656
00367efa4e2562a3613db70ee5b4effb05c9cb25
```

## Each of the three browser failures

The old check's three failures share one missing precondition: its captured
40m request has `aiming: true`, with zero misses in that selected run. It has
real shots and damage; `kind === 'hit'` is not a wire event kind, so the old
diagnostic's zero-pellet count does not establish that nothing fired.
The captured UI read contains53 events. Its requests use1000 runs even though
the old probe assigns `sim.runs = 8`; `simRuns()` reads the canonical run
setting instead. Polling an already-existing record container also does not
establish that a new record finished loading. Main's repair addresses those
probe/input synchronization defects without changing this engine.

| old assertion | observed failed predicate | baseline/final failure log |
| --- | --- | --- |
| `a wide shot MISSES, and the record says so` | `m.misses > 0` is false | baseline line20; final line20 |
| `...each with a reason` | The same `m.misses > 0` guard is false; this does not prove an empty reason on an actual miss | baseline line21; final line21 |
| `...and "only the misses" is a view` | `m.drawn > 0` is false because the selected run has no misses | baseline line23; final line23 |

The baseline log is the1c calculation
`1c2647db21f248c197a05f352511daa6015f3660`; the final old-probe log is the014e
calculation. Both report36 passing and the same three failures. Original9a8c
and540 are additionally witnessed by exact captured-request engine replays,
rather than falsely labelled as separate full browser runs.

Durable logs:
[baseline1c](native-integration-20261116/baseline-1c-browser.log),
[final with old probe](native-integration-20261116/final-old-probe-browser.log),
[final with current-main probe](native-integration-20261116/final-current-main-probe-browser.log).
Every raw file has its byte count and SHA-256 in the
[evidence index](native-integration-20261116/evidence.json).
The two failing-log text views remove end-of-line spaces. Their original
bytes are retained as base64 with separate raw sizes/SHA-256 in that index;
the displayed views also have their own hashes. No failure text or raw
observation is discarded to satisfy the whitespace check.

## Same-request controls and final attribution

The captured request fixes Laetum, its evolutions/mods, Crewman level1,
8seconds,40m, the1000-run-selected state `[2149058048,2809998059]` and record
limit300. The controlled second request changes only `aiming` to false.
All six snapshots pass binary-hash checks and return untruncated records.

| calculation SHA | captured aimed misses | hip-only control misses | durable response |
| --- | ---: | ---: | --- |
| `9a8c30a2f793cc6d449425d1e57deaae2cae2d48` | 0 | 27 | [engine0](native-integration-20261116/engine-0.json) |
| `540eb587d0fe08c849237c8369c25acf5045dffe` | 0 | 15 | [engine1](native-integration-20261116/engine-1.json) |
| `1c2647db21f248c197a05f352511daa6015f3660` | 0 | 15 | [engine2](native-integration-20261116/engine-2.json) |
| `94b62a5d0ceda4832206521ac28315c031e3d748` | 0 | 15 | [engine3](native-integration-20261116/engine-3.json) |
| `32686c89efad3c06d1ee95fea1b2706f5ad094c4` | 0 | 15 | [engine4](native-integration-20261116/engine-4.json) |
| `014e2ee3c7fa47f5691e5cf210e4ae3f10c1e656` | 0 | 15 | [engine5](native-integration-20261116/engine-5.json) |

The326 snapshot was captured with Git HEAD66bf and working changes; its entire
compiled source-file map matches326 byte-for-byte. Its manifest HEAD and source
hash are both retained, rather than treating the earlier HEAD as its runtime.

Both aimed and hip request responses are strictly identical between326 and
final014e, including every event and number. N07A introduces no difference in
these probes. The27→15 old/native hip difference is already present at540;
this bounded control does not isolate every earlier RNG/engagement change.
It establishes that actual miss serialization is present on every baseline.

Main already contains the probe repair
`288e47e773b4f12cffa3e934b30b0b730b7762f4`: it sets hip fire, uses the real run
setter, awaits simulation/record loading, checks record identity and counts
shots instead of nonexistent hit events. The current-main checker, source
blob `9f65c0e4b51648e5abb139c922123db6fa7dbc2f`, passes39/39 against the final
native preview. It observes52 events,16 shots,20 misses with nonempty reasons,
correct shot causes and20 displayed miss rows; loading is false, error is null
and record identity matches. This freshly selected UI run is separate from
the fixed-run API matrix, so its20 is not a version delta against15.

Keep main's existing probe/UI repair when integrating; the native feature
branch does not change `check_combat_record.mjs`. The old three failures are
not waived, and the corrected positive probe is green. This focused check
does not replace the release owner's combined candidate CI.

## Ownership and open boundaries

No full CI, main push, production/DB action or166/engine transition ran here.
The release owner must validate the combined core candidate and coordinate
the original166/engine boundary with the original DB operator before any
switch. P0's release and CI cannot supply that validation.

Actual Phantasma beam-to-callback cadence, runtime handler installation and
full-game parity remain unclosed. The handoff establishes dependencies and
probe causality; it does not broaden the bounded source qualification.
