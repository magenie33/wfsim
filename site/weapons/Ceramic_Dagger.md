# Ceramic Dagger

Chinese name: 陶瓷匕首

Dagger · Melee · Mastery Rank 3. 140 base damage (impact 14, puncture 126), 10% crit chance, 1.5x crit multiplier, 20% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- In Incarnon Form, each standing or Tennokai heavy input emits two spectral daggers at its first physical sweep marker. Their exact release times, spread and maximum flight range are unmeasured. Projectile base-damage exclusions follow the wiki's live-bug rules and are not calibrated in game. Spectral projectiles on heavy slams are not modelled.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Ceramic_Dagger
- Every published board row, as JSON: https://wfsim.app/board/ceramic_dagger.json
