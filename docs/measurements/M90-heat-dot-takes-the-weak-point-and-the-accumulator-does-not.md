# M90 — a Heat DoT DOES take the weak point, and the accumulator's 1 does NOT ✅ (owner, 2026-09-13)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A **Braton Prime**, no Incarnon — base 35, read off the infobox as
`21 Impact + 1.8 Puncture + 12.2 Slash` — carrying **+200% Heat** and nothing
else. Two numbers per line, `direct — status`:

```
body   105 — 54
head   315 — 159
```

### The fixture is what it says it is

`35 × (1 + 2.00) = 105` and `105 × 3 = 315`, so the element bracket is 3.00 and
the body part is the page's own ×3. Nothing else is in the way.

### What it settles: the DoT takes the part, the accumulator does not

A tick is `(Σ seeds + 1) × C × M` (M58) with `C = 0.5` for Heat. Three readings
of "which halves take the body part" survive as arithmetic, and only one of
them lands on 159:

| | body | head |
| --- | --- | --- |
| neither half takes it | 54 | 54 |
| both halves take it | 54 | **162** |
| **the seed takes it, the accumulator does not** | **54** | **159** |
| measured | **54** | **159** |

```
body  0.5 × 35     × 3.00  +  0.5 × 3.00  =  52.5  + 1.5 =  54.0
head  0.5 × 35 × 3 × 3.00  +  0.5 × 3.00  = 157.5  + 1.5 = 159.0
```

`159 / 54 = 2.944`, and that it is NOT 3.000 is the whole reading: a tick that
scaled whole would be 162.

### Why it was worth a session

**THE WIKI SAYS THE PART APPLIES, AND THAT SENTENCE HAS BEEN WRONG BEFORE.**
Verbatim on `Damage/Heat_Damage`: *"Additional Multipliers include modded
critical multiplier on Critical Hit and multipliers on Enemy Body Parts"*. The
Toxin page carries the same sentence and M54 measured it FALSE there. Heat is
built in its own arm — a singleton accumulator, not a `Dot` — so it never
passed the list that exception lives in, and its part factor was applied
unconditionally on the strength of that line alone. Both rulers score every
shot at the weak point, so the line was worth ×3 on every Heat build on the
board. It is now known rather than believed, and it holds.

**AND IT CONFIRMS M58 FROM A DIRECTION NOBODY HAD PUSHED.** That the `1` sits
outside the instance's own facts — its crit, its body part — was implemented
from the page's arithmetic (`(40 × 1.55 + 1) × 0.5 × 3.25 × 1.55`, the faction
bonus inside the seed and in `M` with the `1` between them) and had never been
read off a number. The 159 reads it directly: the difference between 159 and
162 is the accumulator declining a multiplier the seed took.

### No residue

Both halves are measured, in both positions. What is still believed rather
than known is the same sentence's OTHER clause — the modded critical
multiplier on the tick — which this fixture cannot see, having none.
