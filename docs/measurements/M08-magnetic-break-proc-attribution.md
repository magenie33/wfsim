# M8 — Magnetic break-proc attribution with mixed appliers

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** The shield/overguard-break Electricity proc scales with the
applier's mods (status-damage ×3.61 double-dip, faction double-dip,
literal Electricity mods). With stacks from MULTIPLE sources, whose mod
context does it read? Hypotheses: A first stack's applier (Heat Inherit
pattern), B last stack's applier (Frozen/M7 pattern), C the
shield-breaking hit's source, D per-stack attribution (each stack's 3%
chunk uses its own applier — suggested by the "per stack" wording).
Stakes: up to ~×8.7 between a modded and a bare applier.
**Prediction: D** (per-stack) — "per stack" wording, the Blast-radial
per-stack-snapshot precedent, and proc instances demonstrably carrying
source context; the counter-precedents (Heat first-proc, Frozen trigger)
both stem from special causes absent here.

**Method.** Magnetic weapons M+ (heavy status-damage + faction mods) and
M0 (bare); high-shield Corpus target. Read the post-break Electricity
tick numbers (6 s window):
1. All stacks M+, break with a non-magnetic weapon → applier baseline.
2. All stacks M0, break with M+ → big ticks = C; small = applier-owned.
3. Mixed 5×M+ then 5×M0, break with a third source → intermediate = D;
   all-M+ value = A; all-M0 value = B. Swap application order to
   separate A/B.

**Result:** _not yet run._
