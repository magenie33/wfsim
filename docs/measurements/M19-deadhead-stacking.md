# M19 — Do two Deadheads stack? (Primary + Secondary on one weapon)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

An Arch-Gun seats a PRIMARY and a SECONDARY arcane, so Primary Deadhead and
Secondary Deadhead can sit on the same weapon. They are the same effect twice:
+120% Damage per stack to 3 stacks, and +30% to the Headshot Multiplier.

**What we model:** two independent buffs. Six damage stacks, not three, and the
headshot bonuses add to +60%. Larkspur Prime at assumed-max, 100% headshots,
and the arithmetic is exact:

| arcanes | ratio | = |
|---|---|---|
| one Deadhead | 5.98x | 4.6 (base-damage bucket) x 1.3 (headshot bracket) |
| both | 13.12x | 8.2 x 1.6 |

**What the wiki supports:** the two rules each bonus obeys, and nothing about
the pair. `Secondary_Deadhead` states "The damage bonus stacks additively with
other damage mods like Hornet Strike" and "Headshot bonus stacks additively
with similar buffs, such as Prowl" — which is why the damage half sits in
Serration's bucket and the headshot half in one additive bracket. It says
nothing about two Deadheads, or about identical buffs from two slots sharing a
cap.

**The open question is the CAP.** If the game treats them as one named buff,
the second arcane refreshes the first and the ceiling stays 3 stacks — worth
2.99x here instead of 8.2/4.6 = 1.78x more. Independent buffs is the reading
we take because they are separate arcanes with separate names, and because a
shared cap would make the second one nearly worthless, which DE tends to say
outright when it is true.

**What settles it:** equip both on an Arch-Gun, get four headshot kills, and
read the buff icons — one stack counter at 3 or two at 3 each. Or compare a
body-shot damage number at full stacks against the same build with one arcane.

**Result:** _not yet run._
