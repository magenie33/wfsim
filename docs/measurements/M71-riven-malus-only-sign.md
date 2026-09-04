# M71 — a malus-only riven stat keeps DE's sign, and the listings confirm the size (2026-09-01)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Two riven stats ship with a negative base and only one of them flips sign in
the malus slot.** DE's export carries `WeaponRecoilReductionMod` at -0.0099999998
and `WeaponMeleeComboPointsOnHitMod` at -0.01165.

### The rule, which is derivable and was not measured

DE ships ONE wiki row as TWO entries: `WeaponMeleeComboBonusOnHitMod` at
+0.00653 ("Additional Combo Count Chance") and `WeaponMeleeComboPointsOnHitMod`
at -0.01165 ("Chance to Gain Combo Count"). A pool does not carry a stat twice,
so the pair exists because one entry is the bonus form and the other is the
malus form — and then the negative base already IS the malus. Multiplying it by
the malus multiplier's own minus sign would print a card whose negative slot
reads as a benefit, which is not what a negative slot is.

So the sign follows from what a malus IS, and `RivenStat::bonus` is the flag
that says which of the two readings a negative base takes:

- **Weapon Recoil is a stat whose BONUS is negative** — "-90% Weapon Recoil" is
  the good one — so the malus multiplier flips it positive.
- **Chance to Gain Combo Count can never be a bonus**, so only the
  multiplier's SIZE applies and the sign is DE's.

### The measurement, which is the cross-check on the SIZE

Live auction listings, `api.warframe.market` (the same source as M35). What they
settle is not the sign but WHICH multiplier a malus takes, and that the ordinary
formula produces the rest:

| card | slot | reads |
| --- | --- | --- |
| Braton, Boltor, 3+1, rank 8 | Weapon Recoil, malus | **+82.2 / +88.7 / +92.9** |
| Magistar, 3+1, rank 8 | Chance to Gain Combo Count, malus | **-96.8 to -112.4** across eight cards |
| Magistar, 2+1, rank 8 | Chance to Gain Combo Count, malus | **-71.7** |

The Magistar's disposition is 1.35, so `0.01165 x 90 x 1.35 x 0.75` is 106.2% at
roll 1.0 and the whole 0.9-1.1 band is 95.6 to 116.8: every 3+1 card sits inside
it, and the 2+1 card lands inside its own band (63.7 to 77.9) at the smaller
0.5 multiplier.

`RivenSpec::value_of` reads `RivenStat::bonus` for exactly this, and
`a_malus_only_stat_is_negative_where_recoil_turns_positive` pins both halves.

### What this does NOT settle

**Whether any other class has a malus-only stat.** Melee and Zaw carry the only
one in DE's export today, and the flag is per stat rather than per class, so a
new one is a data change and not a code change.
