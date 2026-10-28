# Stug

Chinese name: 史特克

Pistol · Secondary · Mastery Rank 2. 4 base damage (corrosive 4), 5% crit chance, 1.5x crit multiplier, 10% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-29

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | cycle | 6.5100 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Anemic Agility, Cascadia Flare | 2026-10-26 14:55 UTC · 14c617506965 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | cycle | 26.1090 | Primed Convulsion, Jolt, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Galvanized Shot, Lethal Torrent, Pistol Elementalist, Lethal Momentum, Secondary Enervate | 2026-10-26 12:05 UTC · 14c617506965 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | cycle | 1.7715 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Anemic Agility, Cascadia Flare | 2026-10-26 15:03 UTC · 14c617506965 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | cycle | 4.8639 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Anemic Agility, Cascadia Flare | 2026-10-26 15:03 UTC · 14c617506965 |

## Not modelled here

- Bouncing Incarnon child projectiles are assumed to leave the fight and hit nobody, including crowds (M127). Their damage and parameters are recorded.
- Attachment is resolved per body part; separate nearby patches on one part are not modelled (M127).
- Unmeasured: spawn-time projectile quantities beyond one full blob are capped here; the emitter's handling of the remainder is not traced (M109).
- Unmeasured: the explosion's falloff is read as 70%, from the page's bug note, rather than the 30% in its table.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Stug
- Every published board row, as JSON: https://wfsim.app/board/stug.json
