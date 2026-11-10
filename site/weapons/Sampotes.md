# Sampotes

Chinese name: 三宝聚

Hammer · Melee · Mastery Rank 0. 248 base damage (impact 173.6, puncture 37.2, slash 37.2), 30% crit chance, 3x crit multiplier, 28% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-11

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 0.5618 | Fever Strike, Blood Rush, Galvanized Elementalist, Galvanized Steel, Killing Blow, Organ Shatter, Seismic Wave, Drifting Contact, Shattering Storm, Master's Edge, Melee Exposure | 2026-10-22 13:11 UTC · 99dec9ad6d01 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 0.5618 | Fever Strike, Blood Rush, Galvanized Elementalist, Galvanized Steel, Killing Blow, Organ Shatter, Seismic Wave, Drifting Contact, Shattering Storm, Master's Edge, Melee Exposure | 2026-10-23 01:15 UTC · 62a82930d1d3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | forward | 0.04536 | Fever Strike, Blood Rush, Galvanized Elementalist, Galvanized Steel, Killing Blow, Organ Shatter, Seismic Wave, Drifting Contact, Shattering Storm, Master's Edge, Melee Exposure | 2026-10-23 01:15 UTC · 62a82930d1d3 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 0.2092 | Fever Strike, Blood Rush, Galvanized Elementalist, Galvanized Steel, Killing Blow, Organ Shatter, Seismic Wave, Drifting Contact, Shattering Storm, Master's Edge, Melee Exposure | 2026-10-23 01:15 UTC · 62a82930d1d3 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki gives this weapon combos of its own (heavy attack, heavy slam and slam attack), which replace the stance's and are not transcribed; the stance's (or its class's) are used instead.
- Not modelled: a slam's line of 8 explosions 5 m apart, and a heavy slam's three lines of 4.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Sampotes
- Every published board row, as JSON: https://wfsim.app/board/sampotes.json
