# Computation performance

Use this procedure for engine cost, request preparation, native compilation and
interfaces around simulation. Search enumeration, scheduling and scoring also
require [optimizer accuracy validation](OPTIMIZER.md). Build commands, prepared
simulations and native PGO are documented in [DEVELOPMENT.md](DEVELOPMENT.md).

## Locate the cost

Separate request parsing and build resolution, per-run combat, shard validation
and folding, JSON conversion, worker transport and scheduling. Measure the layer
the proposal changes. A faster inner fight does not establish the same gain for
an API call, a desktop round trip or fleet throughput.

Profile several shapes: direct hits, fields and chains, dense targets, status
ticks, casting, transformations, melee and multiple combatants. Inclusive sample
shares overlap; do not add them or treat them as independent savings. Measure
native and actual wasm execution separately.

## Candidate directions

| Direction | Preserve |
| --- | --- |
| Reuse immutable request preparation | The complete build and fight, including riven rolls, modes, assembly, buffs and combatants; independent mutable state and RNG per run |
| Borrow catalog metadata and reuse views | Catalog ownership and bounded keys; request-derived names or rolls must not accumulate in permanent storage |
| Reduce allocations in hit, field and event paths | Event order, target selection, active lifetimes and bounded retained capacity |
| Reuse ability or faction context | Only values invariant over the chosen interval; casts, stacks, procs and transformations still update on their actual events |
| Inline or specialize measured hot paths | Exact arithmetic and every supported shape; code size and wasm performance remain part of the comparison |
| Prepare immutable execution plans | Shared engine rules, complete identity and request ownership; no second damage model or simplified fight |
| Tune native compilation | Target-host execution, representative training and held-out validation; portable release requirements |
| Reduce interface overhead | Indexed runs, one shard per run, ordered merging, ownership, cancellation, progress and bounded memory |

Inspect existing `PreparedSimulation`, catalog borrowing and memoization before
adding another cache. A cached value needs a complete key and a lifetime rule.
Do not share mutable run results or freeze a value that changes during combat.

## Compare the same work

Record both source revisions. Hold data, toolchain, flags, allocator, requests,
seeds, run indices, run counts, engagement durations and target geometry fixed
except for the factor under test. Keep a baseline binary. Change one factor at
a time; compare interacting changes with separate variants.
Use warmups and alternating ABBA runs, retain individual timings and inspect
each workload alongside an aggregate.

Train PGO on different requests from the validation set. Generate fresh profiles
for the source and native target being built. Use the pinned compiler's LLVM
tools and the [Rust PGO workflow](https://doc.rust-lang.org/rustc/profile-guided-optimization.html).
The official builder owns its selection and fallback thresholds; see
`scripts/native_pgo.py` and DEVELOPMENT.md rather than copying those constants.

For interface work, include useful batch sizes as well as single-run calls.
Retaining preparation across batches and reducing JSON traffic are separate
effects. A trial that combines them must report both in its scope. Include the
existing prepared batch route as the baseline.

Record CPU, target, complete build options, source and artifact hashes, sample
set, timing distribution and regressions. Distinguish cost reduction from
throughput increase. Wasm linear memory is not process RSS, and a local timing
is not evidence about another CPU, platform, client or the fleet.

## Preserve the answer

Compare complete API responses, scores, counters, gas and work under the wire's
number semantics. Native and wasm must agree. Preserve run indices, RNG use,
per-run validation and merge order; regrouping floating-point sums can move bits.
Handle non-finite values, signed zero and integers beyond the wire's exact range
explicitly. An interface that declines such input must use an established path.

Cover partitioned, reversed and parallel delivery, fresh and reused instances,
same-name rivens with different rolls, casts, cycles, melee and combatants. For
an interface change, exercise incomplete work, duplicate and invalid ranges,
cancellation, stale handles and releases during active calls.

Run the affected checks and prove a new gate rejects a deliberately broken
input. Search changes require `wfsim-truth`; mechanics changes retain their
in-game measurement and golden requirements. Never change a golden value to
make a performance patch pass.

Performance changes do not select new contribution prices. Metering and pricing
follow [WASM.md](WASM.md#gas) and [BOARD.md](BOARD.md); a price-policy change is a
separate task. Preserve the client contract and update it when obligations change.

## Carry the result forward

Prefer changes with repeatable benefit across representative work and a clear
ownership rule. A new ABI needs its own compatibility and lifecycle review;
compiler tuning does not require a new client protocol. Borrowed ideas and
reused implementation are different provenance: record source references, and
check licensing before incorporating external code or artifacts.

Keep reusable scripts and compact receipts in Git. Keep bulky regenerable
results and profiles with the task artifacts. State what was measured, what is
still unverified and which checks remain for the combined release. A retained
proposal or benchmark is evidence for a future task, not permission to execute it.
