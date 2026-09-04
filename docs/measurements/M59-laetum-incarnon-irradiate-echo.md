# M59 — the Laetum's Incarnon form doubles Secondary Irradiate's echo ✅ (owner, 2026-08-24)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A **Laetum**, base damage 220, read across both forms. Most of what was
measured confirms what the engine already computed; one number does not, and it
is the reason this entry exists.

### The reports, verbatim

```
220基伤
512/10752

2/2 768 160*（1+2.2+0.4*2*2）
2/1 640 160*（1+2.2+0.4*1*2）

灵化后
0基伤   100/300
220基伤 320/960
3/1-440/960
3/2-560/960
100*(1+2.2+1*3*0.4)=440
100*(1+2.2+2*3*0.4)=560
300*(1+2.2)=960

照射测试
灵化前 1536 / 隔壁 2764.8
       (可以受到这个伤害，并且也可以 x21，互相独立不影响，
        相当于旁边单独一次 1.8x)
灵化下 320/960   隔壁 1152/24192
       960/2880  隔壁 3456/24192
```

### What confirms the model

**Devouring Attrition is ×21 on the instance**, and it is INDEPENDENT of the
echo. `10752 / 512 = 21.000`, and the owner's note beside the echo says the
same from the other side: the echo "can also take the ×21, independently and
without affecting each other". That is what `noncrit_mult` already does — a
roll per non-critical damage instance, in its own multiplicative bracket.

**The Incarnon form's radial is 3× its direct**, and the stack bonus reaches
the direct hit only: `320/960` and `960/2880` at 220% base, with the AoE
sitting at 960 across `3/1-440/960` and `3/2-560/960` while the direct climbs.
`300 × (1 + 2.2) = 960` is the whole of the AoE's arithmetic.

**Only a DIRECT hit triggers the echo; an AoE hit never does** — stated
outright by the owner. The engine had this right and could not easily have had
it wrong: `spread_from_echo` is called from the direct path alone.

### What does not: the echo is 3.6× on one form and 1.8× on the other

Secondary Irradiate deals `1.8 × the hit` at max rank, and the owner measured
several pure single-target weapons at exactly that. Not here:

| form | direct | echo | ratio |
| --- | --- | --- | --- |
| base | 1536 | 2764.8 | **1.80** |
| Incarnon | 320 | 1152 | **3.60** |
| Incarnon | 960 | 3456 | **3.60** |

Twice, on two different direct-hit sizes, and the base form of the same weapon
is ordinary — so it is the FORM and not the gun.

**THE OWNER'S READING**, offered as a hypothesis: the game sees TWO damage
components on this attack — a direct hit and a radial — so it computes an echo
for each, `1.8 + 1.8`, while only the direct one actually fires. That sits
exactly on top of the other measurement in the same session (an AoE hit never
triggers the echo), and it gives the number a meaning: **one per damage
component**, rather than a magic 2.

### What is implemented, and what is deliberately not

`WeaponSpec::echo_multiplier` is that coefficient, `2.0` on
`laetum_incarnon` and 1.0 everywhere else — a per-ENTRY figure, not a rule
about AoE weapons. Every direct+radial weapon in the roster is a candidate for
the same doubling and **none of the others has been read**; generalising one
measurement to a class is what `docs/CATALOGS.md` forbids, and the owner's own
framing was "other weapons with an AoE seem to have a bit of this problem",
which is a lead rather than a finding.

`only_the_measured_entry_carries_an_echo_coefficient` is the note to come back
to: it asserts the roster holds exactly one, so the day a second weapon is
measured the test fails, names both, and forces the decision to be made on
purpose instead of by a default.

### …and an EXTRA HIT does not roll the ×21 again

Same session, and it closes a question this file had left open. Xata's Whisper
fires a second damage instance worth a percentage of the hit that triggered it.
Devouring Attrition's own rule is "per damage instance that did not crit", and
an extra hit IS a second instance — so a second roll was the reading a careful
person would have argued for, and it would reach **×441**.

It does not. **"真理密语不能再继续触发那个 x21，从而达成 x441。只能简单的 x21，
就是原本的实现"** (owner, 2026-08-24). The extra hit inherits the ×21 the
trigger already took, through the `raw` it is a percentage of, and stops there
— which is what this engine already did, on the strength of a comment about
crit and the body part rather than about this perk. Now measured rather than
inherited.

**The one thing that DOES reach ×441 is Primary Debilitate's DoT**, and for an
unrelated reason: its zero-damage instance leaks its multipliers into the burn
it leaves. That is a LIVE BUG in the game, declared as one on the card, and
recorded in M37.
