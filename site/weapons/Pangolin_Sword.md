# Pangolin Sword

Chinese name: 鲮鲤剑

Sword · Melee · Mastery Rank 3. 150 base damage (impact 7.5, puncture 22.5, slash 120), 8% crit chance, 1.5x crit multiplier, 22% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Pangolin_Sword
- Every published board row, as JSON: https://wfsim.app/board/pangolin_sword.json
