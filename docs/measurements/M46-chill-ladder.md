# M46 — the chill ladder, walked one stack at a time ✅ (owner, 2026-08-16)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Setup.** Laetum, BASE form (crit multiplier 2.2), evolutions chosen not to
move damage, Lavos's +200% Cold infusion (forced procs), every shot on the
TORSO of a Demolisher — a target that cannot be frozen, so the ladder can be
walked all the way to ten instead of converting at the top. Non-crit held at
**192** throughout (192.3 before the display rounded it).

| stacks | crit | crit / 192.3 | implied bonus | ladder |
|---|---|---|---|---|
| 0 | 423 | 2.20 | 0.00 | — |
| 1 | 442 | 2.30 | 0.10 | 1st rung |
| 2 | 452 | 2.35 | 0.15 | 2nd |
| 3 | 462 | 2.40 | 0.20 | 3rd |
| 5 | 481 | 2.50 | 0.30 | 5th |
| 10 | 529 | 2.75 | 0.55 | **10th** |

Every row lands within half a point of `2.2 + 0.10 + 0.05 x (n - 1)`. Three
things fall out of it at once.

### 1. THE LADDER HAS TEN RUNGS

+0.55x at ten, one past the published table — the page stops at nine because on
everything it describes the tenth stack IS Frozen, whose own +1.0x replaces the
ladder anyway. Only a target that reaches ten WITHOUT freezing can show it.

A NINE-RUNG CAP SHIPPED FOR ONE COMMIT and this is what removed it. The
inference was the wiki's `Demolisher` line — *"will not freeze at 10 procs,
instead their movement will be Slowed by 90%"* — read across from the SLOW
table, whose ninth rung is 90%. The slow does cap at 90% (owner, confirmed);
the crit ladder does not, and a measurement beats a reading of a neighbouring
table.

### 2. A HIT IS SCALED BY THE STACKS ALREADY ON THE TARGET, NOT BY ITS OWN

The rows are labelled *before -> after*: the 423 was the shot that took the
target from 0 to 1, and it was scaled by **zero**. The Cold status a hit
applies does not pay that hit.

The engine already worked this way — `cd_abs` is read at the top of the pellet
body, before `settle_procs` applies that pellet's status — and now the ordering
is measured rather than incidental. Earlier pellets of the SAME pull do count,
because they landed first.

### 3. IT EXPLAINS A READING THAT LOOKED LIKE A FAULT

The same weapon on the same target alternated between **529 and 423** with an
unchanged non-crit of 192, which read as a bonus flickering on and off. It is
not: 423 is the first shot into a fresh target (0 stacks) and 529 is a shot
once the ladder is full (10). One rule, both numbers.

### Also measured

**Lavos's +200% Cold is x3.2** on this weapon against this target (60 -> 192
non-crit). The arithmetic checks: 160 base (64 Impact + 96 Slash) plus 320 Cold
is x3.0 before the target's own damage-type column and x3.2 after it.

### How to measure here

TAKE THE DIFFERENCE AT A FIXED NON-CRIT. Armour, faction, level and the
infusion are common factors of both crits and cancel; the absolute ratio does
not behave as cleanly (an earlier pair on this target, without the infusion,
gave 141/60 = 2.35, which fits no rung). The clean form is

    (crit_at_n - crit_at_0) / non_crit = the nth rung

which is how +0.55x was separated from +0.50x: `(529 - 423) / 192 = 0.552`
against a nine-rung prediction of 0.495.
