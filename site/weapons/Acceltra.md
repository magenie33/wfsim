# Acceltra

Chinese name: 迅发电浆炮

Rifle · Primary · Mastery Rank 8. 26 base damage (impact 26), 32% crit chance, 2.8x crit multiplier, 6% status chance.

## Best riven-free build on the WFSim board, as of 2026-11-11

A score belongs to its ruler: compare it only with scores under the same ruler (https://wfsim.app/method#ruler).

Each row is measured on its own, and says when and by which WFSim commit; the game and WFSim both change, so an older row may be behind.

| Ruler | Fight | Mode | Score | Build | Measured |
| --- | --- | --- | ---: | --- | --- |
| Standard Single Target | Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 7.7586 | Primed Cryo Rounds, Malignant Force, Heavy Caliber, Primary Acuity, Serration, Galvanized Scope, Vile Acceleration, Vital Sense, Vigilante Supplies, Primary Compression | 2026-11-03 19:25 UTC · 51a5068e9d8b |
| Standard Multi Target | 5x5 at 3 m · Thrax Centurion Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 7.7586 | Primed Cryo Rounds, Malignant Force, Heavy Caliber, Primary Acuity, Serration, Galvanized Scope, Vile Acceleration, Vital Sense, Vigilante Supplies, Primary Compression | 2026-11-03 19:25 UTC · 51a5068e9d8b |
| Demolisher | one target · Demolisher Devourer Lv 9999 SP · 100% headshots, aiming · 4-player health · 180 s · KPM | base | 0.7333 | Primed Cryo Rounds, Malignant Force, Heavy Caliber, Primary Acuity, Serration, Galvanized Scope, Vile Acceleration, Vital Sense, Vigilante Supplies, Primary Compression | 2026-11-03 19:25 UTC · 51a5068e9d8b |
| Heavy Gunner | one target · Eximus Corrupted Heavy Gunner Lv 9999 SP · 100% headshots, aiming · 180 s · KPM | base | 3.9438 | Primed Cryo Rounds, Malignant Force, Heavy Caliber, Primary Acuity, Serration, Galvanized Scope, Vile Acceleration, Vital Sense, Vigilante Supplies, Primary Compression | 2026-11-03 19:25 UTC · 51a5068e9d8b |

## Not modelled here

- Native projectile collision dimensions are bound as evidence; unverified width and sweep consumers retain the declared collision approximation.
- The parsed falloff floor exceeds the direct packet's base damage; that falloff consumer is unverified and the declared falloff convention remains in use.
- Flight uses the declared constant speed; native projectile acceleration is not simulated.
- Sustained-fire spread growth, recovery and smoothing are not simulated; shots use the resting spread for the selected aiming state. Camera recoil is compensated by holding the reticle steady.
- The rockets arm only after 7 m (wiki); closer, they hit without exploding. The sim applies the explosion at every range, so a fight set inside 7 m reads high by the whole explosion, most of the weapon.
- Reloading while sprinting gives +25% reload speed (+50% on Gauss); nobody moves here, so the reload is always the standing one.
- The blast needs no line of sight. There is no cover here, so the perk adds nothing.
- In game the blast staggers the wielder; the player has no body here, so it costs nothing.

## In WFSim

- Build, simulate and optimize it: https://wfsim.app/weapons/Acceltra
- Every published board row, as JSON: https://wfsim.app/board/acceltra.json
