# M128 — Aksondol: the Cold wave applies at reload start ✅ (owner, 2026-10-27)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md).*

## The reading

The reload's area Cold effect applies at the instant the reload starts.
It does not wait for ammunition to enter the magazine or for the animation to end.

## Other rules and evidence

The [Aksondol page](https://wiki.warframe.com/w/Aksondol) specifies an instant
wave centred on the wielder, with no lingering field: one Cold proc and one
metre of base radius per round expended. Fulmination scales its radius; status
duration scales Cold and Frozen. The wave requires line of sight. The arena
has no occluders.

The wave does not fire Primary Frostbite, Shivering Contagion or Cascadia
Empowered. Secondary Encumber can add one random proc within its simultaneous
proc limit. Its bonus status uses the weapon's modified base and live damage
buffs, with no crit or body-part multiplier on the wave (owner, 2026-10-27).
Arcane Ice Storm is independently unmodelled.

Only the timing has an owner measurement. The proc count, range, duration
scaling and trigger interactions follow the wiki and remain unverified in game.

## Where it lives

`attack.reload_cold_radius_per_round_m` declares the wave. `fight::reload`
schedules it before the reload advances the clock, and `fight::entities`
settles it in time order, including a reload the engagement ends inside.
`fight::tests::reload_cold` covers timing, radius, duration, partial reloads,
status restrictions and trigger exclusions.
