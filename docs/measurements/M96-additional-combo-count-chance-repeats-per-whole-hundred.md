# M96 — Additional Combo Count Chance: each whole 100% repeats a hit's points, the rest rolls per base point ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Additional Combo Count Chance** (Quickening, True Punishment, Enduring Strike,
a riven's positive line — all one sum) is resolved for every hit:

- every **whole 100%** of it pays that hit's combo points **once more**;
- what is **left over** is rolled **once per base combo point** of the hit, each
  win adding **one** point.

What a base point is, and why an Exalted stance has fewer of them than it shows,
is MEASUREMENTS M97.

### The readings

**Hysteria's aerial combo** (One Point), 2 / 2x 2 / 3 = 9 points, one base point
a hit:

| chance | measured | reading |
| --- | --- | --- |
| 100% | 18, every round | the 9 doubled, nothing rolled |
| 120% | 18 to 22 over a long run, never above | 18 plus one 20% roll on each of the four hits |

**Dakra Prime, Vengeful Revenant's neutral opener**, always 3 points, all base:

| chance | measured | reading |
| --- | --- | --- |
| +20% | 4 4 3 4 4 3 | 3 plus three 20% rolls |
| +59.4% | 3 5 4 5 4 5 | 3 plus three 59.4% rolls |
| +59.4% +20% | 5 5 6 6 6 5 | the two ADD: three 79.4% rolls |
| +159.4% +20% | 9 9 9 8 7 8 | 6 plus three 79.4% rolls — 9 half the time, 8 in four, 7 in ten |
| 100% | 6, every time | doubled, nothing rolled |

### What it settles

W`Melee_Combo`: "Certain mods supply Additional Combo Count Chance, awarding an
extra combo point either on hit, on block, or under other specific
circumstances … starts at +0% and benefits additively". It does not say what a
chance past 100% does, nor what the roll is counted over. One roll a hit is
refuted by the Dakra's 5 and 6 at 79.4%; a remainder that repeats the hit's
points instead of adding one is refuted by its 7 and 8 at 179.4%; one roll per
DISPLAYED point is refuted by Hysteria never passing 22.

Shockwave Synergy agrees with the doubling: "True Punishment affects Shockwave
Synergy, effectively doubling the Combo Count gain from 4 to 8", so its grant is
`4 x (1 + chance)` per body.

Past 200% is not measured; the engine carries the same rule.

### Where it is read

`dummy::swing_combo_gain`, once per landed hit;
`a_hit_earns_its_base_points_through_the_gain_gate_and_the_extra_chance` holds
every row above as the totals it can and cannot reach.
