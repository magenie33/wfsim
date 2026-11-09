# Pennant

Chinese name: 尖幡

Two Handed Nikana · Melee · Mastery Rank 7. 200 base damage (impact 20, puncture 140, slash 40), 32% crit chance, 2.4x crit multiplier, 10% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.
- Not modelled: each heavy attack kill grants +50% attack speed for 2 to 46 s, longer with the combo multiplier, and each kill's stack runs on its own clock.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Pennant
- Every published board row, as JSON: https://wfsim.app/board/pennant.json
