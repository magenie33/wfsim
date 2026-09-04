# M81 — an `Independent` Condition Overload multiplier applies AFTER quantization ✅ (owner, 2026-09-04)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Four readings on a **Ballistica Prime, Incarnon form**, one build, Galvanized
Shot walked up a rung at a time. They settle an ORDER that nothing had asked:
a CO term that does not join the base-damage bucket does not scale the
quantization denominator either, so — unlike the `Adding` class — it does not
commute with the snap, and one of the two orders had to be wrong.

Base **833** (830 + 3 from the tier-2 evolution, M66), mono-Slash. Mods:
Hornet Strike +220%, Primed Convulsion +165% Electricity, Deep Freeze +90%
Cold, Maim +120% Slash. Rows are `stacks – status types`.

### The readings

```
ModdedBase = 833 × 3.2 = 2665.6      s = 2665.6 / 32 = 83.3
  Slash    833 × 3.2 × 2.2 = 5864.32   ÷ s = 70.40 -> 70
  Magnetic 2665.6  × 2.55  = 6797.28   ÷ s = 81.60 -> 82
                                            152 units × 83.3 = 12661.6
```

| `a – b` | 0–0 | 1–1 | 2–1 | 3–1 |
| --- | --- | --- | --- | --- |
| CO multiplier `1 + 0.4·a·b` | ×1.0 | ×1.4 | ×1.8 | ×2.2 |
| quantize, THEN multiply | 12662 | **17726** | **22791** | **27856** |
| multiply, THEN quantize | 12662 | 17743 | 22824 | 27906 |
| **measured** | **12662** | **17726** | **22791** | **27856** |

**Every row is `12661.6 × (1 + 0.4·a·b)`.** The snap happens once, against
ModdedBase, and the independent multiplier is spent on the already-quantized
vector.

### Why nothing had settled it before

**The 0–0 row settles nothing** and is here as the control: with no CO the two
orders are the same expression.

**Neither did M66's Incarnon reading.** Its 5864 was taken with no elemental
mod, so the vector was mono-Slash — 32/32 of its own scale, lossless — and
`833 × 3.2 × 2.2` is the answer under either order. A second damage type is
what makes the orders diverge, because each type rounds on its own: here Slash
loses 0.4 of a unit and Magnetic gains 0.4, and multiplying first moves both
across different integers.

**And the `Adding` class cannot show it at all.** There the CO term joins the
base-damage bucket, so it multiplies the numerator AND the scale, and the two
orders are algebraically equal — M80's ladder is 130 units in all six rows for
exactly that reason. `pellet_layers` says as much in its own comment; what this
entry adds is that the licence stops at the bucket.

### What was already right

`loadout.rs` builds both forms to the digit — checked directly against the
0–0 rows, which isolate everything except the CO: charged **2015.0000** and
Incarnon **12661.6** against measured 2015 and 12662, with every component
landing on 1.60 / 17.60 / 28.16 / 81.60 and 70.40 / 81.60 units. The engine
quantizes once in `dummy.rs` and spends CO as a later layer, which is this
entry's order.

### Not settled by this

**Whether a `Multiplying` catalog entry behaves the same way.** This weapon's
Incarnon form is `independent` because its catalog row says so; M51 read the
`Multiplying` class on a different weapon and did not test the order. The
rule stated here is the one the engine applies to every `Independent` entry,
and only this one has been read.
