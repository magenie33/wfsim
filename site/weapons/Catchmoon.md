# Catchmoon

Chinese name: 捕月

Shotgun · Primary · Mastery Rank 0. 216 base damage (heat 126, impact 90), 21% crit chance, 2x crit multiplier, 21% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.2080 | Blaze, Toxic Barrage, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Magnetic Strafe, Pax Bolt, Fractalized Reset | 2026-09-22 17:52 UTC · 96748ef976 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.2086 | Blaze, Toxic Barrage, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Magnetic Strafe, Pax Bolt, Fractalized Reset | 2026-10-06 08:55 UTC · 92ec4b1a70 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.001185 | Blaze, Toxic Barrage, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Magnetic Strafe, Pax Bolt, Fractalized Reset | 2026-09-22 17:39 UTC · 96748ef976 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.08874 | Blaze, Toxic Barrage, Galvanized Hell, Primed Point Blank, Primed Ravage, Galvanized Savvy, Critical Deceleration, Magnetic Strafe, Pax Bolt, Fractalized Reset | 2026-10-09 12:59 UTC · 395e2b5c61 |

## Not modelled here

- The guaranteed Impact proc reaches only 9 m (further with Projectile Speed mods) but is applied at every distance here.
- The projectile has a width, smaller than the secondary's; the sim fires a ray, so with infinite punch through only bodies on the line are hit.
- Its 42 m range is the projectile's 0.42 s lifetime, so Projectile Speed mods extend it in game; here they move the falloff and the cut-off stays at 42 m.
- Eclipse's damage buff does not reach this chamber in game (wiki, Bugs); here a buff reaches every weapon, so with Eclipse up the number reads high.
- The projectile has travel time and must be led at range (wiki). Every shot here connects instantly, so against a moving target the real hit rate is lower.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Catchmoon
- Every published board row, as JSON: https://wfsim.app/board/catchmoon_primary.json
