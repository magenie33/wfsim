# M118 — the Vectis Prime: Primary Compression's +12%, and the shot combo counts a hit before it pays it ✅ (owner, 2026-10-22)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The M117 build — Vectis Prime with Inciting Incident and Critical Parallel,
Serration — plus Primary Compression at rank 5 and no radius mod. Target: a
Corrupted Heavy Gunner fully stripped of armour.

## The readings

| form | shot | numbers |
|---|---|---|
| base | body | 1776 / 4442 (crit) |
| base | scoped head | 8526 / 42642 (crit) |
| Incarnon | body, 10 Cold (frozen) | direct 4675 (crit), orb 2338 (crit) |

The arcane's panel line reads **+12%** in the Incarnon form.

**The shot combo counter**: at 4, the next landing hit takes it to 5 and that
hit already pays the x1.5.

**Galvanized Aptitude** (rank 10, two stacks: +80% per status type) added to
the same Vectis Prime build, base form, the Heavy Gunner:

| status types | shot | numbers |
|---|---|---|
| 0 | body | 1776 |
| 1 | body | 2118 / 5295 (crit) |
| 1 | scoped head | 10164 |
| 2 | body, x1.5 combo | 9223 (crit) |

The same with Primary Compression, Incarnon form (Cold stacks approximate):

| status types | shot | numbers |
|---|---|---|
| 0 | body | 3637 (crit) + orb 668; the orb's next hits 788, 788 |
| 1 | body | 1159 + orb 788; an orb hit 2758 (crit) |
| 1 | body, x1.5 | an orb hit 4137 (crit) |
| 1 | scoped head, x1.5 | 8343 + orb 1182 |
| 1 | scoped head (6x), x2 | 11124 + orb 1576; the explosion 10391 (crit) x 2 |
| 1 | the same | 11124 + orb 5516 (crit); the explosion 10391 (crit) + 2968 |
| 1 | the same, frozen | 100139 (crit) + orb 5516 (crit); the explosion 10391 (crit) + 2968 |
| 1 | scoped head (3.5x, +40%), x2 | 9734 + orb 1576; the explosion 9092 (crit) + 2597 |
| 1 | the same | 74968 (crit) + orb 5516 (crit); the explosion 9092 (crit) + 2597 |

Scoped throughout. The same without Primary Compression, x2 combo:

| status types | shot | numbers |
|---|---|---|
| 0 | scoped head | 62343 (crit) + orb 1193; the explosion 10391 (crit) x 2 |
| 1 | scoped head | 10056 + orb 5015 (crit); the explosion 10391 (crit) + 2968 |

**Rubico Prime** (Minimum Combo 1), unmodded, the same Heavy Gunner's body,
the counter at 0: from the hip 207, the first scoped shot 311.

## What it settles

- **The base form takes nothing from the arcane**: its numbers are M117's to
  the unit (8526 = 1776.33 x 3 x 1.6, 42642 = 8526 x 2 x 2.50061).
- **The Incarnon form pays +12%, not the catalog's +8%**: 0.8 x 0.15 m at
  +100% per metre, so the embed radial the arcane reads is 0.15 m, not the
  row's 0.1 m.
- **It multiplies**, on both parts of the shot: 2338 = 596 x 1.12 x 3.50085,
  4675 = 927.5 x 1.12 x (3.50085 + 1.0). Joining Serration's bracket would pay
  (2.65 + 0.12) / 2.65, +4.5%.
- **Galvanized Aptitude's term is Adding CO on the weapon's own 350**, not on
  Inciting Incident's +200: the neutral hit is `350 x (2.65 + 0.8 n) + 200 x
  2.65`, the Heavy Gunner's 1776.33 / 1457.5 on top. One type: 1737.5 ->
  2117.7, crit 5295.6, scoped head x 3 x 1.6 = 10164.9; two types: 2017.5 ->
  2459.0, x 1.5 x 2.50061 = 9223.6.
- **Compression does not reach Galvanized Aptitude's term; the combo, the
  head and the crit do.** One type adds 150 x 0.8 = 120 outside the x1.12:
  1159 = 927.5 x 1.12 + 120 (inside it would be 1173), 8343 = 1158.8 x 1.5 x 3
  x 1.6.
- **The orb's hit takes the term on the parent's 150**, not its own 25: 788 =
  596.25 x 1.12 + 120, 2758 = 787.8 x 3.50085, 4137 = 787.8 x 1.5 x 3.50085,
  1182 = 787.8 x 1.5.
- **The orb's first hit counts only what was on the target before the shot**:
  668 with no status before it, though the shot's own Cold lands with it.
- **The headshot explosion takes no Galvanized Aptitude**: 10391 = 927.5 x 1.6
  x 2 x 3.50085 with a status on the target.
- **The orb's hits take the x1.12 and the explosion does not**: 1576 = 667.8
  x 2 + 120 x 2, 5516 = 1575.6 x 3.50085; the explosion 2968 = 927.5 x 1.6 x 2
  and 2597 = 927.5 x 1.4 x 2 under the arcane as without it. It keeps the
  scope's headshot bracket at either zoom: 11124 = 1158.8 x 2 x 3 x 1.6, 9734
  = 1158.8 x 2 x 3 x 1.4, 100139 = 11124.5 x 2 x (3.50085 + 1.0).
- **Without compression the orb's term is the same +120**: 5015 = (596.25 +
  120) x 2 x 3.50085, 10056 = (927.5 + 120) x 2 x 3 x 1.6.
- **A shot's own Cold gives its own direct hit no crit damage**: 62343 = 927.5
  x 2 x 3 x 1.6 x 2 x 3.50085.
- **Frozen's +1.0 crit damage reaches the direct hit and not the orb**, the
  same Area of Effect rule M117 read for Cold's stacks.
- **The shot combo counter is read after the hit is counted**, so the hit
  that reaches Minimum Combo pays its first tier (`fight::pellet`). The
  Rubico Prime shows it from an empty counter: 207 is the x1.0 hit (149.6 /
  28.1 / 9.3 snapped to 26 / 5 / 2 units of 187/32, Puncture x1.5: 207.46),
  and the first scoped hit counts itself to 1 and pays x1.5, 311.2.

Tests: `fight::tests::m117_vectis_prime_chamber`,
`data::weapons::tests::play_mode::the_roster_reproduces_primary_compressions_published_column`.
