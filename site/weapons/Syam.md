# Syam

Chinese name: 业珀

Nikana · Melee · Mastery Rank 0. 270 base damage (impact 54, puncture 108, slash 108), 22% crit chance, 2x crit multiplier, 34% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-14

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block | 1.6380 | Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Blind Justice, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:32 UTC · dac6ecbaf97e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block | 35.2091 | Shocking Touch, Blood Rush, Primed Fury, Primed Reach, Galvanized Elementalist, Galvanized Steel, Weeping Wounds, Magnetic Rush, Blind Justice, Dreamer's Wrath, Melee Influence | 2026-10-22 17:43 UTC · 62a82930d1d3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | block | 0.09723 | Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Blind Justice, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:32 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | block | 0.4887 | Condition Overload, Blood Rush, Primed Fury, Primed Reach, Galvanized Steel, Gladiator Might, Weeping Wounds, Magnetic Rush, Blind Justice, Dreamer's Wrath, Melee Exposure | 2026-10-24 05:32 UTC · dac6ecbaf97e |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- This weapon has a separate heavy-attack profile. Its timing, geometry and unsupported components are disclosed on that heavy mode.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Syam
- Every published board row, as JSON: https://wfsim.app/board/syam.json
