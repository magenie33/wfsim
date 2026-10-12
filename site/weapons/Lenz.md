# Lenz

Chinese name: 楞次弓

Bow · Primary · Mastery Rank 8. 50 base damage (impact 50), 50% crit chance, 2x crit multiplier, 5% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-12

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.8747 | Primed Cryo Rounds, Malignant Force, Split Flights, Serration, Galvanized Aptitude, Galvanized Scope, Vile Acceleration, Vital Sense, Primary Compression | 2026-10-11 05:36 UTC |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 0.8747 | Primed Cryo Rounds, Malignant Force, Split Flights, Serration, Galvanized Aptitude, Galvanized Scope, Vile Acceleration, Vital Sense, Primary Compression | 2026-10-11 07:48 UTC |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.06131 | Primed Cryo Rounds, Malignant Force, Split Flights, Serration, Galvanized Aptitude, Galvanized Scope, Vile Acceleration, Vital Sense, Primary Compression | 2026-10-11 05:35 UTC |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.3989 | Primed Cryo Rounds, Malignant Force, Split Flights, Serration, Galvanized Aptitude, Galvanized Scope, Vile Acceleration, Vital Sense, Primary Compression | 2026-10-09 07:55 UTC |

## Not modelled here

- The module gives this shot three attacks and the engine carries one explosion: the Blast collapse is modelled, and the initial blast is not - neither its 10 Cold damage nor its guaranteed Cold status in 7.2 m, so a build that profits from Cold stacks or a status count reads low.
- The collapse's 1.3 s delay after the initial blast is not modelled: the blast arrives with the arrow, so a short fight gets its damage early.
- The bubble holds what it catches until it collapses (wiki); crowd control, and enemies here neither move nor stop.
- In game the blast staggers the wielder; the player has no body here, so it costs nothing.
- On a kill the body follows the bolt, damaging anyone in its path (wiki). There is no ragdoll here, so against a crowd the number reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Lenz
- Every published board row, as JSON: https://wfsim.app/board/lenz.json
