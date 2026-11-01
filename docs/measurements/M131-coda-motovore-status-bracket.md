# M131 — Coda Motovore status bracket and physical mod potency (owner, 2026-11-01)

The owner reported two in-game panel readings while checking Coda Motovore:

| Setup | Reported reading |
| --- | --- |
| No physical damage mods, max-rank Melee Prowess | 87% Status Chance |
| Collision Force selected on the weapon | 720% physical damage on the mod card |

The progenitor element/roll and other loadout details were not supplied.
Neither reading is a damage pop-up or an observed proc frequency.

## Status chance

The weapon's 30% base, its default Slash wheel's +100% and Melee Prowess's
+90% reproduce the panel as `0.30 × (1 + 1.00 + 0.90) = 0.87`.
Multiplying the wheel into the base first would instead give
`0.30 × 2 × 1.90 = 1.14`, which does not match the reported panel.
The wheel therefore joins the ordinary status-chance mod bracket in this
setup; the weapon's stored base remains 30%.

## Physical mod potency

Collision Force's ordinary +120% becomes +720%, consistent with the wiki's
sixfold physical-mod potency. The model applies that coefficient to the
physical mod's damage type and leaves the base used by elemental mods alone.
This reading checks the coefficient, not a final hit's damage or quantization.
The site retains its ordinary mod cards; the extra potency is resolved in the
shared engine rather than represented by a separate weapon-specific card.

Attack-speed/range stacking, exact physical-type ties, negative physical
rivens, and progenitor Impact's wheel selection remain unmeasured. The heavy
charge and slam-source differences are also outside these readings.

Golden fixture: `tests/golden/M131-coda-motovore.json`.
Test: `build::loadout::tests::physical_mods::golden_m131_motovore_panel_readings`.
