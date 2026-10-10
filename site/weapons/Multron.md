# Multron

Chinese name: 多连穿甲枪

Sentinel Weapon · Sentinel · Mastery Rank 3. 10 base damage (puncture 10), 15% crit chance, 2x crit multiplier, 22% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.01611 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-09-29 10:29 UTC · a96c10c3c2 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.01765 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-05 16:52 UTC · 052c2ed8c6 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.0001023 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-09-29 09:45 UTC · a96c10c3c2 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.008492 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-08 21:33 UTC · 531aca13c9 |

## Not modelled here

- The dart's 1.5 s explosion delay is not modelled: the blast arrives with the dart, so a short fight gets its damage early.
- The companion fires this weapon: it picks its targets, fires when it decides, and stops while reviving or out of range. Here it fires continuously at one target, so the number is the ceiling.
- This attack's spread is not in the data, so none of its shots miss. At range the number is the ceiling; at point blank it is unaffected.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Multron
- Every published board row, as JSON: https://wfsim.app/board/multron.json
