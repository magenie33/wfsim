# Native engine migration and comparison

Preserve a frozen executable before changing calculation semantics. Run a
candidate independently against the same captured inputs. Native fixtures and
gameplay measurements decide whether a changed answer is correct; the frozen
engine identifies what changed.

## Executable snapshots

`scripts/engine_compare.py freeze` builds and copies `simulation_cost` and
`one_fight` into a new directory. Its manifest records source/data and binary
hashes, compiler versions, build options, relevant environment, commit and
working changes. Sources are checked around compilation. Each worktree uses
its own `target/`; the saved programs are independent of later build output.

```sh
python3 scripts/engine_compare.py freeze --worktree ../baseline --output target/compare/baseline
python3 scripts/engine_compare.py freeze --worktree . --output target/compare/candidate
python3 scripts/engine_compare.py run --baseline target/compare/baseline --candidate target/compare/candidate --calls tests/fixtures/engine-compare/calls.json --output target/compare/ab
```

Every destination must be new. Retain the baseline worktree, binary, manifest,
inputs and results together. Current main is a separate integration reference,
never a replacement for the frozen baseline. Snapshots report working changes;
releases require a clean candidate and [CI.md](CI.md) validation.

The runner is `webapi/examples/simulation_cost.rs`, using `route_text`
directly. It warms requests, measures them and retains complete JSON responses.
It uses no server, generated site or alternate calculation formula.

## Comparison contract

The frozen fixture is an array of `{id, path, body}` calls with unique ids.
Requests carry builds, scene inputs, random seeds and explicit combat-record
run states. Extend a suite without rewriting archived request bytes.
Optional `coverage` requirements name positive damage sources, event kinds or
the minimum combatant count, so a case cannot silently stop exercising its
declared mechanism.

The tool runs ABBA rounds and compares each side with itself and with the
other side. It verifies binary/input hashes around execution. Failed endpoints,
case identity/order changes and truncated records fail. Interrupted or failed
runs retain incomplete reports and their available output.

Every response field, including gas, participates. Object key order is ignored;
array order and number types remain significant. JSON floats compare by
binary64 bits, including signed zero. A difference reports its first semantic
JSON pointer, values and applicable float bits. Native numeric fixtures and
node traces separately cover internal binary32 boundaries absent from JSON.

Raw case timings exclude warmup and include API parsing/serialization. Process
wall/CPU time and peak RSS accompany each invocation; unsupported process
measurements are null. The report summarizes minimum, median and maximum.
These observations describe this machine and workload.

No automatic tolerance or gameplay-field ignore list is provided. A native
correction needs scoped evidence and an independent expected-value test.
Different RNG contracts need native sequence checks and explicitly scoped
statistical comparisons, rather than assuming the same integer seed means
the same draws. `one_fight save` retains its own existing workloads and binary;
measure after compilation and other comparison work finishes.

## Module boundaries

| Boundary | Current owner | Migration contract |
| --- | --- | --- |
| Request and scene | `webapi::fight::parse_fight`, `Arena` | Shared simulate, solve and replay construction |
| Resolved build | `build::loadout::resolve`, `ResolvedPanel` | Separate static results, dynamic queries and snapshots |
| Maximum Shield | `Foe::max_shield`, `scaling::shield_maximum` | Integer queries around stored level scaling |
| Magnetic source | `status::magnetic_break_source`, `magnetic_break_proxy_source` | Float32 conversion, proxy selection and normalization |
| Status applications | `fight::procs::settle_procs` | Ownership, packet counts and copied context |
| Records and groups | `fight::dot::Dot`, `DebuffState` | Seed, owner, expiry, per-type landing and reduction |
| Tick and settlement | `fight::ticks`, `fight::ledger` | Ordering, causes and source credit |
| Weapon actions | `fight::run`, `flight`, `reload` | Update phases, pending impacts and action state |
| Output | `fight::trace`, `record`, `webapi::log` | Ordered events and stable wire adapters |
| Search | `optimizer` | Score through the same simulator and replay the winner |

One state domain has one active authority per fight. Compare complete
independent programs. A reconstructed path reports unsupported dependencies
rather than silently mixing its state with a legacy approximation.

## Native fields

Source schemas retain original spelling, case, declaration owner, inheritance,
raw numeric tokens and bits. Missing, zero, empty, unsupported and ambiguous
remain distinct. Native naming allowances are local to corresponding modules;
wire names remain adapters, never additions to `naming::FROZEN`.

| Native field | Domain | Required information |
| --- | --- | --- |
| `UpgradeType` | Attribute identity | Native namespace distinct from damage and proc ids |
| `OperationType` | Fold operation | Operation and order, beyond the final multiplier |
| `Value` | Declared/rank-resolved scalar | Raw input, binary32 and rank-application stage |
| `DamageType` in an upgrade | Filter | Owning object, distinct from packet payload meaning |
| `ValidDoubleProcs` | Target candidate mask | Native enumeration and selection filters |
| `CanReproc` | Activation gate | Default provenance and current condition state |
| `ReprocTime` | Conditional timing | Update and removal semantics, including zero |
| `ModeSwitchTime` | Behavior transition | Separate reload and accepted-next-action boundaries |

