# M39 — Secondary Fortifier's value is LEVEL-SHAPED, and can be negative ✅ (2026-08-09)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Audited because the owner did not feel the arcane in game: *"我打200级的eximus的
堕落重型机枪手感觉没那么强啊，你是不是什么地方多算了"*. **Nothing is
over-counted** — at his level the model agrees with him.

Ocucor, the board's own top build, against a Corrupted Heavy Gunner, 40 runs of
300 s per cell:

| target | without | with (max rank) | gain |
| --- | --- | --- | --- |
| level 200 Eximus | 8.62 | 8.38 | **−2.8%** |
| level 200, no Eximus (no Overguard) | 27.01 | 27.01 | **0.0%** |
| level 60 Eximus | 66.17 | 61.94 | **−6.4%** |
| level 9999 Eximus | 2.16 | 2.83 | **+31%** |

The no-Overguard row is the control: **exactly** zero, so the "only while the
pool is up" gate is doing its job and nothing leaks past it.

### Why +31% at the ruler and nothing at 200

The official ruler is level 9999, where an Eximus carries **12.4 M** Overguard
and this weapon only gets through ~104 M of damage in 300 s — so the Overguard
is 12% of everything the gun does in the fight, and the arcane turns that into
1.3%. At level 200 the same pool is 366 k against a target that costs ~424 k to
kill *in total*: there is nothing left to save.

### Why it goes NEGATIVE, which is the interesting half

Monotonic in the multiplier — at level 60 it is −2.5% at ×4, −5.2% at ×6, −6.4%
at ×9 — so it is caused by the bonus rather than by noise. Decomposed:

| | without | with | |
| --- | --- | --- | --- |
| direct | 19.0 M | 22.6 M | **+19%** |
| DoT | 9.1 M | 4.6 M | **−50%** |
| total | 28.1 M | 27.2 M | −3.4% |

**Overguard carries no armor and a DoT ticks on its own clock.** So the window
in which the Overguard is up is a window where FREE ticks land unmitigated on a
unit whose health would keep 10% of them (2700 armor). The arcane shortens that
window from 1.9 s to 0.25 s and throws the windfall away — and at low level the
windfall is worth more than the direct damage the arcane adds.

Two modelled facts, both correct, producing a result neither of them announces.
It is also exactly why the arcane feels weak in the owner's own play and strong
on the board.

### The premise that had to be checked, and was ✅

The whole result rests on damaging statuses applying while the Overguard is up.
They do — owner-confirmed in game (2026-08-09: "可以在敌人身上啊"). Overguard
blocks CROWD CONTROL, not damage. Had it blocked damaging status too, the model
would have been over-crediting every DoT weapon against every Eximus in the
roster, which is a far larger error than the arcane this audit started from.

`a_dot_ticks_into_a_full_overguard_bar` pins it, and says in its own comment
that M39 flips with it.

### Not caused by the M38 tick change

Checked by removing it: the level-60 loss reads −6.5% / −6.7% / −6.2% across
three seeds with the tick multiplier in, and −6.6% / −6.7% / −6.5% with it out.
The behaviour predates 2026-08-09.
