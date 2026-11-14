# Native calculation bindings

The primary implementation contract is the immutable Drive batch
`20261114T152215Z-native-calculation-refactor-spec`, manifest SHA-256
`71521af8e0a0ea0fbc683b9f98f6669879623dd352df3cdefd23e9fd7b570b1f`.
Its 46 stages identify calculation boundaries, including unbound interfaces.
A recovered numeric kernel does not establish its live producer or filters.

Original `UpgradeType`, `OperationType`, `Value`, `FusionPowerStep` and
`FusionPowerCurve` spellings are retained in mod input records. Numeric stat,
damage, proc and part identifiers occupy distinct namespaces. `NativeScalar`,
`CriticalInputs`, the Rust packet types and `native_consumer` tags are engineering
interfaces, not recovered C++ class or member names. Offset-bound opaque fields
must retain their source offsets until their meanings are established.

`tests/fixtures/client/native-consumers.json` maps recovered stat identities to
Rust consumers. `scripts/client_upgrade_changes.py` joins changed exact-owner
inputs and ancestry to mod files and native attribute consumers. Unknown fields,
removed members, native code hashes and unlinked changes remain in its output;
none are declared irrelevant or automatically accepted.

A production resolved panel carries pending critical queries into its direct,
radial and lingering parts. `critical_inputs: Some` is authoritative; numeric
critical fields are its display projections. `None` selects the explicitly
supplied resolved-stat adapter used by synthetic fixtures and caller-created
profiles. A supplied chance override preserves the independently bound damage query;
fixing a measurement fixture’s tier must not bypass its native damage path.
Such a profile is not evidence of a native installer. Weakpoint grant
selection and other unbound eligibility decisions retain the scene model.

