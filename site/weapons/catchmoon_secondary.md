# Catchmoon

Chinese name: 捕月

Pistol · Secondary · Mastery Rank 0. 290 base damage (heat 184, impact 106), 21% crit chance, 2x crit multiplier, 21% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.08746 | Primed Heated Charge, Pistol Pestilence, Frostbite, Galvanized Diffusion, Primed Target Cracker, Galvanized Shot, Lethal Torrent, Magnetic Might, Pax Bolt, Akimbo Slip Shot | 2026-09-29 12:13 UTC · 4507901cf6 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.08786 | Primed Heated Charge, Pistol Pestilence, Frostbite, Galvanized Diffusion, Primed Target Cracker, Galvanized Shot, Lethal Torrent, Magnetic Might, Pax Bolt, Akimbo Slip Shot | 2026-10-06 02:58 UTC · 38faffec31 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.0004955 | Primed Heated Charge, Pistol Pestilence, Frostbite, Galvanized Diffusion, Primed Target Cracker, Galvanized Shot, Lethal Torrent, Magnetic Might, Pax Bolt, Akimbo Slip Shot | 2026-09-29 11:48 UTC · 4507901cf6 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.04610 | Primed Heated Charge, Pistol Pestilence, Frostbite, Galvanized Diffusion, Primed Target Cracker, Galvanized Shot, Lethal Torrent, Magnetic Might, Pax Bolt, Akimbo Slip Shot | 2026-10-09 09:29 UTC |

## Not modelled here

- The guaranteed Impact proc reaches only 9 m (further with Projectile Speed mods) but is applied at every distance here.
- The projectile is wide, like the Arca Plasmor's (page); the sim fires a ray, so with infinite punch through only bodies on the line are hit.
- Its 20 m range is the projectile's 0.42 s lifetime, so Projectile Speed mods extend it in game; here they move the falloff and the cut-off stays at 20 m.
- Eclipse's damage buff does not reach this chamber in game (wiki, Bugs); here a buff reaches every weapon, so with Eclipse up the number reads high.
- The projectile has travel time and must be led at range (wiki). Every shot here connects instantly, so against a moving target the real hit rate is lower.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/catchmoon_secondary
- Every published board row, as JSON: https://wfsim.app/board/catchmoon_secondary.json
