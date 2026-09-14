# Warframes — the builder's frame

A Warframe has a BUILDER and nothing else. What it produces is a frame's stats
and its four abilities at those stats; nothing here reaches a fight yet, and a
weapon page does not read it. `engine::warframes_data` is the whole engine side,
`/api/warframe/catalog` and `/api/warframe/panel` are its two doors, and
`/warframes/<Wiki_Name>` is the page.

## What a build is

| part | count | data |
| --- | --- | --- |
| mods | 8 | `data/warframe_mods/` |
| exilus | 1 | an exilus-eligible card from the same pool |
| aura | 1 | `data/auras/` |
| arcanes | 2 | `data/warframe_arcanes/` |
| archon shards | 5 | `data/shards/` |
| Helminth | 0 or 1 | a `subsumable` card in `data/warframe_abilities/` over one of the four slots |

**AN AURA IS FILED ONCE.** `data/auras/` is read by the fight's Tenno and by
this builder; a card the fight cannot use says `kind: out_of_scope`, and
`auras_data::in_fight` keeps it off the simulator's squad list. Coaction Drift is
filed there because the fight reads it, and is seated in the EXILUS slot
(`exilus: true`).

**A FRAME'S ARMOR, ENERGY AND SPRINT ARE `data/frames.yaml`'s**, the same row
the fight's Tenno reads. `data/warframes/<id>.yaml` carries only what a build
needs beyond them, and loading one whose id is not in the roster panics.

## The numbers, and where each rule comes from

- **A card at rank r** is `max × (r + 1) / (max_rank + 1)` (`at_rank`). No page
  states it; it is the rule Umbral Vitality's own table fits at every rank.
- **Health, shields, armor, energy** are `base × (1 + Σ mods) + Σ flat`
  (W`Health`, W`Shield`, W`Armor`, W`Energy_Capacity`). The base is rank 30:
  `Health + 100`, `Shield + 100` or the module's `ShieldRank30`, `Energy + 50`.
- **Ability stats** start at 100% and add (W`Ability_Duration`: "Modifiers
  combine additively"). No floor is applied to strength, duration or range,
  because no page states one.
- **Cost and drain**, W`Ability_Efficiency`:
  `cost × max(2 − efficiency, 25%)` and
  `drain × max((2 − efficiency) / duration, 25%)`.
- **Casting speed** divides a cast time: `time ÷ (1 + bonus)`.
- **Archon shards**: Crimson and Amber percentages add to their stat; each Azure
  number is "a flat value increase after all bonuses are applied". A shard line
  that is not one of those stats is an admission, not a number.
- **The Umbral set** adds a share of each card's OWN value
  (`set_bonus:` on the three cards, from W`Umbral_Set`).
- **An aura's capacity** doubles on its own polarity and is 80% rounded down on
  another (W`Aura`). W`Mod` says 75% rounded half up; the two differ only at a
  capacity of 2, 6, 10 or 14, which no max-rank aura has. The stance slot follows
  the same page (`mods::stance_capacity`).
- **Polarity** is a set without positions (the module names none) and moves
  freely between the eight slots, the exilus slot and the aura slot: the exilus
  slot's since Techrot Encore, the aura slot's since 38.5.

## Tags

**A FRAME'S BUILD IS READ BY WHAT IT CAN DO, NOT RANKED BY A NUMBER.** A frame
answers survival, abilities and movement at once, so no single score orders two
builds. What a build states instead is a closed set of tags
(`warframes_data::Capability`), each with every source that grants it:

| tag | means |
| --- | --- |
| `invulnerable` | takes no damage at all for a while |
| `status_cleanse` | removes status effects already on the frame |
| `status_immunity` | no new status effect lands; what is already there stays |
| `damage_cap` | damage taken has a hard ceiling, not a percentage reduction |

A tag is DATA on the item that grants it — `tags:` on a mod, arcane or ability,
`passive_tags:` on a frame — as `{tag, when}`, with the wiki sentence quoted in a
comment. An unknown tag panics at load. An augment whose ability is not in the
loadout grants no tag, as it pays nothing. A CLEANSE and an IMMUNITY are two
tags because they answer two questions — one removes what landed, the other
stops what is coming — and one ability may grant both (Defy, Fire Walker).

## Shield gate

**THE SHIELD GATE IS ITS OWN ENTRY**, because every frame with shields has one
and a build only decides how long it lasts and whether casting re-opens it.

- Its length is W`Shield`'s approximation of the shields held when they break:
  `S/180 + 1/3` under 53, `(S/350)^0.65 + 1/3` to 1,150, then 2.5 s
  (`shield_gate_seconds`). No shields, no gate — Arcane Persistence is ×0.
- Catalyzing Shields is two structured lines: ×0.80…×0.20 shields and a fixed
  0.33…1.33 s gate, per rank.
- Casting re-opens it through energy converted to shields: the Augur set
  (`energy_to_shield_by_count` in `data/mod_sets/augur.yaml`) plus Brief Respite
  (`energy_to_shield` on the aura). Channelled drain converts nothing.
- Under Catalyzing Shields ANY refill re-opens the fixed gate, however small —
  measured (MEASUREMENTS M92), which settles the mod page's "upon
  recovering any amount of Shields" against the Update 34 notes' scaling.
  Without it, a refill's length is the formula at the shields it restored.

## Operator

**AN OPERATOR BUILD IS MADE ONCE AND LINKED.** `/operator` holds a Focus
school and which of its conditional nodes count as running; a Warframe build
links one by the Operator build's `id` (`operator:` in its state, resolved to
`OperatorPick` when sent), so a Focus choice made once reaches every frame and a
rename cuts no link. An `operator:` holding a name becomes that build's id.

