# M17 — Do Arch-Guns have an exilus slot, and is Zodiac Shred eligible?

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** Two sources disagree about one Arch-Gun mod, and the answer is
worth a slot of free damage.

| source | says |
|---|---|
| wiki `Module:Mods/data` | `IsExilus: True` for **Zodiac Shred** (+90% Slash) |
| wiki `Category:Exilus_Weapon_Mods` | Zodiac Shred is **not** in the list — and the 74 that are, are ALL utility (ammo, zoom, recoil, silence, speed). Not one grants damage. |
| WFCD export | carries no exilus field for any Arch-Gun mod — silent, not a vote |
| wiki `Arch-Gun` page | does not say whether Arch-Guns have an exilus slot at all |

**Implemented: `exilus: false`**, which is the safer error — a wrong `true`
puts a damage mod in a free slot, a wrong `false` only costs an option. The
module's flag is otherwise perfect (153/153 against our verified rifle and
pistol pools), which is exactly why this one row is recorded rather than
quietly trusted. `verify_mods.py --type Archgun` reports the divergence on
every run BY DESIGN: it is an open question, and it should stay visible.

**How to settle it.** Equip any Arch-Gun with an Exilus Weapon Adapter (if the
slot exists at all) and try to seat Zodiac Shred in it. One look answers both
halves — whether the slot exists, and whether this mod may enter it.

**Until then** no weapon in the roster is an Arch-Gun, so nothing depends on
it; it becomes live the moment one is added.
