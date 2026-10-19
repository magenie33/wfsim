# Haalvu

Chinese name: 哈尔武

Rifle · Primary · Mastery Rank 14. 33 base damage (tau 33), 25% crit chance, 2.5x crit multiplier, 19% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-19

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 12.6174 | Primed Cryo Rounds, Malignant Force, Hellfire, Primary Acuity, Galvanized Aptitude, Vile Acceleration, Vital Sense, Magnetic Capacity, Vigilante Supplies, Primary Deadhead | 2026-10-18 03:07 UTC · 72866b65d9 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 14.7834 | Hellfire, Primary Acuity, Primed Shred, Galvanized Aptitude, Galvanized Scope, Rifle Elementalist, Vital Sense, Magnetic Capacity, Vigilante Supplies, Primary Crux | 2026-10-15 18:36 UTC |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 1.1185 | Primed Cryo Rounds, Infected Clip, Hellfire, Heavy Caliber, Primary Acuity, Serration, Vile Acceleration, Vital Sense, Primary Crux | 2026-10-17 02:58 UTC |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 3.4542 | Primed Cryo Rounds, Infected Clip, Hellfire, Heavy Caliber, Primary Acuity, Serration, Vile Acceleration, Vital Sense, Primary Crux | 2026-10-18 06:53 UTC |

## Not modelled here

- Tau's status effect is not modelled. Tau's neutral damage is applied correctly, but its proc raises the status chance the target takes from every other source by 10% a stack, up to ten (wiki). With other statuses stacked, the number reads low.
- Switching firing modes takes 3 s, can be interrupted and scales with reload speed (wiki); the sim never switches, so each mode is measured as the whole weapon and the switch costs nothing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Haalvu
- Every published board row, as JSON: https://wfsim.app/board/haalvu.json
