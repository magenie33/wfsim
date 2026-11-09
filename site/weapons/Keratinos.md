# Keratinos

Chinese name: 卡提努之爪

Claws · Melee · Mastery Rank 9. 244 base damage (impact 79, puncture 45, slash 87, viral 33), 13% crit chance, 2.1x crit multiplier, 29% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: a heavy attack at 12x combo grants +2.8 m range to every melee attack for 180 s.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Keratinos
- Every published board row, as JSON: https://wfsim.app/board/keratinos.json
