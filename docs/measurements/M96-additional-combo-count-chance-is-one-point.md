# M96 — Additional Combo Count Chance: one roll per hit, for one point ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Additional Combo Count Chance** (Quickening, True Punishment, Enduring Strike)
is rolled **independently for every hit**, and a successful roll adds **exactly
one** combo point. A chance of 100% or more makes that one point certain; it
never adds a second.

### What it settles

W`Melee_Combo`: "Certain mods supply Additional Combo Count Chance, awarding an
extra combo point either on hit, on block, or under other specific
circumstances … starts at +0% and benefits additively". It does not say what a
chance past 100% does. The engine had read it the way an over-100% crit or status
chance reads — a certain point plus a roll for another — which pays a build
carrying True Punishment with Quickening (120%) a second point one hit in five.

Shockwave Synergy is a different sentence and is not this roll: "True Punishment
affects Shockwave Synergy, effectively doubling the Combo Count gain from 4 to 8",
so its grant stays `4 x (1 + chance)` per body.

### Where it is read

`dummy::extra_combo_point`, called once per landed hit;
`additional_combo_count_chance_rolls_once_per_hit_for_one_point` holds it.
