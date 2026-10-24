# M124 — a mod's value a rank is the game package's, and the card rounds it ✅ (SuperExboom, 2026-10-24)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Not a reading in a mission: the values the game itself loads. **SuperExboom**
unpacked the PC client's local cache and read every `/Lotus/Upgrades` record
in it, 9,482 objects with no parse error, cross-checked on 37 of them against
an independent parser.

| | |
|---|---|
| client | FileVersion `2026.10.15.13.08`, cache dated 2026-10-21 |
| `Packages.bin_H` | 53,613,446 bytes, SHA-256 `e31e78ed55ff6def14eb6b770ea6d0db40259b1f13f216a9c18859d91e7f00a8` |
| the dump | one row per record, line and rank: the text token, its float32, the rank curve, the card's rounding |

## What it settles

**Each line of a mod stores one value, and rank `r` is `(r + 1) x` it**
(`QC_LINEAR`, the curve of every card below). The card multiplies the same
value and rounds for display (`RoundTo`, `RM_ROUND`). So the data's `rank0` is
the stored value and `rankMax` is it times the rank count, and a card's round
max says nothing about the third decimal: Pistol Acuity stores 0.318, which
the card shows as 350% and M123's five readings take as 3.498.

**The value is what the designer typed.** The text token is a float32 printed
long (`0.31799999` is 0.318, `0.33333001` is 0.33333), so the data takes the
shortest decimal that is the same float32. A token that is not the round
number's float32 is a different number, and the data keeps it: Carnis
Stinger's status chance prints `0.10000001` where every card that stores 0.1
prints `0.1`.

What a float32 rounds is a part in ten million, out of reach of any number the
game draws; what the designer typed is up to a part in a hundred (Galvanized
Reflex, Amalgam Organ Shatter).

## What changed

Every effect whose card names a package line, against the data before:

