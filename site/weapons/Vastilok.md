# Vastilok

Chinese name: 瓦斯提枪刃

Gunblade · Melee · Mastery Rank 9. 279 base damage (impact 19.53, puncture 25.11, slash 234.36), 19% crit chance, 1.7x crit multiplier, 31% status chance.

## Not modelled here

- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- This legacy sequence retains its declared motion and recovery approximation; current metadata does not establish a complete replacement event timeline. A verified weapon wind-up is applied independently.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- One attack input's animation length is unpublished: the wiki gives each combo's total at 1.0x attack speed, so totals are exact and inputs are split evenly. This shifts a status tick by fractions of a second and changes no total.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Gunblade shooting within light stance combos is not modelled. Its separate heavy-shot mode uses the ranged attack's own profile.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Vastilok
- Every published board row, as JSON: https://wfsim.app/board/vastilok.json
