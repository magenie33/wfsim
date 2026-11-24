# Arca Plasmor

Chinese name: 弧电离子枪

Shotgun · Primary · Mastery Rank 10. 600 base damage (radiation 600), 22% crit chance, 1.6x crit multiplier, 28% status chance.

Current published rankings and revisions: https://wfsim.app/api/board/pages/catalog?weapon=arca_plasmor

## Archived riven-free leaders · 2026-11-07

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% weak-point aim · 180 s · KPM | base | 0.1480 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Vigilante Armaments, Galvanized Acceleration, Primary Frostbite | 2026-10-26 05:27 UTC · 3194962a0af7 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% weak-point aim · 180 s · KPM | base | 0.1881 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Vigilante Armaments, Galvanized Acceleration, Primary Frostbite | 2026-10-26 05:43 UTC · 3194962a0af7 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% weak-point aim · 4-player health · 180 s · KPM | base | 0.009576 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Vigilante Armaments, Galvanized Acceleration, Primary Frostbite | 2026-10-26 05:43 UTC · 3194962a0af7 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% weak-point aim · 180 s · KPM | base | 0.07775 | Primed Charged Shell, Toxic Barrage, Primed Chilling Grasp, Galvanized Hell, Primed Point Blank, Primed Ravage, Critical Deceleration, Vigilante Armaments, Galvanized Acceleration, Primary Frostbite | 2026-10-26 05:43 UTC · 3194962a0af7 |

## Not modelled here

- Native projectile collision dimensions are bound as evidence; unverified width and sweep consumers retain the declared collision approximation.
- Bound standing 3D profiles use shooting-channel spread growth, recovery and smoothing. Equip transitions, moving poses and unmatched profiles retain limits. The player holds the reticle steady.
- The projectile is a wall 3 m thick (4.4 m on the Tenet variant) that sweeps everything in that band; the sim fires a ray, so only bodies on the line are hit and against a crowd the number reads low.
- The guaranteed Impact proc reaches only 14 m but is applied at every distance here, so past 14 m the number reads slightly high.
- Its innate punch through does not pass surfaces when the projectile's centre hits one; this fight has no surfaces, so this changes nothing here.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Arca_Plasmor
- Archived board JSON: https://wfsim.app/board/arca_plasmor.json
- [Using and citing WFSim](https://wfsim.app/reuse)
