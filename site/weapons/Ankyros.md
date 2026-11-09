# Ankyros

Chinese name: 甲龙双拳

Fist · Melee · Mastery Rank 2. 90 base damage (impact 63, puncture 13.5, slash 13.5), 20% crit chance, 2x crit multiplier, 10% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Ankyros
- Every published board row, as JSON: https://wfsim.app/board/ankyros.json
