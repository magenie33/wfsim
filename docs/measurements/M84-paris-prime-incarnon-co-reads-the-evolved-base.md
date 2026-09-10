# M84 — the Paris Prime's Incarnon form computes Condition Overload on ALL of its evolved base, and applies it as a multiplier (owner, 2026-09-10)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**The same build as [M83](M83-gated-flat-add-and-the-co-base.md), transmuted.**
Paris Prime Incarnon Form, +155% base damage, Galvanized Aptitude at **2
stacks**, Guardian's Might with the overshield up, Striking Succession at its
4-stack cap. Two readings, differing only in how many status types the target
carried:

| status types | reading |
| --- | --- |
| 1 | **3095** |
| 2 | **4470** |

### The ratio settles the class and the base at once

The form's base is 520, the flat adds are 94, and Striking Succession is worth
`60 × 2.55`. Everything but the CO term is common to both readings, so it
cancels:

| model | ratio | predicted readings |
| --- | --- | --- |
| **`Multiplying`, CO on all 614** | **1.44444** | **3093.7 / 4468.6** |
| `Multiplying`, CO on the unevolved 520 | 1.40388 | 2883.1 / 4047.6 |
| `Adding`, CO on all 614 | 1.22226 | 2209.9 / 2701.1 |

Measured: **4470 / 3095 = 1.44426**. The first row is right to **1.3 and 1.4 of
a damage point**, and nothing else is close.

**QUANTIZATION IS NOTHING ON THIS FORM**, which is why the readings land on the
bracket directly. 100 Impact / 420 Heat is 6.15 + 25.85 units of
`ModdedBase / 32` and rounds back to 26 + 6 = 32 — where the base form's
2.5 / 17.5 / 80 rounds *up* to 33 for a flat ×1.03125 (M83). One session, one
target, two forms, two different quantization factors: a residual read as a
target multiplier instead would need the target to have changed between them.

### What it confirms

**THE FLAT ADDS FEED IN FULL HERE.** `co_behavior: independent` already made
`excludes_co_base` answer "feeds" for every perk on this form, and the second
row above is what that would read if it did not. The gated +74 feeds too, on
the same rule and by the same `GatedTerm::into_co` M83 introduced — so one card
has its +74 excluded on the charged shot and included on the Incarnon, which is
the CO catalog's two rows for this weapon paying out in numbers.

**AND THE TERM IS A FREE-STANDING MULTIPLIER**, not a member of the base bucket:
the third row is the same 614 read as `Adding`, and it is 30% out.
