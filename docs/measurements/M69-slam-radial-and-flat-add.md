# M69 — a slam's radial figure is what a body takes, and a flat base add lands once ✅ (owner, 2026-08-31)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Magistar, no mods, the evolution ladder up to Critical Parallel — so `Base
Damage +100` is taken — against an INFESTED DEIMOS target, slammed as close to
it as it goes.**

| reading | value | what it is |
| --- | --- | --- |
| light slam, highest white | **520** | `(420 + 100) x 1.0` |
| heavy slam, highest white | **1095** | `(630 + 100) x 1.5` |
| a heavy critical | **3186** | at the 3.0x Critical Parallel's +1x makes of 2.0 |
| **heavy slam, off Deimos** | **730** | `630 + 100`, with nothing on top |

**ALL FOUR CLOSE TO THE DIGIT**, and the third line only because of the target:
Blast deals *"x1.5 damage to Infested Deimos"* and Impact's own x1.5 is against
Grineer, Anarch and Scaldra — so the light slam's Impact is unmodified there and
the heavy slam's Blast is half again.

**THE SAME SLAM ON A TARGET WITHOUT THAT MULTIPLIER READS 730**, which is the
line that settles it: no faction reading to unpick, no falloff to allow for, and
`630 + 100` exactly. A reading whose target is not named can be read two ways —
the 1095 alone says 1050, the arsenal's `heavySlamAttack` — and two targets are
what refuse it.

### THE RADIAL FIGURE IS WHAT THE BODY TAKES

420 and 630 — `slamRadialDamage` and `heavySlamRadialDamage`, 2x and 3x the
weapon's 210 base, which is the wiki's *"Slam attacks do 2x the damage of a
normal attack (3x for heavy slam)"* and holds with NO exception across the
export's melee weapons. The bigger `slamAttack` / `heavySlamAttack` pair (3x and
5x here) is a different number for the same attack and is not it.

### …AND A FLAT BASE ADD LANDS ONCE

`420 + 100`, not `2 x (210 + 100)` — the light slam is exact at 520 and the
multiplied reading would be 620. So a slam takes the same ABSOLUTE add the rest
of an explosion does, and the multiple it is a multiple OF does not reach it.

**WHAT WAS BROKEN IS THAT IT REACHED THE SLAM AT ALL.** A form whose whole
attack is its explosion states `damage: {impact: 0}`, and both the perk path and
the fold returned early on a zero direct vector — so `Base Damage +100` was
worth nothing in the heavy slam mode, which is 51% of that mode's damage.

### What this does NOT settle

- **Whether a distant body takes a different figure.** One explosion with
  falloff is what these readings support; a second, weaker radial past the
  impact point would need a crowd to see.
- **The Incarnon Form's `+100% Melee Damage`.** It is EVO1 and therefore on the
  ladder, but the form was not entered for these readings — 1095 would have
  been 2190 — so they neither confirm nor deny the engine applying it for the
  whole engagement (docs/MELEE.md §7).
