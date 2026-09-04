# M33 — what base a Primary Debilitate split burns off (2026-08-08; base DECIDED, exponent OPEN)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The owner brought a community formula for a Primary Debilitate build, with an
in-game number beside it, and asked whether the case it describes generalises
(2026-08-08: "我们是否可以反推到一般情况呢").

```
Damage x Cyte% x Dot% x (1+Elemental) x (1+DotElemental) x Bane^4 x Elementalist

350 * 0.5 * 0.5 * (1+6+0.6+0.6) * (1+6+0.6) * (1+0.3)^4 * (1+0.9)
= 29591.20        in game: 29551   (-0.14%)
```

### It decodes exactly, which is why it is worth taking seriously

Every term is identifiable, and the one that pins it is the shard bracket. The
Violet Archon Shard reads "+30% (+45%) Primary Electricity Damage. Gain an
additional +10% (+15%) per Crimson, Azure, or Violet Archon Shard equipped."
Five Tauforged Violet: `5 x (45% + 15% x 5)` = **600%** — the bare `6` in both
brackets, and the count includes the shard itself (owner, 2026-08-08).

| term | what it is |
|---|---|
| `350` | Vectis Prime base |
| `Cyte%` `0.5` | Cyte-09's **Resupply** — "triggering an Extra Hit of … 50% for Sniper Rifles … that procs a guaranteed status effect from the selected element", at 100% Strength, on a sniper, with **Corrosive** selected |
| `Dot%` `0.5` | the elemental DoT coefficient |
| `(1+6+0.6+0.6)` = 8.2 | the **CORROSIVE** bracket — shards + the Toxin 60/60 + the Electricity 60/60, i.e. both components |
| `(1+6+0.6)` = 7.6 | the **ELECTRICITY** bracket — the component the split landed on |
| `(1+0.3)^4` | a normal faction Bane, four layers |
| `(1+0.9)` | Rifle Elementalist |

Read as a chain it is not two rules but one, applied at each link — the Extra
Hit is a damage instance that guarantees a Corrosive status, the target is
saturated so Debilitate splits that status into Electricity, and the split's DoT
is what the 29551 is:

```
Extra Hit      = 350 x 0.5 x 8.2          <- Resupply, sniper, Corrosive
Debilitate split instance                  <- guaranteed status, saturated -> Electricity
split DoT      = (that) x 0.5 x 7.6        <- the number on screen
                                     x Bane^4 x Elementalist
```

**THE FOURTH BANE LAYER IS RESUPPLY'S OWN HIT**, and the source says so
outright (owner, 2026-08-08, relaying it):

> WeaponInitialHit -> ResupplyInitialHit -> DeliberateInitialHit -> DeliberateDoT
> (Bane/Roar reapplies itself every other layer of damage)

Which reconciles with the wiki's `f^3` exactly: drop Resupply and the chain is
WeaponInitialHit -> split instance -> split DoT, three links. The `^4` is the
`^3` with one more producer in it, which is the answer to the question as
asked — the exponent is a COUNT, and `faction_at(f, depth)` already is that
count.

### Two laws, and only one of them generalises

**FACTION IS ALREADY GENERAL HERE.** `faction_at(f, depth)` is `f^depth` and the
depth composes by recursion, so nothing is hardcoded: a hit is 1, a status is
its parent + 1, and Debilitate's split reaches 3 because it goes through an
extra instance. The video's **4** is not a different rule — it is this rule with
one more producer in the chain. So the answer to "can we generalise from the 4x
case" is that the 4x case IS the general case; the wiki's 3 and this 4 are the
same law counted over different chains.

**THE BASE IS NOT.** The formula carries BOTH brackets — the parent's 8.2 and
the child's 7.6 — and the engine carries only the child's:

```
ours:  0.5 x ModifiedBase       x (1 + child bracket) x f^3
video: 0.5 x [the parent's hit] x (1 + child bracket) x f^4
       where the parent's hit already includes its own 8.2
```

