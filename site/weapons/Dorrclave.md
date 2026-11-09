# Dorrclave

Chinese name: 怒斩

Blade And Whip · Melee · Mastery Rank 8. 222 base damage (slash 222), 26% crit chance, 2.2x crit multiplier, 24% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.
- Not modelled: Rising Vendetta: 10 kills or assists grant +0.5 Follow Through and +100% flat Status Chance to the next 10 attacks that hit.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Dorrclave
- Every published board row, as JSON: https://wfsim.app/board/dorrclave.json
