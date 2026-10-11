# M113 — Arbucep multishot is a fixed damage factor, and it reaches the DoT ✅ (owner, 2026-10-12)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**THE READINGS.** +110% multishot, no other damage mod, an enemy weak to Gas
(x1.5), non-crit:

- the Gas missile's explosion: **720**
- one Gas stack's tick, no Viral: **~360**
- one Gas stack's tick under one Viral stack: **720**, the 360 at Viral's x2

**WHAT FITS THEM.**

    explosion:  228 x 2.1 x 1.5                   = 718.2
    rolled instead (12.6 -> 12 or 13, over 6):      684 or 741
    tick:       (228 x 2.1 + 1) x 0.5 x 1.5       = 359.9
    tick without the factor in its seed:            171.8

The factor is the effective multishot over the six missiles, never a rolled
count, and it sits in the instance's ModifiedBase, so the status seed carries
it (`mb_live` in `fight::pellet`).

**TWO STACKS FOLD INTO ONE TICK.** One Gas stack under one Viral stack also
read **411**, a seed of 273: `(273 + 1) x 0.75 x 2 = 411`. Two Gas stacks under
two Viral stacks read **1270**, which is that seed and the full one in one
tick with one accumulator: `(478.8 + 273 + 1) x 0.75 x 2.25 = 1270.4`.

**OPEN.** Where the 273 seed comes from: 57% of the full explosion's 478.8
(an off-centre explosion; its falloff is unpublished) or `130 x 2.1`. Not
recorded with the readings.
