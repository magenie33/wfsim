# Valkyr Talons

Chinese name: Valkyr 的利爪

Claws · Exalted · Mastery Rank 0. 250 base damage (puncture 62.5, slash 187.5), 50% crit chance, 2x crit multiplier, 10% status chance.

## Not modelled here

- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- This legacy sequence retains its declared motion and recovery approximation; current metadata does not establish a complete replacement event timeline. A verified weapon wind-up is applied independently.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Ability and cast overrides retain the declared damage/status profile and measured combo rules. The raw melee weapon packet does not establish final ability damage; verified weapon clock inputs apply separately.
- Hysteria drains 5 Energy a second while the claws are out; Energy is not modelled, so they stay out for the whole fight.
- Every hit restores 100 health; nothing damages the Tenno here, so it adds nothing.
- One attack input's animation length is unpublished: the wiki gives each combo's total at 1.0x attack speed, so totals are exact and inputs are split evenly. This shifts a status tick by fractions of a second and changes no total.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Valkyr_Talons
- Every published board row, as JSON: https://wfsim.app/board/valkyr_talons.json
