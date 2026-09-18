# M101 — the Furis's Condition Overload term reads its own base in both forms, and both halves of Haven Foray stay out of it (owner, 2026-09-18)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Furis, +220% base damage, Galvanized Shot, Haven Foray with overshields up**
(*"Increase Base Damage by +28. With Overshields: Increase Base Damage by +30"*).
Readings by Galvanized stacks and status types on the target:

| form | stacks x types | reading |
| --- | --- | --- |
| base | 3 x 2 | **298** |
| base | 2 x 2 | **282** |
| base | 1 x 2 | **266** |
| Incarnon | 3 x 1 | **626** |

### What the readings solve for

Each hit is `panel x 3.2 + 0.4 x stacks x types x C`, where `panel` is the form's
own base plus 58 (20 + 58 = 78 base, 100 + 58 = 158 Incarnon) and `C` is the
absolute the CO term reads. Quantization is exact on both: the base form's
3 / 14 / 3 lands on 4.8 + 22.4 + 4.8 = 32 units, the Incarnon is all Heat.

**The step between stacks fixes `C` on the base form by itself**: each stack at
two types adds 16, so `0.8 x C = 16` and `C = 20`, the unevolved 3 + 14 + 3.

| what the CO term reads | base 1 / 2 / 3 stacks | Incarnon 3 x 1 |
| --- | --- | --- |
| **the form's own base (20 / 100)** | **265.6 / 281.6 / 297.6** | **625.6** |
| own + the +28 (48 / 128) | 288.0 / 326.4 / 364.8 | 659.2 |
| the whole evolved base (78 / 158) | 312.0 / 374.4 / 436.8 | 695.2 |

The first row is every reading to 0.4 of a point. The other two miss by 22 or
more on the first reading alone.

### What it settles

- The `co_base_excludes_this_evolution: true` on Furis Evolution II holds for
  the gated +30 as well as the +28, in both forms — the rule M83 measured on the
  Paris Prime, now on a second weapon and in a second form.
- The engine already answered this way; the reading is pinned by
  `loadout::tests::furis_co_reads_its_own_base_under_haven_foray_in_both_forms`.
