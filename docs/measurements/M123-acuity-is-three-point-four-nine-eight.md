# M123 — Pistol Acuity at rank 10 is +349.8%, not the card's +350% ✅ (owner, 2026-10-23)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A **Laetum** at every shot to the head of a **Deimos Runner** (wiki: *"Head:
3.0x"*, faction Infested Deimos, which takes Impact, Slash and Radiation as
written; 100 health, no armour, no shield). Rank 10 Pistol Acuity, rank 10
Hornet Strike (+220%), and the Laetum's tier-4 Elemental Excess and tier-5
Devouring Attrition where a line names them. Every number is one pellet's
damage types summed, as the game draws it.

## The readings

| # | form | build | number |
|---|---|---|---|
| 1 | base | Pistol Acuity | **5816** yellow |
| 2 | Incarnon | Pistol Acuity | **3635** yellow, the direct hit · **300** white, the explosion |
| 3 | base | Hornet Strike + Pistol Acuity | **18610** yellow |
| 4 | base | Hornet Strike, Devouring Attrition | **32256** white, a Devouring proc |
| 5 | base | Hornet Strike + Pistol Acuity, Elemental Excess, Devouring Attrition | **88672** white, a Devouring proc |

Elemental Excess is there for line 5 alone: Acuity puts the Laetum's head at
22% x (1 + 3.498) = 98.96% crit chance, and the flat -10% is what lets a head
read white at all.

## What it settles

**Rank 10 is `0.318 x 11 = 3.498`**, and the rule the wiki states holds
around it: on a weak point that is a head it is added to the part's
multiplier at 1.5x, and the sum multiplies the critical-headshot factor.

```
head                      3 + 1.5 x 3.498            = 8.247
critical headshot, tier 1 1 + (2 x 2.203663 - 1)     = 4.407326   (M60, M117: base crit damage on the 32/4095 grid)

1   160 x 8.247 x 4.407326 =  5815.6   read  5816
2   100 x 8.247 x 4.407326 =  3634.7   read  3635      explosion 300, no head, no Acuity
3   512 x 8.247 x 4.407326 = 18609.8   read 18610
4   512 x 3     x 21       = 32256     read 32256
5   512 x 8.247 x 21       = 88671.7   read 88672
```

With the card's 3.5 the same lines read 5818, 3636, 18616, 32256 and 88704.

### How the layers were told apart

Lines 1 to 3 all sat about 0.03% under a model built on 3.5, by one factor
across both forms, three weapons' worth of base damage, and two damage
vectors (Impact + Slash, Impact alone). Uniform means no layer that depends
on the damage type, the form or the base. Each remaining layer was ruled in
or out by a reading that has it alone:

- **The target.** The explosion reads exactly 300, and a Deimos Runner has no
  modifier for the three types: nothing on the enemy's side.
- **Crit damage quantization.** M60's two unmodded critical headshots and
  M117's three crits take the 32/4095 grid to the digit with no Acuity, and
  line 4 is exact with no crit at all.
- **The head itself.** Line 4 is a 3x head, Hornet Strike and Devouring's x21
  with nothing rounded away.
- **Acuity on the head, or Acuity under a crit.** Lines 1 to 3 cannot tell
  these apart: each is a crit. Line 5 is white, and it is Acuity's 8.247
  (88672) and not 8.25 under a crit-side error (88704). A uniform x4095/4096
  on the head was a candidate, too: it reads 18611 on line 3 and 88677 on
  line 5, and both rule it out.

### The card says so too

The card's table, identical in the wiki and in DE's export, is
`32 64 95 127 159 191 223 254 286 318 350`. Rank `r` is `(r + 1) x` a
per-rank value, rounded for display. The eleven rows together put that value
in **31.786% to 31.8125%**, and 0.318 is inside. An exact 350/11 = 31.818% is
not: its rank 7 is 254.55%, shown 255, and the card shows **254**. This is
evidence independent of the readings and it gives the same answer.

### What it does not read

- **The crit chance half** is 3.498 as well, and so is **Primary Acuity** in
  both halves: the package says so (below). None of those numbers has been
  read; the 0.044-point difference in crit chance on a Laetum head is out of
  reach of any count of crits.

### The game's package says so too

The package stores both Acuity lines as `0.31799999`, the float32 of 0.318,
a rank, eleven ranks: 3.498 for the weak point damage and the weak point crit
chance alike, on both cards. That and every other card's value a rank is M124.

### M60's unexplained 0.19% was crit damage quantization

M60 left its six crit numbers +0.14% to +0.21% above `base x tier` and open.
That is the 32/4095 grid on the base crit damage (2.2 is 2.203663 there),
which M117 settled later. Five of the six take it to the digit, and one is a
point and a quarter off:

| head | yellow `x 4.407326` | read | orange `x 7.814652` | read |
|---|---|---|---|---|
| 240 | 1057.8 | 1058 | 1875.5 | 1876 |
| 360 | 1586.6 | 1587 | 2813.3 | **2812** |
| 432 | 1904.0 | 1904 | 3375.9 | 3376 |

Tests: `fight::tests::m123_laetum_acuity`,
`data::mods::tests::catalog` (Pistol Acuity's two effects at 3.498).
