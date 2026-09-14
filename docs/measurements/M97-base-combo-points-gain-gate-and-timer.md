# M97 — Base combo points, the gain gate, and a hit that earns nothing ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A hit has two numbers: the combo points it **shows** and how many of them are
**base** points. Every combo chance acts on the base points.

- **An ordinary weapon's stance: every point is a base point.**
- **An Exalted stance (Hysteria): one base point a hit**, the rest of what it
  shows riding along.

**Chance to Gain Combo Count** — a riven's malus — is a **gate**: each base point
survives with `1 + chance` and a lost one takes its points (and anything
Additional Combo Count Chance gave it) with it. It is not netted against
Additional Combo Count Chance.

**A hit that comes to 0 points does not refresh the combo timer.**

### The readings

| test | setup | measured | reading |
| --- | --- | --- | --- |
| 1 | Hysteria, 120%, the aerial combo's 3-point finisher | never 8 or 9 in 30 | one base point: 6 or 7 only (three would reach 9 in one hit in ten) |
| 2 | 100% exactly, Hysteria's aerial round and the Dakra's neutral opener | 18 and 6, no variation | the whole hundred doubles and rolls nothing |
| 3 | Dakra Prime, aerial opener (200%, 2 points), a chance under 100% | 3 3 2 4 4 2 2 2 4 4 3 | two base points: 2 without a doubling, 4 needs two wins |
| 4 | Dakra Prime, -57.3% Chance to Gain Combo Count alone, same hit | 1 0 0 1 2 2 1 1 2 0 1 2 0 0 2 | a gate per base point: a 1 is each point rolled on its own |
| — | Dakra Prime, neutral opener (3 points), -57.3% gate beside +20% and +100%, first series | 2 5 2 4 4 4 2 2 2 3 2 0 0 5 2 5 4 — 17 hits, mean 2.82, no 1 | each base point kept 42.7% of the time with its 2 or 3 points: expected 2.818, and a 1 impossible |
| — | the same, second series | 3 2 5 0 2 2 2 2 5 0 2 5 4 5 4 4 2 4 3 0 6 3 0 2 0 0 2 2 — 28 hits, mean 2.54, no 1 | together: 45 hits, mean 2.64 (0.6 standard errors under 2.818), nine 0s against 8.5 expected, not one 1 |
| — | Dakra Prime, -57.3% alone, neutral opener | 1 0 2 2 0 2 0 2 0 0 1 2 1 0 1 1 2 3 | three base points, each kept 42.7% |
| — | a 0 in test 4 | the combo timer did not refresh | |

Netting the gate against the chance (120% - 57.3% = 62.7%) would never go under
3 on the neutral opener; over both series it read 0 nine times and averaged
2.64.

### ONE RIVEN AXIS, TWO MECHANICS

On a riven card **Additional Combo Count Chance** (bonus) and **Chance to Gain
Combo Count** (malus) are the two poles of ONE axis — the stat a card rolls
either up or down. In the fight they are **two mechanics that stack without
netting**: the bonus joins the chance pool beside Quickening and True
Punishment, the malus is the gate above. DE's export already files them as two
stats (`WeaponMeleeComboBonusOnHitMod`, `WeaponMeleeComboPointsOnHitMod`), and
`data/rivens/melee.yaml` gives them two kinds. Summing them into one signed
chance is the natural reading of the card and the one these readings refute.

### What it settles

Nothing published separates base points from shown points, and W`Melee_Combo`
does not say what Chance to Gain Combo Count does. The reading that fits both
weapons: combo count is stored per attack as base points that the chances act
on, and a stance's attack values were refilled from its damage multipliers for
ordinary weapons — which makes every point base — while the Exalted stance kept
one per hit with the extra carried beside it. That is an explanation of the
data, not a measurement of the game's code.

### Where it is read

`ComboHit::combo_points_base` on every row (`combo_points` on an ordinary stance,
1 on the `valkyr_talons*` entries, 0 on a spending row), `dummy::swing_combo_gain`
and `dummy::refreshes_combo_timer`, and `ModEffect::ComboGainChance` for the
riven's line. `a_hit_earns_its_base_points_through_the_gain_gate_and_the_extra_chance`
and `a_hit_that_earned_nothing_does_not_refresh_the_combo_timer` hold them.
