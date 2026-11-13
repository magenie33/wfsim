# Volnus Prime

Chinese name: 创伤 Prime

Hammer · Melee · Mastery Rank 14. 250 base damage (impact 55, puncture 80, slash 115), 20% crit chance, 1.8x crit multiplier, 34% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-13

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 0.4073 | Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Gladiator Might, Seismic Wave, Corrupt Charge, Crushing Ruin, Master's Edge, Melee Exposure | 2026-11-03 07:00 UTC · 4393e1730621 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 0.4073 | Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Gladiator Might, Seismic Wave, Corrupt Charge, Crushing Ruin, Master's Edge, Melee Exposure | 2026-11-03 07:01 UTC · 4393e1730621 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | forward | 0.03936 | Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Gladiator Might, Seismic Wave, Corrupt Charge, Crushing Ruin, Master's Edge, Melee Exposure | 2026-11-03 07:00 UTC · 4393e1730621 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | forward | 0.1781 | Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Gladiator Might, Seismic Wave, Corrupt Charge, Crushing Ruin, Master's Edge, Melee Exposure | 2026-11-03 07:00 UTC · 4393e1730621 |

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: with Gara, its signature bonus: +100% slam radial damage.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Volnus_Prime
- Every published board row, as JSON: https://wfsim.app/board/volnus_prime.json
