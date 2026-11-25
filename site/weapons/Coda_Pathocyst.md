# Coda Pathocyst

Chinese name: 终幕·病囊飞刃

Glaive · Melee · Mastery Rank 17. 270 base damage (impact 58, puncture 55, slash 65, viral 92), 20% crit chance, 2.3x crit multiplier, 35% status chance.

Current published rankings and revisions: https://wfsim.app/api/board/pages/catalog?weapon=coda_pathocyst

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- A glaive's heavy attack is the detonation of a thrown glaive, which is its own mode (a throw, detonated), so there is no standing heavy mode and a Tennokai window converts no swing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Coda_Pathocyst
- Archived board JSON: https://wfsim.app/board/coda_pathocyst.json
- [Using and citing WFSim](https://wfsim.app/reuse)
