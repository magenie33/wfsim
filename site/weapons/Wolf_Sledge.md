# Wolf Sledge

Chinese name: 恶狼战锤

Hammer · Melee · Mastery Rank 7. 259 base damage (impact 132.09, puncture 7.77, slash 119.14), 17% crit chance, 1.9x crit multiplier, 33% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- The second target's seeking and all return-flight strikes are not modelled. Flight uses the package's 50 m/s. The 0.75 s charge and nominal 1 s animation use attack speed; exact hold, release, catch timing and the bare-stance wind-up exception are not calibrated. Nearby ragdoll effects are not modelled.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Wolf_Sledge
- Every published board row, as JSON: https://wfsim.app/board/wolf_sledge.json
