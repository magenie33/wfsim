# Vermisplicer

Chinese name: 虫置

Rifle · Primary · Mastery Rank 0. 33 base damage (impact 5, puncture 6, slash 10, toxin 12), 25% crit chance, 2x crit multiplier, 25% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-28

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 15.0053 | Primed Cryo Rounds, Malignant Force, Thermite Rounds, Primary Acuity, Galvanized Aptitude, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Deadhead | 2026-10-22 14:24 UTC · 99dec9ad6d01 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 19.7208 | Primed Cryo Rounds, Malignant Force, Thermite Rounds, Primary Acuity, Galvanized Aptitude, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Deadhead | 2026-10-24 06:44 UTC · dac6ecbaf97e |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.5297 | Malignant Force, Rime Rounds, Hellfire, Primary Acuity, Galvanized Aptitude, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Deadhead | 2026-10-24 08:41 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 8.7061 | Primed Cryo Rounds, Malignant Force, Thermite Rounds, Primary Acuity, Galvanized Aptitude, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Deadhead | 2026-10-24 09:16 UTC · dac6ecbaf97e |

## Not modelled here

- The three tendrils fan out within 6 m of the main target (page); here the chain hops 6 m from each previous enemy. Against a tight group the two agree; an enemy 6-12 m from the first is reached here and not in game.
- With punch through, chains come only from the furthest target the beam hits (page); here every body the beam crosses starts its own chain, so the number reads high.
- Chains skip enemies already hit by the main beam (page); here a chain can hit a body the beam passed through, so the number reads high.
- The tendril auto-targets, with a noticeable delay between firing and dealing damage while ammo is spent (page); the delay is not modelled.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Vermisplicer
- Every published board row, as JSON: https://wfsim.app/board/vermisplicer_primary.json
