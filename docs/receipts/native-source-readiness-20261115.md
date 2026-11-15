# Native source sufficiency review

Batch `20261115T065558Z-ordinary-proc-generation` supplies enough evidence to
implement and qualify N07A's bounded ordinary proc-generation kernel.
`engine/src/rules/status/ordinary.rs` implements the source loop, evaluator
graph and target117 query. Actual fight consumers use that loop through the
scene adapter in `engine/src/rules/status.rs`.

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
| Ordinary chance/type loop and RNG writes | Original `0x310c60` execution and sampling traces | Native inclusive chance/CDF loop and lazy evaluation |
| Evaluator selectors and handler maximum | Original evaluator, constructor, handler and selector predicates | Supplied graph/maximum/F32-fold interface |
| Late target proc-chance query | Nine target-stat-117-to-original-sampler controls | Existing native operation buckets; selected Tau entries use the query |
| Per-type generated counters and NumHits | Preserved counters, saturation and NumHits 1/2/20 controls | Raw counters survive final immunity; NumHits is independent |
| Actual Phantasma submissions and accepted Primary callbacks | Explicitly outside this batch's recovery | Still EG-01-M3/M4; a status total does not determine callback count |

Every attempted ordinary unit consumes a chance draw, including guaranteed
units. Each successful attempt consumes a second type draw. At effective
chance 2 with positive weights the loop consumes four draws. Native sequence
checks cover integer attempts, exact ties, zero weights and saturated counters.

Native handler weights are maxima of evaluator outputs. Evaluators sum matching
positive components in native order with separate F32 operations, then apply
WeightMultiplier. Type sampling uses replacement and preserves existing byte
counters. Final immunity is a later stage. The supplied handler graph and final
immunity mask are separate inputs; the fight filters accepted applications
after generating raw counters. Explicit handler selections override the scene
eligibility fallback. Unbound enemies retain wiki eligibility; default13 is
not installed universally. Void/Tau alternatives remain scene policies with
no guessed native proc ids. Native per-enemy handler installation remains open.

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
`target/source-review-1115/`. The durable fixture is regenerated with
`scripts/import_ordinary_proc_contract.py --exchange <batch> --check`.
Native sequence agreement closes this bounded kernel. A complete-engine safety
claim still requires the remaining binding evidence and gameplay measurements.
