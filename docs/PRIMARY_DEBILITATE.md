# Primary Debilitate callback contract

The client script processes each qualifying compound type once per accepted
callback. It creates one fresh DamageData object before the type loop and
submits that object once after the loop. The incoming counter is a presence
test, not the number of random selections or callbacks to generate.

For each compound type, both conditions must hold: the incoming packet contains
that proc, and the victim's queried count is at least ten. Existing stacks alone
do not grant a selection. After activation is accepted, every qualifying type
chooses one of its two basic elements. Repeated choices of one basic element
coalesce to one forced counter in the submitted child packet.

| qualifying input in one accepted callback | submitted child counters |
| --- | --- |
| Magnetic counter 1 or 20 | one Cold or Electricity |
| Radiation and Gas both choose Heat | Heat 1 |
| Radiation chooses Electricity; Gas chooses Toxin | Electricity 1 and Toxin 1 |
| Gas already at ten; incoming Gas counter zero | no Gas-derived selection |

Multiple compound types can therefore contribute to the same child packet.
One callback does not mean at most one basic type. Equal basic selections merge
within this child; separate accepted callbacks construct separate children.
The recovered script does not spend groups of ten stacks. The inherited
CanReproc/ReprocTime inputs do not install Encumber's SET0 suppression route.

## Original evidence

Root resource:
`/Lotus/Upgrades/CosmeticEnhancers/Offensive/SplitStatusProcsOnStackedStatusHit`.
The matching client is 2026.10.28.09.54. Script stem
`fa9f3da7532dd587fe91` validates as four prototypes and 210 instructions.
In callback proto 2:

| operation | original PC |
| --- | --- |
| allocate fresh DamageData | 48 |
| query target count and incoming proc counter | 80, 85 |
| compare target count against ten and incoming counter against zero | 88, 91 |
| choose a component with RandomInt(1,2) | 99 |
| set selected forced proc | 104 |
| submit the child after the type loop | 110 |

Counter getter hash `c29924e4` binds to RVA `0x19c6550` and reads the byte at
packet + `0x218 + procId`. Forced-proc wrapper hash `3d8e91b5` binds to
`0x19c5fc0`: it calls `0x92e1e0` to clear the byte, then `0x146fa80` to increment
it. Two forced writes of Heat leave one Heat counter. Ordinary direct increments
are a different operation and saturate at 255.

`scripts/import_primary_debilitate_contract.py <batch> --check` verifies all
declared source files and archive members, then checks the imported contract.
The source batch is `20261109T095946Z-primary-debilitate-client-only`, manifest
SHA-256 `681cc93ffc168138cc39c70742ab271178316ef54cddda3b97c39b8483b5d2db`.
The fixture retains 16 original native counter cases and 24 original RNG cases.
Tests check the counter wrapper and 96 combinations of the six script pairs,
eight original two-way RNG vectors, and incoming counts one/twenty. These are
native helper oracles and script-body evidence, not a full Luau VM replay.

## Phantasma Prime submission boundary

The scene merges its same-target continuous-beam hit and passes one status
submission to this callback adapter. Tests using the actual Phantasma Prime
metadata cover six/twenty-four multishot, many same-type procs and simultaneous
compound types. These tests establish the scene's behavior, not the native
weapon's complete notification cadence.

The native status application loop at `0x1283230` and the later attack
notification helper `0x1213f50` are distinct stages. The call graph does not
justify turning each ordinary status application into another script callback.
Likewise, Phantasma Prime's MultiBeam/CentralBeam/fireIterations fields and
beam-target cache reuse do not alone establish callback multiplicity.

Exact weapon submission/update and challenge-manager event-count writers remain
open under EG-01-M3/M4. Twenty incoming status applications do not by themselves
prove twenty callbacks. The per-callback contract above remains valid whether
the real producer supplies one callback or several. Existing WFSim output is
a comparison input; it does not override the recovered callback algorithm.
