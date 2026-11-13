# Tenet Exec

Chinese name: 信条·枢密

Heavy Blade · Melee · Mastery Rank 16. 190 base damage (impact 102.6, slash 87.4), 38% crit chance, 2.4x crit multiplier, 22% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-13

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.3457 | Primed Fever Strike, Blood Rush, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Organ Shatter, Seismic Wave, Rending Crane, Melee Exposure | 2026-11-04 03:30 UTC · 1f738fedc8a4 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.3458 | Primed Fever Strike, Blood Rush, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Organ Shatter, Seismic Wave, Rending Crane, Melee Exposure | 2026-11-04 03:30 UTC · 1f738fedc8a4 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.02951 | Primed Fever Strike, Blood Rush, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Organ Shatter, Seismic Wave, Rending Crane, Melee Exposure | 2026-11-04 03:39 UTC · 1f738fedc8a4 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 0.1595 | Primed Fever Strike, Blood Rush, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Organ Shatter, Seismic Wave, Rending Crane, Melee Exposure | 2026-11-04 03:30 UTC · 1f738fedc8a4 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki gives this weapon combos of its own (heavy slam and slam attack), which replace the stance's and are not transcribed; the stance's (or its class's) are used instead.
- Its Tenet valence bonus is not modelled: the build cannot state the element and bonus this copy rolled, so it reads without one.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Tenet_Exec
- Every published board row, as JSON: https://wfsim.app/board/tenet_exec.json
