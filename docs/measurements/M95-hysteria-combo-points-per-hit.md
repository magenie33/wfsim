# M95 — Hysteria's combo points per hit follow no rule of the multiplier ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Combo points each hit of the **Hysteria** stance earns on **one target**, per
attack input in order. Against a crowd each hit earns its points per body it
lands on.

**NOTATION — the wiki's stance table's:** `/` separates attack inputs, `Nx v` is
N hits of v (the COUNT comes first, as in the table's `2x 200%`), and `a + b`
is separate hits of one input.

| combo | attack multipliers (W`Hysteria_(Stance)` table) | combo points per hit (measured) |
| --- | --- | --- |
| Neutral — Fervor | 100% / 100% / 2x 200% / 2x 200% / 300% / 2x 300% | 1 / 1 / 2x 2 / 2x 2 / 2 / 2x 3 |
| Forward — Rage | 100% / 200% / 200% / 300% | 1 / 1 / 2 / 2 |
| Forward Block — Madness | 100% / 2x 150% / 3x 150% / 2x 200% / 3x 250% / 300% + 300% + 300% | 2 / 2x 2 / 3x 2 / 2x 2 / 3x 3 / 1 + 3 + 1 |
| Block — Delirium | 300% / 2x 300% / 3x 250% / 300% + 300% + 300% | 2 / 2x 3 / 3x 3 / 1 + 3 + 1 |
| Slide — Launching Spring | 6x 300% | 6x 1 |
| Slam — Slam Attack | 200% | 1 |
| Aerial — One Point | 200% / 2x 200% / 300% | 2 / 2x 2 / 3 |
| Wall — Through Strike | 300% | 2 |
| Finisher — Roaring Drums | 5x 250% | 4x 2 + 3 |

The last input of Madness and of Delirium is THREE hits — three damage numbers —
earning 1, 3 and 1 in the table's order.

### What it settles

W`Melee` gives combo points as the stance damage multiplier (100% = 1 point).
Hysteria does not follow it — a 100% opener earns 1 on Rage and 2 on Madness, and
the 300% slide earns 1 a hit. Nor is it that rule plus an extra: Fervor's fifth
input, 300%, earns 2, which is LESS than its multiplier.

The stance page's Notes read as absolute counts, an unlisted hit earning 1,
match Fervor ("3rd through 5th hits grant 2 Combo and the 6th hit grants 3") and
Madness's first five inputs, and miss Rage (notes 2 / 3 / 3 / 3), Delirium (notes
2 on the 3rd and 4th only) and Madness's last input (notes 3, measured
1 + 3 + 1). No rule is taken from them.

Nothing here says the multiplier rule fails for any OTHER stance; they still read
it, unmeasured.

### Where it is read

`ComboHit::combo_points` on every row of the `valkyr_talons*` entries (the
modes: neutral, forward, block, block forward, slide); the counter reads it in
place of the multiplier (`dummy::swing_combo_points`), and
`hysteria_earns_its_measured_combo_points` holds each combo's round. Slam,
Aerial, Wall and Finisher are not modes of this arena and are recorded for the
day they are.
