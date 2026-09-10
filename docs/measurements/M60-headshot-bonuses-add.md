# M60 — headshot bonuses ADD, and the crit-tier ladder holds ✅ (owner, 2026-08-25)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

An unmodded **Laetum in its BASE form**, every shot to the head of a **Techrot
Babau** (wiki: *"Head: 1.5x"*, faction Techrot, 10,000 health, 500 shield, no
armour). Two numbers per line, the crit tier the pop-up was:

```
none at all
head 1058/1876
+50% headshot damage
head 1587/2812
+50% headshot damage + Deadhead
head 1904/3376
```

and a second capture on a built weapon — Deadhead at 3 stacks, +50% headshot
damage, 187% crit chance, Galvanized Scope at full stacks and Avenger (+45% crit
chance, additive):

```
15529 (orange crit) / 22300 (red crit)
```

### What it settles: the two headshot bonuses are ONE ADDITIVE BUCKET

`+50% Headshot Damage` is the Laetum's tier-4 evolution **Caput Mortuum**;
Secondary Deadhead's rank-5 passive is **`+30% to Headshot Multiplier`**. The
two could add into one bucket or multiply, and the arithmetic tells them apart:

| | additive `1+0.5+0.3` | multiplicative `1.5 × 1.3` | measured |
|---|---|---|---|
| step from +50% to +50%+Deadhead | **×1.200** | ×1.300 | **1904/1587 = ×1.1998** |

So they ADD. The engine already summed them
(`headshot_multiplier_bonus + streak_bonus + headshot_damage_bonus`, with
`headshot_bonus_multiplicative` false for everything but Cernos Prime) — but
that came from reading the wiki, and this is the first time it has been
measured. Reproduced end to end: 1.4999 / 1.2000 / 1.7998 against the
measurement's 1.5000 / 1.1998 / 1.7996.

### …and the CRITICAL HEADSHOT ladder is the wiki's formula

`Headshot Crit Tier Multi = Headshot Multi × (1 + Tier × (2 × CD − 1))` (wiki
`Critical_Hit`), which at CD 2.2 gives tier multipliers **1 / 4.4 / 7.8 / 11.2**:

| step | formula | measured | off by |
|---|---|---|---|
| yellow → orange | 7.8/4.4 = 1.77273 | 1876/1058 = 1.77316 | +0.024% |
| orange → red | 11.2/7.8 = 1.43590 | 22300/15529 = 1.43602 | +0.009% |

The first capture's pair is therefore **yellow/orange**, not white/yellow: white
→ yellow would be ×4.4 and the numbers are nowhere near it.

### The 0.19% that is NOT explained

All six numbers of the first capture sit **+0.14% to +0.21%** above
`160 × 1.5 × tier` — one uniform factor of about ×1.0019, not a structural
error: every RATIO above is exact to three decimals. The three published inputs
were checked against the wiki and all match what `data/` holds (160 = 64 Impact
+ 96 Slash, head 1.5x, crit multiplier 2.20x), so the factor is not in any of
them. Left open deliberately (2026-08-25): a small discrepancy is acceptable and
is not worth chasing.

What would close it is three pop-ups from an unmodded gun on the same target: a
body white (should be 160), a body yellow (352), and a head white (240). Between
them the factor is pinned to one layer.

### A note on which number to compare

The perk's own multiplier is **exactly 1.500000** on the DIRECT hit (480 → 720
white, 2112 → 3168 yellow). A dps RATIO gives 1.49988 and does not converge with
run count, because dps is the whole engagement and about 0.025% of it is status
DoT, which does not take the headshot bonus (M54). Comparing a perk's multiplier
through dps measures the perk plus everything the perk does not touch.

**Techrot Babau is not in `data/enemies/`**, so this capture cannot be replayed
in the app as it stands.
