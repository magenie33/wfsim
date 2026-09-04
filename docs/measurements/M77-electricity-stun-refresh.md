# M77 — Electricity's stun cannot be re-applied while it runs (owner, 2026-09-03)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Repeated Electricity procs on one target.** The ~3 s stun has to END before
another can start, so a stream of procs leaves a GAP between stuns rather than
holding one — and Status Duration does not extend it either, which the wiki
already states.

**It is the opposite of Heat.** Ignite's note says *"re-procs re-trigger the
panic"*, so the two crowd-control effects are not one rule with two durations,
and neither can be written from the other.

**Costs no number today** and is recorded so it is not re-derived: nothing in
this arena acts, so a stunned enemy and an unstunned one shoot back equally
never. It would matter the day a card reads the state.

### Not settled: whether a crowd-control effect blocks other statuses

Observed with Lavos forcing Heat: panic, no other status during it, then panic
again — read as CC possibly suppressing other procs. THE SIMPLER READING FITS
THE SAME PICTURE: an ability that makes every point of the weapon's damage one
element leaves only that element able to proc at all, so nothing else was ever
going to appear.

To separate them: force the element on a weapon that still deals a SECOND
damage type, and watch whether that second type's status lands while the panic
runs. If it does, CC blocks nothing and the first reading was the coating.
