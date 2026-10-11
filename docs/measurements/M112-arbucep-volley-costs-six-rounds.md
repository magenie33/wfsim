# M112 — an Arbucep pull spends six rounds ✅ (owner, 2026-10-12)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**THE READING.** One pull of the trigger fires the six missiles together and
takes **6** from the magazine — one round a missile. The 36 magazine is six
volleys, not 36.

**WHAT IT SETTLES.** The wiki's infobox gives no ammo cost for the weapon, and
the entry had carried the default of 1. `attack.ammo_cost` is now 6 in
`data/weapons/archgun/arbucep.yaml`. It also prices the Atmosphere column's
non-resupplied 540 reserve at 90 volleys.
