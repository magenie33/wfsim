# Arca Plasmor

Chinese name: 弧电离子枪

Shotgun · Primary · Mastery Rank 10. 600 base damage (radiation 600), 22% crit chance, 1.6x crit multiplier, 28% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-17

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.1141 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Shotgun Elementalist, Galvanized Acceleration, Primary Frostbite | 2026-10-11 15:25 UTC · a5018fcb4e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.1607 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Shotgun Elementalist, Galvanized Acceleration, Primary Frostbite | 2026-10-11 18:26 UTC · 2910a8eff8 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.007245 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Shotgun Elementalist, Galvanized Acceleration, Primary Frostbite | 2026-10-11 05:27 UTC |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.06016 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Shotgun Elementalist, Galvanized Acceleration, Primary Frostbite | 2026-10-12 10:33 UTC |

## Not modelled here

- The projectile is a wall 3 m thick (4.4 m on the Tenet variant) that sweeps everything in that band; the sim fires a ray, so only bodies on the line are hit and against a crowd the number reads low.
- The guaranteed Impact proc reaches only 14 m but is applied at every distance here, so past 14 m the number reads slightly high.
- Its innate punch through does not pass surfaces when the projectile's centre hits one; this fight has no surfaces, so this changes nothing here.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Arca_Plasmor
- Every published board row, as JSON: https://wfsim.app/board/arca_plasmor.json
