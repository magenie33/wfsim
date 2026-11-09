# Masseter Prime

Chinese name: 咀嚼金杖 Prime

Heavy Blade · Melee · Mastery Rank 14. 260 base damage (impact 145.6, slash 114.4), 26% crit chance, 2.2x crit multiplier, 36% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: a stance's mini-slam, one without the full slam visual, is multiplied by the combo multiplier and can crit.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Masseter_Prime
- Every published board row, as JSON: https://wfsim.app/board/masseter_prime.json
