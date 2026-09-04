# M63 — the Grimoire's orb is six unaimed strikes at ×0.8, and one of them is not a shot ✅ (owner, 2026-08-28)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The report opened as a data question and turned out to be a mechanic. Verbatim,
in the order it arrived — the last three messages CORRECT the first, and the
corrections are kept rather than folded in, because two of them caught a wrong
model that had already been built:

> 我发现一个武器和wiki写的完全不一样，那就是grimorie，次要射击完全就不是面板上
> 写的样子，例如我测试实际上次要射击的点球是每下280而不是350，最好的爆炸也是另
> 外一个伤害，我很有理由怀疑，里面的百分比还是不对的

> 实际会有6下直击加最后一个爆炸 … 数值上就是官方的*0.8，直击的时候会强
> 制触发电，但是爆炸的时候没有这个强制触发

> 然后那个球，完全不吃GunCO，直击部分以及爆炸部分都不吃，因为这个算范围直击
> （也就是变相的范围，类似于field）

> 还有这个球无法multishot，永远只有一个

> 这个球实际上碰到以后马上开始电第一下，这个第一下就是field啊，和后面的5下是
> 一摸一样的，然后结束爆炸（有正常falloff），range_m这个没有错，标识的是自己的攻
> 击范围，飞行6m/s和总共飞6s也都是没错的数据

> 我修正一下，电球实际上是选半径6m随机一个人射一下，只有一条chain，chain默认是2个，
> 后续的multishot加成是1*multishot+2，也就是如果面板的multishot面板是2.6，那么就
> 说明稳定4个，概率5个意思
>
> 后续爆炸才是范围内的全部（因为有falloff），电球的射程和最终爆炸的范围都是
> 6m，受范围增益影响

The wiki page agrees with all of it and adds the two lines nobody had read:
*"Orb will shock 1 enemy within 6 meters of it every 1 second. Each enemy hit
chains to an additional 2 enemies within 6 meters"*, *"Every strike from the
alternate fire has a forced Electricity status effect. The strikes and the
forced Electricity proc can hit weakspots"*, *"Tick rate is not affected by
Fire Rate"*, *"Number of chains is affected by Multishot"*.

### The numbers

`Module:Weapons/data/secondary` gives the active attack one hit of 350
Electricity and one blast of 250. Both are the module's own value ×0.8:

| part | module | measured | ratio |
| --- | --- | --- | --- |
| strike | 350 | 280 | 0.800 |
| blast | 250 | 200 | 0.800 |

TWO RATIOS, ONE MULTIPLIER. That is why this was transcribed as one fact rather
than as two corrected numbers: had the second come back at anything but 0.800,
the two halves would be independent slips and each would need its own evidence.

The measurement does NOT settle whether the ×0.8 lives in the weapon or in the
module's column — `350 × 0.8` and a published 280 are indistinguishable under
every later multiplier — and nothing downstream depends on which.

### The shape

The orb is not a shot with an explosion. It lives 6 s and STRIKES six times —
one random body inside 6 m each second — then detonates. It flies at 6 m/s and
drops to 2 m/s once it touches something, which changes WHERE the later strikes
happen and not how many there are. `range_m: 6.0` is the strike's reach, not a
flight distance.

**All six strikes are the same thing.** The owner said it twice, the second time
to correct a model that had made the first one special: *"碰到以后马上开始电第
一下，这个第一下就是field啊"*. That is the property the engine now has to hold
rather than a description of it.

### Four rules, each a different mechanism

**Every strike forces Electricity; the final explosion does not.** One attack
answering the same question both ways, which is why `forced_procs` is declared
per part — the Astilla splits the same way between its collision and its
radial, the Scourge the other way round.

**Nothing here takes Condition Overload** — not the strikes, not the blast. The
owner's reason is what a strike IS: the orb's rather than the gun's, a ranged
strike that behaves like a field. "An AoE part takes no CO unless its own row
says so" is the standing rule, and the wiki's catalog was re-read on the PAGE
the same day with **no Grimoire row of any kind**.

**Multishot does not add orbs — it adds CHAIN TARGETS.** `multishot + 2` enemies
a strike, so a panel reading ×2.6 is four for certain and a fifth 60% of the
time. The chain is not modelled, so the bucket is pinned at the weapon's default
(`locks: [multishot]`), which is the right answer against one target and an
understatement against a crowd — and the weapon says both halves on its own
page, because a padlock with no explanation reads as "worthless everywhere".

**The strikes can find a weak point, and so can the Electricity they force.**
How often is **assumed at a flat 10% each** — the owner's number, and an
assumption rather than a measurement, on the page as well as in the yaml.

### How it is modelled: an ENTITY, not a field

The first two attempts filed the orb as a lingering FIELD, and the owner
rejected the type rather than the numbers:

> 我觉得这个不能算是field，因为field是殴打范围内全部的，有falloff的。这个应该是
> 其他类型，是一个实体有范围的，打击范围内一个目标的，前6下伤害都是一样。以及严
> 谨我们发射的时候，发射点是圆心你应该搞一个更准确的

