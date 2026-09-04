# M57 — quantization divides by ModdedBase, not by the vector's total ✅ (owner, 2026-08-23)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Four direct-hit readings on a **Braton Prime, base 35** (1.75 Impact / 12.25
Puncture / 21 Slash), each under a different element bonus. They were taken as
part of the Blast and Gas work in M56 and are separated here because what they
settle is neither of those mechanics: it is the **denominator of damage
quantization**, which is under every damage number this app produces.

| build | raw total | measured pop | ours (before) | ours (after) |
| --- | --- | --- | --- | --- |
| 90% Toxin + 90% Heat | 98 | **98** | 101.06 → 101 | 98.4375 → 98 |
| +200% Corrosive | 105 | **105** | 105.0 → 105 | 105.0 → 105 |
| +200% Gas, +90% Toxin | 136.5 | **137** | 132.23 → 132 | 136.7188 → 137 |
| +200% Blast, +90% Cold, +90% Heat | 168 | **168** | 162.75 → 163 | 168.4375 → 168 |

### What the page actually says

`Damage/Calculation` §Quantization states it as two formulas, and both name the
same quantity:

```
Scale = ModdedBase / 32
x     = TotalDamageTypeValue / ModdedBase
Quantized(x) = sign(x) × floor(|x| × 32 + 0.5) / 32
```

**ModdedBase** is `base × (1 + damage mods)` with the elemental portions
excluded — the number this engine already carried as `dot_modified_base` for
status payloads, one line below the call that needed it. Elements are in the
numerator only.

`DamageVector::quantized()` divided by `self.total()` instead, which includes
them. On the first row that snaps the four components to **33** units of a
larger scale rather than 32 of the right one, and the hit comes out 3.1% high.

### Why it survived, and why four readings were needed

The only test on the function is the page's own worked example — 30 Impact / 30
Puncture / 40 Slash with **no mods at all**, so `ModdedBase == total == 100` and
the example passes under either reading. It cannot distinguish them, and neither
could anything else: quantization is invisible on a physical-only weapon and
this is a calculator whose calibration cases were physical.

MECHANICS §Quantization even contains the sentence that names the case —
*"the two descriptions differ only when elemental mods change the vector's
composition"* — written in July as a note about a "pseudo-conflict" flagged on
the wiki. The reasoning was right and nobody ran it against the code.

The **second row is why one measurement would not have done it**: +200%
Corrosive on this weapon agrees under both denominators (32 units either way). A
single reading that happened to be that build would have confirmed the bug.

### What moved

`one_fight` reports all three shapes moved, in both directions, which is what
quantization does — the page's own note says mixed-type damage is *"frequently
gained or lost by the conversion"*:

```
torid        kill progress 0.185578 -> 0.186538   (+0.52%)
gotva_prime  kill progress 0.223337 -> 0.217810   (-2.47%)
scourge      kill progress 0.053459 -> 0.053732   (+0.51%)
```

One test changed with it, and it was already documented as the exception: the
Xata's Whisper worked example (M40) is written as `98 × 2.2`, but 117.6 Blast is
38.4 steps of the `98/32` scale and snaps to 38, so the vector a hit deals is
214.375 and the bracket a status burns off is 2.1875 rather than 2.2. Its final
assertion also had a tolerance of ±0.002 around a ratio read off **two whole
numbers popped in game** (63 and 71) — a precision two integers cannot carry.
The band is `[62.5/71.5, 63.5/70.5]` now, which is what the capture actually
pins, and both readings sit inside it.

### Not settled by this

The DoT tick gap from M56 is untouched: every tick in those blocks is 36/35
above what the engine computes, and quantization does not explain it — a mono
DoT instance of 17.5 on a ModdedBase of 35 is exactly 16 units and is lossless
under the corrected rule too.
