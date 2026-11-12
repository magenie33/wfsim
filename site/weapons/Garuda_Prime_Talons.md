# Garuda Prime Talons

Chinese name: Garuda Prime 之爪

Claws · Exalted · Mastery Rank 1. 280 base damage (impact 14, puncture 28, slash 238), 35% crit chance, 2x crit multiplier, 36% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-12

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 6.7495 | Primed Fever Strike, Vicious Frost, Volcanic Edge, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Vermillion Storm, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:26 UTC · dac6ecbaf97e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | slide | 30.5961 | Primed Fever Strike, Vicious Frost, Volcanic Edge, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Vermillion Storm, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:30 UTC · dac6ecbaf97e |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | forward | 1.4012 | Primed Fever Strike, Vicious Frost, Volcanic Edge, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Vermillion Storm, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:30 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 5.2523 | Primed Fever Strike, Vicious Frost, Volcanic Edge, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Vermillion Storm, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:26 UTC · dac6ecbaf97e |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Garuda_Prime_Talons
- Every published board row, as JSON: https://wfsim.app/board/garuda_prime_talons.json
