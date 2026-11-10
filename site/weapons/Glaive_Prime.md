# Glaive Prime

Chinese name: 战刃 Prime

Glaive · Melee · Mastery Rank 10. 164 base damage (impact 24.6, puncture 24.6, slash 114.8), 22% crit chance, 2x crit multiplier, 30% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-11

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | charged throw detonate | 0.9586 | Primed Fever Strike, Sacrificial Steel, Primed Pressure Point, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Dispatch Overdrive, Melee Duplicate | 2026-10-28 17:25 UTC · be8a95fc1ba2 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | charged throw detonate | 11.7937 | Primed Fever Strike, Sacrificial Steel, Primed Pressure Point, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Dispatch Overdrive, Melee Duplicate | 2026-10-28 17:29 UTC · be8a95fc1ba2 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | charged throw detonate | 0.05435 | Primed Fever Strike, Sacrificial Steel, Primed Pressure Point, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Dispatch Overdrive, Melee Duplicate | 2026-10-28 17:29 UTC · be8a95fc1ba2 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | charged throw detonate | 0.3197 | Primed Fever Strike, Sacrificial Steel, Primed Pressure Point, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Dispatch Overdrive, Melee Duplicate | 2026-10-28 17:26 UTC · be8a95fc1ba2 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- A glaive's heavy attack is the detonation of a thrown glaive, which is its own mode (a throw, detonated), so there is no standing heavy mode and a Tennokai window converts no swing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Glaive_Prime
- Every published board row, as JSON: https://wfsim.app/board/glaive_prime.json
