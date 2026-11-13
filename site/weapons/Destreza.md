# Destreza

Chinese name: 技巧之剑

Rapier · Melee · Mastery Rank 7. 158 base damage (impact 3.95, puncture 134.3, slash 19.75), 28% crit chance, 2x crit multiplier, 14% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-13

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 10.1692 | Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Gladiator Vice, Organ Shatter, Weeping Wounds, Magnetic Rush, Vulpine Mask, Master's Edge, Melee Exposure | 2026-11-04 15:04 UTC · 9e4647c46c76 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 17.3995 | Molten Impact, Primed Fury, Primed Pressure Point, Galvanized Elementalist, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Vulpine Mask, Master's Edge, Melee Duplicate | 2026-11-04 15:37 UTC · 9e4647c46c76 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | forward | 0.9468 | Molten Impact, Primed Fury, Primed Pressure Point, Galvanized Elementalist, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Vulpine Mask, Master's Edge, Melee Duplicate | 2026-11-04 15:37 UTC · 9e4647c46c76 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 5.0558 | Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Gladiator Vice, Organ Shatter, Weeping Wounds, Magnetic Rush, Vulpine Mask, Master's Edge, Melee Exposure | 2026-11-04 15:11 UTC · 9e4647c46c76 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- A plain slam at the arming combo also activates its Incarnon Form; here only a heavy attack does, so a loop that slams without a heavy arms it later than in a mission, or never.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Destreza
- Every published board row, as JSON: https://wfsim.app/board/destreza.json
