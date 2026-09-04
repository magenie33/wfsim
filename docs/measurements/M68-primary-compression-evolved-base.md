# M68 — Primary Compression pays the EVOLVED base into the base-damage bucket, on the form that carries the AoE ✅ (owner, 2026-08-31)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Burston Prime**, Primary Compression at **rank 1** (the card reads +60%
damage and +3.5% ammo efficiency per metre), Serration (+165%), Incarnon form
with a tier-2 evolution (+42 base damage). The HUD's own readout carries the
metres: a 2.0 m radius, a fifth of it kept, **1.6 m lost → +96%**.

| shot | reading | engine |
| --- | --- | --- |
| base form, aimed and not | 240 / 432, unchanged | no row — nothing to compress |
| Incarnon, from the hip | 146 / 437 | 145.75, x3.0 = 437.25 |
| Incarnon, aimed | 199 / 596 | 198.55, x3.0 = 595.65 |

The arithmetic is one pair of numbers and it settles four columns at once:

```
hip     55 x (1 + 1.65)        = 145.75
aimed   55 x (1 + 1.65 + 0.96) = 198.55
```

- **THE BASE IS THE EVOLVED 55**, 13 + the tier-2 perk's +42 — and not the 13
  that the CO term reads on the same attack (M48). One attack, two bases, and
  only CO gets the smaller one.
- **`Adds` is real.** The bonus joins Serration's bucket and is diluted by it.
  A `Multiplies` row on this build would read 145.75 x 1.96 = **286**, which is
  87 above what the game shows.
- **Effectiveness is 100% of the attack's own radius.** Any other radius, and
  any discount on the payment, moves the +96% the HUD prints.
- **THE RANK RAMP INTERPOLATES.** The wiki publishes ranks 0 and 5 only; a
  rank-1 card reading +60% / +3.5% is 0.5 + (1.0 - 0.5) x 1/5 and
  0.03 + (0.055 - 0.03) x 1/5, i.e. the linear reading of the two endpoints.

**THE ROW BELONGS TO A FORM, and the base form is the control.** Burston
Prime's base form is absent from the table and carries no AoE, and aiming does
not move its number — which is what the engine does by reading the FIRING
form's row (`ap`, not the build's) rather than the weapon's.

**What is NOT settled here.** The explosion's own aimed number was not read.
The engine pays the radial the same bonus as the direct hit, which follows
from a row that names an attack rather than a part, and no reading covers it.

And the base form's `240 / 432` is not reconciled: 88 x 2.65 is 233.2, and
432/240 = 1.8 is not that form's crit multiplier, so the pair is not one
condition's white and crit. The Incarnon's readings are pure Heat and match to
the digit, which is why they carry this entry; the base form's are IPS into a
target whose resistances the reading does not name. Only the half that is load
bearing — the number does not move on aim — is used.