The gap is a whole elemental bracket: **x8.2 on that build**, ~x2.8 on an
ordinary two-mod Corrosive one. It is not a correction, it is a different
weapon — which is exactly why the next section is careful about what the
formula does and does not demonstrate.

### THE CRUX — and it is narrower than it looked (2026-08-08)

Shipped for one commit, then reverted, because the owner put his finger on what
the evidence actually covers: **"那个resupply的例子就是说明，类似toxic lash的例子
啊，不是常规武器的"**.

The source's own analogy is the reason. Toxic Lash's page carries the worked
example:

> "with an unmodded weapon whose damage sheet says it hits for 200 damage, a
> Rank 3 Toxic Lash, and a Rank 5 Intensify, Toxic Lash will deal:
> 200 x 0.3 x 1.3 = **78** direct Toxin damage, and always trigger a Toxin proc
> that ticks for **78 x 0.5 = 39** Toxin damage per second"

39 is half of **78** — the ABILITY's own damage — not half of the weapon's 200.
So "base damage" is not a property of the weapon. It is whatever applied the
status, and DE has two rules for it:

| who applied the status | its base |
|---|---|
| the WEAPON's own hit | `ModifiedBase` = unmodded x (1 + BaseDamageBonuses) — **elements excluded** |
| an ABILITY / an instance | that thing's own damage number — **elements included** |

**The formula that decodes the 29551 is entirely in the second row.** Its chain
is `WeaponInitialHit -> ResupplyInitialHit -> DeliberateInitialHit ->
DeliberateDoT`, and the number the DoT burns off is Resupply's Extra Hit —
`350 x 0.5 x 8.2`, an ABILITY's damage. That is Toxic Lash's rule, demonstrated
on Toxic Lash's case. It says nothing about a plain weapon shot.

So the open question is exactly one thing, and only one:

