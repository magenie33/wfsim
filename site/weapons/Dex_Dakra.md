# Dex Dakra

Chinese name: Dex 达克拉双剑

Dual Swords · Melee · Mastery Rank 6. 142 base damage (impact 14.2, puncture 14.2, slash 113.6), 16% crit chance, 2x crit multiplier, 24% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Dex_Dakra
- Every published board row, as JSON: https://wfsim.app/board/dex_dakra.json
