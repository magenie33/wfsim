# Gazal Machete

Chinese name: 加扎勒反曲刀

Machete · Melee · Mastery Rank 5. 178 base damage (impact 17.8, puncture 26.7, slash 133.5), 10% crit chance, 2x crit multiplier, 30% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: with a Djinn, each Fatal Attraction grants it +25% damage as Corrosive for 30 s.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Gazal_Machete
- Every published board row, as JSON: https://wfsim.app/board/gazal_machete.json
