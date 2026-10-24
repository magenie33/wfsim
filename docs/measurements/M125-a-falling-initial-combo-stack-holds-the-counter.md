# M125 — a Galvanized Reflex stack that falls leaves the counter where it was for one combo duration ✅ (owner, 2026-10-24)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Simulacrum, a melee weapon with rank 10 Galvanized Reflex and no other source
of Initial Combo. Four melee kills inside 20 s for four stacks, then no input
while the counter settles and the stacks run out.

## The readings

| stacks | counter |
|---|---|
| 4 | **81** |
| 3 | **61** |

When the fourth stack falls the counter does not drop. It keeps 81, the combo
timer's bar appears at that instant, and **5 s** later, the weapon's combo
duration, the counter reads 61. Each stack that falls after does the same.

## What it settles

- **+20.46 Initial Combo a stack, stored with its decimals.** `4 x 20.46 =
  81.84` and `3 x 20.46 = 61.38`, shown floored. The package's `1.86 x 11`
  (M124) is the value the game keeps, and nothing truncates a stack or the
  sum: a truncated stack would read 80 and 60.
- **A floor that steps down turns its points into earned ones.** The counter
  holds what it read before the drop, on a combo timer started at the drop,
  and the timer's expiry takes it down to the new floor. Hits in that window
  add to it and refresh the timer as they would on any earned count, up to
  the weapon's cap.

```
before the drop   floor 81.84                     counter 81.84   tier 5
drop at t         floor 61.38, held 81.84 to t+5  counter 81.84   tier 5
t + 5             held expires                     counter 61.38   tier 4
```

The engine had dropped the counter with the floor, so a heavy swung in those
5 s paid a tier less.

Tests: `fight::tests::m125_reflex_floor_hold`.
