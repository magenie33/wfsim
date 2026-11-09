# Vitrica

Chinese name: 金璃剑

Heavy Blade · Melee · Mastery Rank 13. 333 base damage (impact 99.9, puncture 99.9, slash 133.2), 23% crit chance, 2.3x crit multiplier, 33% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Slam damage and critical rules retain the declared component model. Radius uses the maximum-height convention; ascent, descent, ground contact and recovery are not a calibrated physical cycle. Native attenuation and clip lengths do not establish that cycle.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- Not modelled: Glass Explosion. Of this weapon's attacks, only the swing, the slam and the heavy slam are.
- The wiki gives this weapon combos of its own (aerial attack), which replace the stance's and are not transcribed; the stance's (or its class's) are used instead.
- Not modelled: every slam sends a ground wave, 10 Impact, that shatters glassed enemies — 40 m from a slam, 80 m and a forced Impact status from a heavy slam.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Vitrica
- Every published board row, as JSON: https://wfsim.app/board/vitrica.json
