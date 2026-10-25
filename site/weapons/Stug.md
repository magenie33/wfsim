# Stug

Chinese name: 史特克

Pistol · Secondary · Mastery Rank 2. 4 base damage (corrosive 4), 5% crit chance, 1.5x crit multiplier, 10% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-25

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | cycle | 9.9693 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Galvanized Crosshairs, Anemic Agility, Secondary Deadhead | 2026-10-24 02:20 UTC · 5c64c380e5f2 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | cycle | 9.9320 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Hornet Strike, Primed Target Cracker, Galvanized Crosshairs, Anemic Agility, Secondary Deadhead | 2026-10-24 03:49 UTC · 5c64c380e5f2 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | cycle | 2.9654 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Anemic Agility, Lethal Momentum, Cascadia Flare | 2026-10-24 02:20 UTC · 5c64c380e5f2 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | cycle | 7.4144 | Frostbite, Pistol Pestilence, Primed Heated Charge, Galvanized Diffusion, Primed Target Cracker, Primed Pistol Gambit, Lethal Torrent, Anemic Agility, Lethal Momentum, Cascadia Flare | 2026-10-24 02:15 UTC · 5c64c380e5f2 |

## Not modelled here

- The Incarnon form's bouncing secondary projectiles are not modelled: a stacked shot releases 2 to 5, each exploding on up to 3 bounces.
- The charged alt-fire is not modelled: a full 10-stack blob over 2 s, for 10 ammo.
- Unmeasured: how many stacks a shot lays. Read as floor(multishot), the count the Grimoire's multishot buys; rolling the fraction would lay more.
- Unmeasured: a shot already over the cap (multishot over 10) is read as holding the pile at 10 and waiting 1.5 s, refreshed by each later shot, so steady fire never sets it off.
- Unmeasured: a shot that crosses the cap is read as setting the pile off at once for 10 stacks, the rest lost.
- Unmeasured: the explosion's falloff is read as 70%, from the page's bug note, rather than the 30% in its table.
- Unmeasured: when a body dies, its pile is read as staying where it stood and going off on whoever is there.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Stug
- Every published board row, as JSON: https://wfsim.app/board/stug.json
