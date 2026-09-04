# M38 — Secondary Fortifier: the RULE is settled, the NUMBER is not (2026-08-09)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Audited against the whole wiki page at the owner's request. Everything
transcribed is right — x3/x4/x5/x6/x7/x8 by rank, Overguard only (and this
engine sends the WHOLE instance to Overguard while it holds, with no carry-over
to shields or health, so multiplying the instance is right), lost the moment the
pool breaks, and the steal half deliberately unmodelled and disclosed as such
(`secondary_fortifier :: overguard on damage`).

### MEASURED: a status tick takes it, and takes exactly what the hit takes ✅

**In game** (owner, 2026-08-09): Ocucor, 220 base + 225 Heat, into a Techrot
Babau Eximus's body. Left column is that tick's damage, right column is each
Heat DoT's:

| | without the arcane | with it (rank 3, "x6 Extra") |
|---|---|---|
| | 64 – 34 | 384 – 202 |
| | 103 – 53 | 672 – 346 |
| | 36 – 20 | 535 – 277 |
| | 74 – 39 | 725 – 372 |

**The DoT is 52% of its hit in BOTH columns** — 0.531 / 0.515 / 0.556 / 0.527
without, 0.526 / 0.515 / 0.518 / 0.513 with. That ratio is the whole
measurement, and it is the one number in this table that four uncontrolled
samples CAN pin, because it is taken within each shot rather than across the two
runs.

Half of ModifiedBase is what a Heat tick is, so 0.52 is the tick unmultiplied
relative to its own hit. **Under the old model it would have read 0.52 ÷ 7 =
0.075 with the arcane on.** It reads 0.52. The tick takes the same multiplier
the hit takes, once — which is what the reasoning below had already concluded
and is now a measurement rather than a reading.

### The reasoning it confirms

The wiki's own two sentences:

> "The Overguard steal effect can be 'inherited' if the first source of Heat
> status applied to an enemy was from a secondary with this Arcane active."
>
> "Extra damage to Overguard is **not inheritable and is dynamically applied**,
> so the effect is lost entirely after depleting the Overguard from an enemy."

**"Dynamically applied"** is the phrase that settles it: the bonus is not baked
in anywhere, it is checked when damage LANDS — which is exactly what a DoT tick
is. The card says "Deals x8 Extra Damage to Overguard" with no qualifier about
hits, and the same page says DoTs trigger the steal half.

**"Not inheritable" is not evidence against it.** It names `Heat_Inherit` — the
mechanic that attributes later Heat damage to whoever applied the first Heat
status — and says the damage bonus does not travel down THAT path. The owner
read it first (2026-08-09: "这里的继承应该是说……在 warframe 引擎看来，还是这把枪
造成的，所以战甲的 heat 伤害也可以吃到加成"), and it is the reading that makes
both sentences say something rather than one of them contradicting the card.

**ONCE, not squared** (owner: "那dot也是9倍，而不是9*9倍率吧"). Faction damage is
re-applied per derivation step because DE re-applies it — `faction_at(f, depth)`
— and nothing says that here. A tick is not a derivation; it is the same
instance's payload landing later.
`the_arcane_multiplies_a_status_tick_exactly_once` reads x8.0 and would read
x64 if it were treated like faction.

### MEASURED: "x8" is the EXTRA, so the total is ×9 ✅ (owner, 2026-08-09)

DE's card says "Deals **x8 Extra** Damage to Overguard"; the wiki's stats table
column is headed "Overguard Damage Buff" with the value "x8". Those two
phrasings disagree: "extra" reads as +8x on top of the hit (**x9 total**), the
table reads as the total (**x8**). There is no worked example on the page and no
datamined figure to hand.

This engine read it as the TOTAL until now (`rank0: 2.0` … `rankMax: 7.0`).
**The owner's call is ×9** ("应该是9倍，你先执行"), on the plain reading of the
word DE chose: `x8 Extra` is eight times extra, on top of the hit. The ladder
moves with it — `x3 Extra` … `x8 Extra` is ×4 … ×9 — so the stored bonus is now
the number DE prints rather than one less than it.

**DECIDED: ×9 at max, ×7 at rank 3** (owner, 2026-08-09 "应该证明了，就是*7",
reaffirmed 2026-08-10 "是*9"). The rows are NOT four matched pairs — they are
eight independent samples of a beam whose ramp and crit tier move under it, so
nothing here is meant to be divided row by row.

The arithmetic that survives unpaired samples is thin but points the same way.
Dividing the buffed column by each candidate and looking for the unbuffed
column's own values:

| ÷ | gives | against the unbuffed column (36, 64, 74, 103) |
|---|---|---|
| ÷6 | 64, 112, 89.2, 120.8 | 64 exactly, nothing else |
| **÷7** | 54.9, 96, **76.4**, **103.6** | **74 and 103**, both within 3% |
| ÷8 | 48, 84, 66.9, 90.6 | nothing |
| ÷9 | 42.7, 74.7, 59.4, 80.6 | 74.7 against 74 |

Two near-hits for ×7 against one exact for ×6, on four values and four
candidates, which is suggestive rather than conclusive on its own. It agrees
with the plain reading of the word DE chose, and the owner ran it.

**What would overturn it,** kept because a decision is not a measurement and
this one is worth 12.5% on every Overguard hit: hold the beam on a fresh Eximus
until the ramp tops out and the number stops moving, read the ordinary
(non-crit) tick with the arcane on and off, Overguard bar still up both times.
`with ÷ without` is the multiplier exactly, and the one line to change is
`rank0`/`rankMax` in `data/arcanes/secondary/secondary_fortifier.yaml`. Nothing
else moves with it.

Shipped ladder: ×4 / ×5 / ×6 / ×7 / ×8 / ×9 by rank.

One consequence worth knowing about: DE's card prints the EXTRA here while
`fill_x`'s "xX" convention exists because DE usually prints the TOTAL over a
stored bonus. The card text is therefore un-converted for this one effect rather
than the data being bent to fit a formatting rule, and the panel's own line says
both numbers ("×8 extra damage to Overguard (×9 in total)").
