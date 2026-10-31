# Tenet Envoy

Chinese name: 信条·典客

Launcher · Primary · Mastery Rank 16. 100 base damage (impact 100), 28% crit chance, 2.6x crit multiplier, 24% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-31

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 7.2957 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Heavy Caliber, Galvanized Aptitude, Galvanized Scope, Vile Acceleration, Magnetic Capacity, Vigilante Supplies, Primary Compression | 2026-10-22 12:48 UTC · 99dec9ad6d01 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 7.2957 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Heavy Caliber, Galvanized Aptitude, Galvanized Scope, Vile Acceleration, Magnetic Capacity, Vigilante Supplies, Primary Compression | 2026-10-24 03:36 UTC · db272037fe1d |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.4236 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Heavy Caliber, Galvanized Aptitude, Critical Delay, Vile Acceleration, Vital Sense, Vigilante Supplies, Primary Compression | 2026-10-24 03:12 UTC · db272037fe1d |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 4.6875 | Primed Cryo Rounds, Malignant Force, Galvanized Chamber, Heavy Caliber, Galvanized Aptitude, Critical Delay, Vile Acceleration, Vital Sense, Vigilante Supplies, Primary Compression | 2026-10-24 03:23 UTC · db272037fe1d |

## Not modelled here

- The blast needs no line of sight. There is no cover here, so the perk adds nothing.
- The rockets can be guided by aiming and fired from behind cover, at half speed while guided (20 m/s against 40). The target here does not move, so guidance changes nothing.
- A rocket that hits nothing detonates after 6 s; the sim does not model a miss.
- It reloads 10% of its magazine a second while holstered or deployed; it is never holstered here, so the reload is always the full one.
- In game the blast staggers the wielder; the player has no body here, so it costs nothing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Tenet_Envoy
- Every published board row, as JSON: https://wfsim.app/board/tenet_envoy.json
