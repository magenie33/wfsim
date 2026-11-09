# Rakta Dark Dagger

Chinese name: 绯红·暗黑匕首

Dagger · Melee · Mastery Rank 8. 246 base damage (puncture 88, radiation 96, slash 62), 12% crit chance, 1.8x crit multiplier, 30% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Rakta_Dark_Dagger
- Every published board row, as JSON: https://wfsim.app/board/rakta_dark_dagger.json
