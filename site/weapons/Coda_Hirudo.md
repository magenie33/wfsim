# Coda Hirudo

Chinese name: 终幕·蚂蟥

Sparring · Melee · Mastery Rank 17. 175 base damage (impact 26.25, puncture 140, slash 8.75), 34% crit chance, 3.1x crit multiplier, 18% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-11

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 2.6048 | Primed Fever Strike, Shocking Touch, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Brutal Tide, Discipline's Merit, Melee Exposure | 2026-10-31 10:03 UTC · ea828b371375 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 2.6048 | Primed Fever Strike, Shocking Touch, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Brutal Tide, Discipline's Merit, Melee Exposure | 2026-10-31 09:34 UTC · ea828b371375 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.2657 | Primed Fever Strike, Shocking Touch, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Brutal Tide, Discipline's Merit, Melee Exposure | 2026-10-31 10:03 UTC · ea828b371375 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 1.0062 | Primed Fever Strike, Shocking Touch, Primed Fury, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Brutal Tide, Discipline's Merit, Melee Exposure | 2026-10-31 09:38 UTC · ea828b371375 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Its Coda valence bonus is not modelled: the build cannot state the element and bonus this copy rolled, so it reads without one.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Coda_Hirudo
- Every published board row, as JSON: https://wfsim.app/board/coda_hirudo.json
