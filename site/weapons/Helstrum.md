# Helstrum

Chinese name: 赫尔斯壮

Sentinel Weapon · Sentinel · Mastery Rank 0. 9 base damage (impact 4.95, puncture 4.05), 5% crit chance, 1.5x crit multiplier, 30% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.006257 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-09-29 12:16 UTC · 4507901cf6 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.006809 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-06 05:01 UTC · 2a93152888 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.00003654 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-09-29 11:49 UTC · 4507901cf6 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.003298 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-09 08:00 UTC |

## Not modelled here

- The missiles home, which is not modelled; with one target at zero distance, homing and straight missiles land the same.
- The explosion's falloff is not published, so it is not modelled; the target stands at the centre and takes the full 30.
- The companion fires this weapon: it picks its targets, fires when it decides, and stops while reviving or out of range. Here it fires continuously at one target, so the number is the ceiling.
- This attack's spread is not in the data, so none of its shots miss. At range the number is the ceiling; at point blank it is unaffected.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Helstrum
- Every published board row, as JSON: https://wfsim.app/board/helstrum.json
