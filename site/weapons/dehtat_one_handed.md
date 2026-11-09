# Dehtat (Rapier)

Rapier · Melee · Mastery Rank 0. 220 base damage (impact 22, puncture 110, slash 88), 18% crit chance, 2x crit multiplier, 18% status chance.

## Not modelled here

- Animation and event clocks use the pinned earlier package export. Current metadata verifies action references and properties, but does not re-establish every clip duration, playback fit, collision window or accepted-input boundary.
- Melee reach, sweep/thrust arcs and bone motion retain the declared arena approximation. Native collision dimensions alone do not establish the animated contact surface.
- Zaw statistics use the declared assembly tables. Native strike and handedness records are bound, but the complete runtime part-composition consumer is unverified; unassembled placeholders do not replace the finished weapon.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/dehtat_one_handed
- Every published board row, as JSON: https://wfsim.app/board/dehtat_one_handed.json
