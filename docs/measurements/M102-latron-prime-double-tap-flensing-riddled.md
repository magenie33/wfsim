# M102 — Latron Prime: Double Tap reaches only the Incarnon explosion and is snapshotted per form; Flensing Spikes counts bullets; Riddled Target's stacks keep their own clocks (owner, 2026-09-20)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Latron Prime, +165% base damage, Riddled Target (+6 base damage), Galvanized
Aptitude.** Readings are written `stacks-types` for Galvanized Aptitude; the
Incarnon form reads as `collision + explosion`.

### The panel, without Double Tap

| form | Galvanized | reading | predicted |
| --- | --- | --- | --- |
| normal | 0-0 | 254 | 96 x 2.65 = 254.4 |
| normal | 1-2 | 326 | 254.4 + 90 x 0.8 = 326.4 |
| normal | 2-2 | 398 | 254.4 + 90 x 1.6 = 398.4 |
| normal | 2-3 | 470 | 254.4 + 90 x 2.4 = 470.4 |
| Incarnon | 0-0 | 148 + 387 | 56 x 2.65 = 148.4; 146 x 2.65 = 386.9 |
| Incarnon | 1-3 | 326 + 387 | 148.4 x 2.2 = 326.5 |
| Incarnon | 2-3 | 505 + 387 | 148.4 x 3.4 = 504.6 |

The base form's Galvanized term reads the weapon's own 90 and not the +6 (the
`Adding` class rule, as M88 and M101); the Incarnon collision takes it as a
free-standing factor (the CO catalog's "Multiplying" row) and the explosion
not at all. Riddled Target's +6 reaches the collision (50 → 56) and the
explosion (140 → 146). All of this was already the engine's rule.

### Double Tap, full pile (+400%)

| form | Galvanized | reading |
| --- | --- | --- |
| normal | 0-0, no Double Tap stacks | 254 |
| normal | 0-0 | 1272 = 254.4 x 5 |
| normal | 1-3 | 1812 = 362.4 x 5 |
| normal | 2-3 | 2352 = 470.4 x 5 |
| Incarnon | 0-0, no stacks | 148 + 387 |
| Incarnon | 0-0 | 148 + 1935 |
| Incarnon | 2-3 | 505 + 1935 |

### What it settles

1. **On the Incarnon form Double Tap reaches only the explosion.** The
   collision reads the same with the pile empty and full. (The wiki files this
   under Bugs; it is how the game plays.)
2. **An Incarnon shot adds +40% — two hits, collision and explosion.**
   Occasionally +20% was seen, rarely; read as one of the two missing. Only
   the aimed landing counts: a ricochet adds nothing.
3. **Each form keeps its own pile, frozen when a transform completes.** Going
   in, the base form's pile is saved as it stands at the instant the
   transmute completes — +300% with 0.5 s left comes back as +300% with 0.5 s
   left when the way out completes — and the Incarnon form's pile is
   saved the same way on the way out. A form never yet fired starts from
   nothing, which is the same rule seen from a 0-stack snapshot.
4. **Flensing Spikes counts bullets, not Puncture stacks.** Each bullet that
   procs Puncture removes 20% of the armour once, even when it procs Puncture
   twice; five bullets remove all of it, and five stacks off fewer bullets
   leave armour standing. The armour never comes back.
5. **Riddled Target's multishot stacks each run on their own 8 s clock.**

### What is implemented

`consecutive_hit_radial_only: true` on the three Latron Incarnon attacks
(points 1 and 2); one Double Tap pile per form, swapped at each transform's
completion (point 3); `DebuffState::flensed`, counted once per bullet per body
and never pruned (point 4). Point 5 was already `BuffDecay::PerStackExpiry`.

Pinned by the `m102_*` tests in `engine/src/dummy.rs`
(`m102_latron_prime_tests`).
