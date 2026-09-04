# M50 — the Torid Incarnon's CO reads a flat 51, and the default flipped ✅ (owner, 2026-08-16)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The measurement M49 asked for, on the weapon that was picked because it would
settle the most: the most-played Incarnon in the game, and one whose catalog
rows this repo had read as an explicit "100%".

**THE READINGS.** Torid, Incarnon form, Galvanized Aptitude only.

| perk | panel | bare crit | stacks x types | measured |
|---|---|---|---|---|
| Final Fusillade (+51) | 102 | 316 | 1 x 1 | **380** |
| Plentiful Mayhem (+31) | 82 | 254 | 0 x 0 | **254** |
| Plentiful Mayhem (+31) | 82 | 254 | 1 x 1 | **318** |
| Plentiful Mayhem (+31) | 82 | 254 | 2 x 1 | **381** |

**THE FORM IS IDENTIFIED BY THE CRIT.** 316/102 = 3.098 and 254/82 = 3.098,
which is the Incarnon form's 3.1; the base form's is 2.0 and would have printed
302 off its own 151 panel. No second run was needed to pin which was measured.

**THE DECISIVE SHAPE IS NOT THE FRACTION — IT IS THAT THE BASE IS CONSTANT.**
Solved as an absolute rather than a ratio, `co_base = (hit/bare - 1) / (0.4 x
stacks) x panel`:

| reading | panel | solved CO base |
|---|---|---|
| Final Fusillade, 1 stack | 102 | 51.65 |
| Plentiful Mayhem, 1 stack | 82 | 51.65 |
| Plentiful Mayhem, 2 stacks | 82 | 51.25 |

All three land on the unevolved **51**, off two different panels and two
different perks. A CO term that fed on the evolution would have solved to 102
and 82 — two numbers — and would not have agreed with itself across the pair.
A ratio alone cannot say that, which is why the second perk was worth measuring.

If the +51 fed the term the first reading would have been 442 against 380, and
the Plentiful Mayhem pair 356 and 457 against 318 and 381.

### And the CO term is not multiplied by base damage — the class, measured

The same weapon and perk again with a **+165% base-damage mod** on the build.
This is the experiment that separates `Adding` from `Multiplying`, and it needs
that mod present: without one the two are algebraically identical.

| | bare | 1 stack | 2 stacks | increment |
|---|---|---|---|---|
| no Serration | 254 | 318 | 381 | **+64 / +127** |
| +165% | 674 | 737 | 801 | **+63 / +127** |

**The absolute increment does not move.** `Multiplying` would have scaled it by
2.65 to +170 and +337. `Adding` predicts exactly what was read, because the CO
chunk joins the base-damage bucket and the whole bucket is then divided by the
same `(1 + bd)` — so the term lands as a flat addition:

```
damage = ( panel x (1 + base_damage_mods) + 51 x 0.4 x stacks x types ) x crit
```

Six readings, worst error 1 point (0.1%, the display's rounding). The Torid
Incarnon's `Adding` class was catalogued; it is now measured.

**AND THE RESIDUAL IS NOISE, NOT A PATTERN.** With three readings it looked
systematic — all +1 above a CO base of 51.0. Six readings solve to 51.6 / 51.2 /
50.8 / 51.2, scattering both sides. It is display rounding and the base is 51.

### And the default flipped

Two weapons, four perks, four exclusions. The reading of the catalog that
produced the old default did not survive it, and the reason is in the table's
own columns:

* Its **"Attack Unmodded Damage"** column prints a DOUBLE value — "100 or 124
  (with Evolution II)" — on exactly **eleven** rows. Those are the only rows
  where anyone measured the weapon with an evolution installed. **All eleven are
  excluded.**
* Every other row prints a single number, and that number is the UNEVOLVED
  base. "Torid | Main-fire | 100 | 100%" says the CO bonus equals the base of a
  Torid with no evolution on it, which is true by construction and answers a
  different question. This repo read it as "the evolution feeds in full".
* So the score on the question actually asked is **15 to 0**: eleven catalog
  rows plus four owner measurements, all excluded, and **nothing anywhere
  measured an evolved weapon and found its evolution fed the term.**

**THE DEFAULT FLIPPED FOR `Adding` ENTRIES ONLY** (owner, 2026-08-16). An
undeclared perk on an Adding entry now keeps its flat damage out of the CO term;
238 weapon+perk pairs moved, by 37% on average at two Galvanized stacks against
two status types. The board rescores on the push and every Adding Incarnon
carrying a base-damage perk falls.

**`Multiplying` IS UNTOUCHED — 24 pairs — because nothing has measured one.**
All four owner measurements are Adding entries, and the owner stopped the
version of this change that covered both ("don't extrapolate"). The rule may
well be the same on both sides, since which base the term reads sits upstream of
how it combines, but that is an argument and not a reading.

> **SUPERSEDED THE SAME DAY BY M51**, and kept because the reasoning is the
> record. The argument in the last sentence was WRONG, not merely unproven: a
> `Multiplying` entry reads its FULL evolved base, so the two classes disagree
> and "upstream of how it combines" was the wrong picture. Refusing to
> extrapolate is what stopped that argument from being written into 24 entries
> as a fact — the flip would have been backwards on every one of them.

**AND A DECLARATION IS SCOPED TO THE FORM IT WAS MEASURED ON.** A perk reaches
both entries of its transform group while a reading comes off one of them, and
the Torid is where that bites: `co_base_excludes_only_form: incarnon` on both
its perks, so recording the measurement does not silently assert the base form
nobody fired.

**THE ERROR IS ASYMMETRIC**, which is why the Adding half is the right call at
this sample size. The old default OVERSTATES, and for a calculator whose promise
is matching in-game measurements that is the worse direction — it ranks weapons
on damage the game does not deal. One measurement finding an INCLUDED Adding
perk reverses it, and
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages` is the
loop that would lose a line.

### Still open

* ~~**THE TORID'S BASE FORM**~~ and ~~**a Multiplying entry, any Multiplying
  entry**~~ — both ANSWERED the same day, by the same readings. See **M51**.
  The four-hypothesis table this section printed came back on its first row.
