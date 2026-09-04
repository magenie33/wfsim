# M80 — the quantization denominator is `base × (1 + base-damage mods)` and nothing else ✅ (owner, 2026-09-04)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Three builds on a **Ballistica Prime**, Charged Shot, taken alongside M66 and
separated here for the reason M57 was separated from M56: what they settle is
neither the charge nor this weapon. M57 found the denominator is `ModdedBase`
and not the vector's total, on four builds that differ only in their ELEMENTS.
These add the two buckets those four could not distinguish — a per-type
physical mod, and a bonus that joins the base-damage bucket.

Base 155 throughout (`76 × 2 + 3`, M66), split 5 Impact / 40 Slash /
55 Puncture, so `ModdedBase = 155 × 3.2 = 496` and `s = 15.5` under the rule
below.

**And with an element on it, which is the reading M57's denominator is about**
(owner). +220% base damage, +165% Electricity, +90% Cold on the Charged Shot
pops **1783**:

```
ModdedBase = 155 × 3.2 = 496,  scale = 15.5
Electricity + Cold combine FIRST -> Magnetic 255%, one type, not two
  2 + 18 + 13 (physical, unchanged by a base-damage mod)
+ 82          (2.55 × 32 = 81.6, the element on the SAME denominator)
= 115 units × 15.5 = 1782.5 -> 1783
```

`496 × 3.55 = 1760.8` is what the vector's own total would give, 22 low. The
reading does **not** separate combining-then-quantizing from quantizing each
element (53 + 29 is also 82); it pins the denominator, which is the thing M57
found wrong.

**AND A PHYSICAL DAMAGE MOD IS OUT OF THE DENOMINATOR TOO**, which M57 never
separated — it tested elements only. Add +120% Slash and +120% Puncture to the
build above and the Charged Shot pops **2341** (owner):

| type | value | `÷ s` | units | `× s` |
| --- | --- | --- | --- | --- |
| Impact | 155 × .05 × 3.2 = 24.80 | 1.600 | **2** | 31.00 |
| Slash | 62 × 3.2 × 2.2 = 436.48 | 28.160 | **28** | 434.00 |
| Puncture | 85.25 × 3.2 × 2.2 = 600.16 | 38.720 | **39** | 604.50 |
| Magnetic | 496 × 2.55 = 1264.80 | 81.600 | **82** | 1271.00 |
| | | **150.08** | **151** | **2340.50** → **2341** |

**`s` is still 15.5**, i.e. ModdedBase is still `155 × 3.2 = 496` — the physical
mods raised the numerators and left the scale alone. Folding them in instead
makes ModdedBase 2326.24, `s` = 72.7, and the pop **3781**: the two readings are
not close, so this one reading settles it. So the denominator is `base × (1 +
base-damage mods)` and NOTHING else — not the elements, not the per-type
physical mods, not the vector's total.

**AND GunCO GOES IN THE DENOMINATOR, because it joins the base-damage bucket**
(owner). Five readings on the same build with the Puncture mod dropped, written
`Galvanized Shot stacks – status types on target`:

| `a – b` | 0–0 | 1–1 | 2–1 | 3–1 | 2–2 | 3–2 |
| --- | --- | --- | --- | --- | --- | --- |
| `a × b` | 0 | 1 | 2 | 3 | 4 | 6 |
| pop | **2015** | **2145** | **2275** | **2405** | **2535** | **2795** |

`2015 + 130 · a · b` to the digit, and the 130 is not a fitted constant. The CO
term is `0.4 · a · b · C` **added to ModdedBase**, so with C = 80 the base grows
by exactly 32 a step and the scale `ModdedBase / 32` by exactly 1:

```
a·b :   0      1      2      3      4      6
MB  : 496    528    560    592    624    688   = 496 + 32·a·b
s   : 15.5   16.5   17.5   18.5   19.5   21.5  = MB / 32
units: 2 + 28 + 18 + 82 = 130, IN EVERY ROW
pop : 130 · s = 2015  2145  2275  2405  2535  2795
```

**3–1 IS THE ROW THAT RULES OUT ADDITION.** 3–1 and 2–2 are the same `a + b`
and would print the same number under a sum; they print 2405 and 2535. 3–1 and
3–2 share `a` and would print the same number if only the stack count mattered;
they print 2405 and 2795. The term is a PRODUCT, and the smaller of the two
factors is the one that binds.

**THE UNIT COUNT NEVER MOVES.** A base-damage-bucket bonus multiplies numerator
and denominator together, so quantization has nothing new to round — every row
is the same 130 units at a bigger scale. It is the same argument the +220%
reading makes, run five more times, and it re-derives **C = 80** independently
of §3's four-row decode: a C of 76 would step 30.4 and print 2139 at `a·b` = 1.

**The engine already had this right and had never been asked.**
`loadout.rs` builds `modified_base` as `base_vector.total() * (1 + base_damage)`
and spends the physical bucket on the components alone, so this reading confirms
a line rather than changing one — the case M57 could not distinguish, closed by
a build that separates it.

**The ladder also re-measures the 155.** With ModdedBase built off 158 instead,
the 3–2 row is `158 × 3.2 + 192 = 697.6`, `s = 21.8`, still 130 units, and it
would print **2834** against a measured 2795 — and all six rows miss by the same
proportion. M66 derives 155 from one full-charge pop; this derives it from six
modded ones.

It is also the clearest case of quantization paying in both directions at once:
Slash loses 0.16 of a unit, Puncture gains 0.28, Magnetic gains 0.40, and the
hit ends up 0.92 units — 14 damage — above what the unquantized vector would
deal.

**The `Adding` class reads the UNEVOLVED base** (M50). Headcracker's +3 is out
of the CO term on all three attacks, which is the class default and needs no
per-perk declaration: 137.6 + 2.4×40 = 233.6 → **241**, and 43 in place of 40
would have printed 248.

**`independent` on the Incarnon form.** 833 × 3.2 × 2.2 = 5864.3 → **5864**,
the CO term as a free-standing final multiplier and the +3 inside the base.

### Why three builds and not one

Each closes a bucket the one before it could not see, and the wrong answer is
far away in every case rather than a rounding apart:

| build | if the bucket were IN the denominator | measured |
| --- | --- | --- |
| +220%, Magnetic 255% | 1761 (the vector's own total, M57's bug) | **1783** |
| …plus +120% Slash, +120% Puncture | 3781 | **2341** |
| …GunCO instead of the Puncture mod | the unit count would move off 130 | **130 in all five rows** |

The third is the one that cannot be read backwards: a base-damage-bucket bonus
is in BOTH the numerator and the denominator, so it is the only kind of bonus
that changes a hit without changing what quantization does to it.

### Not settled by this

**The order inside the elements.** 165 Electricity + 90 Cold reaches 82 units
whether it combines to Magnetic first (2.55 × 32 = 81.6) or quantizes as two
elements (52.8 → 53, 28.8 → 29). A build that separates them needs fractional
parts that cross an integer differently — 30% and 90% would, at 9.6 → 10 and
28.8 → 29 against a combined 1.2 × 32 = 38.4 → 38.
