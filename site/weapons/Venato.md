# Venato

Chinese name: 脉纹

Scythe · Melee · Mastery Rank 9. 220 base damage (impact 33, puncture 110, slash 77), 27% crit chance, 2.1x crit multiplier, 24% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-10

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 7.8901 | Primed Fever Strike, Vicious Frost, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Weeping Wounds, Magnetic Rush, Reaping Spiral, Dreamer's Wrath, Melee Exposure | 2026-11-04 14:58 UTC · 9e4647c46c76 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 7.8974 | Primed Fever Strike, Vicious Frost, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Weeping Wounds, Magnetic Rush, Reaping Spiral, Dreamer's Wrath, Melee Exposure | 2026-11-04 15:30 UTC · 9e4647c46c76 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | forward | 1.2539 | Primed Fever Strike, Vicious Frost, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Weeping Wounds, Magnetic Rush, Reaping Spiral, Dreamer's Wrath, Melee Exposure | 2026-11-04 15:04 UTC · 9e4647c46c76 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 5.6742 | Primed Fever Strike, Vicious Frost, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Weeping Wounds, Magnetic Rush, Reaping Spiral, Dreamer's Wrath, Melee Exposure | 2026-11-04 15:29 UTC · 9e4647c46c76 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.
- Not modelled: wielded by Caliban, attacks have a 50% chance of an extra combo point.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Venato
- Every published board row, as JSON: https://wfsim.app/board/venato.json
