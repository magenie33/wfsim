# M126 — Stug's Blazing Barrel clears on reload and either transform ✅ (owner, 2026-10-26)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Stug with its Incarnon adapter installed and Evolution II Blazing Barrel selected.
The owner observed the buff while firing, reloading and switching forms.
Other equipment and the target were not specified; these observations concern
the buff's lifecycle rather than damage numbers.

## The readings

| action or state | observed buff |
|---|---|
| Fire in base form | Stacks normally |
| Fire in Incarnon form | Stacks normally |
| Reload | Clears |
| Enter Incarnon form | Clears |
| Exit Incarnon form | Clears |
| Wait without a clearing action | No duration limit |

With the adapter installed, charged alternate fire is unavailable.

## What it settles

- Blazing Barrel uses one firing-fed buff in both forms, with no timer.
- Reload and either Incarnon transform clear the pile. Its reset condition is
  the magazine refill shared by those actions, rather than the reload-only
  condition used by Strun's same-named perk.
- Charged alternate fire is outside Stug's model; its absence with the adapter
  installed is not a missing attack to implement.

Tests: `fight::tests::cadence_ammo_multishot::m126_stug_blazing_barrel_resets_in_both_forms`.
