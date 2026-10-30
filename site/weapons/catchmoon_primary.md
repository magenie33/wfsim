# Catchmoon

Chinese name: 捕月

Shotgun · Primary · Mastery Rank 0. 216 base damage (heat 126, impact 90), 21% crit chance, 2x crit multiplier, 21% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-30

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 1.6226 | Incendiary Coat, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Shotgun Elementalist, Magnetic Strafe, Pax Seeker, Primary Crux | 2026-10-27 12:34 UTC · 14c617506965 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 8.4878 | Incendiary Coat, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Shotgun Elementalist, Magnetic Strafe, Pax Seeker, Primary Crux | 2026-10-27 12:55 UTC · 14c617506965 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.1635 | Incendiary Coat, Blaze, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Shotgun Elementalist, Pax Seeker, Primary Crux | 2026-10-27 12:59 UTC · 14c617506965 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.5788 | Incendiary Coat, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Shotgun Elementalist, Magnetic Strafe, Pax Seeker, Primary Crux | 2026-10-27 12:35 UTC · 14c617506965 |

## Not modelled here

- The guaranteed Impact proc reaches only 9 m (further with Projectile Speed mods) but is applied at every distance here.
- The projectile has a width, smaller than the secondary's; the sim fires a ray, so with infinite punch through only bodies on the line are hit.
- Its 42 m range is the projectile's 0.42 s lifetime, so Projectile Speed mods extend it in game; here they move the falloff and the cut-off stays at 42 m.
- Eclipse's damage buff does not reach this chamber in game (wiki, Bugs); here a buff reaches every weapon, so with Eclipse up the number reads high.
- The projectile has travel time and must be led at range (wiki). Every shot here connects instantly, so against a moving target the real hit rate is lower.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/catchmoon_primary
- Every published board row, as JSON: https://wfsim.app/board/catchmoon_primary.json
