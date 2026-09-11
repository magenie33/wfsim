# M87 — a Demolisher's Radiation proc never lands; its Impact proc lands and does nothing (owner, 2026-09-11)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Demolisher Devourer, in game.** A Radiation weapon fired into it produces **no
Radiation status at all** — the proc does not appear. An Impact weapon produces
the Impact proc normally; what is missing is only the stagger.

So the two are different mechanics and the difference is which side of the roll
they are on:

| | the proc | the roll | Condition Overload |
| --- | --- | --- | --- |
| **Radiation** | never appears | **dropped from the draw** | not a type on this target |
| **Impact** | appears | takes its share | counts |

### Why this is written down rather than read

**THE WIKI PUTS BOTH IN ONE SENTENCE AND CANNOT SETTLE IT.** Its Demolisher
page lists, verbatim:

```
*{{D|Confusion}}, {{D|Knockdown}}, {{D|Lifted}}, {{D|Stagger}}, {{D|Stun}} effects.
```

Confusion is what a Radiation proc does and Stagger is what an Impact proc
does, so the sentence reads as one mechanic — "these five effects do not move
this unit" — and says nothing about whether the proc lands. Taken that way it
turns Radiation into the Impact case, which is the opposite of the measurement.
The page's only stated proc immunity is Viral, on the **Infested** Demolishers,
which this Grineer unit is not.

That reading was acted on once and reverted by this measurement. It is the
reason the file now cites a measurement instead of a source that cannot answer.

### What it is worth

The two arms are not a detail. A type dropped from the draw is not merely
absent: the remaining types **renormalise onto the roll**, so every other status
on the build gets more likely. Reading Radiation as the Impact case therefore
moves every status build's numbers on any ruler fought against this unit, in
three places at once — the Radiation build loses its status half, every other
build's status mix shifts, and a Condition Overload build gains or loses a type.

**AND THE DAMAGE IS UNTOUCHED EITHER WAY.** Neither of these is a `x0` column:
Radiation damage lands in full on this unit and only the status is refused.
