# M96 — Additional Combo Count Chance: each whole 100% repeats a hit's points, the rest rolls for one ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Additional Combo Count Chance** (Quickening, True Punishment, Enduring Strike)
is resolved **independently for every hit**:

- every **whole 100%** of it pays that hit's combo points **once more**;
- what is **left over** is **one roll for one point**.

So under 100% a hit gets its points plus, on a win, one point; at 120% it gets
its points twice plus, on a win of the 20%, one point.

**Measured at 120%** on **Hysteria's aerial combo** (One Point), which earns
2 / 2x 2 / 3 = **9** points: the round came to **18 to 22** — the 9 doubled, plus
one point for each of the four hits that won its 20% roll. The top of the range
is not fully sampled yet; what the readings rule out is the other two readings
of the sentence — a certain single point per hit past 100% (9 + 4 = 13) and a
second doubling for the remainder (up to 27).

Past 200% is not measured; the engine carries the same rule (each whole 100% one
more repeat).

### What it settles

W`Melee_Combo`: "Certain mods supply Additional Combo Count Chance, awarding an
extra combo point either on hit, on block, or under other specific
circumstances … starts at +0% and benefits additively". It does not say what a
chance past 100% does.

Shockwave Synergy agrees with the doubling: "True Punishment affects Shockwave
Synergy, effectively doubling the Combo Count gain from 4 to 8", so its grant is
`4 x (1 + chance)` per body.

### Where it is read

`dummy::extra_combo_points`, called once per landed hit with that hit's points;
`additional_combo_count_chance_doubles_per_whole_hundred_and_rolls_the_rest`
holds the aerial combo's 18 and 22.