> With no ability in the chain, does Debilitate's split DoT read the weapon's
> `ModifiedBase` (elements excluded, DE's weapon-status rule), or the weapon's
> whole modded hit (elements included, the instance rule)?

Both readings survive everything known. For `ModifiedBase`: the parent is still
a weapon shot, and DE's weapon-status rule is exactly the special case that
exists to keep a DoT scaling with its OWN element rather than with all of them.
For the hit: the arcane's intermediate instance "has no damage", so the DoT
reads THROUGH it to whatever is above — which in the video is an ability hit
including elements, and in the plain case would be a weapon hit including
elements. The `x f^3` proves the intermediate link exists either way.

**The engine keeps the `ModifiedBase` reading** — the one it has always had —
and `a_debilitate_split_burns_off_modified_base_not_the_hit` pins it so the
question cannot be settled by accident. Flipping that assertion from 1.0 to 2.0
and passing the hit's damage into the recursion is the whole of the change.

What the reverted commit cost is the argument for not shipping it: it moved
published board rows by up to **+112%** (Torid, no-aim) on an inference.

### More material, and what it changed (2026-08-08)

Asked to collect more (owner: "你再多搜集点资料"), and the sources changed the
shape of the question rather than answering it.

**The weapon-status rule is documented to the digit, and we match it.** The
Toxin page: `Toxin Proc Damage per Tick = 0.5 x Modified Base Damage x (1 +
Toxin Damage Bonuses) x (1 + Status Damage Bonuses) x (1 + Faction Damage
Bonuses)`, with `Modified Base Damage = Un-modded Weapon Damage x (1 + Base
Damage Bonuses) x (1 + Faction Damage Bonuses) x Additional Multipliers` and
the note "**Note the lack of elemental bonuses in the Modified Base Damage
formula**". The Electricity page says it twice as plainly: "Modded Base Damage
is not the same as normal damage calculations, **ignoring physical and elemental
damage bonuses**". Its worked example — 100 Puncture, Serration, Infected Clip,
Rifle Elementalist, Bane of Infested — comes out at `0.5 x 344.5 x 4.693 =
808.37`, which is this engine's arithmetic exactly.

**MELEE INFLUENCE SAYS A THIRD THING — AND IT IS FILED SEPARATELY** (owner,
2026-08-08: "melee influence是传染比较特别，你单记"). It is a SPREAD: the damage
it names is dealt to OTHER enemies, as the price of contagion, and a mechanic
that moves an effect sideways is not the same kind of thing as one that splits a
status on the target in front of you. Recorded here as a data point about how DE
scales derived elemental damage, NOT as the precedent for Debilitate. Its page:

> "When an elemental Status Effect is spread by Melee Influence, affected
> enemies are also dealt damage equal to **that element's damage from the
> original attack**" … "based on the amount of matching elemental damage after
> quantization, including effects such as Condition Overload and critical
> multiplier" … "Faction Damage Bonuses … are applied **twice** on damage done
> by Melee Influence"

Not `ModifiedBase`, and not the whole hit either — **that element's own damage
on that hit**. A player thread on Debilitate says the same thing from the other
end ("It scales based on the damage value of the element not the mods. So if you
have 100 gas damage the heat and toxin procs would be calculated against that"),
though nobody there measured anything — and a thread is not a page.

### DECIDED: (a), the weapon's own algorithm (owner, 2026-08-08)

"a版本吧，我觉得是对的，先按照a来设计". The weapon is the SOURCE, so the base is
computed the weapon's way — which is also the only one of the three that is
documented for a weapon-applied status, matched to the digit on the Toxin page's
own worked example, and already what ships. Nothing changes; what changes is
that the question is now closed by decision rather than left open, and the two
rivals below are what a measurement would have to overturn it with.

The three candidates for the plain weapon case were:

| | the split's base | on the M33 build |
|---|---|---|
| **(a)** what ships | `ModifiedBase`, elements excluded | 350 |
| **(b)** the reverted attempt | the whole modded hit | 350 x 8.2 = 2870 |
| **(c)** the Melee Influence rule | the COMBINED element's damage on the hit | 350 x 7.2 = 2520 |

**(b) and (c) are indistinguishable in the video's chain**, which is why it
decodes under both: Resupply's Extra Hit is entirely of the selected element, so
"the whole hit" and "that element's damage" are the same number there. The
video therefore rules out (a) for the ABILITY case and separates nothing else.

### THE EXPONENT IS NOW THE OPEN ONE — 3 or 2 (2026-08-08)

Choosing (a) puts a second question in relief, and the owner raised it in the
same breath: "我们已经多吃一次bane加成了，理论应该是只有2的，而不是3". If the
split's base is the WEAPON's `ModifiedBase` — the same base an ordinary weapon
status uses — then the arcane's instance is not acting as a damage layer, and
an ordinary weapon status double-dips faction, `f^2`. Charging `f^3` while
reading the weapon's base looks like having it both ways.

**The counter-argument, and it is the sources', not mine.** The wiki states the
three outright: "applied as a separate damage instance, causing Faction Damage
Bonuses to multiply the Damage over Time effect of Heat, Electricity, and Toxin
status **three separate times**". And the video's own description says the
instance "**has no damage**". Those two together are consistent in exactly one
way: the instance is real enough to add a faction layer and carries no damage of
its own, so the DoT's MAGNITUDE has to come from somewhere else — the weapon —
while the extra layer is the only trace the instance leaves. Which is also the
only thing it predicts that anyone can see, and is what this file has said since
M-notes were first written for this arcane.

**We are not double-counting it.** `ModifiedBase` here carries no faction at all
(`base_vector.total() x (1 + bd)`), and `fm2 = faction_at(f, depth)` supplies
every layer: `f^2` for an ordinary weapon DoT — which is the wiki's double dip,
exactly two — and `f^3` for the split. Three is a deliberate one-more, not a
stray multiply.

It is also the cheapest thing on this page to measure: the exponent is a RATIO,
so it needs no absolute numbers and no theory about the base at all.

### What decides it — three tests, in this order

On any weapon, in the Simulacrum, with a Corrosive build saturated to 10 stacks.
It needs no frame, no shard and no exalted weapon, and it reads as a RATIO, so
every mitigation, faction column, crit and body-part factor cancels out of it.

A pure Corrosive build cannot proc plain Toxin or Electricity at all — both are
combined into Corrosive — so **any Toxin or Electricity DoT on screen is the
split**. That is a clean signal, and it is what makes this cheap.

**TEST 0 — the exponent, and it settles `f^3` vs `f^2` on its own.** Take one
build, saturate Corrosive, read the split's tick with a Bane mod OFF, then with
it ON. Nothing else changes, so the ratio IS the exponent:

| reading | tick with Bane / tick without, for a +30% Bane |
|---|---|
| `f^2` (an ordinary weapon status) | 1.3^2 = **1.69** |
| **`f^3`** (what ships, and the wiki's number) | 1.3^3 = **2.197** |

30% apart, and it needs no absolute number, no unarmoured target and no view on
the base question. Do this one first.

**TEST 1 — does the base include the element at all?** Watch the **Electricity**
split's tick while adding a **Toxin** 60/60. Toxin is not in the Electricity
bracket, so under (a) nothing can move; under (b) and (c) the Corrosive the hit
carries grew, and the split grew with it.

| reading | with the Toxin mod added |
|---|---|
| **(a)** what ships | **no change** |
| (b) / (c) | up, by the Toxin mod's share of the Corrosive |

**TEST 2 — the whole hit, or just the element?** Only if test 1 moved. Add ONE
**Heat** mod in the LAST slot. Heat is the odd element out, so Corrosive still
forms and its value does not change — only the hit's total does.

| reading | with the Heat mod added |
|---|---|
| (a) / **(c)** | **no change** |
| (b) | (1+0.6+0.6+0.9)/(1+0.6+0.6) = **+41%** |

An IPS mod does test 2's job with a smaller swing and no second DoT colour on
screen, which may read more cleanly.

A single absolute reading works too, for a base-100 weapon with Serration:
`ModifiedBase` = 265, the Corrosive on the hit = 265 x 1.2 = 318, the whole hit
= 583, a Toxin split's bracket 1.6 — so **212** (a) against **254** (c) against
**466** (b). Against an unarmoured target, with no Bane, no crit and no
status-damage mods, those are far enough apart to tell by eye.

### Also settled by this: the split deals no damage of its own

The owner's other half — "殴打的那一下，是没有伤害的…就是直接上dot（电还是会立刻
电一下）" — is what the engine already does, and only the DATA said otherwise.
`settle_procs` applies the split as a status and never calls `target.apply`; the
Electricity tick that lands immediately is the DoT's own first tick (delay-0),
not a hit. `primary_debilitate.yaml` opened with "IT DEALS AN INSTANCE", which
reads as a damage number even though the paragraph below it says the opposite;
that is now stated once, in the direction the code goes.

---

### CLOSED 2026-08-09 — it was an EXTRA HIT all along

The wiki's `Extra_Hit` page names this arcane outright — *"a 0-damage Extra Hit
that applies a guaranteed status effect"* — and states the general rule its
status follows: *"Damage over Time status effects created by an Extra Hit will
use the Extra Hit Damage as Modded Base Damage"*, which is also why such a
status takes the ELEMENTAL bonuses an ordinary one is denied.

Read literally that gives ZERO here. The rule that covers both members (owner,
2026-08-09: "如果为0，那么就找上一级去找base") is that an Extra Hit **replaces**
the base its status would have used — so a 0% one replaces nothing and the level
above stands, which is the `ModifiedBase` this engine already used.

It also explains why the third reading — the full modded hit — decoded the 29551
above and still moved published board rows by +112% when shipped: that reading
is CORRECT, for an Extra Hit with damage. The Cyte-09 chain it came from is a
10–25% Extra Hit, and *上一级被 resupply 替换了*. Debilitate is the one member
with nothing to replace. See docs/EXTRA_HIT.md.

The exponent is closed with it: the same page derives `f³` rather than asserting
it, so the "理论应该是只有2的" doubt is answered — the missing rung is the Extra
Hit itself, which carries the bonus twice before its status carries it again.
