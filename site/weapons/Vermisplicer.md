# Vermisplicer

Chinese name: 虫置

Rifle · Primary · Mastery Rank 0. 33 base damage (impact 5, puncture 6, slash 10, toxin 12), 25% crit chance, 2x crit multiplier, 25% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 9.3257 | High Voltage, Primary Acuity, Galvanized Aptitude, Rifle Elementalist, Vile Acceleration, Vital Sense, Magnetic Capacity, Radiated Reload, Vigilante Supplies, Pax Charge, Primary Deadhead | 2026-10-05 04:02 UTC · a759a842f4 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 18.1308 | High Voltage, Heavy Caliber, Primary Acuity, Galvanized Aptitude, Rifle Elementalist, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Debilitate | 2026-10-05 16:56 UTC · 052c2ed8c6 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.2395 | High Voltage, Heavy Caliber, Primary Acuity, Galvanized Aptitude, Rifle Elementalist, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Debilitate | 2026-10-05 05:05 UTC · dccca205c1 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 1.9393 | High Voltage, Heavy Caliber, Primary Acuity, Galvanized Aptitude, Rifle Elementalist, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Pax Charge, Primary Debilitate | 2026-10-08 21:43 UTC · 531aca13c9 |

## Not modelled here

- The three tendrils fan out within 6 m of the main target (page); here the chain hops 6 m from each previous enemy. Against a tight group the two agree; an enemy 6-12 m from the first is reached here and not in game.
- With punch through, chains come only from the furthest target the beam hits (page); here every body the beam crosses starts its own chain, so the number reads high.
- Chains skip enemies already hit by the main beam (page); here a chain can hit a body the beam passed through, so the number reads high.
- The tendril auto-targets, with a noticeable delay between firing and dealing damage while ammo is spent (page); the delay is not modelled.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Vermisplicer
- Every published board row, as JSON: https://wfsim.app/board/vermisplicer_primary.json
