# Stinger

Chinese name: 毒刺

Sentinel Weapon · Sentinel · Mastery Rank 0. 15 base damage (toxin 15), 2.5% crit chance, 1.5x crit multiplier, 20% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-31

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.007744 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 06:10 UTC · dac6ecbaf97e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.007744 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 05:49 UTC · dac6ecbaf97e |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.00004672 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 05:49 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.004082 | Hellfire, Wildfire, Malignant Force, Thermite Rounds, Rime Rounds, Split Chamber, Serration, Magnetic Capacity | 2026-10-24 05:49 UTC · dac6ecbaf97e |

## Not modelled here

- The Toxin embed, a second attack dealing damage over 3 s on top of the hit (page), is not modelled; only the hit is carried.
- The companion fires this weapon: it picks its targets, fires when it decides, and stops while reviving or out of range. Here it fires continuously at one target, so the number is the ceiling.
- This attack's spread is not in the data, so none of its shots miss. At range the number is the ceiling; at point blank it is unaffected.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Stinger
- Every published board row, as JSON: https://wfsim.app/board/stinger.json
