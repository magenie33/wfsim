# M98 — Overguard takes Void ×1.5 on any unit; a Thrax Centurion's body takes it too, and the two never stack (owner, 2026-09-15)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

One weapon, one shot per reading, with and without Xata's Whisper at 100%
Ability Strength, which adds a Void instance to the hit. Each cell is
the weapon's own number, then the added Void number beside it. "Body" is the
unit after its armour was removed, so the armour term is not in the reading.

| target | no ability | on Overguard | on the body |
| --- | --- | --- | --- |
| Thrax Centurion | 35 | 35 + 14 | 35 + 14 |
| another Overguard unit | 35 | 35 + 14 | 35 + 9 |

### What it settles

- **Overguard is ×1.5 Void on every unit.** The other unit's Void number is 9
  on its body and 14 on its Overguard: `9 × 1.5 = 13.5`, displayed 14.
- **A Thrax's BODY is ×1.5 Void as well.** 14 on the body where the other unit
  shows 9. This is the Zariman column the unit borrows through
  `FactionDamageOverride`, while its `Faction` stays Unknown.
- **The two do not multiply.** A Thrax's Overguard reads 14, not
  `9 × 1.5 × 1.5 = 20.25`. Damage on Overguard reads the Overguard column and
  never the unit's own.

### What is implemented

Already the engine's rule, so nothing moved:

- `engine::data::factions` gives Overguard its own column (neutral, Void ×1.5),
  and `engine::dummy` routes the pool to its column — "Overguard … is a layer
  over the unit rather than part of it, so the unit's own column never reaches
  it."
- `data/enemies/thrax_centurion.yaml` carries `faction: unknown` with
  `faction_damage_override: zariman`, so the body reads the Zariman column's
  Void ×1.5 and no faction mod applies.

The Overguard half is asserted in `engine/src/fight/` (Void ×1.5 on Overguard whatever
the unit's faction). The body half and the no-stacking half have no test of
their own; this entry is their source.
