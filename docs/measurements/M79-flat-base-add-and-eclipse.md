# M79 — a flat base-damage add rides BESIDE an attack's own multiplier, and Eclipse does not reach Condition Overload (owner, 2026-09-04)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Magistar with a `+100 Base Damage` Incarnon perk** (arsenal 210 → 310), read
four ways against a Deimos Runner, whose physical column is ×1.0 and whose Blast
column is ×1.5. One formula fits all four:

```
damage = mods × (weapon base × attack multiplier + flat)
```

| what was swung | the formula | measured |
| --- | --- | --- |
| Shattering Storm's forward combo, first hit ×2, nothing else equipped | `210×2 + 100` = 520, snapped → **536.25** | **536** |
| ordinary slam ×2, Primed Pressure Point +165% | `2.65 × (210×2 + 100)` = **1378.0** | **1378** |
| heavy slam ×3, same build, ×1.5 Blast | `2.65 × (210×3 + 100) × 1.5` = **2901.75** | **2902** |
| the forward hit again, +60% Heat, one Condition Overload stack, 30% Eclipse | **1644.5** (below) | **1644** |

**THE FLAT ADD IS A PACKET OF ITS OWN.** It takes the base-damage mods, the
elemental mods and a Warframe ability's bonus, and it takes NEITHER the attack's
own multiplier NOR Condition Overload. Snapping it on its own grid (`flat / 32`)
and the weapon's on its own (`base / 32`) reproduces the readings to the digit,
and so does snapping the sum against the sum: the two packets carry the same
composition, so their unit counts are identical (26 / 5 / 2 / 19 on the fourth
reading) and the split is a FRACTION rather than a second snap.

**THE EXPLOSION HAD THIS RIGHT ALREADY.** A radial takes the same add as an
ABSOLUTE, on a base its slam multiplier has already been spent on (M69) — which
is why the two slam readings landed while the swings did not. The engine was
inconsistent with itself and the readings say the explosion's half was right.

### The fourth reading, in full

`1 + 0.3 (Eclipse) + 0.8 (Condition Overload, one status type)` is ONE bracket,
and the flat packet is outside the CO half of it:

```
weapon:  snap(210 × 1.6) = 341.25 → × 2 × (1 + 0.3 + 0.8) = 1433.25
flat:    snap(100 × 1.6) = 162.50 →       × (1 + 0.3)     =  211.25
                                                            1644.50
```

**ECLIPSE DOES NOT MULTIPLY THE CO TERM.** Multiplying the finished bracket
reads 1808. The reading cannot tell "a unique multiplier the CO term does not
see" apart from "a term in the same bracket as CO" — they are the same
arithmetic — so the engine keeps the page's own word for Eclipse ("The damage
buff is an unique multiplier") and takes the CO share out of it, which is the
smaller of the two claims. ONLY ECLIPSE: Roar's column and the elemental
augments are untouched and unread.

### What changed

`WeaponBase::unswung_base` carries the flat add, `unswung_fraction` is the share
of the vector the swing must leave alone, and the fold spends
`(1 − f)·k + f` where it spent `k`. The GunCO bracket follows the weapon's half
(`co_base_fraction × k / ((1 − f)k + f)`), and `eclipse_at` spends the ability's
bonus on everything but the share the term put in the bracket.

**AND THE LEDGER WAS SHORT BY THE WHOLE SWING.** `pellet_layers` was handed the
UNSWUNG vector against a grid the fold had already grown, so a melee row's
printed chain multiplied out to less than the number beside it and the missing
factor arrived unlabelled at the pop — a ×6 heavy attack drew its snap as 196.9
against an actual 1299.4. It is handed the swung vector now.

### Not read

The heavy attack's own ×6 (the class multiplier), and the combo COUNTER's
multiplier. Both are folded through the same `swing_mult` and therefore take the
same rule here; neither is one of the four readings.
