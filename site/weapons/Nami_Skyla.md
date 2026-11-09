# Nami Skyla

Chinese name: 海波斯库拉对剑

Dual Swords · Melee · Mastery Rank 2. 125 base damage (impact 18.75, puncture 18.75, slash 87.5), 10% crit chance, 1.5x crit multiplier, 15% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Nami_Skyla
- Every published board row, as JSON: https://wfsim.app/board/nami_skyla.json
