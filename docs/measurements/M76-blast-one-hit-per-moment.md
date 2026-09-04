# M76 — a Blast going off is one hit per MOMENT, not per stack (owner, 2026-09-03)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Secondary Enervate, counting what advances its ramp.** A Blast fuse paying out
is a damage number at a moment, and that is what the arcane counts:

| what was applied | Enervate gains |
| --- | --- |
| nine stacks left to expire on their own | **9** |
| two applied by the same shot | **1** |
| ten, reaching the cap at once | **1** |

However many stacks share a moment, they are one — the same rule that makes a
shotgun's pellets one hit.

**The ten-stack row is one either way.** A full pile detonates where the tenth
stack lands rather than on its clock, so under the wiki's *"explosions from max
stacks or a kill do not [count]"* the 1 is the SHOT that filled the pile. The
two readings cannot be told apart from outside and the model takes the wiki's.

**BLAST NEVER CRITS**, so a pop only ever BUILDS the ramp — it can never fill
the big-crit counter that resets it. That is the half that changes what a build
is worth.

### What changed

Blast expiries fed the ramp not at all: it advanced only on a trigger pull.
`RunResult::blast_pops` counts the moments, and no golden moved — nothing in the
suite had ever combined the two.
