# Pangolin Prime

Chinese name: 鲮鲤剑 Prime

Sword · Melee · Mastery Rank 14. 248 base damage (impact 12.4, puncture 37.2, slash 198.4), 26% crit chance, 2.2x crit multiplier, 30% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-11

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block forward | 5.8966 | Condition Overload, Blood Rush, Primed Fury, Primed Pressure Point, Primed Reach, Galvanized Steel, Weeping Wounds, Magnetic Rush, Swooping Falcon, Dreamer's Wrath, Melee Exposure | 2026-10-22 13:14 UTC · 99dec9ad6d01 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block forward | 55.0919 | Shocking Touch, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Organ Shatter, Weeping Wounds, Magnetic Rush, Swooping Falcon, Dreamer's Wrath, Melee Influence | 2026-10-22 17:41 UTC · 62a82930d1d3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | block forward | 0.6857 | Molten Impact, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Organ Shatter, Weeping Wounds, Magnetic Rush, Swooping Falcon, Dreamer's Wrath, Melee Exposure | 2026-10-22 19:34 UTC · 62a82930d1d3 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block forward | 2.4343 | Molten Impact, Condition Overload, Blood Rush, Primed Fury, Primed Reach, Organ Shatter, Weeping Wounds, Magnetic Rush, Swooping Falcon, Dreamer's Wrath, Melee Exposure | 2026-10-22 16:16 UTC · 99dec9ad6d01 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Pangolin_Prime
- Every published board row, as JSON: https://wfsim.app/board/pangolin_prime.json
