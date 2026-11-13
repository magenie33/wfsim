# Reaper Prime

Chinese name: 收割者 Prime

Scythe · Melee · Mastery Rank 10. 200 base damage (impact 30, puncture 30, slash 140), 35% crit chance, 2.5x crit multiplier, 25% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-13

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 9.1294 | Primed Fever Strike, Vicious Frost, Condition Overload, Blood Rush, Primed Fury, Galvanized Steel, Weeping Wounds, Magnetic Rush, Reaping Spiral, Dreamer's Wrath, Melee Exposure | 2026-11-06 12:25 UTC · d8787cb351c5 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Reaper_Prime
- Every published board row, as JSON: https://wfsim.app/board/reaper_prime.json