He is right, and the distinction is not cosmetic. A `lingering:` field is an
AREA: it sits where it landed and burns everyone standing in it, each at their
own falloff distance. An orb has a PLACE OF ITS OWN, it moves, and every strike
reaches exactly one body — so who is in reach is a question about where the orb
is, and a field cannot ask it.

`weapons_data::OrbSpec` is that type. It carries geometry and a clock and
nothing about damage: a fuse, a strike interval, a reach, the two speeds, and
the chain. What a strike DEALS is the attack's own `damage:`, and what the fuse
ends in is the attack's own `radial:` — the same division `beam:` already makes.
An attack with an `orb:` settles no collision and no explosion when it is fired;
it deploys, and everything it deals is delivered later and elsewhere.

THE ORB LEAVES THE MUZZLE, which is the accuracy the owner asked for. It starts
at `space::muzzle(player, aim)` — a point on the shooter's own circumference,
the same place every other shot in this arena leaves from — travels along the
aim ray at 6 m/s, and drops to 2 m/s at the first body it touches without
turning. Its reach is measured from ITSELF, so "within 6 metres" is finally a
statement about a real position rather than about the target.

THE STRIKE COUNT IS NO LONGER WRITTEN DOWN. Six ticks over a six second fuse,
and a tick with nobody inside the reach strikes nobody and is spent. `ceil(6 -
flight)` — the owner's rule — falls out of that: a throw that connects in under
a second loses none, one that takes 2.5 s lands four, and one thrown at nothing
lands none. `a_strike_with_nobody_in_reach_is_spent` asserts both ends and the
ladder between them.

The strike itself is settled by the same function a cloud's tick is, because the
arithmetic of a damage instance on a clock of its own does not depend on what
produced it — and sharing it is what stops the two drifting apart. What is NOT
shared is who it lands on. The record tells them apart: `Origin::Orb`.

### The chain count and the headshot rate, measured off the Invocation mods

The four Invocation mods gain a stack per HIT, which makes a strike's body count
readable off a buff instead of inferred from a damage total. Against twenty
enemies:

| multishot | 1.0 | 1.6 | 2.1 | 2.7 | 3.6 | 3.9 |
| --- | --- | --- | --- | --- | --- | --- |
| bodies a strike reaches | 3 | 4 | 6 | 8 | 10 | 11 |
| `floor(3 × multishot)` | 3 | 4 | 6 | 8 | 10 | 11 |

**`floor(3 × multishot)`**, the struck body included, and a hard floor rather
than a rolled remainder — x2.1 hits six every time, not six-or-seven.

THE ENTRY HAD `multishot + 2` UNTIL THIS. The wiki's two sentences support it
just as well — *"chains to an additional 2 enemies"* and *"Number of chains is
affected by Multishot"* — and the two readings agree at x1.0 and part company
immediately after: at x2.1 the sum says 5 and the product says 6. Both were
consistent with everything known; only the measurement separates them, and it is
worth 47% more bodies at x3.9.

`the_orbs_chain_reaches_three_bodies_per_point_of_multishot` pins the whole
table for exactly that reason — a test at the unmodded count alone passes on the
reading it replaced. Verified to bite: restoring the sum reddens it at x1.6.

**And the headshot rate is measured too, at about 10% per body hit** — five weak
points in 48 hits, counted as hits/heads over six strikes of eight bodies:

```
8/3   8/0   8/0   8/0   8/0   8/2
```

`unaimed_headshot_chance` is declared on the ATTACK rather than on any of its
parts, because the orb picks its own body and the scenario's `headshot_pct` — a
statement about the player's aim — is the wrong number for every strike. Each
body a strike reaches rolls its own, chained ones included, which the sample
shows directly: three of the eight in one strike, two in another.

It is a small sample, so the weapon says "about 10%" rather than 10.4%, and it
is an AVERAGE over a crowd rather than geometry — a fight where the enemies line
up beats it and one against a single tall target may not reach it. The owner's
own framing: *"因为视觉上chain很少，但是实际上应该是这么计算的"* — the chain looks
rare and is not.

Both the collision path and the orb path draw their body part through one helper
(`unaimed_part`), so the six strikes cannot answer differently. A head strike
takes the critical-location fold-in like any other hit on an eligible weak point
— no exception was invented for it, because inventing one would be a claim with
no measurement behind it.

### What the position model found: six is not reachable

**Four strikes land on a lone standing enemy, and no throw distance buys six.**
It is arithmetic rather than a simulation result. A stationary body is inside
the reach for a bounded window:

```
  approach   (reach + body radius) / launch speed   6.25 / 6 = 1.04 s
  departure  (reach + body radius) / slowed speed   6.25 / 2 = 3.13 s

  at contact                          (no approach)            3.13 s  ->  4
  thrown from beyond the reach                                 4.17 s  ->  5
```

