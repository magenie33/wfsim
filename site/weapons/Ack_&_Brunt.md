# Ack & Brunt

Chinese name: 认知 & 冲击

Sword And Shield · Melee · Mastery Rank 3. 149 base damage (impact 14.9, puncture 14.9, slash 119.2), 20% crit chance, 2x crit multiplier, 10% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: Spectral Axe and Spectral Axe Explosion. Of this weapon's attacks, only the swing, the slam and the heavy slam are.
- The wiki publishes no slide attack figure or duration for this weapon, so there is no slide mode.
- Not modelled: blocking an elemental attack stores a charge adding 17.5% of that element to the next attack, up to 4.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Ack_%26_Brunt
- Every published board row, as JSON: https://wfsim.app/board/ack_and_brunt.json