- ONLY THE ACTIVE SCHOOL APPLIES: "Active and Passive ways are only usable in the
  specific focus school they belong to" (W`Focus`). No Waybound reaches a
  Warframe, so `data/focus/` carries none.
- `always: true` counts whenever the school is active (Stone Skin). A node that
  needs an Operator action counts only when the Operator build ASSUMES it, which
  is the house rule for a condition about the Tenno.
- A node's TAG counts whenever its school is active: the node can be used, and a
  tag says what a build can do rather than what is running.

**THE OPERATOR IS NOT A WARFRAME.** The home page lists it in a group of its
own between Warframes and Weapons, with the wiki Operator page's portrait: a
player owns many Warframes and is one Operator.

**THE PLAYER HAS ONE OPERATOR, AND ANY NUMBER OF OPERATOR BUILDS.** An Operator
build is a school, its assumed nodes and that school's Tektolyst Artifact; the
Amp is not part of it yet.

- One artifact per school (`artifact:` in `data/focus/<school>.yaml`), with 5
  mod slots and 1 arcane slot (W`Tektolyst_Artifact`). Every card is at max
  rank.
- An Antique mod (`data/artifact_mods/`) is Universal with a base drain of 0, so
  an artifact has no capacity and no polarity. Any artifact seats any school's
  card, and one card is seated once.
- A card's `bonus` is its second line, paid once per thing `per` counts: a
  school id counts that school's cards; `unique_school` counts each OTHER school
  seated once, never the card's own (MEASUREMENTS M93). `/api/operator/panel`
  pays it out for what is seated.
- Nothing on an artifact moves a Warframe's own numbers: its mods are the
  Operator's and the Amp's, and its arcanes reach the Tauron Strike, Amp and
  Warframe WEAPONS (`data/artifact_arcanes/`).

## Abilities

`data/warframe_abilities/` holds the CARD of an ability — cost, description,
icon, and for a frame the builder seats, its numbers. An ability's number is
written once, at max rank and 100% of every stat, with the stat that scales it
read off the page's `{{Stat|Ability X|icon=only}}` marker; an underlined number
scales with nothing.

`data/abilities/` is a different thing: the BUFF an ability hands a weapon in a
fight (Roar's faction bracket). The two share ids by design and are joined by
them.

- `helminth_value` is the subsumed version's number where the page prints one.
- `adds_to_base: armor` is a share of the frame's BASE stat, additive with that
  stat's mods (Warcry). `channel_multiplier` multiplies it while another ability
  runs (Warcry × 3 in Hysteria). Each produces a derived line.

## The Helminth

One ability over one slot. The pool is every `subsumable: true` card — the
module's `Subsumable` flag, which covers the Helminth's own abilities too.

- **A frame is never offered its own ability.** No page states it either way,
  and infusing Valkyr with Warcry would put two copies of one ability in a
  loadout.
- **The damage-buff restriction is not modelled**: Eclipse, Roar and Xata's
  Whisper may only replace a named ability on nine frames (W`Helminth`
  §Damage buff restrictions). No frame in `data/warframes/` is one of them.
- **An augment pays only while its ability is in the loadout**; otherwise the
  card is seated and says it is inert.

## Passives

**A PASSIVE THAT MOVES A WEAPON'S NUMBER IS SIMULATED**, and it reaches a fight
through the wielder. Valkyr's Nimble moves none. Rage is `rage:` on both
Valkyrs, run by `engine::rage`:

- Every body a melee hit lands on adds 3% and every melee kill 12%, up to 300%.
  The meter sits in the base-damage bucket: "additive with mods like Pressure
  Point".
- After 5 s without building, the meter decays along the page's curve from
  where it stands on it. The page calls the rate "dependent on the amount of
  Rage".
- A kill is paid at the next swing. A Finisher's 27% is in no loop.
- The `valkyr_rage` buff card opens the meter at a percent, and its lock holds
  the meter there.

## A weapon's wielder

**A WEAPON IS ALWAYS HELD BY SOMEBODY, AND A WEAPON BUILD SAYS WHO.** The
`wielder` build axis (`builds::BUILD_AXES`) is a link to one of this module's
saved builds — by its preset `id` — or a modelled frame with no build, or the
**Prototype**: `data/tenno/default.yaml`, every stat the lowest any released
Warframe has at rank 30, with no mods and no passive.

- The fight's player is that build's `resolve` — health, shields, armor, energy,
  sprint — with its archon shards and its own aura (`webapi::wielder_from`).
- The FIGHT keeps what others hand the wielder: its state, its own stat bonuses,
  the squad's auras, and the ticked overrides, which replace a resolved number.
  A scenario names no frame and no shards.
- A weapon a frame summons names its wielders (`wielders:`) and is held by the
  first when a link names any other; `wielder_names:` renames it in one frame's
  hands ("Valkyr Prime Talons").
- The BOARD never records one: every ruler scores in the Prototype's hands.
- A share link does not carry the wielder yet (`SHARE_EXCLUDED_AXES`).

## Adding a frame

1. `data/warframes/<id>.yaml`: rank-30 `health` and `shield`, the innate
   `polarities`, `aura_polarity`, `passive`, and the four ability ids.
2. Its four abilities' `stats:` in `data/warframe_abilities/` — the subsumable
   one is already there without numbers.
3. Its augments into `data/warframe_mods/` with `augments: <ability>`.
4. `python scripts/gen_assets.py --write`, then the zh names
   (`scripts/wfcd_i18n.py fill --section warframe_mods`), then
   `python scripts/fetch_images.py`.
