# Tenet Arca Plasmor

Chinese name: 信条·弧电离子枪

Shotgun · Primary · Mastery Rank 16. 760 base damage (radiation 760), 22% crit chance, 2x crit multiplier, 34% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 3.6033 | Primed Chilling Grasp, Toxic Barrage, Galvanized Hell, Magnetic Welt, Primed Point Blank, Galvanized Savvy, Shotgun Barrage, Shotgun Elementalist, Galvanized Acceleration, Shotgun Vendetta | 2026-09-29 21:47 UTC · 92c94eb9a6 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 16.6358 | Frigid Blast, Toxic Barrage, Incendiary Coat, Blaze, Galvanized Hell, Primed Point Blank, Galvanized Savvy, Shotgun Barrage, Primary Crux | 2026-10-08 04:03 UTC |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.4173 | Frigid Blast, Toxic Barrage, Incendiary Coat, Blaze, Galvanized Hell, Primed Point Blank, Galvanized Savvy, Shotgun Barrage, Primary Crux | 2026-10-08 04:05 UTC |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 1.1881 | Frigid Blast, Toxic Barrage, Incendiary Coat, Blaze, Galvanized Hell, Primed Point Blank, Galvanized Savvy, Shotgun Barrage, Primary Crux | 2026-10-08 19:22 UTC · 531aca13c9 |

## Not modelled here

- The projectile is a wall 4.4 m thick that sweeps everything in that band; the sim fires a ray, so only bodies on the line are hit and against a crowd the number reads low.
- Shots bounce off surfaces up to 4 times, which can double a shot's hits off a wall behind the target (page); there are no surfaces here.
- The guaranteed Impact proc reaches only 29 m but is applied at every distance here; past 29 m the damage is already at its falloff floor.
- Its innate punch through does not pass surfaces when the projectile's centre hits one; there are no surfaces here.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Tenet_Arca_Plasmor
- Every published board row, as JSON: https://wfsim.app/board/tenet_arca_plasmor.json
