# Kogake Prime

Chinese name: 科加基 Prime

Sparring · Melee · Mastery Rank 10. 242 base damage (impact 169.4, puncture 36.3, slash 36.3), 16% crit chance, 1.8x crit multiplier, 38% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Kogake_Prime
- Every published board row, as JSON: https://wfsim.app/board/kogake_prime.json
