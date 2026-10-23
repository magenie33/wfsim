# Enkaus

Chinese name: 绘墨

Rifle · Primary · Mastery Rank 12. 28 base damage (corrosive 20, puncture 8), 16% crit chance, 1.8x crit multiplier, 32% status chance.

## Best riven-free build on the WFSim board, as of 2026-10-23

A score belongs to its ruler: compare it only with scores under the same ruler.

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 1.0044 | Stormbringer, High Voltage, Primary Acuity, Primed Shred, Galvanized Aptitude, Critical Delay, Rifle Elementalist, Vital Sense, Vigilante Supplies, Primary Crux | 2026-10-16 07:14 UTC |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 180 s · KPM | base | 11.6196 | Stormbringer, High Voltage, Primary Acuity, Primed Shred, Galvanized Aptitude, Critical Delay, Rifle Elementalist, Vital Sense, Vigilante Supplies, Primary Crux | 2026-10-16 07:53 UTC |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 4-player health · 180 s · KPM | base | 0.08472 | Stormbringer, High Voltage, Primary Acuity, Primed Shred, Galvanized Aptitude, Critical Delay, Rifle Elementalist, Vital Sense, Vigilante Supplies, Primary Crux | 2026-10-16 07:18 UTC |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 180 s · KPM | base | 0.4714 | Stormbringer, High Voltage, Primary Acuity, Primed Shred, Galvanized Aptitude, Critical Delay, Rifle Elementalist, Vital Sense, Vigilante Supplies, Primary Crux | 2026-10-15 22:53 UTC |

## Not modelled here

- An Enkaus hit that leaves an ink-stained enemy below 35% health kills it outright, as one hit at 600% critical chance, 1x critical multiplier and a forced Corrosive status (wiki); the stain comes from the alternate fire or Follie's passive, and bosses and enemies with special states are spared. Nothing is ink-stained here, so every enemy is shot down to zero and the number reads low.
- Primary fire on an ink-stained enemy spends no ammo and restores 0.33 rounds per enemy hit (wiki). Nothing is ink-stained here, so every tick spends a round and the beam reloads on its usual schedule; the number reads low.
- Unmeasured: the page gives the beam a 30 m range and the weapon module 36 m. The sim uses 36 m, so a fight set between 30 m and 36 m reads high if the page is right.
- Unmeasured: the page says the weapon uses no ammo pickups and has an infinite reserve, and the weapon module lists a reserve of 60. The sim uses the 60, so a fight with Infinite ammo unticked can run dry where the game would not, and reads low.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Enkaus
- Every published board row, as JSON: https://wfsim.app/board/enkaus.json
