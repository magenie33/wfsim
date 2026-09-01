# Extra Hit — one law behind four things we built separately

An **Extra Hit** is a second damage instance that lands beside a hit, worth a
percentage of it, rolling its own status. It is DE's own category (wiki:
[`Extra_Hit`](https://wiki.warframe.com/w/Extra_Hit)) and we arrived at it three
times from three directions before reading the page that names it:

- **Primary Debilitate's split** — deduced from "the status it leaves carries
  the faction bonus three times", because three is only reachable through one
  more link in the chain;
- **Cyte-09's Resupply** — the community formula that decodes MEASUREMENTS
  M33's 29551, which M33 filed as "the ABILITY case" and could not connect to
  the arcane;
- **Xata's Whisper** — implemented 2026-08-09 from its own wiki page, as an
  ability that "displays two damage numbers".

They are the same mechanic. This file is the law; the members only supply a
percentage and an element.

> *"Extra Hit is a unique buff that adds an additional hit to the target,
> dealing a percentage of the original damage value and independently rolling
> Status Effects."*

## The ladder, and why the numbers are what they are

    Extra Hit Damage = Weapon Hit Damage × Extra Hit % × (1 + Faction Bonuses)

The faction bonus appears **twice**: once inside `Weapon Hit Damage`, which
already carried it, and once again outside. That is the whole reason
`faction_at(f, depth)` has three rungs and not two:

| what | faction | in the engine |
| --- | --- | --- |
| a hit | `f¹` | `DEPTH_HIT` |
| a status the hit left | `f²` | `DEPTH_PROC` |
| **an Extra Hit** | `f²` | `trigger_raw × f` in `fire_extra_hits` |
| **a status an Extra Hit left** | `f³` | `DEPTH_DERIVED_PROC` |

> *"The Damage over Time formula applies Faction Damage Bonuses again, allowing
> status effects created by an Extra Hit to 'triple-dip' on these bonuses."*

MEASUREMENTS M33 held the exponent at 3 because the wiki stated it while the
reasoning suggested 2. **The number was right and the
reasoning was incomplete** — this page supplies the missing rung.

## What a status left by an Extra Hit burns off

> *"Damage over Time status effects created by an Extra Hit will use the Extra
> Hit Damage as Modded Base Damage in their damage calculations."*
>
> *"This allows Elemental Bonuses to contribute to the Modded Base Damage of a
> status effect when they normally would not."*

So an Extra Hit's status is **not** scaled the way an ordinary weapon status is.
An ordinary one reads `ModifiedBase`, which excludes the elemental portions; an
Extra Hit's reads the Extra Hit's own damage, which includes them.

### …and when the Extra Hit deals ZERO

Primary Debilitate is listed on that page as *"a 0-damage Extra Hit that applies
a guaranteed status effect"*. Read literally, the rule above gives a status base
of zero — and the status plainly does damage, so the rule cannot be read
literally at 0.

**THE RULE:**

> An Extra Hit's status burns off the Extra Hit's damage. **A 0% Extra Hit
> replaces nothing, so the base is the one it would have replaced — the
> triggering instance's own.**

That is one rule covering both members instead of an arcane-shaped exception:

- **Resupply / Xata's Whisper (pct > 0)** — the extra hit's damage IS the base.
  Said from the other direction: the level ABOVE is what Resupply replaced — the level
  above is REPLACED by it.
- **Primary Debilitate (pct = 0)** — nothing replaced it, so the level above
  stands. Which is what this engine already computed (`mb_live` from the
  triggering instance, elements excluded, matching the Toxin page's worked
  example to the digit).

It also retires the last open half of M33. The reading that read the FULL modded
hit shipped for one commit and was reverted because it moved published board
rows by up to +112% — and it was reverted for the right reason: it took a rule
written for an Extra Hit **with damage** and applied it to the only member that
has none.

## The members

| source | % (max) | element | status | where |
| --- | --- | --- | --- | --- |
| Xata's Whisper (Xaku) | 26% | Void | weapon's roll | `data/abilities/xatas_whisper.yaml` ✅ |
| **Toxic Lash** (Saryn) | 30% | Toxin | **guaranteed** | `data/abilities/toxic_lash.yaml` ✅ |
| **Resupply** (Cyte-09) | 25% (**50% sniper**) | **chosen, 10** | **guaranteed** | `data/abilities/resupply.yaml` ✅ |
| Primary Debilitate | **0%** | the split component | guaranteed | `data/arcanes/primary/primary_debilitate.yaml` ✅ |
| Silken Stride | 40% | Toxin | ? | — |
| Uriel's Demonium Rune | 30% | Heat | ? | — |
| Reconifex Active Reload | 25% | Heat | ? | — |
| Melee Duplicate | 100% | — | — | out of scope (melee) |
| **Melee Influence** | the element's own share | the status that landed | **guaranteed** | `data/arcanes/melee/melee_influence.yaml` ✅ |

### The three things a member may differ in, and nothing else

Adding Toxic Lash and Resupply is what proved the category, because between them
they needed exactly three fields and no new mechanism:

- **`forced_status`** — Xata's extra hit rolls the weapon's own chance
  ("附加的虚空伤害具有基于武器本身触发几率的独立触发几率"); Toxic Lash is
  "100% (Toxin status chance)" and Resupply grants "the selected Elemental
  Damage **and Status Effect**". A forced one goes down the same `forced`
  channel a weapon's own guaranteed proc uses, so caps, immunities and
  Condition Overload all see it identically.
- **`element: selectable` + `elements:`** — Resupply's gear wheel. The choice
  rides on the PICK, so one definition serves all ten and the page draws its
  dropdown from the data's own list in the game's own order.
- **`class_bonus_for` / `class_bonus`** — Resupply is 20/30/40/50% on Sniper
  Rifles against 10/15/20/25%. Applied in `abilities_data::resolve`, the one
  function handed both the ability and the weapon it is cast on, so the sim
  never learns what a sniper is.

The three unimplemented ability sources are `kind: extra_hit` entries in
`data/abilities/` and nothing else — the machinery is `dummy::fire_extra_hits`,
and it reads the list rather than any weapon or ability name.

## …and the one that is not a percentage

**Melee Influence** is a member by its faction ladder and by nothing else, and
that is exactly what makes it worth writing down. It takes an eligible
elemental status the swing applied and lands it again on every body within its
radius of the one that took it, each of them dealt *"damage equal to that
element's damage from the original attack"*.

The wiki settles its depth twice over:

> *"Due to the nature of Faction Damage Bonuses, they are applied **twice** on
> damage done by Melee Influence and **thrice** on damaging status procs caused
> by it."*

and then from the other side, with a worked example: a 100-base melee with
Shocking Touch and +55% faction deals **294** on the hit, its ordinary
Electricity proc ticks **228**, and the SPREAD proc ticks **353**. 353 / 228 is
1.547, which is one more faction multiplier and nothing else. So `DEPTH_PROC`
on the spread instance and `DEPTH_DERIVED_PROC` on the status it leaves, with
nothing invented.

**TWO CLAUSES ARE ITS OWN**, and both come off the same page:

- **Its Condition Overload is the STRUCK body's.** *"Condition Overload's
  damage bonus is based on the number of status effects on the target directly
  struck by the attack … the number of status effects on the enemies receiving
  Melee Influence damage is not part of the calculation."* Every other spread in
  `dummy` divides the aimed body's CO bucket out and multiplies the receiver's
  in; this one must not, which is why it is not a `SpreadBy` arm.
- **Its status burns off the SWING's base, not the spread instance's.** An
  ordinary Extra Hit replaces the base its status reads because it is a
  percentage of something else. This one applies the *same status the swing
  applied*, one derivation further out — so the scale arrives from the hit and
  only the faction rung differs, which is what the 353 / 228 above measures.

## Rules the page states that we do and do not model

- ✅ **"Each hit of Multishot triggers a separate Extra Hit"** — fired per damage
  instance.
- ✅ **"If a hit that would trigger an Extra Hit kills the enemy, the Extra Hit
  will not be triggered"** — `fire_extra_hits` returns early on a kill, and the
  Debilitate split is rolled inside `settle_procs`, which a killing hit does not
  reach.
- ✅ **"independently rolling Status Effects"** — its own roll, at the weapon's
  own chance, off its own one-type vector.
- ✅ **enemies killed by an Extra Hit are credited to the weapon.**
- ⚠️ **Body part twice** and **crit inherited, never rolled** are stated on
  Xata's own card rather than here; see MECHANICS §7 §"Extra Hit".
- ⚠️ **A Blast detonation triggers only Xata's Whisper** — the EN wiki files it
  under Bugs, and it is declared as a `live_bugs:` line on that ability rather
  than as a property of the category (MEASUREMENTS M40).

## Why this is a category and not four special cases

Every member differs in exactly two values. When the next one is transcribed —
Toxic Lash, Resupply, Silken Stride — it is a yaml file, and the faction ladder,
the status base, the multishot rule and the kill rule are already right for it
because they were never written per-member. The one thing that IS per-member is
the trigger: a weapon hit for the abilities, ten stacks of a combined element
for Debilitate. That belongs where it is.

## How the multipliers stack, factor by factor

`engine::dummy::extra_hit` takes the triggering instance's finished
`trigger_raw` and MULTIPLIES rather than rebuilding anything, which is why the
wiki's one-line formula

> Extra Hit Damage = Weapon Hit Damage × Extra Hit Percentage
>                    × (1 + Faction Damage Bonuses)

produces every oddity people report: `Weapon Hit Damage` already contains a
faction layer, a crit multiplier and a body-part multiplier.

- **Faction, again.** One `faction_at_time` on top of however many the trigger
  already carried. A direct hit is at depth 1, so its extra hit is at 2; a Blast
  detonation is at depth 2, so ITS extra hit is at 3 — which is the "triple dip"
  both wikis name, and it is not hardcoded anywhere.
- **The body part, again**, and the CALLER is what knows. A direct headshot
  passes its `part_factor`; a radial, a field tick and a Blast detonation pass
  1.0, since none of them struck a body part. DE's CN card states both halves:
  「同理，弱点倍率也会被计算两次」for a hit, 「弱点倍率只会被计算一次」off a
  Blast detonation.
- **Crit, once and inherited.** The extra hit rolls no crit of its own (the EN
  wiki files "Xata's Whisper's Extra Hits cannot crit" under Bugs), but
  `trigger_raw` critted — so the number behind an orange hit is orange-sized,
  which is what "affected by … critical … damage mods (e.g. Vital Sense)" on the
  ability's own page means.
- **The bracket** is the trigger's elemental correction, 1.0 everywhere except
  where the trigger's own bracket differs from the base attack's. The Blast
  detonation is the loud case: it takes NO elemental bonus, and the extra hit
  off it takes the whole one.

It is a real instance, so it lands through `TargetState::apply` like any other:
Void's ×1.5 against Overguard is the vulnerability column doing its job, not a
rule written in the function.