| mod | effect | was rank0 / max | now |
|---|---|---|---|
| Amalgam Javlok Magazine Warp | `unmodelled` | 0.167 / 1.00 | 0.16666 / 0.99996 |
| Amalgam Organ Shatter | `crit_damage_bonus` | 0.141667 / 0.85 | 0.141 / 0.846 |
| Amalgam Serration | `base_damage_bonus` | 0.1409 / 1.55 | 0.141 / 1.551 |
| Amalgam Serration | `sprint_speed_bonus` | 0.0227 / 0.25 | 0.0225 / 0.2475 |
| Amalgam Shotgun Barrage | `fire_rate_bonus` | 0.1417 / 0.85 | 0.141 / 0.846 |
| Ammo Chain | `ammo_max_bonus` | 0.1667 / 1.0 | 0.1667 / 1.0002 |
| Archgun Ace | `buff` | 0.0833 / 0.5 | 0.08333 / 0.49998 |
| Archgun Ace | `buff` | 0.1667 / 1.0 | 0.1666 / 0.9996 |
| Berserker Fury | `stacking_buff` | 0.058333 / 0.35 | 0.0583333 / 0.3499998 |
| Blood Rush | `crit_chance_per_combo` | 0.036364 / 0.4 | 0.0364 / 0.4004 |
| Carnis Stinger | `status_chance_bonus` | 0.1 / 0.6 | 0.10000001 / 0.60000006 |
| Cautious Shot | `self_stagger_reduction` | 0.0909 / 1.0 | 0.0909 / 0.9999 |
| Combo Killer | `melee_combo_duration_bonus` | 0.8 / 5.0 | 0.8333 / 4.9998 |
| Condition Overload | `condition_overload` | 0.133333 / 0.8 | 0.1333 / 0.7998 |
| Creeping Bullseye | `crit_chance_bonus` | 0.333 / 2.0 | 0.33333 / 1.99998 |
| Creeping Bullseye | `fire_rate_bonus` | -0.033 / -0.2 | -0.03333 / -0.19998 |
| Critical Deceleration | `crit_chance_bonus` | 0.3333 / 2.0 | 0.33333 / 1.99998 |
| Critical Deceleration | `fire_rate_bonus` | -0.0333 / -0.2 | -0.0333 / -0.1998 |
| Critical Delay | `crit_chance_bonus` | 0.3333 / 2.0 | 0.33333 / 1.99998 |
| Critical Delay | `fire_rate_bonus` | -0.0333 / -0.2 | -0.0333 / -0.1998 |
| Enduring Affliction | `status_chance_on_lifted` | 0.167 / 1.00 | 0.16666 / 0.99996 |
| Galvanized Acceleration | all four lines | 0.0273 / 0.3 | 0.0272727 / 0.2999997 |
| Galvanized Aptitude | `buff` | 0.0364 / 0.4 | 0.0363636 / 0.3999996 |
| Galvanized Aptitude | `status_chance_bonus` | 0.0727 / 0.8 | 0.072727 / 0.799997 |
| Galvanized Chamber | `buff` | 0.0273 / 0.3 | 0.02727 / 0.29997 |
| Galvanized Chamber | `multishot_bonus` | 0.0727 / 0.8 | 0.07273 / 0.80003 |
| Galvanized Crosshairs | `buff`, aiming | 0.109 / 1.20 | 0.10909 / 1.19999 |
| Galvanized Crosshairs | `buff`, per kill | 0.036 / 0.40 | 0.036363 / 0.399993 |
| Galvanized Diffusion | `buff` | 0.027 / 0.3 | 0.0272727 / 0.2999997 |
| Galvanized Elementalist | `stacking_buff` | 0.027273 / 0.3 | 0.02727 / 0.29997 |
| Galvanized Elementalist | `status_damage_bonus` | 0.072727 / 0.8 | 0.07272 / 0.79992 |
| Galvanized Hell | `buff` | 0.027 / 0.3 | 0.02727 / 0.29997 |
| Galvanized Reflex | `heavy_attack_efficiency` | 0.045455 / 0.5 | 0.04545 / 0.49995 |
| Galvanized Reflex | `stacking_buff`, Initial Combo | 2.0 / 20.0 | 1.86 / 20.46 |
| Galvanized Savvy | `buff` | 0.036 / 0.4 | 0.0363636 / 0.3999996 |
| Galvanized Savvy | `status_chance_bonus` | 0.0727 / 0.8 | 0.07273 / 0.80003 |
| Galvanized Scope | `buff`, aiming | 0.1091 / 1.2 | 0.1091 / 1.2001 |
| Galvanized Scope | `buff`, per kill | 0.0364 / 0.4 | 0.03636 / 0.39996 |
| Galvanized Shot | `buff` | 0.036 / 0.4 | 0.0363636 / 0.3999996 |
| Galvanized Shot | `status_chance_bonus` | 0.073 / 0.8 | 0.072727 / 0.799997 |
| Galvanized Steel | `stacking_buff` | 0.027273 / 0.3 | 0.02727 / 0.29997 |
| Jugulus Spines | `status_chance_bonus` | 0.1 / 0.6 | 0.10000001 / 0.60000006 |
| Metal Auger | `punch_through_bonus` | 0.4 / 2.1 | 0.35 / 2.1 |
| Pistol Acuity | both lines | 0.32 / 3.5 | 0.318 / 3.498 |
| Pistol Ammo Mutation | `ammo_conversion` | 0.08 / 0.5 | 0.084 / 0.504 |
| Power Throw | `punch_through_bonus` | 0.3 / 2.0 | 0.3334 / 2.0004 |
| Primary Acuity | both lines | 0.3182 / 3.5 | 0.318 / 3.498 |
| Primed Counterbalance | `recoil_reduction` | -0.0773 / -0.85 | -0.07727 / -0.84997 |
| Primed Pistol Ammo Mutation | `ammo_conversion` | 0.08 / 0.92 | 0.084 / 0.924 |
| Primed Reach | `melee_range_bonus_m` | 0.272727 / 3.0 | 0.2727 / 2.9997 |
| Primed Rifle Ammo Mutation | `ammo_conversion` | 0.08 / 0.92 | 0.084 / 0.924 |
| Primed Shotgun Ammo Mutation | `ammo_conversion` | 0.08 / 0.92 | 0.084 / 0.924 |
| Primed Stabilizer | `recoil_reduction` | -0.0773 / -0.85 | -0.07727 / -0.84997 |
| Primed Steady Hands | `recoil_reduction` | -0.077 / -0.85 | -0.07727 / -0.84997 |
| Primed Tactical Pump | `reload_speed_bonus` | 0.0909 / 1.0 | 0.0909 / 0.9999 |
| Resolute Focus | `accuracy_bonus` | 0.0833 / 0.5 | 0.0835 / 0.501 |
| Resolute Focus | `stagger_resist_bonus` | 0.1667 / 1.0 | 0.167 / 1.002 |
| Rifle Ammo Mutation | `ammo_conversion` | 0.08 / 0.5 | 0.084 / 0.504 |
| Rubedo-Lined Barrel | `base_damage_bonus` | 0.1667 / 1.0 | 0.1666 / 0.9996 |
| Saxum Spittle | `status_chance_bonus` | 0.1 / 0.6 | 0.10000001 / 0.60000006 |
| Seeker | `punch_through_bonus` | 0.4 / 2.1 | 0.35 / 2.1 |
| Shotgun Ammo Mutation | `ammo_conversion` | 0.08 / 0.5 | 0.084 / 0.504 |
| Split Flights | `stacking_buff` | 0.167 / 1.0 | 0.16667 / 1.00002 |
| Spring-Loaded Blade | `stacking_buff` | 0.2 / 1.0 | 0.166667 / 1.000002 |
| Vigilante Offense | `punch_through_bonus` | 0.3 / 1.5 | 0.25 / 1.5 |
| Volatile Quick Return | `bounce_blast_radius_bonus_m` | 0.8 / 3.0 | 0.75 / 3 |
| Weeping Wounds | `status_chance_per_combo` | 0.066667 / 0.4 | 0.06666 / 0.39996 |

Two more keep a rank 0 of `max / ranks` that the package does not show: Power
Throw's consecutive-throw damage (0.166667) and Primary Frostbite (0.00375).
Their lines are run by a script, which the dump does not carry.

A card's table bounds the value; it does not pin it. Condition Overload's
0.7998 shows as the round number on every rank, and Galvanized
Elementalist's 0.07272 sits inside the interval its table allows, where the
shortest decimal is 0.0727. Critical Delay's -0.0333 a rank reads 16.6% at
rank 4 only because its float32 is 16.6499.

## What it does not settle

Galvanized Reflex's +20.46 Initial Combo is a count; M125 reads it to the digit,
four stacks at 81 and three at 61: the game keeps the decimals and shows the
counter floored.

- **Script-driven lines.** Arcanes and mods run by an apply script (107 of
  195 arcane records, Reflex Draw, Shivering Contagion, Hata-Satya) store
  parameters whose arithmetic lives in a script the cache does not hold.
  Their data is unchanged.
- **How the game multiplies.** The package is the input. The order and
  precision of the arithmetic on it is what the readings measure.

Tests: `data::mods::tests::catalog`, `build::loadout::tests::resolve`,
`fight::tests::m123_laetum_acuity`.
