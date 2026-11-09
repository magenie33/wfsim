# Spinnerex

Chinese name: 丝蛛皇

Whip · Melee · Mastery Rank 12. 168 base damage (toxin 168), 16% crit chance, 2.4x crit multiplier, 40% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: an enemy it hit in the last 5 s that dies has a 10% chance per Toxin status on it to explode for 480 Toxin in 10 m, with a forced Toxin status.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Spinnerex
- Every published board row, as JSON: https://wfsim.app/board/spinnerex.json
