# M88 — Boar Prime's Condition Overload reads the weapon's own base in BOTH forms; Reified Bane's +10 and +14 stay out (owner, 2026-09-11)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Boar Prime, +165% base damage, Galvanized Savvy, Reified Bane with both
halves up** (the +10, and the +14 an empty reload opens — M29). Two readings,
one per form, written `stacks-types`:

| form | Galvanized Savvy | reading |
| --- | --- | --- |
| normal (per pellet) | 2 stacks, 3 status types | **266** |
| Incarnon (per tick) | 2 stacks, 1 status type | **167** |

### What the readings solve for

Galvanized Savvy is +40% a stack per status type, an `Adding` class term, so a
hit is `panel x (1 + 1.65 + 0.4 x stacks x types x C / panel)`, where `panel`
is the base after Reified Bane and `C` is the absolute the CO term reads.

| form | `C` and the flat add | predicted |
| --- | --- | --- |
| normal | **C = 40, +10 +14** | **64 x 2.65 + 40 x 2.4 = 265.6** |
| normal | C = 64, +10 +14 | 64 x 5.05 = 323.2 |
| normal | C = 40, +10 +10 (the card's figure) | 60 x 2.65 + 96 = 255.0 |
| Incarnon | **C = 30, +10 +14** | **54 x 2.65 + 30 x 0.8 = 167.1** |
| Incarnon | C = 54, +10 +14 | 54 x 3.45 = 186.3 |
| Incarnon | C = 30, +10 +10 | 50 x 2.65 + 24 = 156.5 |

Only the bold rows reach either reading, and they reach both to within the
display's rounding. Quantization is neutral on both attacks: the pellet's
65 / 15 / 20 split lands on 21 + 5 + 6 = 32 units, and the tick is one type.

### What it settles

1. **The CO term reads the weapon's own base, 40 a pellet and 30 a tick**, with
   every flat add of the perk left out — the `Adding` class's rule
   (`EvolutionDef::excludes_co_base`), measured here on a weapon and a trigger
   (a reload-gated add) that no earlier entry covered, and on the Incarnon
   form as well as the base one.
2. **The gated half is +14, not the card's +10**, which is the wiki's own note
   ("Reload From empty bonus is incorrectly listed as +10 in game") read in
   the damage rather than on the card.
3. **Reified Bane reaches the Incarnon form**: without its +24 the tick would
   be 103.5.

Pinned by `boar_prime_co_reads_its_own_base_under_reified_bane_in_both_forms`
in `engine/src/dummy.rs`.
