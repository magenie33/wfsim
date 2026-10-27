# Falcor

Chinese name: 猎鹰轮

Glaive · Melee · Mastery Rank 8. 230 base damage (electricity 84, impact 36, puncture 18, slash 92), 12% crit chance, 1.6x crit multiplier, 34% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-28

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | charged throw detonate | 0.2115 | Focus Energy, Primed Pressure Point, Galvanized Reflex, Galvanized Steel, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Whirlwind, Melee Influence | 2026-10-24 04:52 UTC · dac6ecbaf97e |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | charged throw detonate | 4.1882 | Virulent Scourge, Sacrificial Pressure, Sacrificial Steel, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Whirlwind, Melee Influence | 2026-10-22 21:58 UTC · 62a82930d1d3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | charged throw detonate | 0.01211 | Virulent Scourge, Sacrificial Pressure, Sacrificial Steel, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Whirlwind, Melee Influence | 2026-10-24 05:22 UTC · dac6ecbaf97e |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | charged throw detonate | 0.09388 | Virulent Scourge, Sacrificial Pressure, Sacrificial Steel, Galvanized Reflex, Amalgam Organ Shatter, Killing Blow, Melee Elementalist, Volatile Quick Return, Astral Twilight, Whirlwind, Melee Influence | 2026-10-24 04:50 UTC · dac6ecbaf97e |

## Not modelled here

- One attack input's animation length is unpublished: the wiki gives each combo's total at 1.0x attack speed, so totals are exact and inputs are split evenly. This shifts a status tick by fractions of a second and changes no total.
- Power Spike's partial combo decay is a Warframe passive and is not modelled: the counter drops to zero when its timer runs out, so with that passive the number reads low.
- A glaive's heavy attack is the detonation of a thrown glaive, which is its own mode (a throw, detonated), so there is no standing heavy mode and a Tennokai window converts no swing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Falcor
- Every published board row, as JSON: https://wfsim.app/board/falcor.json
