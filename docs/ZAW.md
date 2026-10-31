# Zaw source intake

Zaws and Exodia arcanes are not enabled in the calculator. The
[source bundle](zaws/source-intake.json) contains component statistics,
weapon identities, package action bindings and eight Exodia records for their
implementation. [Chinese names](zaws/names-zh.json) are transcribed from DE's
Public Export, joined by internal name. No existing score changes here.

## Weapon identity

Each strike has two roster entries, one for each handedness. The weapon's
class, stance pool and bare action tree are fixed by that entry. Grips offered
on its page belong to that handedness; links fit either entry. This follows
[Kitgun identity](KITGUNS.md), with a strike replacing the chamber.

| Strike | One-handed class | Two-handed class |
| --- | --- | --- |
| Balla | Dagger | Staff |
| Cyath | Machete | Polearm |
| Dehtat | Rapier | Polearm |
| Dokrahm | Scythe | Heavy Blade |
| Kronsh | Machete | Polearm |
| Mewan | Sword | Polearm |
| Ooltha | Sword | Staff |
| Plague Keewar | Scythe | Staff |
| Plague Kripath | Rapier | Polearm |
| Rabvee | Machete | Hammer |
| Sepfahn | Nikana | Staff |

The proposed ids are `<strike>_one_handed` and `<strike>_two_handed`, with the
class in their display labels. They are design candidates, not saved wire
identifiers. Each pair shares its strike's internal name, wiki page, mastery
track and riven family. Ordinary melee inputs remain modes of each weapon.

Eleven strikes, five grips per handedness and sixteen links give 22 weapon
entries, 80 assemblies per entry and 1,760 assemblies in total. A complete
assembly remains part of the build, including in sharing, search and board
identity.

## Composition

The wiki's [assembly module](https://wiki.warframe.com/w/Module:Modular) and
[component table](https://wiki.warframe.com/w/Module:Modular/data) define these
operations. Values remain unrounded until the engine's existing damage
quantization and display formatting apply.

```
total_damage = (sum(strike.damage) + grip.damage + link.damage)
               * strike.base_multiplier[handedness]
attack_speed = grip.speed + strike.speed + link.speed
crit_chance = strike.crit_chance + link.crit_chance
status_chance = strike.status_chance + link.status_chance
crit_multiplier = strike.crit_multiplier
```

For an ordinary strike, each damage type keeps its share of the original
total. For Plague Keewar and Plague Kripath, each of their four damage types
receives one quarter of the difference between the composed total and the
strike's original total. Proportional rescaling would change their Viral
share. The per-handedness base multiplier participates in this difference.

The weapon-specific
[infobox module](https://wiki.warframe.com/w/Module:Weapons/data/modular)
overrides the assembly overview for riven disposition. Ten overview values
disagree; the source bundle records both and selects the weapon-specific value.
The export's empty or placeholder combat statistics are not assembled stats.

Component speed values retain the wiki's decimal precision. The package
fixture's unassembled weapon has its own placeholder stats; it does not
establish more precise grip/link speed values. Finished reach and follow-through
also need their own source or measurement; the strike infobox is not a complete
assembled weapon.

## Existing engine paths

The pinned [melee package](../tests/fixtures/melee/package-2026-10-08.json)
contains all eleven strikes, their two bare action trees and their slam
profiles. All 22 tree bindings resolve in the shared action catalog. These
references support reuse of the existing melee timeline; they do not verify
assembled component damage or special Exodia attacks in game.

The existing `assembly` build axis is retained. Kitgun payloads keep
`{grip, loader}`; the proposed Zaw payload is `{grip, link}`. The weapon's
declared assembly family determines how the payload is validated and composed.
The current `kitguns::Assembly` type and metadata helper cannot accept links
without extension. Composition belongs before the shared weapon panel, so
simulation and search receive the same assembled stats.

Each Zaw has its ordinary `melee` arcane seat and an independent `zaw` seat.
Exodia belongs only in the latter. The slot-driven arcane registry and the
existing `arcanes` build axis can carry this seat without a separate saved-build
field. Pax/Residual remains in the `kitgun` seat.

## Exodia intake

All eight records include internal names, rarity, max rank, rank parameters
and source links in the bundle. Their package rank-zero records distinguish
native stat grants from script-driven attacks; a scripted zero is not a
zero-damage effect. No row in this table implies a working runtime effect.

| Arcane | Source rule at max rank | Integration requirement |
| --- | --- | --- |
| [Triumph](https://wiki.warframe.com/w/Exodia_Triumph) | +50% additional combo count chance | Reuse the measured combo mechanism, M96; package rank zero is 0.125. |
| [Valor](https://wiki.warframe.com/w/Exodia_Valor) | +200% additional combo count chance against Lifted enemies | Reuse the existing target-conditioned combo stat; package rank zero is 0.5. |
| [Brave](https://wiki.warframe.com/w/Exodia_Brave) | Direct heavy-attack kills grant 5 energy/s for 4 s, up to three stacks | Energy regeneration, channeling exclusions and refresh behavior need support. |
| [Might](https://wiki.warframe.com/w/Exodia_Might) | Finisher kills have 50% chance to grant 30% melee lifesteal for 8 s | Finisher activation and wielder healing; ground finishers are excluded by the page. |
| [Hunt](https://wiki.warframe.com/w/Exodia_Hunt) | Slams have 50% chance to pull within 12 m | Pull geometry and its short persistent window; it is not extra melee reach. |
| [Force](https://wiki.warframe.com/w/Exodia_Force) | Weapon status has 50% chance to cause a 200% weapon-damage burst within 6 m | Player-centered radial hit, full falloff, no crit/status; stance, faction and quarter-combo scaling with CO excluded. |
| [Contagion](https://wiki.warframe.com/w/Exodia_Contagion) | Aim-gliding melee after a bullet/double jump launches an explosive projectile | Separate direct hit, embedded ticks and explosion; inherited crit/status, stance and distance/combo scaling. |
| [Epidemic](https://wiki.warframe.com/w/Exodia_Epidemic) | A post-jump slam emits a wave that suspends targets for 4 s | Separate wave and explosion profiles; the page's approximate damage/geometry needs measurement. |

Contagion is not a flat melee damage buff. Its page describes a 2x direct hit
split equally across IPS, 1x Blast embedded ticks and a 5x Viral explosion with
an 8 m radius. Some buffs apply twice, while CO affects direct hits separately.
Its rank table and mechanics prose express the distance bonus differently;
the full bracket, timing and stance dependence need in-game calibration.

Force excludes crit and status generation even when the triggering hit crits.
Epidemic's suspension does not trigger Melee Afflictions or contribute a
Condition Overload status according to its page. Those rules require distinct
attack/control handling rather than assigning a generic Lifted status.

## Evidence and open work

The source bundle retains URLs and SHA-256 digests for the raw wiki pages and
the existing package fixture. All 1,760 source-derived assemblies have positive
finite stats and damage components that sum to their total; all 22 package
tree bindings resolve. Removing a strike was rejected by the intake guard.
These are source consistency checks, not golden game measurements.

Runtime work includes assembling the two weapon identities, metadata/UI,
request validation, sharing, optimizer enumeration and board persistence.
Triumph and Valor can use existing measured mechanics. Special attacks need
their own calibration before they contribute scores. All 37 components and
eight arcanes have official localized names and internal ids in the intake;
assembled geometry and component precision remain open source work.
