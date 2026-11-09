# Dual Zoren

Chinese name: 佐伦双斧

Dual Swords · Melee · Mastery Rank 2. 70 base damage (impact 3.5, puncture 3.5, slash 63), 25% crit chance, 3x crit multiplier, 5% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Dual_Zoren
- Every published board row, as JSON: https://wfsim.app/board/dual_zoren.json
