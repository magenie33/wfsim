# Furax Wraith

Chinese name: 弗拉克斯·亡魂

Fist · Melee · Mastery Rank 9. 139 base damage (impact 97.3, puncture 20.85, slash 20.85), 30% crit chance, 2.7x crit multiplier, 15% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Native initial-combo, tier-step or counter-cap differences are recorded. Unsupported counter rules retain the declared model and do not apply merely because their parameters are available.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.
- Not modelled: its combo counter starts at 20.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Furax_Wraith
- Every published board row, as JSON: https://wfsim.app/board/furax_wraith.json
