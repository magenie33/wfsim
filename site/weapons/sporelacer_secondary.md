# Sporelacer

Chinese name: 孢射

Pistol · Secondary · Mastery Rank 0. 0 base damage (), 21% crit chance, 3x crit multiplier, 21% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-31

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 8.0512 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Galvanized Shot, Lethal Torrent, Pistol Elementalist, Magnetic Might, Primed Steady Hands, Pax Charge, Secondary Enervate | 2026-10-22 12:26 UTC · 99dec9ad6d01 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 17.2219 | Primed Convulsion, Galvanized Diffusion, Hornet Strike, Galvanized Shot, Lethal Torrent, Pistol Elementalist, Pressurized Magazine, Magnetic Might, Primed Steady Hands, Pax Charge, Secondary Enervate | 2026-10-23 01:14 UTC · 62a82930d1d3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 1.3595 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Lethal Torrent, Pistol Elementalist, Pax Charge, Secondary Enervate | 2026-10-26 19:12 UTC · 14c617506965 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 3.8349 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Lethal Torrent, Magnetic Might, Pax Charge, Secondary Enervate | 2026-10-23 01:03 UTC · 62a82930d1d3 |

## Not modelled here

- The three bomblets the sac splits into are not fired. The module lists a Bomblet Impact and a 2.1 m Bomblet Explosion, both Toxin, and nothing prices the explosion. All the chamber's Toxin goes to the first explosion, which overpays it if the bomblets share that Toxin and underpays the shot if they add to it.
- The Condition Overload catalog prices this weapon's term on a fixed 57 damage whatever the grip, which cannot be expressed as a share of the direct hit. The direct hit takes the ordinary term on its own Impact instead: none on the default Gibber grip, 261 on Haymaker.
- The projectile falls off from 13 m to 26 m to an unpublished amount, so no falloff is applied.
- The sac arcs heavily, reduced by Projectile Speed; not modelled.
- The projectile has travel time and must be led at range (wiki). Every shot here connects instantly, so against a moving target the real hit rate is lower.
- In game the blast staggers the wielder; the player has no body here, so it costs nothing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/sporelacer_secondary
- Every published board row, as JSON: https://wfsim.app/board/sporelacer_secondary.json
