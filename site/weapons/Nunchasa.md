# Nunchasa

Bow · Primary · Mastery Rank 14. 400 base damage (cold 200, puncture 200), 32% crit chance, 2.4x crit multiplier, 32% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-25

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 8.7265 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Serration, Galvanized Scope, Critical Delay, Vital Sense, Radiated Reload, Vigilante Supplies, Longbow Sharpshot | 2026-10-24 06:01 UTC · dac6ecbaf97e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 31.5988 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Serration, Galvanized Scope, Critical Delay, Vital Sense, Radiated Reload, Vigilante Supplies, Longbow Sharpshot | 2026-10-24 05:40 UTC · dac6ecbaf97e |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.4196 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Serration, Critical Delay, Hunter Munitions, Vital Sense, Radiated Reload, Vigilante Supplies, Longbow Sharpshot | 2026-10-24 05:40 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 3.6178 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Serration, Galvanized Scope, Critical Delay, Vital Sense, Radiated Reload, Vigilante Supplies, Longbow Sharpshot | 2026-10-24 05:40 UTC · dac6ecbaf97e |

## Not modelled here

- The alt-fire, a second charged shot, is not modelled: a weapon holds one charged form. It is 1.5 s of draw for 1,000 damage (500 Cold, 250 Puncture, 250 Slash) at 36% crit, 3.0x, 36% status and 2 m punch through, against this draw's 400 at 32% and 2.4x. It also leaves a chilling path behind the arrow with no published numbers, a trail the engine has no form for.
- The draw time is not published: the module gives a charge time for the alt-fire (1.50 s) and none for this attack, so the cadence is the 0.6 s nock alone. A real draw can only be slower, so every rate here is a ceiling.
- The projectile has travel time and must be led at range (wiki). Every shot here connects instantly, so against a moving target the real hit rate is lower.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Nunchasa
- Every published board row, as JSON: https://wfsim.app/board/nunchasa.json