| Stage | Connected implementation | Remaining boundary |
| --- | --- | --- |
| ALN-ID-01 | Exact internal-path joins in client importers; per-form data | Complete behavior/action/component and ancestry installation |
| ALN-ID-02 | Raw native rank inputs; binary32 attribute, critical and speed kernels | Other scalar widths and upstream input precision |
| ALN-ID-03 | Verified whole-field source composition | Native defaults and partial-struct inheritance |
| ALN-UP-01 | `rules/upgrades::rank_value`; unlock checks; 150 mod bindings | Unsupported entry classes, Riven rolls and custom overrides |
| ALN-UP-02 | Eight operation buckets; pending/materialized scalar; proc-mask predicate | Full owner/type/symbol/posture filters and operation traversal order |
| ALN-UP-03 | Conditional chance/duration/u8 stack construction; Encumber inputs | Other event installers and script-specific overrides |
| ALN-UP-04 | Typed packet source/weapon handles; selected queries | All equipped-operation owners and eligibility |
| ALN-DMG-01 | Source-total WEAPON_DAMAGE_AMOUNT query; existing component projection | Native component/element construction and source canonicalization |
| ALN-DMG-02 | Shared simulation CO/Compression; independent CO record bracket | Native per-route bases, applicability and operation order |
| ALN-CRIT-01 | Native LCG and ordinary tier roll; nonpositive/integer draw rules | Global stream installation and special forced/promotion branches |
| ALN-CRIT-02 | Binary32 canonical critical quantizer/factor; pending hit-time queries | Complete upstream installer and quantizer pending-operation branches |
| ALN-RECIP-01 | Bound Magnetic proxy selection and part/copy primitives | Live proxy-list installation and special classifiers |
| ALN-RECIP-02 | Existing shared recipient simulation | Complete native armor/resistance/attenuation dispatch |
| ALN-RECIP-03 | Existing measured pool/floor policy | Native per-pool integer, gate, overflow and setter dispatch |
| ALN-NPC-01 | Native stored/effective Shield max queries and writeback | All live level-table overrides and other pool writers |
| ALN-NPC-02 | Existing mode policies; bounded Shield aura query | Complete mode operation installation timing |
| ALN-NPC-03 | Bound Overguard getter and source vectors | All Eximus/Thrax variants and live spawn installation |
| ALN-MAG-01 | Native pool/count snapshot multiply and truncation | Complete live ownership and mode installation |
| ALN-MAG-02 | Binary32 normalization/rounding and generic proxy multiplier | Additional eligible weakpoint operation and proxy-state writers |
| ALN-MAG-03 | Fresh parent/forced child context; typed origin and counters | Actual pending scalar/filtered query path through status records |
| ALN-PROC-01 | Native counted payload primitives; forced coalescing at arcane submission | Ordinary SC/type selection, multiplicity and immunity dispatch |
| ALN-PROC-02 | Existing per-family clocks/caps; native Encumber suppression input | Native ordinary insertion/expiry/refresh/decay state |
| ALN-PROC-03 | Existing measured Electric/Gas simulation | Raw retained seeds, four opaque key inputs, visitation and radial preparation |
| ALN-PROC-04 | Per-family measured landing retention | Complete native application, moving origin and expiry |
| ALN-EVT-01 | Primary one-callback type loop, positive counter presence, fresh child | Native notification cadence, beam submission and full VM dispatch |
| ALN-EVT-02 | Encumber native sampler, SET0 suppression and positive effective update | Earlier eligibility, resource loading and ancillary owner refresh |
| ALN-EVT-03 | Existing kill/weakpoint trigger attribution | Native lethal event and ownership dispatch |
| ALN-TIME-01 | Bound RPM/refire, reload and generic charge binary32 consumers | All action routes and dynamic query input installation |
| ALN-TIME-02 | Native continuous fields; two FIRE updates before ordinary refire query | Mode input, holster and full empty/partial battery lifecycle |
| ALN-BEAM-01 | Existing shared beam ramp/cache simulation | Native cache membership, first tick and emission cadence |
| ALN-SPREAD-01 | Existing scene spread and aim simulation | Native channel update, smoothing and movement writers |
| ALN-SPREAD-02 | Separate scene streams and supplied multishot model | Native spread u32 generator, draw ownership and emission multiplicity |
| ALN-PHYS-01 | Bound projectile speed clamp; shared scene collision/piercing | Native world collision, homing and general punchthrough |
| ALN-PHYS-02 | Shared scene bounce/blast/radius/falloff | Complete native cover and blast branches |
| ALN-AMMO-01 | Shared ammo, battery, gauge and transform simulation | Complete native lifecycle and action-state transitions |
| ALN-MELEE-01 | Shared data-driven stance/combo/heavy simulation | Native per-stance coefficients, permissions and finishers |
| ALN-MELEE-02 | Shared jump/slam/recovery schedule | Native ascent, collision and recovery consumers |
| ALN-ABILITY-01 | Shared Warframe/ability/extra-hit model | Every native installer, override and acceptance boundary |
| ALN-COMP-01 | Shared companion attack/Swipe/action simulation | Native special attacks, sentinel aim and live cadence |
| ALN-RIVEN-01 | Existing Riven/evolution/modular construction | Native roll generation and complete composition/renaming |
| ALN-DROP-01 | Existing kill reward inputs used by buffs | Complete native death rewards, loot and orb probabilities |
| ALN-MAINT-01 | Pinned imports, per-field diff and consumer registry | Rebinding changed code and uncaptured client dependencies |
| ALN-BUFF-01 | Selected native attribute folds; existing shared buff model | Full per-query contribution ownership and receiving order |
| ALN-PROC-05 | Existing measured Slash/Heat/Toxin/Blast family behavior | Native handler-specific seed inheritance and negative-duration timing |
| ALN-PROC-06 | Existing debuff/counter/vulnerability/control simulation | Native effect magnitude, icon and receiving order |
| ALN-RECOIL-01 | Existing scene recoil/aim policy | Native spring, camera, compensation and world coupling |

The combat record uses the actual settled raw damage. Its explanatory layers
remain views of the scene model. Complete per-node native traces, including
matched operations, materialization, RNG transitions, record/group identities
and event phase, require the corresponding live boundary. Final damage equality
alone cannot certify these stages.
