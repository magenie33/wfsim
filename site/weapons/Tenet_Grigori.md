# Tenet Grigori

Chinese name: 信条·格里高利

Scythe · Melee · Mastery Rank 14. 228 base damage (impact 9.12, puncture 82.08, slash 136.8), 24% crit chance, 1.6x crit multiplier, 38% status chance.

Current published rankings and revisions: https://wfsim.app/api/board/pages/catalog?weapon=tenet_grigori

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Tenet_Grigori
- Archived board JSON: https://wfsim.app/board/tenet_grigori.json
- [Using and citing WFSim](https://wfsim.app/reuse)
