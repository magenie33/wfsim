# M15 — Does every chain NODE carry a damage sphere, or only the beam's contact point? (Torid Incarnon)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** The Incarnon beam puts a **2.3 m damage sphere** at its contact
point, and every enemy that sphere catches starts its own chain (5 hops, 7 m
each, ×0.75 per hop). Does a chain HOP also drop a 2.3 m sphere where it lands,
or is there exactly one sphere in the whole attack?

It decides how the weapon scales into a crowd, and therefore what Firestorm is
worth: with one sphere, Firestorm buys more *chain origins*; with a sphere per
node, it multiplies the whole cascade.

**Current model: ONE sphere, at the beam's contact point**
(`beam.chain.nodes_have_radius: false`). Flipped 2026-08-06 from the `true` this
carried since 2026-07-30 — the earlier value was the user's in-game read of a
clump lighting up, and the user retracted it on the falloff argument below.
**Still not a citation, and this protocol still stands**: an argument narrows
which default is defensible, it does not measure anything.

**The falloff argument** (user, 2026-08-06), the one that moved it. Unlike the
four signals below it is structural, not circumstantial:

- An explosion in this engine is *a separate damage instance with linear falloff
  from epicentre to edge* (MECHANICS §Area of Effect). Every `radial:` in `data/`
  carries a falloff — Torid base 1.0, Burston Incarnon 1.0, Phantasma charged
  0.5, Laetum Incarnon 0.2 — and Detonate has to declare `falloff: none` out loud
  to be the exception. **This sphere carries none**, because the wiki denies it
  what falloff attaches to: *"The damage radius is not a separate damage instance
  from the beam."*
- So the sphere is not an explosion at all — it is the beam's **hit-detection
  volume**, widening what the single instance touches. Which is exactly why a
  directly struck target *"is still only hit once."*
- A sphere at a chain node could not belong to the beam, whose contact point is
  elsewhere. It would have to be the node's **own** damage instance — an
  explosion, needing a falloff nothing in the wiki or the datamine documents.
  `true` implied five undocumented explosions per attack.

Third-party agreement, not a citation: `malurth.github.io/AoE-simulator` — the
author of the Torid/Primed Firestorm mechanics video — chains node to node with
no sphere at any node.

**The four circumstantial signals**, recorded when the value was `true` and now
pointing the same way as it. None is a statement about chain nodes:
1. the wiki calls the first one *"the **initial** damage radius"* — a qualifier
   that only earns its place if there is exactly one;
2. the sphere is defined *"from the point of impact **against a surface**"*,
   while a chain lands on an **enemy**;
3. the datamined attack table carries **no radius at all** for the Incarnon
   attack, while the Poison Cloud (a real AoE part) carries its falloff;
4. the chain sentence is boilerplate shared with Atomos and Amprex, neither of
   which spheres at a chain node.

What held `true` in place for a week was that four inferences do not outrank
someone with the game open. What moved it was not a fifth inference but the
player retracting the read — so the count never decided this, and it should not
decide it now either. The wiki still never addresses the question.

**Method — the trap is that a clump cannot tell the two apart.** With several
enemies inside the initial sphere, BOTH models produce a wall of numbers: the
instance count is `1 + 5·Y` in Y, so eight clumped enemies give ~41 instances
with a single sphere. Worse, *"an enemy can be struck by multiple chains"*, so
one enemy shows several numbers either way. Counting damage numbers in a pile
proves nothing.

Force **Y = 1** and put the crowd out of the sphere's reach:

1. One enemy alone, against a wall. Aim so the sphere catches **only** that one
   — Y = 1, so a single chain leaves it.
2. A **tight cluster** (6+ enemies within ~2 m of each other) placed **more than
   7 m** from the impact point, so nothing there can be reached except by a
   chain hop.
3. Fire one tick and read the CLUSTER only:
   - **at most 5 enemies damaged** ⇒ one sphere. A chain hits one enemy per hop
     and there are 5 hops.
   - **the whole cluster damaged** ⇒ nodes sphere. One hop landed and splashed
     its neighbours.
4. Repeat with the cluster at ~15 m (two hops out) to check the answer does not
   only hold for the first hop.

**Confounds to avoid.** Multishot changes nothing here (the sphere and chains
from it take none) but a second enemy drifting into the initial sphere doubles
the chains and ruins the count — check Y is really 1. No Firestorm: enlarging
the sphere is exactly what you are trying to keep out of the test. Watch enemy
COUNT damaged, not damage numbers: the beam ticks 8×/s and the numbers pile up
under either model.

**Outcome mapping.** `false` (current) is what is implemented; `true` flips one
line in `data/weapons/primary/torid_incarnon.yaml` and its pinned assertion in
`weapons_data`. Neither value changes a single-target result — the sphere adds
no damage to a target the beam already struck — so nothing is blocked today.
It stops being free the moment the 2D model reads this line, which is why the
default was corrected before that model exists rather than after.
