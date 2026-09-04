# M54 — a BLAST detonation carries the weak point ×3 to everything its sphere reaches, and a TOXIN DoT carries nothing ✅ (owner, 2026-08-22)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Burston Prime, no Incarnon, a Lavos syndicate mod at +200% of the matching
element. Numbers as typed, `direct — status`:

```
BLAST                       TOXIN (on a Runner)
打身体  115 — 11            打身体  115 — 107
        231 — 21            打头    346 — 159
打头    346 — 32
        1385 — 126
```

> 打身体/头，爆炸会有10条（相当于原本身上的10层都爆了，而不是一个）
> 周围会受到1次伤害
>
> 打身体的时候10层
> 周围1260 （暴击过1次）
> 周围1050 （完全没暴击）
>
> 打头 (暴击情况各异）
> 周围3675
> 3150
> 3380

### What the direct column pins first

`231/115 = 2.01` is the crit multiplier, `346/115 = 3.01` the head multiplier,
and `1385/346 = 4.00` is the HEADCRIT — `1 + (2−1)×3 = 4`, the game's own rule,
arriving unasked. That is what makes the rest of the numbers trustworthy: four
samples reproduce three published multipliers before anything about Blast is
read off them.

### The single-target detonation takes both

`11 → 32` is ×2.9 for the head and `32 → 126` is ×3.94 for the crit, i.e. a
blast stack is stamped with the multipliers of the hit that applied it. The
engine already did this.

### THE AoE TAKES THEM TOO, AT EXACTLY ×3

The clean pair is the two runs with **no crits at all**:

| where the 10 stacks were applied | a neighbour takes |
| --- | --- |
| body | **1050** |
| head | **3150** |

`3150 / 1050 = 3.000`. No remainder. The weak-point multiplier of the hit that
applied the stack reaches every body the 5 m sphere catches.

And `1050 / 10.5 = 100`, i.e. ten stacks at 300% each against the same ten at
30% each — the published 10× between the radial and the single-target halves,
confirmed rather than assumed.

The shape is confirmed too: *"爆炸会有10条…周围会受到1次伤害"* — ten separate
numbers on the host, ONE combined instance on the neighbours, which is the
wiki's *"The radial damage of all procs will be combined into one damage
instance"*.

**WHAT IT SETTLED.** `data/benchmarks/group_clear.yaml` said "Nothing a chain, a
blast or a cloud reaches can be a weak point hit" and the engine had never
implemented that for a blast. The measurement says the ENGINE was right and the
RULE TEXT was wrong, so the text changed. Worth 14.8× on the Larkspur Prime's
board row (914.8 → 62.0 with headshots off), which is why it was worth measuring
rather than reasoning about.

### A TOXIN DoT DOES NOT TAKE THE WEAK POINT

> 我确定毒不吃爆头

This contradicts the wiki's Toxin page, which lists *"Enemy Body Parts
multipliers"* among the additional multipliers on a Toxin tick. A measurement
beats the wiki (docs/DATA_SOURCES.md), so `dot_takes_weakpoint` returns false for
Toxin and true for everything else — the others are UNMEASURED and keep the
wiki's answer rather than inheriting a rule from one case.

The two lines quoted above do not settle it on their own — `159/107 = 1.486` is
neither 1 nor 3, and the samples differ in crit state and carry a faction bonus
that applies twice to a status — which is why this rests on the owner's own
in-game reading and says so.

### Electricity and Gas tick on ONE clock

> 电也是一个大dot的模式，无论多少层，只会跳一下…毒气也是这种，伤害频率和第一次上dot的时候保持一致

Confirmed on the wiki for Electricity with a dated patch note — Update 33.6,
*"multiple procs on an enemy no longer deal their respective damage separately,
like current Slash statuses, but once per second, similar to Heat status.
However, they still maintain each own timer and will not refresh, unlike Heat"*
— and confirmed in game for Gas, which the wiki does not state.

So there are THREE DoT models and the engine had two:

| status | model |
| --- | --- |
| Slash, Toxin | per instance, own clock, own timer |
| **Electricity, Gas** | **one clock per body, own timer, no refresh** |
| Heat | one clock per body, one timer, every proc REFRESHES all of it |

`push_dot_capped` moves a joining instance onto the family's clock. It is
tick-count neutral by arithmetic — an instance with `k` ticks joining a clock
`φ < 1` ahead fires `ceil(k − φ) = k` times — so this is fidelity, not a
rebalance, and every golden value held.
