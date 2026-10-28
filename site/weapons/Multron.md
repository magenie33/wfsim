# Multron

Chinese name: 多连穿甲枪

Sentinel Weapon · Sentinel · Mastery Rank 3. 10 base damage (puncture 10), 15% crit chance, 2x crit multiplier, 22% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-29

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.01895 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 05:41 UTC · dac6ecbaf97e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.02026 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 05:41 UTC · dac6ecbaf97e |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.0001149 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 06:01 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.009991 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 05:41 UTC · dac6ecbaf97e |

## Not modelled here

- The dart's 1.5 s explosion delay is not modelled: the blast arrives with the dart, so a short fight gets its damage early.
- The companion fires this weapon: it picks its targets, fires when it decides, and stops while reviving or out of range. Here it fires continuously at one target, so the number is the ceiling.
- This attack's spread is not in the data, so none of its shots miss. At range the number is the ceiling; at point blank it is unaffected.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Multron
- Every published board row, as JSON: https://wfsim.app/board/multron.json
