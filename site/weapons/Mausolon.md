# Mausolon

Chinese name: 惩戒者

Archgun · Archgun · Mastery Rank 14. 180 base damage (heat 75, impact 35, puncture 70), 30% crit chance, 2.2x crit multiplier, 26% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-10

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 25.8018 | Primed Polar Magazine, Primed Venomous Clip, Primed Dual Rounds, Primed Rubedo-Lined Barrel, Automatic Trigger, Critical Focus, Hollowed Bullets, Magnetized Cycle, Primary Crux, Secondary Deadhead | 2026-10-09 23:10 UTC · 77fd7f0324 |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 25.8018 | Primed Polar Magazine, Primed Venomous Clip, Primed Dual Rounds, Primed Rubedo-Lined Barrel, Automatic Trigger, Critical Focus, Hollowed Bullets, Magnetized Cycle, Primary Crux, Secondary Deadhead | 2026-10-09 23:00 UTC · 77fd7f0324 |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | cycle | 3.3218 | Contamination Casing, Hypothermic Shell, Combustion Rounds, Primed Dual Rounds, Primed Rubedo-Lined Barrel, Critical Focus, Hollowed Bullets, Parallax Scope, Primary Crux, Cascadia Flare | 2026-10-09 22:44 UTC · 77fd7f0324 |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 15.1019 | Primed Polar Magazine, Primed Venomous Clip, Primed Dual Rounds, Primed Rubedo-Lined Barrel, Automatic Trigger, Critical Focus, Hollowed Bullets, Magnetized Cycle, Primary Crux, Secondary Deadhead | 2026-10-09 16:49 UTC · ca98611dee |

## Not modelled here

- The Lifted synergy is not modelled: the wiki gives up to 13 extra direct-hit instances against a Lifted enemy, and its alt-fire guarantees Lifted. Only the Lifted status is applied (Condition Overload counts it), so the primary reads low while the target is lifted.
- Primary Compression's 444% case is not modelled: the catalog gives a second value for aiming during the alt-fire, a state the sim does not have. The ordinary 100% applies.
- The radial's falloff is not modelled; the one target at the centre always takes the full 72, never the 58 at the rim.
- The Arch-Gun Deployer's five-minute cooldown is not modelled: in a mission the weapon is gone once its reserve runs dry; here it simply stops firing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Mausolon
- Every published board row, as JSON: https://wfsim.app/board/mausolon.json
