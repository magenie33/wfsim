# War Prime

Chinese name: 战争之剑 Prime

Heavy Blade · Melee · Mastery Rank 15. 270 base damage (impact 194.4, puncture 32.4, slash 43.2), 26% crit chance, 3.2x crit multiplier, 32% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-12

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | heavy | 2.2733 | Volcanic Edge, Lasting Sting, Sacrificial Steel, Condition Overload, Primed Fury, Amalgam Organ Shatter, Corrupt Charge, Magnetic Rush, Dreamer's Wrath, Melee Exposure | 2026-10-05 16:16 UTC |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 123.8336 | Shocking Touch, Voltaic Strike, Blood Rush, Primed Pressure Point, Primed Reach, Galvanized Elementalist, Galvanized Reflex, Galvanized Steel, Cleaving Whirlwind, Dreamer's Wrath, Melee Influence | 2026-11-06 11:37 UTC · 0ae1bf8edb0f |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | heavy | 0.2756 | Volcanic Edge, Lasting Sting, Sacrificial Steel, Condition Overload, Primed Fury, Amalgam Organ Shatter, Corrupt Charge, Magnetic Rush, Dreamer's Wrath, Melee Exposure | 2026-10-05 15:48 UTC |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block forward | 0.1580 | Shocking Touch, Blood Rush, Primed Pressure Point, Primed Reach, Galvanized Elementalist, Galvanized Steel, Berserker Fury, Weeping Wounds, Cleaving Whirlwind, Melee Exposure | 2026-11-01 16:04 UTC · 4393e1730621 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/War_Prime
- Every published board row, as JSON: https://wfsim.app/board/war_prime.json
