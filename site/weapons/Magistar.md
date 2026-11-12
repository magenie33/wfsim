# Magistar

Chinese name: 执法者

Hammer · Melee · Mastery Rank 1. 210 base damage (impact 168, puncture 31.5, slash 10.5), 20% crit chance, 2x crit multiplier, 10% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-12

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 6.7191 | Molten Impact, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Shattering Storm, Discipline's Merit, Melee Exposure | 2026-11-03 19:14 UTC · 51a5068e9d8b |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | heavy slam | 11.5204 | Volcanic Edge, Sacrificial Pressure, Sacrificial Steel, Galvanized Reflex, Killing Blow, Organ Shatter, Seismic Wave, Corrupt Charge, Shattering Storm, Melee Exposure | 2026-10-23 20:15 UTC · 5c64c380e5f2 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.7120 | Molten Impact, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Crushing Ruin, Dreamer's Wrath, Melee Exposure | 2026-11-05 10:09 UTC · 9e4647c46c76 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 3.0631 | Molten Impact, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Crushing Ruin, Dreamer's Wrath, Melee Exposure | 2026-11-05 10:09 UTC · 9e4647c46c76 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Magistar
- Every published board row, as JSON: https://wfsim.app/board/magistar.json
