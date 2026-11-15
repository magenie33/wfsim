# Primary Debilitate mechanism audit

The recovered client callback is the mechanics reference. Existing WFSim
output is a version comparison, not independent evidence that a rule is right.
Where no in-game measurement contradicts it, preserve the source-backed
callback instead of restoring an older simulator's repeated splitting.
[Callback contract](../PRIMARY_DEBILITATE.md),
[machine receipt](primary-debilitate-20261115.json).

The original script allocates one child at proto 2 PC 48, visits qualifying
compound types, and submits once at PC 110. It tests each incoming counter
against zero at PC 91; that counter is not a loop bound. The selected basic
type is forced at PC 104. Native clear-then-increment leaves one counter when
two compounds select the same basic type.

Radiation and Gas can both contribute when their target counts and incoming
presence gates pass. If both choose Heat, the submitted child has Heat 1.
If they choose Electricity and Toxin, both counters are present. Existing
ten-stack status alone does not trigger a type absent from this incoming
packet. One accepted callback can therefore contain several basic types;
repeated counters of one compound do not multiply its selections.

The pinned source is client-only batch
`20261109T095946Z-primary-debilitate-client-only`, manifest
`681cc93ffc168138cc39c70742ab271178316ef54cddda3b97c39b8483b5d2db`.
The importer verifies the original files/archive members and records the
decoded callback PCs, 16 native counter cases and 24 native RNG vectors.
Tests verify the 16 counter cases and 96 component/counter combinations
against the original two-way RNG outputs. An altered native wrapper result
is rejected by the source check, then the original fixture is restored.

Controlled scene tests load Phantasma Prime's actual six-beam metadata and
cover six/twenty-four multishot, a merged twenty-proc stress input and multiple
qualified compounds with one/twenty counters. Shared-component coalescing
and distinct-component selection are both exercised across 128 supplied
native seeds. The Primary-filtered tests and packet suite pass, as do affected
Clippy and comment/whitespace checks. These are controlled tests, not a live
game capture of the weapon-to-challenge notification frequency.

The read-only published baseline is engine `84dd66d22de7`, release
`ba3d14dbc316`, WASM `975c1db4f970`, data `dc95a7dd35c2`. Its actual WASM,
glue and packs were downloaded, hashed against their immutable names and
rechecked against an unchanged release descriptor. It is compared with
candidate calculation `32686c89efa`; no official channel is changed.

The fixed requests use 12 seconds, four runs, seed 24301, level 300 Steel Path,
body shots and infinite reserve. The fixture includes Radiation, Gas plus
Radiation, Corrosive plus Radiation, and Gas/Magnetic/Radiation, each with and
without Hell's Chamber. Requests and complete responses are retained under
`target/debilitate-audit/`.

| example | published DPS | candidate DPS | published / candidate without arcane |
| --- | ---: | ---: | ---: |
| Gas + Radiation, Hell's Chamber | 28596.35 | 10837.88 | 892.06 / 900.24 |
| Corrosive + Radiation, Hell's Chamber | 49918.87 | 19475.09 | 4745.88 / 4747.44 |
| Gas + Magnetic + Radiation, Hell's Chamber | 32055.69 | 16386.83 | 1509.43 / 1503.26 |

Release-lineage code invokes the split hook inside the ordinary proc loop.
The candidate instead constructs one child after that loop and selects each
qualified type once. Restoring the former rule merely to recover the larger
published DPS would discard the parsed contract. The no-arcane controls do
not reproduce the large arcane-dependent differences. The numbers above are
version comparisons, not new in-game damage measurements or proof of every
downstream multiplier.

Phantasma's exact native submission, challenge event-count writers and accepted
callback cadence remain EG-01-M3/M4. The callback contract must not be confused
with a claim that a real beam tick universally generates exactly one callback.
The documented scene adapter remains explicit. This audit changes tests and
evidence, preserves the existing source-backed candidate runtime, and changes
no measured golden values. Full native damage equivalence is not certified.
