# Sporelacer

Chinese name: 孢射

Pistol · Secondary · Mastery Rank 0. 0 base damage (), 21% crit chance, 3x crit multiplier, 21% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 3.2209 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Pax Charge, Cascadia Overcharge | 2026-09-29 11:09 UTC · 9b0496567c |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 6.0990 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Pax Charge, Cascadia Overcharge | 2026-10-05 20:06 UTC · 40071ee3b3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.6702 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Pax Charge, Cascadia Overcharge | 2026-09-29 10:45 UTC · 9b0496567c |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 1.9233 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Pax Charge, Cascadia Overcharge | 2026-10-08 23:51 UTC · 7cbf9a5d27 |

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
