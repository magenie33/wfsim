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

## Adding a frame

1. `data/warframes/<id>.yaml`: rank-30 `health` and `shield`, the innate
   `polarities`, `aura_polarity`, `passive`, and the four ability ids.
2. Its four abilities' `stats:` in `data/warframe_abilities/` — the subsumable
   one is already there without numbers.
3. Its augments into `data/warframe_mods/` with `augments: <ability>`.
4. `python scripts/gen_assets.py --write`, then the zh names
   (`scripts/wfcd_i18n.py fill --section warframe_mods`), then
   `python scripts/fetch_images.py`.
