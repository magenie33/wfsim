# M4 — Which health curve do Anarchs use? ✅ (2026-07-24)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** `Enemy_Level_Scaling` lists Anarchs in two health tabs with
different exponents: "Anarchs, Corrupted" (`0.015·Δ^2.1 / 10.7332·Δ^0.685`)
vs the "Murmur, Sentient, and Unaffiliated" tab whose *text* also names
Anarchs (`0.015·Δ^2 / 10.7332·Δ^0.5`). Engine currently follows the tab
structure (Anarchs = Corrupted curves).

**Method.** Read a plain (non-Eximus, non-Commandeered) Anarchs unit's HP
at a known level and compare (bases @L1: Anarch Arcus 100, Gladius 175):

| unit | level | A: Corrupted curves | B: Unaffiliated curves |
|---|---|---|---|
| Arcus | 50 | 5,415 | 3,702 |
| Arcus | 60 | 7,950 | 5,322 |
| Arcus | 100 | 25,088 | 10,779 |
| Gladius | 60 | 13,913 | 9,313 |
| Gladius | 100 | 43,905 | 18,864 |

A 2.3x gap at level 100 — a health-bar read or a shots-to-kill count
decides it.

**Result (2026-07-24):** **Anarchs = Corrupted curves.** The wiki's own
calculated stat block for Commandeered Ash Prime @L1000 (wiki calculator:
18,275,927.85 HP / 623,680.94 shields / 2,700 armor / 27,531 affinity)
matches our Corrupted health (2.1/0.685) and Corrupted shield (2.0/0.75)
curves **to the cent**; the Unaffiliated pair is 3.6x off. Bonus
confirmations: affinity = base 5,000 × (1 + 0.1425·√level) floored, with
the module's Affinity field being the base value. Pinned as a regression
test (`commandeered_ash_prime_at_1000_matches_wiki_calculator`). The
Murmur-tab text naming Anarchs is a wiki typo.
