# M91 — one weapon, five elements, four rows apiece: every damaging DoT takes the weak point, and a full Blast pile is ten numbers ✅ (owner, 2026-09-13)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A **Braton Prime**, no Incarnon — base 35, read off the infobox as
`21 Impact + 1.8 Puncture + 12.2 Slash` — with **Valence Formation at +200%** of
one element at a time and nothing else. Four rows per element, `direct —
status`, and the second block is the same fixture against a body with a **×1.5
vulnerability** to the element:

```
                +200% Heat / Toxin / Electricity / Gas      +200% Blast
body            105 — 54                                    105 — 11
body, crit      210 — 107                                   210 — 21
head            315 — 159                                   315 — 32
head, crit     1260 — 632                                   1260 — 126

…and on a x1.5 vulnerable body
body            140 — 81                                    140 — 16
body, crit      280 — 160                                   280 — 32
head            420 — 239                                   420 — 47
head, crit     1680 — 947                                   1680 — 189
```

Heat, Electricity and Gas tick as one consolidated number however many stacks
are live; Toxin ticks per stack. Gas is read as a Gas mod would be, and no
ordinary loadout carries one — so the bonus is a fixture, not a build.

### The direct column is what names the rows

`35 × (1 + 2.00) = 105`, then `×2` for the crit, `×3` for the head, and `×12`
for the head crit — the ladder M60 measured, `3 × (1 + 1 × (2 × 2 − 1))`. So
the four rows are body, body-crit, head, head-crit, and nothing else is in the
way.

### Every damaging DoT takes the weak point, Toxin included

```
tick = 0.5 × 35 × (part × crit) × 3.00  +  0.5 × 3.00
       52.5 + 1.5 =  54.0        105.0 + 1.5 = 106.5 → 107
      157.5 + 1.5 = 159.0        630.0 + 1.5 = 631.5 → 632
```

Four elements, four rows, one formula. **This supersedes M54's Toxin line**,
which rested on a flat reading rather than on its own two numbers — that file
says so itself: *"`159/107 = 1.486` is neither 1 nor 3"*. Toxin stops being an
exception and `dot_takes_weakpoint` stops having one.

### …and the accumulator takes neither the part nor the crit

`159` rather than `162`, and `632` rather than `639`: the seed is multiplied
and the accumulator's own `1` is not. Read directly here on four pairs, where
M58 had only the page's arithmetic for it.

### Blast keeps both of its exceptions

`0.3 × 35 × (part × crit)` reproduces 11 / 21 / 32 / 126 — so a Blast stack
takes **no elemental bonus** (35 and not 105, M56) and has **no accumulator**
(10.5 and not 11.5, M58), while taking the part and the crit like everything
else.

### A ×1.5 vulnerability multiplies a tick whole

A direct hit is 35 physical plus 70 of the element, so only the element's half
moves: `35 + 70 × 1.5 = 140`. A tick is the element and nothing else, so all of
it moves: `54 × 1.5 = 81`, `159 × 1.5 = 238.5 → 239`, `631.5 × 1.5 = 947.25 →
947`, and Blast the same. Both halves of the column's rule, read off one
fixture.

### A FULL PILE IS TEN NUMBERS ON THE HOST AND ONE ON EACH NEIGHBOUR

Ten Blast stacks reaching the cap detonate together: the body carrying them
pops **ten** numbers, and every body the 5 m sphere reaches pops **one**. The
sum is what it always was; the INSTANCE is not, and an instance is the unit
attenuation clamps, a shield gate multiplies and overkill is measured against.

**AND THE COUNT IS NOT WHAT AN ARCANE SEES.** A pile going off at the cap feeds
no hit-counting ramp however many bodies it reaches — one shot can add at most
one, which is M76 from the other side and the reason splitting the damage must
not split the count.
