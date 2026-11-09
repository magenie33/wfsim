# M121 — Haalvu: a battery stopped short refills after 0.5 s at 300 a second, and an empty one waits 2 s ✅ (owner, 2026-10-23)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Parsed-input adoption, 2026-11-09.** The owner requests parsed primary timing
on conflicts. Haalvu's bound record configures `BatteryRegenDelayFromEmpty=2`
and `BatteryRegenRate=300`; the calibrated empty refill is 2 + 150/300 = 2.5 s.
Its 0.5 s partial delay remains separate, and the 2 s empty animation is a lower
bound rather than a replacement for this configured refill. The original
reported readiness below remains provenance; no new in-game timing was measured.

## The readings

- **Firing stopped with rounds left:** regeneration starts 0.5 s after the last
  shot and runs at 300 rounds a second until the battery is full.
- **Fired dry:** 2 s before the weapon fires again.

## The rule

The page's two sentences are both right, and they describe different cases:
"Has a 0.5 second delay after the weapon stops firing before regenerating
ammo. Then, regenerates 300 ammo per second", and "If ammo is fully depleted,
the weapon will enter a reload animation of 2 seconds." The wiki module's
`Reload = 1` and the export's `reloadTime: 1` are the delay plus 150/300 s of
refill, which the empty case does not pay: it pays the animation.

So one shot short of empty is a 1 s refill and empty is 2 s, and the most the
weapon deals is fired in runs that stop one shot short — a mode of its own on
each form (`base_recharge`, `alternate_recharge`).

## Where it lives

- `data/weapons/primary/haalvu.yaml`, `haalvu_alt.yaml`: `battery`,
  `empty_animation_seconds: 2.0`, `reload_seconds: 2`, and the declared modes.
- Pinned by `fight::tests::battery_recharge`.