Native damage, proc, upgrade and WFSim catalog ids use separate typed mappings.
Derived indices/caches remain simulator-owned. Source/provenance work stays
outside hot loops; runtime specifications carry their content identity.

## State ownership

| State | Owner | Required lifetime semantics |
| --- | --- | --- |
| Immutable declarations | Compiled specification | Client/source identity and inheritance |
| Upgrade entries | Actor or weapon instance | Install, query, activate and uninstall |
| Firing state | Weapon behavior instance | Counter, ammo, transition and reset |
| Submitted damage | Packet and parent event | Source, scalar/vector, counters, part, proxy and position |
| Status application | Receiving handler record | Seed, copied context, expiry and ownership |
| Group reduction | Target/type/source-dependent group | Key, visitation, accumulator and retained/overwritten fields |
| Conditional timer | Activated condition instance | Active operation, effective dt and cleanup |
| RNG | Recovered stream owner | Comparisons, state, skipped draws and consumption order |

The current Primary Debilitate implementation splits inside `settle_procs`'s
proc loop. Its replacement boundary is one accepted callback and one fresh
packet with per-type presence checks. Proc count cannot define callback count.
The current Encumber path's mutable `encumber_done` batch flag cannot express
arbitrary condition updates between arrivals; its replacement owns weapon
upgrade state and synchronous suppression installation/removal.

These are representation boundaries. They do not establish missing live
notification cadence or authorize changing measured values. Unanswered native
questions stay in the shared research queue.

## Coverage and dependency register

| Domain | Available foundation | Remaining migration dependency |
| --- | --- | --- |
| Upgrade queries | Scalar build effects and bounded Shield consumers | Native operation/filter entries and dynamic installation ownership |
| Numeric precision | Float32 Shield/source/proxy and speed fixtures | Per-consumer boundary map beyond recovered slices |
| Packet context | `InstanceScale`, status vectors, seat and cause | Native packet identity, counted procs, part/proxy and fresh-copy semantics |
| Electricity and Gas | Per-record seeds and separate latest landings | Complete native insertion, recipient, expiry and lethal lifecycles |
| Conditional Arcanes | Existing proc handlers and configured opening state | Callback unit, operation installation and effective update phase |
| Weapon behaviors | Shared action scheduler, pending flight and battery state | Complete transition/reset contract and native ordering by behavior |
| Spatial consumers | Planar `Arena`, chain, radial and flight primitives | Versioned spatial policy; recovered contact/cover and target-selection branches |
| Enemy controllers | `Foe`, scaling curves and per-body `TargetState` | Controller instance/query/writeback lifecycle and installed mode operations |
| Abilities and passives | `data::casting`, active ability windows and handlers | Instance identity, script branch/side-effect coverage and shared upgrades |
| Companions | Source importers and multi-combatant fight | Complete AI/action/lifecycle scope per recovered mechanism |
| RNG and replay | Engine deterministic streams and captured run state | Native stream installation, consumer sequence and owned lifetimes |
| Build and elements | Shared resolver, forms and configured mod order | Preserve native query sites instead of flattening dynamic properties |
| Search and outputs | Same simulator, record and route transport | Versioned new-engine identity, caller adaptation and final truth/parity checks |

Each mechanism records declaration, consumer, lifecycle, implementation and
verification scope separately. A parsed roster or passing output comparison
does not establish native lifecycle coverage. Implementation dependencies
belong here; unanswered source questions retain their shared queue ids.

## First numeric slice

Reuse the Shield-query implementation and its bounded fixtures. Attribute
integration must retain independently visible nodes:

| Node under the stated fixture installation | Value | Consumer |
| --- | ---: | --- |
| Base Shield | 120 | `Foe::base_shield` |
| Query before level scaling | 190 | `scaling::shield_query` |
| Integer level writeback | 12116 | `scaling::shield_maximum` |
| Later maximum query | 19157 | `Foe::max_shield` |
| Source truncation | 957 | `status::magnetic_break_source` |
| Proxy-adjusted record | 1914 | `status::magnetic_break_proxy_source` |
| Electric accumulator | 1915 | `Dot` and tick reducer |
| Ordinary coefficient | 957.5 | Tick evaluation |
| Recipient integer rounding | 958 | Native control and matching gameplay observation |

`tests/fixtures/client/shield-query-2026-10-28.json` includes installation and
cross-level variants. Retain its declared scope and owner-approved installer
assumption. Matching the pool does not prove the complete installer, recipient
or display lifecycle.

The first integration touches the pure attribute-query primitive in `rules`,
original operation fields in `model`, source compilation in `data` and existing
Shield consumers. Keep explicit caller adapters and add diagnostic node traces
whose absence changes neither answers nor random consumption.

Exit requires original-byte node agreement, matching gameplay controls, exact
unchanged-path comparisons and measured additional query cost. Folding the
queries together or changing truncation to rounding must fail a real check.
This primitive needs no search enumeration or scheduling change.
