# M27 — the buff seed decides nothing, or everything (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Every stacking buff used to start at full stacks. The replacement rule is in
[`BUFFS.md`](BUFFS.md) §Activation policy: a timed buff starts at 0, a
permanent one starts full. This is what made the change necessary rather than
merely preferable.

Torid + Galvanized Chamber + Galvanized Aptitude + Primary Deadhead, 300 s,
60 runs, seed 7, KPM:

| target | full-start | zero-start | apart |
|---|---|---|---|
| Lv 30 | 524.80 | 520.00 | 0.9% |
| Lv 100 | 58.81 | 49.94 | 15% |
| Lv 300 | 38.82 | 28.63 | 26% |
| Lv 1000 | 22.80 | 7.95 | 65% |
| Lv 9999 SP | 4.87 | 1.95 | 60% |

The seed washes out completely where kills are fast — 0.9% at Lv 30, because
the fight rebuilds the stacks within seconds of the run starting. It dominates
where kills are slow: at Lv 9999 SP the build kills 1.95 times per minute, so
an on-kill stack is essentially never earned, and starting full granted it a
buff the fight cannot produce and then sustained it for the entire 300 s.

So the old default was harmless exactly where it did not matter and wrong
exactly where it did. Engagement length is not the lever people assume: at
300 s the two answers are still 2.5x apart, because nothing is re-earning.

Seven engine tests moved. All seven were asserting what full stacks are worth
or how they decay, and all seven now SEED the stacks they measure
(`arc_stacked`, an explicit `initial_stacks`) instead of inheriting a default —
which is what they should have done from the start, and why they were the
tests that broke. One was rewritten rather than seeded: Primary Crux's
weak-point trigger can now assert that a body-only run is IDENTICAL to no
arcane at all, instance for instance, which is a stronger claim than the
seeded version could make.
