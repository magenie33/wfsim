# M56 — a BLAST detonation takes NO elemental bonus, and Lavos can imbue Gas as its own element ✅ (owner, 2026-08-23)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Two mechanics measured in one sitting on a **Braton Prime, base 35**, and they
are opposite answers to the same question — *does an element bonus reach the
status damage?* — which is why they are recorded together.

### The reports, verbatim

```
90冰mod+90火mod
爆炸98-11

200爆炸+90冰mod+90火mod
爆炸337-21
168-11
1011-63（爆头）
```

```
200毒气+90毒mod
毒137-34 / 毒气137-54 / 火137-18

200毒气+90毒mod+90火mod
毒168-34 / 毒气168-54 / 火168-34

90毒mod+90火mod
毒气98-18
```

The `200<element>` source is **Valence Formation** (效价炼成), Lavos's passive
augment: +200% of one element, and the ONLY source that can add a COMBINED
element as its own.

### Blast: the element bracket is nowhere in it

| build | direct | detonation |
| --- | --- | --- |
| 90% Cold + 90% Heat | 98 = 35 × 2.8 ✓ | **11** |
| …+200% Blast | 168 = 35 × 4.8 ✓ | **11** |

The hit moved by 71% and the detonation did not move at all: `0.3 × 35 = 10.5`
both times, displayed as 11. That is the wiki's own sentence, measured —
*"Unlike other damaging statuses, adding more elemental damage (Heat and Cold)
will not increase the Blast proc damage"* (`Damage/Blast_Damage`) — and it is a
CONTROLLED PAIR rather than a single reading: the same run proves the imbue
landed.

The other two lines of the second block are the same shot at higher
multipliers, and they confirm the two the detonation DOES take:

- `337-21` — a critical hit. `168 × 2` and `10.5 × 2`, the crit multiplier
  reaching both halves.
- `1011-63` — a critical **headshot**, exactly `3.000 ×` the line above in both
  columns. Head 2 and crit 2 give a critical-headshot multiplier of
  `1 + (2 − 1) × 2 = 3`, and it is the same 3.000 M54 measured on the AoE half.

So: crit and weak point yes, elements no. The engine was already right and for
the right reason — a stack reads `modified_base`, which is the Serration bucket
alone, while elements are a bracket applied at the hit — but nothing asserted
it, and every OTHER status wants that bracket there.
`elemental_damage_moves_the_hit_and_never_the_blast_detonation` is that
assertion, both halves, verified to bite (607.5 against 202.5).

### Gas: the element bracket is the whole of it, and only a LITERAL source counts

The opposite answer, on a DoT. A tick reads `1 + Σ THAT ELEMENT's own bonuses`,
and only a source naming that element literally is in the sum:

| build | Gas direct | Gas DoT | reads |
| --- | --- | --- | --- |
| 90% Toxin + 90% Heat | 98 = 35 × 2.8 | 18 | bracket **1.0** |
| +200% Gas, +90% Toxin | 137 = 35 × 3.9 | 54 | bracket **3.0** |

The two mods that CREATE the Gas contribute nothing to the Gas burn — they are
Toxin and Heat sources, and the burn is Gas. Only Valence Formation, which adds
Gas *as Gas*, moves it. The wiki states the mechanism from the other side:
*"Bonus Elemental Damage will be added parallel to the weapon's Elemental
Damage, meaning it will NOT combine with elements on the weapon."* DE's own card
says 附加, not 合成.

The split rows are the same rule seen three times over: adding a 90% Heat mod to
the second build moved the HEAT split DoT 18 → 34 and left the Gas DoT at 54,
because a Heat mod is in Heat's sum and not in Gas's.

### The 36/35 on every tick — RESOLVED, see M58

Every DoT tick in both blocks came back **×1.0286 (= 36/35)** above what the
engine computed, across three independent brackets:

| bracket | computed | measured |
| --- | --- | --- |
| 1.0 | 17.5 | 18 |
| 1.9 | 33.25 | 34 |
| 3.0 | 52.5 | 54 |

The direct hits pin the base at exactly 35, so the DoT half behaved as though
the base were 36. It was left open here because a rounding rule, a per-weapon
quirk and a wrong `DOT_COEFFICIENT` all fit the nine rows, and the coefficient
reaches every elemental DoT in the app.

It is none of those: the status formula's accumulator **starts at 1 rather than
0**, stated on `Damage/Calculation` §Damage Over Time. `(35 + 1) × 0.5 = 18`.
M58 is the whole of it.
