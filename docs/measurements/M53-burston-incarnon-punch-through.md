# M53 — the Burston Incarnon PUNCHES THROUGH, and its blast lands behind you ✅ (owner, 2026-08-20)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Verbatim, both messages, in the order they arrived:

> 我刚刚测试了一下burston是可以punch through的，子弹会传过去，然后爆炸（也就是会在别人的身后爆炸，就是穿过后飞行距离达到极限），也就是说纯单体伤害可能还会降低，你顺便也实现了

> 我们给burston这种的面板带上aoe，但是实际上可以穿透的得定义一种类型（他的aoe算法还不吃多重，我认为是一种假aoe，你应该可以归类一下，这样有些计算更好处理，名字你来定）

### What it overturns

The night before, I told the owner that punch-through mods pay nothing on a
Burston Prime Incarnon and that this was CORRECT — the form carries a `radial:`,
and the punch-through page's class rule says *"weapon projectiles with an area
of effect (AoE) component will not Punch Through enemies or level geometry at
all"*. The card showed `+0m` and I called it honest.

It is not. He went and shot one. The round passes through the enemy and
detonates BEHIND it, at the point where the flight ends.

**AN INFERENCE FROM A CLASS RULE LOST TO A MEASUREMENT**, which is the whole
reason this file exists. The page's own sentence begins *"With a very few
exceptions"* and never says which — so a weapon being in the exception set is
exactly the thing the rule cannot tell you.

Two pieces of published evidence agree with him, and both were reachable the
night before:

- The `Incarnon` page's changelog carries evolution perks that fire **on punch
  through hit** — *"Paris Incarnon's Ardent Trigger Evolution (on punch through
  hit: + 40% Fire Rate for 6s)"*, and Braton Incarnon's Evolution III is *"On
  Punch Through Hit: 20% chance for 10% Ammo restored"*. DE does not build
  perks around a thing the weapon cannot do.
- The **Tenet Ferrox** states the whole mechanic in words, on its own page:
  *"Shots explode in a 4 meter radius after reaching maximum punch through
  distance."* It went into the roster the same night, with that sentence
  transcribed into a comment, and I did not connect the two.

### The classification he asked for

He named the smell before the mechanic: the blast *"不吃多重"* — takes no
multishot — so it is a **假 AoE**, a fake one, and it should be a TYPE rather
than a pile of per-weapon exceptions.

`weapons_data::BlastKind`, two values:

| kind | detonates | punch-through mods | example |
| --- | --- | --- | --- |
| `contact` (default) | on the first thing it touches | **refused** — a true AoE | Tenet Envoy, Kuva Ogris, every grenade |
| `terminal` | where the FLIGHT ends, after the punch-through budget is spent | **allowed** | Burston (Prime) Incarnon, Tenet Ferrox |

### What it does to the number, and why it can go DOWN

**THE BUDGET IS SPENT ON MATERIAL** (owner, 2026-08-20), which is the mechanic's
own definition — *"the total distance of material (object or enemy) that a
weapon's projectile, bullet or beam can pass through before dissipating"*. Air
costs nothing. `space::dissipation_point` therefore crosses `BODY_MATERIAL_M`
per body and detonates in whichever one the round cannot get out of, which is
the same accounting `struck_along` does for the direct hits, read one step
further.

When it clears every body on the line the arena has to answer a question the
game answers with a WALL. There is no geometry here, so nothing would ever stop
it; the leftover budget is spent as flight instead. That is the one place the
model is a stand-in rather than the mechanic, and it is bounded by the weapon's
own punch through rather than by a number invented for it.

Measured on the wire, level 100, 100–200 runs, with the server's own standard
error. **One standing enemy** — the blast moves back and the damage drops:

    Burston Prime Incarnon   Serration      16584.5 ± 38.4
                             + Metal Auger  16357.6 ± 38.2     -1.4%, about 4σ
    Tenet Ferrox             Serration       2659.9 ± 17.8
                             + Metal Auger   2651.6 ± 17.8     no measurable change

**A line of seven, 1.5 m apart** — which is where the accounting actually shows,
and where a distance-based reading gets it wrong:

    Burston Prime Incarnon   Serration      16565.7 ± 50.7    1 body
                             + Metal Auger  53618.9 ± 141.8   5 bodies
                             + Primed Shred 66125.5 ± 264.2   5 bodies

Five is `1 + floor(2.1 / 0.5)` exactly, and the detonation lands on the FIFTH
body rather than 2.1 m past the first.

**TWO READINGS WERE TRIED AND BOTH ARE WORTH RECORDING.** The first sent a round
that crossed every body off the field entirely — which fits the Burston and
*killed the Tenet Ferrox's radial* against a lone target with no mod equipped at
all (2674 DPS to 2416), because its 1.5 m of INNATE punch through cleared the
only body there was. The second read the budget as a flight DISTANCE, which
saved the Ferrox and got the CROWD wrong: it put the blast a fixed 2.1 m past
the first body instead of on the fifth. Only the material accounting fits the
measurement, the Ferrox and a line of enemies at once.

**THE DIRECTION IS MEASURED; THE MAGNITUDE IS NOT.** The owner reported that
single-target damage *"可能还会降低"* — may even decrease — and gave no number, so
what is pinned here is that it drops and that the weapon's own blast radius
decides by how much. A figure from in game would tighten it and nothing in the
model would have to move to accept one.

### The four Braton Incarnons — DEFAULTED to this, not measured

The same evidence points at exactly four more entries and no others. A sweep for
a form carrying BOTH a `radial:` and an evolution whose own text reads *"On
Punch Through Hit"*:

| entry | evolution |
| --- | --- |
| `braton_incarnon` | Gunsmoke Pick Up — *"On Punch Through Hit: 20% chance for 10% Ammo restored"* |
| `braton_prime_incarnon` | the same |
| `braton_vandal_incarnon` | the same |
| `mk1_braton_incarnon` | the same |

Every other weapon with such a perk — the Paris family, the Ballisticas, the
Felarx, the Onos — carries no radial at all, so it already takes punch-through
mods normally and there was nothing to decide.

They were left `contact` for one night as a known-wrong state, on the grounds
that guessing between `terminal` and `contact` plus `punch_through_mods: true`
is the CO rule's own mistake in another mechanic. **The owner then chose the
default (2026-08-20): the Burston's answer.** A form whose own evolution rewards
punch-through hits cannot be a weapon that refuses punch through, and of the two
fixes the Burston's is the one with a measurement behind it.

It is a DEFAULT and the files say so. One shot settles it: fire a Braton
Incarnon with a punch-through mod at a single enemy and watch whether the
explosion lands on it or behind it. Measured at 200 runs the choice currently
costs that weapon nothing detectable on a lone target (9629.3 ± 74.5 against
9602.7 ± 74.5), so what it really buys is the crowd — which is the case worth
checking in game.
