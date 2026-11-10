# Nataruk

Bow · Primary · Mastery Rank 0. 900 base damage (puncture 648, slash 252), 60% crit chance, 2.4x crit multiplier, 50% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-11

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | alternate | 9.4091 | Primed Cryo Rounds, Malignant Force, Hellfire, Heavy Caliber, Primary Acuity, Primed Fast Hands, Hammer Shot, Vital Sense, Vigilante Supplies, Longbow Sharpshot | 2026-11-06 20:23 UTC · d8787cb351c5 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | alternate | 36.0841 | Hellfire, Primary Acuity, Primed Shred, Serration, Galvanized Aptitude, Bladed Rounds, Vital Sense, Magnetic Capacity, Primary Crux | 2026-10-24 05:22 UTC · dac6ecbaf97e |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | alternate | 1.4164 | Hellfire, Thermite Rounds, Heavy Caliber, Primary Acuity, Primed Shred, Serration, Galvanized Aptitude, Vital Sense, Terminal Velocity, Primary Crux | 2026-10-22 18:44 UTC · 62a82930d1d3 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | alternate | 4.1336 | Hellfire, Thermite Rounds, Heavy Caliber, Primary Acuity, Primed Shred, Serration, Galvanized Aptitude, Vital Sense, Terminal Velocity, Primary Crux | 2026-10-24 05:18 UTC · dac6ecbaf97e |

## Not modelled here

- Native projectile collision dimensions are bound as evidence; unverified width and sweep consumers retain the declared collision approximation.
- The parsed fire packet's zero ammo requirement does not establish the state or script's resource debit; the declared ammo convention remains in use.
- The native fan, first-pellet or charge-dependent spread override is not simulated; the declared spread approximation remains in use.
- Sustained-fire spread growth, recovery and smoothing are not simulated; shots use the resting spread for the selected aiming state. Camera recoil is compensated by holding the reticle steady.
- This is the perfect release, not the ordinary full draw: the arsenal shows it and this entry carries its numbers. A mistimed release gets the module's second attack instead, a one-second draw for less damage, which is not carried. The engine has no way to describe a timing window, so one weapon cannot carry both.
- This weapon is silent; there is nobody to alert here, so stealth is worth nothing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Nataruk
- Every published board row, as JSON: https://wfsim.app/board/nataruk_perfect.json
