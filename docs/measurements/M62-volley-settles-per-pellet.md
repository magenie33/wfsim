# M62 — a volley settles pellet by pellet, and every instance re-reads the target ✅ (owner, 2026-08-27)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A **Laetum** — 100 direct, 300 explosion, the card's own 1:3 — carrying one
Viral damage mod that takes each instance to twice its base, an effect forcing
a Viral proc on every hit, and 110% multishot. Fired at a **body** with no
mitigation at all: no shield, no armour, no vulnerability column, no headshot.
Four numbers popped and the target finished on **four Viral stacks**.

The owner's report, verbatim:

> 还有一个问题 奏凯 100直击 300范围 200%病毒加成，每下强制一下病毒 顺序 200 450 1200 1500 最终4层病毒 你可以推测一下顺序吗 是弹头1和弹头2 你帮我推理一下，是怎么样的顺序

> 就是纯伤害加成200%，你可以假设带了一张200的病毒mod，同时还有个特效是让没一下伤害必定触发病毒，还有概率再出发病毒（因为武器自己有tsatus chance，只是这次没有）。mod只带了110多重，目标完全没减伤，打的是身体。那你推理顺序

```
200   450   1200   1500      (4 Viral stacks at the end)
```

### The order is FORCED by the arithmetic

The four numbers are given SORTED, not in the order they appeared — which is
the whole puzzle. Viral is +100% on the first stack and +25% on every stack
after it, so the only multipliers an instance can read are

```
0 stacks x1.00    1 stack x2.00    2 stacks x2.25    3 stacks x2.50
```

Divide the four numbers by the two instance bases (200 and 600, after the mod)
and exactly one assignment survives:

| # | instance | stacks BEFORE it | Viral | damage |
|---|---|---|---|---|
| 1 | pellet 1 direct | 0 | x1.00 | 200 |
| 2 | pellet 1 explosion | 1 | x2.00 | 1,200 |
| 3 | pellet 2 direct | 2 | x2.25 | 450 |
| 4 | pellet 2 explosion | 3 | x2.50 | 1,500 |

200 cannot be an explosion — that would need x0.667, and a stack count only
climbs. 450 cannot be one either: 450/600 = 0.75, below the x2.00 the second
number has already used. So the two small numbers are the COLLISIONS and the
two large ones the EXPLOSIONS, and from there only one ordering has
multipliers that climb.

The owner's own question was which of the two middle instances comes first —
*"我就在纠结是范围1先还是直击2先"*. It is **explosion 1**: if it were direct 2
the second number would be `200 x 2.00 = 400`, which is not among the four,
while `600 x 2.00 = 1,200` is.

### What it establishes, and it is three separate things

1. **A VOLLEY IS PELLET-MAJOR.** A pellet resolves its own explosion before the
   next pellet's collision — `P1 direct, P1 blast, P2 direct, P2 blast` —
   rather than every collision and then every explosion.
2. **AN INSTANCE DOES NOT AMPLIFY ITSELF.** The first collision reads x1.00:
   its own forced proc lands after it has been settled.
3. **EVERY INSTANCE RE-READS THE TARGET** — not every shot, and not even every
   pellet. Pellet 1's explosion already reads the stack pellet 1's collision
   left one instant earlier.

### It found a real bug, and (3) is the one that was wrong

The engine took its mitigation snapshot **once per pellet**, above the stage
loop, and both halves of a pellet settled against it. Against this fixture it
produced

```
200   600   450   1350        (engine, before the fix)
```

— the same ORDER, and each explosion a step behind, sharing its collision's
stack count. It is a few per cent on any status build, always in the direction
of "this build is good", and invisible in every aggregate this engine reports,
because it is already inside the mean.

`DebuffState::amps` is now read inside the stage loop, once per INSTANCE.
Pruning stays where it was — once per pellet, since the whole volley is at one
instant `t` and pruning again could only be a no-op — which is what keeps the
fix free: measured **-1.5%** on `one_fight` alongside the `Replay.pops`
deletion, every answer unchanged on all four shapes.

The golden test is
`a_volley_settles_pellet_by_pellet_and_each_instance_re_reads_the_target`, and
it pins the four numbers rather than the rule, so any of the three properties
regressing reddens it.

### Why the combat record is what found it

Every other output this engine has would have hidden it. The four numbers are a
mean of 850 either way once they are summed, and 837.5 before the fix — 1.5% on
a Monte Carlo whose own standard error is larger. What made it visible is that
a record ROW states the stacks it read, beside the number they produced, so
"600 at 0 stacks" and "1,200 at 1 stack" are two different sentences instead of
one average.
