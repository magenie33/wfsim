# Cerata

Chinese name: 裸鳃刃

Glaive · Melee · Mastery Rank 7. 183 base damage (impact 19, puncture 36, slash 52, toxin 76), 15% crit chance, 2x crit multiplier, 30% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- A glaive's heavy attack is the detonation of a thrown glaive, which is its own mode (a throw, detonated), so there is no standing heavy mode and a Tennokai window converts no swing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Cerata
- Every published board row, as JSON: https://wfsim.app/board/cerata.json
