# Coda Mire

Chinese name: 终幕·米尔

Sword · Melee · Mastery Rank 17. 235 base damage (impact 45, puncture 45, slash 66, toxin 79), 18% crit chance, 2.4x crit multiplier, 40% status chance.

Current published rankings and revisions: https://wfsim.app/api/board/pages/catalog?weapon=coda_mire

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Coda_Mire
- Archived board JSON: https://wfsim.app/board/coda_mire.json
- [Using and citing WFSim](https://wfsim.app/reuse)