Six strikes a second apart need the body in reach for more than five seconds,
and 4.17 s is the most these numbers can buy. The owner proposed that a
mid-range throw would fix it — *"如果是有一定距离，例如10m，那么飞行4m以后，就会
开始第一下（因为半径是6m），那应该就可以完整打完"* — and the model says it does
not: the approach is worth at most another second, so the count goes 4, 4, 5, 4,
5, 4… across the whole range and never six. Measured every metre from contact to
30 m.

WHAT WOULD BUY SIX, measured rather than derived, so the bound above is a
statement about these two numbers and not about the model:

| post-contact speed | reach | strikes at contact |
| --- | --- | --- |
| 2.0 m/s | 6.0 m | 4 |
| 1.5 m/s | 6.0 m | 5 |
| **1.2 m/s** | 6.0 m | **6** |
| 2.0 m/s | 7.5 m | 5 |
| 2.0 m/s | 8.0 m | 5 |
| **0.0 m/s** (it stops) | 6.0 m | **6** |

So the measured six needs the post-contact speed at **1.25 m/s or below**, or a
reach of **9.75 m or more**, or an orb that stays near what it touched.

### …and it is the third one, for a reason the arena cannot have

> 我确定这是对的，之前可以打6个是因为有墙，碰见墙就反弹

**The orb bounces off walls.** Every number in the entry is right and the model
is right; what produced six in game is a room. An orb thrown at a body at
contact meets a wall or the floor within a metre or two, comes back, and spends
its whole fuse near what it was thrown at — so it strikes six times and
detonates on the target. In an open field it drifts twelve metres and does
neither.

THIS ARENA HAS NO WALLS, which is a standing limitation rather than anything new
(the same sentence `ricochet_terrain` has carried since the Latron Incarnons
landed). What is new is a weapon where it is worth a great deal: measured on the
same fight, an orb held near its target against one that drifts away is
**12,611.6 DPS against 8,105.7 — +55.6%**, and the difference is four strikes
becoming six plus a detonation that lands at all.

So the single-target number this app reports for the Grimoire's alt fire is a
FLOOR, and an unusually loose one. It is on the weapon's own page in both
languages rather than left in a yaml, because a player comparing this weapon
against another needs to know that one of them is being measured in a field and
played in a corridor.

The three-row table above stays because it is what MADE the answer findable: it
turned "your six and my four disagree" into three numbers, each checkable in
game, and the owner recognised the mechanism from the third row. That is the
useful thing a position model produces and no aggregate could.

`an_orb_that_drifts_leaves_a_lone_target_behind` pins the whole finding: four at
contact, five as the ceiling over thirty metres, and six at 1.2 m/s and at a
standstill. Whichever of the three turns out to be right, that test says what
the old answer was worth.

### The chain, settled

*"不受增益"* meant the RANGE bucket, not the damage one:

> 这里的增益，是指范围增益，就是跳的距离永远是6m，那些其他的什么暴击等等的都是
> 正常加成的 … 你就认为是chain起来没有衰减的beam chain那种方式就可以，并且存在
> multishot增加跳数的机制

So a hop deals the strike in full — a beam chain with no falloff — and takes
crit, status and every damage mod normally. What it does NOT take is a range
mod: **the jump is always six metres**, while the orb's reach and its detonation
radius both grow with Fulmination. Three distances on one attack, all six metres
unmodded, and only two of them move.

`a_range_mod_widens_an_orbs_reach_and_not_its_chain_hop` asserts the asymmetry
rather than the three numbers, because a test that only read them apart would
pass on an engine that scaled none of them — the mod has to be seen to bite on
two before "and not the third" says anything. Verified to bite: scaling the hop
again reddens it at `7.44 against 6`.

### A bug the question found

Asking it was what exposed the chain's share riding the wrong bracket. It was on
`damage_multiplier`, which is Plentiful Mayhem's and is documented as leaving
the status payload OUT — so a hop at 0.31 of the strike still seeded a full-size
Electricity DoT, and the two readings of the ambiguity came out 4.6% apart when
one of them should have been half the other.

`chain::Instance::share` says which bracket a chain belongs in, and it is
explicit: *"a beam with a smaller base damage, so it scales the hit AND the
status base that hit computes its DoTs from"*. Scaling the part's own
`modified_base` is that, and it put the two readings 1.92x apart (146,590 DPS
against 76,306, three bodies, Hornet Strike) — which is what made the question
worth asking out loud rather than guessing at.

The answer is the first reading, so nothing in the entry changed. The bug did,
and it would have been silently wrong on every chaining orb in a crowd.

### Still open

**`AttackSpec::locks` came and went.** It was added on *"这个球无法multishot，永
远只有一个"* and removed on the correction two messages later: multishot does not
add orbs, it adds CHAIN TARGETS (`multishot + 2` bodies a strike). Pinning the
bucket would have been the right answer to the wrong question, and would have
told a reader the mod is worthless where it is in fact most of what a crowd
build buys. The orb path gives one orb by construction — a deploying shot fires
no pellets — so nothing was needed in its place, and the count goes where the
game puts it.
