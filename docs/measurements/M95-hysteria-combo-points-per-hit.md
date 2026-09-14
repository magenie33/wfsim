# M95 — Hysteria's combo points per hit follow no rule of the multiplier ✅ (owner, 2026-09-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Combo points each hit of the **Hysteria** stance earns on **one target**, per
attack input in order. `a*n` is n hits of a points; `a+b*n` is one hit of a and
n of b. Against a crowd each hit earns its points per body it lands on.

| combo | points per hit |
| --- | --- |
| Neutral (Fervor) | 1 / 1 / 2\*2 / 2\*2 / 2 / 3\*2 |
| Forward (Rage) | 1 / 1 / 2 / 2 |
| Forward Block (Madness) | 2 / 2\*2 / 2\*3 / 2\*2 / 3\*3 / 5 |
| Block (Delirium) | 2 / 3\*2 / 3\*3 / 5 |
| Slide (Launching Spring) | 1\*6 |
| Slam | 1 |
| Aerial (One Point) | 2 / 2\*2 / 3 |
| Wall (Through Strike) | 2 |
| Finisher (Roaring Drums) | 2\*4+3 |

### What it settles

W`Melee` gives combo points as the stance damage multiplier (100% = 1 point).
Hysteria does not follow it — a 100% opener earns 1 on Rage and 2 on Madness, and
the 300% slide earns 1 a hit. Nor is it that rule plus an extra: Fervor's fifth
input, 300%, earns 2, which is LESS than its multiplier.

The stance page's Notes (W`Hysteria_(Stance)`) read as absolute counts, an
unlisted hit earning 1, match Fervor ("3rd through 5th hits grant 2 Combo and
the 6th hit grants 3") and Madness's first five inputs, and miss Rage (notes
2/3/3/3), Delirium (notes 2 on the 3rd and 4th only) and Madness's last input
(notes 3, measured 5). No rule is taken from them.

**THE LAST INPUT OF MADNESS AND OF DELIRIUM IS ONE HIT**, earning 5. The stance
table and `Module:Stances/data` print it as three 300% rows; the game shows one
damage number. Its size is not measured: the entries take the table's 900%, which
is what both combos' published %/s adds up to.

Nothing here says the multiplier rule fails for any OTHER stance; they still read
it, unmeasured.

The Finisher's 5 hits and the Aerial's rows agree with `Module:Stances/data`'s
hit counts; the slide's six agrees with its `Hits = { 6 }`.

### Where it is read

`ComboHit::combo_points` on every row of the `valkyr_talons*` entries (the
modes: neutral, forward, block, block forward, slide); the counter reads it in
place of the multiplier. Slam, Aerial, Wall and Finisher are not modes of this
arena and are recorded here for the day they are.
