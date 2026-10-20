# M117 — the Vectis Prime: the chamber, the shot combo counter, the Incarnon orb, Cold on Area of Effect ✅ (owner, 2026-10-20)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Vectis Prime with Inciting Incident (+200 base damage) and Critical Parallel
(+0.5x crit damage), Serration, Charged Chamber where named. Non-crit numbers
unless marked. Two targets: the Simulacrum's neutral one, and a Corrupted
Heavy Gunner fully stripped of armour (Puncture weakness, head x3).

## The readings

**Base form, neutral target.**

| setup | first round of the magazine | other rounds |
|---|---|---|
| Serration, Charged Chamber | 2041 | 1458 |
| + Lavos's Valence Formation, +200% Heat — the hit | 6122 | 4373 |
| …its Heat tick | 3062 | 2188 |
| shot combo counter at x1.5 | 3061 | 2186 |
| shot combo counter at x2 | 4081 | 2915 |

**Base form, Heavy Gunner.** 2487 / 1776 on the body. Energized Munitions
(75%), unscoped, eight rounds from a full magazine: 4442 (crit), 4442 (crit),
1776, 2487, 2487, 6219 (crit), 2487, 4442 (crit), then the reload. Valence
Heat: 6568 / 3062 and 4691 / 2188; at x1.5, 9852 / 4593. Deadhead, scoped
head: 14175 / 10125.

**Incarnon form, Heavy Gunner.**

| shot | numbers |
|---|---|
| body | 928 and 596, then 596 a second later, then 596 a second after that; several shots never merge |
| body, first round of a full Incarnon magazine | the orb reads 835 on all three |
| scoped head | 4452 + 596; ~27 frames at 60 fps later, 5195 (crit) + 1484 |
| scoped head, 6 Cold stacks | 34288 (crit) + 2087 (crit); then 1484 + 1484 |
| scoped head, x2 combo, 3 / 5 / 7 / 10 Cold | 8904 + 1193; 2968 x 2; 69466 (crit) + 4175; 8904 + 4175 (crit); 2968 + 10391 (crit) |
| unscoped head, x2, frozen | 6494 x 2 (crit) |
| scoped head, Deadhead, x1 | the explosion 1762 |

Only the direct hit builds the counter; its multiplier carries across the
transform both ways. The counter pays scoped or not, builds only scoped, and
falls one point at a time on its timer or on a miss while scoped; a miss from
the hip takes nothing.

**Akarius Prime**, Valence Cold: the direct crit climbs with the Cold stacks,
the explosion's crit stays 3055.

## What it settles

- **The chamber** is its own factor beside Serration's bracket:
  `(350 + 200) x 2.65 = 1457.5`, `x 1.4 = 2040.5`. It reaches the status tick
  for Charged Chamber as for Primed (`x 1.4` on 2188). Its gate is the
  magazine counter the HUD draws — the fraction rounded up — reading max - 1
  after the round: under Energized Munitions rounds 4 to 7 pay, 1.0, 0.75, 0.5
  and 0.25 left all reading 1 (`fight::resolve`).
- **Crit damage is quantized** before the crit-damage mods: 3.5 is 3.50085 and
  2.5 is 2.50061 (MECHANICS §Quantization). 5195 = 1484 x 3.50085, 4442 =
  1776.33 x 2.50061, 34288 = 4452 x 2 x (3.50085 + 0.35).
- **Cold's crit damage reaches no Area of Effect damage** — the embedded orb,
  its explosion and the Akarius Prime's explosion read the bare multiplier at
  any stack count — as the wiki states for Cold and for Puncture's crit chance.
- **The shot combo counter** multiplies every part of a shot and its status
  ticks (`x 1.5` on 3062); `ResolvedPanel::sniper_combo_builds` is the aiming
  gate.
- **Deadhead is +30%**, against the card's +25%: 10125 = 1776.33 x 3 x 1.9,
  1762 = 927.5 x 1.9.
- **The embedded orb** (MECHANICS §7.5): 25 + 200 = 225 base; in a body it hits
  three times a second apart, in a head it hits once and explodes 0.4 s later
  in two hits of 150 + 200, taking the headshot bracket (x1.6 scoped, x1.9 with
  Deadhead, x1 from the hip) and not the head's x3. Every hit carries the
  shot's chamber and combo.

Tests: `fight::tests::m117_vectis_prime_chamber`.
