# Zaws — weapon identity and assembly

Each strike has independent one-handed and two-handed weapons. Their parts
compose before the shared weapon panel; the simulator, quick calculation,
optimizer and library replay read that same composed weapon.

`data/zaws/parts.yaml` is the authoritative runtime component table. The
[source intake](zaws/source-intake.json) is a provenance snapshot, not a second
runtime input. [Chinese names](zaws/names-zh.json) record DE's Public Export
joins; runtime names and card text live in `data/i18n/zh/zaws.yaml`.

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

The ids are `<strike>_one_handed` and `<strike>_two_handed`, with the
class in their display labels. They identify saved builds and published
records. Each pair shares its strike's internal name, wiki page, mastery track and riven family. Ordinary melee inputs remain modes of each weapon.

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
establish more precise grip/link speed values. Reach comes from the finished
weapon table on the
[Zaw page](https://wiki.warframe.com/w/Zaw), not the strike infobox's 1 m
preview. Follow Through comes from each handedness's package action tree.
Slam elements, radii and forced effects use the package's per-strike/handedness
records. Final slam damage, inherited critical stats and falloff follow the
shared melee rule and [M69](measurements/M69-slam-radial-and-flat-add.md). The
measured Magistar carries the same raw canCrit=false and obsolete event
attenuation, so those native event fields are not final attack statistics.
The staff and polearm heavy inputs retain the common melee class fallback:
the package lacks complete attack events, so their charge/swing timing split
is disclosed as assumed, like the existing melee entries. Heavy-slam recovery
uses the existing 2 s maximum-height approximation.

## Existing engine paths

The pinned [melee package](../tests/fixtures/melee/package-2026-10-08.json)
contains all eleven strikes, their two bare action trees and their slam
profiles. All 22 tree bindings resolve in the shared action catalog. These
references support reuse of the existing melee timeline; they do not verify
assembled component damage or special Exodia attacks in game.

The existing `assembly` build axis is retained. Kitgun payloads keep
`{grip, loader}`; the Zaw payload is `{grip, link}`. The weapon's
declared assembly family determines how the payload is validated and composed.
`weapons::assembly::Assembly` dispatches to Kitgun or Zaw composition. A
wrong-handed grip is rejected for a library record; a stale saved build
repairs only the invalid part and keeps its valid partner. Composition belongs before the shared weapon panel, so
simulation and search receive the same assembled stats.

Each Zaw has its ordinary `melee` arcane seat and an independent `zaw` seat.
Exodia belongs only in the latter. The slot-driven arcane registry and the
existing `arcanes` build axis can carry this seat without a separate saved-build
field. Pax/Residual remains in the `kitgun` seat.

## Exodia intake

All eight records include internal names, rarity, max rank, rank parameters
and source links in the bundle. Their package rank-zero records distinguish
native stat grants from script-driven attacks; a scripted zero is not a
zero-damage effect. Triumph and Valor are implemented through the measured M96/M97 combo
mechanism. The other six remain selectable with an explicit not-modelled
marker; their special attacks, control and wielder effects do not contribute
to simulation or scores.

| Arcane | Source rule at max rank | Integration requirement |
| --- | --- | --- |
| [Triumph](https://wiki.warframe.com/w/Exodia_Triumph) | +50% additional combo count chance | Implemented: additional combo chance; package rank zero is 0.125. |
| [Valor](https://wiki.warframe.com/w/Exodia_Valor) | +200% additional combo count chance against Lifted enemies | Implemented: the target must currently be Lifted; package rank zero is 0.5. |
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

The `assembly` axis retains each complete pair in presets, requests, shares,
optimizer replays and canonical library records. Kitgun shares keep the existing
two-id tuple; Zaw shares append `link` as the component tag. The compact v4
assembly field adds an `l` suffix; older Kitgun fields are unchanged. Ordinary
weapon identities are unchanged. Quick calculation enumerates the compatible
parts; optimization keeps the chosen assembly fixed, as it does for Kitguns.

New special Exodia attacks require their own in-game calibration. Source-derived
composition and integration tests do not replace golden game measurements.
