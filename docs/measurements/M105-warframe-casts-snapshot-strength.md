# M105 — A Warframe cast snapshots Ability Strength; Transference is 1 s; Power Ramp drops to zero on a repeat (owner, 2026-09-26)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Three readings from the owner's play, stated as they were given.

| question | reading |
| --- | --- |
| Does a Warframe buff read Ability Strength live? | No: every one SNAPSHOTS. Valkyr's claws take the Ability Strength Hysteria was cast at and keep it for the whole of the ability. |
| Transference out and back | 1 s |
| Arcane Power Ramp on the same ability cast twice running | drops to zero at once |

### What it settles

- **One entry per cast** (`data::casting::plan`): a strength window that
  lapses after a cast does not reach back into it, and one that opens after
  it earns that cast nothing. An Exalted weapon's damage is its summoning
  cast's snapshot (`Tenno::summon_strength`), so Sling Strength pays the claws
  only when it is up at the Hysteria cast.
- `TRANSFERENCE_SECONDS` is 1.0. The Chained Sling and the school's ability
  inside the trip are still unmeasured.
- Power Ramp's repeated cast gains nothing and arms no stack of its own; the
  next different ability starts from zero.
