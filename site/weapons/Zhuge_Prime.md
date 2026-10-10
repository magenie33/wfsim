# Zhuge Prime

Chinese name: 诸葛连弩 Prime

Crossbow · Primary · Mastery Rank 14. 50 base damage (impact 10, puncture 22.5, slash 17.5), 26% crit chance, 2x crit multiplier, 30% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 1.5601 | Hellfire, Primary Acuity, Serration, Galvanized Aptitude, Galvanized Scope, Bladed Rounds, Rifle Elementalist, Vital Sense, Primary Compression | 2026-09-29 23:35 UTC · 92c94eb9a6 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 1.5601 | Hellfire, Primary Acuity, Serration, Galvanized Aptitude, Galvanized Scope, Bladed Rounds, Rifle Elementalist, Vital Sense, Primary Compression | 2026-10-05 18:01 UTC · 40071ee3b3 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.2764 | Hellfire, Primary Acuity, Serration, Galvanized Aptitude, Galvanized Scope, Bladed Rounds, Rifle Elementalist, Vital Sense, Primary Compression | 2026-09-29 23:35 UTC · 92c94eb9a6 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.8210 | Hellfire, Primary Acuity, Serration, Galvanized Aptitude, Galvanized Scope, Bladed Rounds, Rifle Elementalist, Vital Sense, Primary Compression | 2026-10-08 22:21 UTC · 531aca13c9 |

## Not modelled here

- It reloads 50% faster from an empty magazine (wiki); the engine has one reload time per weapon. The sim always empties the magazine, so the 3 s here is the slow reload and the number reads low.
- The explosion arrives 0.6 s after the bolt lands, which lets an enemy walk out of it (page); here it resolves on impact, so nothing escapes.
- On a kill the body follows the bolt, damaging anyone in its path (wiki). There is no ragdoll here, so against a crowd the number reads low.
- The blast needs no line of sight. There is no cover here, so the perk adds nothing.
- In game the blast staggers the wielder; the player has no body here, so it costs nothing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Zhuge_Prime
- Every published board row, as JSON: https://wfsim.app/board/zhuge_prime.json
