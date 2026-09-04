# M51 — a `Multiplying` entry reads its FULL evolved base, and the two CO classes disagree ✅ (owner, 2026-08-16)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The experiment M50 §Still open asked for, on the weapon it named: the Torid's
**base form**, which is `Multiplying` where the form of M50 is `Adding` — the
same two tier-2 perks, the same Galvanized Aptitude, one reading answering both
open questions.

**THE READINGS.** Torid, base form, +165% base damage and +90% Electricity
(Corrosive), as `grenade impact / toxin cloud`. `1 x 2` is one Galvanized stack
against two status types.

| perk | 0 x 0 | 1 x 1 | 1 x 2 |
|---|---|---|---|
| Final Fusillade (+51) | 763 / 460 | **1068 / 644** | **1373 / 827** |
| Plentiful Mayhem (+31) | 662 / 359 | **926 / 502** | **1191 / 646** |

**THE MULTIPLIER DOES NOT MOVE WHEN THE PERK DOES.** 1068/763 = 1.3997 and
926/662 = 1.3988; 1373/763 = 1.7994 and 1191/662 = 1.7991. The CO term is scaled
by NOTHING — the entry reads its full evolved base. Fed on the unevolved 100 it
would print 1.265 under the +51 and 1.305 under the +31, which are neither each
other nor what was read.

**AND THE ANSWER IS THE OPPOSITE OF M50's**, on the same weapon and the same two
perks. That is the finding: which base the term reads is decided BY THE CLASS,
not upstream of it. The M50 paragraph guessing that "the rule may well be the
same on both sides" was wrong.

| | class | the CO term reads |
|---|---|---|
| Torid Incarnon form (M50) | `Adding` | the UNEVOLVED 51 |
| Torid base form (M51) | `Multiplying` | the FULL evolved 151 / 131 |

**THE DECISIVE SHAPE IS THE TWO COLUMNS, and it needs only the +51.** The same
flat +51 lands on both attack parts, so the impact's evolved base is 151 and the
cloud's is 91. Any term reading something other than the evolved base has a
different fraction in each column — 100/151 = 0.662 against 40/91 = 0.440 — and
must therefore print two DIFFERENT multipliers, 1.265 against 1.176, 7.5% apart.
It printed **1.3997 and 1.4000**. The +31 pair is a second, independent
confirmation rather than the argument.

**THE WHOLE SET IS CONSISTENT TO THE DISPLAY'S ROUNDING.** Solving all twelve
readings for the one multiplier every build factor and the target's own column
collapse into, against bases of exactly 100 and 40:

`763/151, 662/131, 460/91, 359/71, 1068/151/1.4, …` → **5.049 to 5.056**, a
spread of 0.14%. Twelve readings, three known inputs, no residual.

**AND THE CLOUD TAKES CO AT ALL** — the doubly-discrepant catalog row confirmed
from the other side. An AoE part is not supposed to receive the bonus, and this
one receives it as `Multiplying`, at the same rate as the main fire:

```
Torid | Toxin AoE Cloud | AoE | 40 | 40 | 100% | Multiplying
```

**SO THE RULE GENERALISED TO ALL 26 `Multiplying` ENTRIES** (owner, 2026-08-16),
deliberately ahead of the catalog and on one weapon's reading: the wiki prints a
fraction for a minority of attacks, this rule beats that table, and a
measurement that contradicts it edits ONE weapon's yaml. The class now answers
BEFORE a perk's declaration on a `Multiplying` entry, so a reading taken off an
`Adding` form cannot reach across a transform group and dilute one —
`no_evolution_dilutes_a_multiplying_co_base` asserts the property roster-wide
rather than the 26 numbers, and so covers a weapon nobody has entered yet.

**IT MOVED NO NUMBER TODAY.** Every `Multiplying` entry already computed on its
full evolved base, because the class default said so and the Torid's two
declarations were scoped away from it. What changed is that it is now MEASURED
rather than defaulted, and structural rather than a coincidence of which
declarations happen to exist.
