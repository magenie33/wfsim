# Cadus

Chinese name: 卡德斯

Staff · Melee · Mastery Rank 4. 130 base damage (electricity 60, impact 70), 15% crit chance, 2x crit multiplier, 25% status chance.

## Not modelled here

- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- This legacy sequence retains its declared motion and recovery approximation; current metadata does not establish a complete replacement event timeline. A verified weapon wind-up is applied independently.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- One attack input's animation length is unpublished: the wiki gives each combo's total at 1.0x attack speed, so totals are exact and inputs are split evenly. This shifts a status tick by fractions of a second and changes no total.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Cadus
- Every published board row, as JSON: https://wfsim.app/board/cadus.json
