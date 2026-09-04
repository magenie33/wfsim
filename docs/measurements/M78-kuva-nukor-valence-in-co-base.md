# M78 — the valence bonus is inside the Condition Overload base, and the Kuva Nukor counts a status type nobody can see (owner, 2026-09-04)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Kuva Nukor on a 60% Magnetic Lich, +220% base damage, Galvanized Shot.**
Direct-hit damage read at each stack count, with **two** statuses on screen:

| Galvanized Shot stacks | reading |
| --- | --- |
| 0 | **108** |
| 1 | **148** |
| 2 | **188** |
| 3 | **228** |

**The step is 40 a stack, and only one arithmetic reaches it.** The base is
21 × 1.60 = 33.6 and the bracket is `1 + 2.20 + 0.40 × types × stacks`:

| what the term reads | 1 | 2 | 3 |
| --- | --- | --- | --- |
| **33.6, three types** | **147.8** | **188.2** | **228.5** |
| 33.6, two types | 134.4 | 161.3 | 188.2 |
| 21 (unvalenced), three types | 132.7 | 157.9 | 183.1 |
| 21, two types | 124.3 | 141.1 | 157.9 |

Only the first row lands on all four readings, and the two errors do not
cancel — the fourth row is further out than either alone.

**THE THIRD TYPE IS MICROWAVE.** Two statuses were on screen and the term
counted three, which is the wiki's *"not listed in the game UI, however is
counted towards the damage calculation bonus with Condition-Overload type
equipment"* paying out in a number. Nothing else on this weapon can be the
third: its whole vector is Radiation plus the Lich's Magnetic.

**AND THE TERM READS THE VALENCED BASE.** 33.6 is the base a copy of this
weapon has; the 21 the infobox prints belongs to no copy in the game, because
every one of them comes out of a Lich carrying an element.

### What changed

`apply_valence` raised the base vector and left `co_base` where it was, so the
term computed on 21 — the third row above. Because `co_base` is an ABSOLUTE, the
CO contribution came out the SAME for every roll (26.25 × 0.80 and 33.6 × 0.625
are both 21), so two copies differing only in their valence scored alike wherever
the term carried the bracket. That is what a build with **no base-damage mod**
is: 25% → 60% moved the direct hit 5.8% where it should move 27.8%, and reads
as a valence that is not being applied at all.

Microwave needed no change — `applies_microwave` already counts it — and this is
the first reading that puts a number on it.
