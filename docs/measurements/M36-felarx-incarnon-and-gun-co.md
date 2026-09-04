# M36 — the Felarx's +2000% and Gun CO multiply ✅ (owner, 2026-08-08)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Measured in game** ("我已经测试过了"). Devastating Attrition's 50% chance of
+2000% on a non-critical hit and CONDITION OVERLOAD are two independent
multipliers on this weapon; they do not share a bracket.

Both sources agree with the measurement, which is why it was worth checking
rather than assuming — agreement is cheap and a shared bracket would have been
invisible:

- the perk's own wiki note: *"Damage bonus is multiplicative to base damage
  bonuses such as Serration"* — so it is not in the base-damage bucket;
- the CO catalog lists the **Felarx** among the Multiplying entries, and the
  owner confirmed the answer is the same on BOTH of its modes, so CO is not in
  that bucket either on this weapon.

Two terms, neither in the bucket, nothing to share.

### What it pins

`raw = qtotal x part x crit x CO x faction x arcane x ATTRITION x ramp x ...` —
the two are separate factors in one product, and
`devastating_attrition_multiplies_with_gun_condition_overload` asserts it as a
RATIO OF RATIOS: whatever the perk is worth alone and whatever CO is worth
alone, having both must be worth their product. It also asserts the product is
nowhere near the additive reading, which is the answer being ruled out. The
perk's own 50% is replaced by 1.0 inside the test — a coin flip in the middle of
a measurement would need thousands of runs to say anything, and the question is
about the bracket rather than the odds.

### Why this weapon and not a rule

The CO half is per-weapon: the catalog lists the anomalies, and a weapon absent
from it is Adding. On a weapon where CO is Adding, its share of the damage joins
the base-damage bucket and this multiplication does not arise — the Attrition
term still multiplies, but there is no second free-standing factor for it to
multiply with.
