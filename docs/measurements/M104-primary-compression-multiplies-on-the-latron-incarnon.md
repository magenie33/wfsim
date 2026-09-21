# M104 — Primary Compression on the Latron Prime Incarnon: a rank-2 card pays +224% and MULTIPLIES (owner, 2026-09-21)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Latron Prime, Incarnon Form, Riddled Target (+6 base damage), a +240%
base-damage bracket, Galvanized Aptitude at three stacks over two status types,
Double Tap full, Primary Compression at RANK 2 (+70% a metre).** Readings are
`collision + explosion`, as M102.

| reading | value |
| --- | --- |
| collision | **2097** |
| explosion | **8042** |
| the base form, aiming | compression pays **nothing** |

### What it settles

1. **The bonus MULTIPLIES; it does not join the base-damage bucket.** This is
   the Latron row's `Multiplies` class, and the two readings are what prove it:
   in the base bucket the same build reads 1074 on the collision.
2. **The radius given up is 0.8 of the modded radius**, continuous: 4.0 m kept
   to a fifth is 3.2 m lost, and at rank 2's +70% a metre that is **+224%**.
   The +240% in the same session is the BUILD's base damage, not the arcane's
   figure — 3.2 m cannot pay 240% at 70% a metre, it would need a 4.29 m
   explosion.
3. **The rank ramp between the two published ranks is linear.** Rank 2 of
   0 → +50% and 5 → +100% is +70%, and no other per-metre value lands on these
   numbers.
4. **The base form has no row and earns nothing**, which is the whole of what a
   missing `compression:` means: no explosion, no radius to trade.
5. **M102's split again, from a different direction.** The ratio alone fixes
   it: 8042 / 2097 = 3.835 = (146 × 5) / (56 × 3.4) — Double Tap on the
   explosion alone, Condition Overload on the collision alone.

```
collision  56 × 3.4 (base) × 3.24 (compression) × 3.4 (CO)       = 2097
explosion 146 × 3.4 (base) × 3.24 (compression) × 5   (Double Tap) = 8042
```

### …AND THE OTHER HALF OF THE TRADE, which these readings cannot see

The arcane BUYS the damage with radius, and the fight kept the full sphere
while the panel paid the bonus. Single-target that changes nothing — the blast
detonates on the aimed body, at the centre of whatever sphere is left, which is
why the readings above are the same either way. In a crowd it was the whole
cost of the arcane going unpaid: a 4 m explosion that reached a formation
should be 0.8 m.

`FightParams::from_panel` now shrinks it, and the fight is the only place that
can: a weapon's row is data, the arcane is a choice, and the panel does not
know whether it is equipped. A row the catalog marks `doesnt_work` takes no
metres and shrinks nothing.

### What is implemented

The damage half, unchanged — these readings confirm the model rather than
moving it: `build::loadout::resolve_for` folds the arcane's per-metre ramp
against the weapon's own row (`docs/CATALOGS.md` §2), and
`latron_prime_incarnon.yaml` carries `effectiveness: 1.00`, `stacking:
multiplies`, `radius_calculation: snapshot`. The radius half is the change this
reading led to.

Pinned by the three `m104_*` tests in
`engine/src/fight/tests/m102_latron_prime.rs`.
