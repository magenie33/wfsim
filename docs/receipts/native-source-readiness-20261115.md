# Native source sufficiency review

Batch `20261115T065558Z-ordinary-proc-generation` supplies enough evidence to
implement and qualify N07A's bounded ordinary proc-generation kernel. Delivery
and implementation are separate: the candidate still uses the earlier count
and type sampler in `engine/src/rules/status.rs`.

Manifest SHA-256:
`ea4b3815f48d45b79fb63f5fc1af573ee008d2532049778db15852439382a1ec`.
READY, all eight outer files and all 223 evidence archive members pass byte-size
and SHA-256 checks. The matching executable is build 2026.10.28.09.54, SHA-256
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.
Evidence contains nine weight vectors, 1895 sampling vectors, 60 selector
predicate vectors and one enum-initializer control. Coverage records 1066
original executed instruction addresses and no function-return adapters.

| contract | available evidence | candidate integration status |
| --- | --- | --- |
| Primary compound presence, one selection per qualifying type, forced-counter coalescing | Parsed callback and original native helper vectors | Implemented; source-linked tests retained |
| Ordinary chance/type loop and RNG writes | Original `0x310c60` execution and sampling traces | Replace floor-plus-fraction draw scheduling |
| Evaluator selectors and handler maximum | Original evaluator, constructor, handler and selector predicates | Add native graph/maximum/F32-fold interface; flat vector sampling does not express it |
| Late target proc-chance query | Nine target-stat-117-to-original-sampler controls | Connect through existing native operation buckets; do not duplicate the modifier |
| Per-type generated counters and NumHits | Preserved counters, saturation and NumHits 1/2/20 controls | Keep generated counters and later status application distinct |
| Actual Phantasma submissions and accepted Primary callbacks | Explicitly outside this batch's recovery | Still EG-01-M3/M4; a status total does not determine callback count |

Every attempted ordinary unit consumes a chance draw, including guaranteed
units. Each successful attempt consumes a second type draw. At effective
chance 2 with positive weights the native loop consumes four draws; the
candidate's floor-plus-fraction scheme consumes three. It is therefore an
implementation gap, not a missing-algorithm excuse.

Native handler weights are maxima of evaluator outputs. Evaluators sum matching
positive components in native order with separate F32 operations, then apply
WeightMultiplier. Type sampling uses replacement and preserves existing byte
counters. Final immunity is a later stage. The candidate's single immunity
list and early weight filtering do not represent the separate supplied handler
graph and final immunity mask; these interfaces need to be distinguished.

The data is sufficient for the kernel's numeric operations, selection order,
counter changes and bounded late query. The runtime graph remains a supplied
input. The thirteen default evaluator declarations do not justify installing
that graph on every enemy. Upstream effective-chance construction, the scanned
source branch, live handler installation, complete recipient acceptance and
weapon-to-challenge notification multiplicity remain outside this recovery.

The batch explicitly states that it establishes no Phantasma callback-count
conclusion or universal per-shot roll count. This limitation must remain
separate from the fully specified per-callback Primary contract.

The additional Tonfa delivery `20261115T060206Z-tonfa-source-projection`
provides 49 sampled source clips and an offline inspector. Its own contract
retains nine unsupported cinematic resources, an unclosed deserializer/runtime
descriptor bridge and an uncertified final collision mask. It is supporting
melee evidence, not closure of the Phantasma or full-engine runtime questions.

Source review and raw verified evidence summaries are retained in
`target/source-review-1115/`. This review changes no runtime formulas, golden
measurements, research queue or production channel. N07A should proceed from
the supplied native vectors; a complete-engine safety claim requires actual
integration and the remaining binding evidence, not merely the delivery file.
