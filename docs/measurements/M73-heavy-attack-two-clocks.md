# M73 — a heavy attack is two clocks and each takes its own bucket; a Tennokai swing skips the first (owner, 2026-09-03)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Praedos, a Tonfa, at 1.0x attack speed**, timing a standing heavy from the
input to the next one being possible.

| wind-up speed | charge | swing | total |
| --- | --- | --- | --- |
| +0% | **0.40 s** | **0.80 s** | 1.20 s |
| +180% | **0.14 s** | 0.80 s | 0.94 s |

`0.4 / 2.8 = 0.143`, which is the +180% row to the digit — so wind-up speed
divides the CHARGE and leaves the swing alone. Attack speed was 1.0x
throughout, so the swing's own bucket is untested here and is the one the
model already applies to it.

**The published figure is the SUM.** The wiki's per-weapon-type table gives the
class a "1.2 s charge", and 1.2 is what a stopwatch gives for the whole thing.
Read as charge alone it makes a wind-up build far faster than it is: 1.2/2.8 is
0.43 s against the 0.94 s measured.

**A TENNOKAI SWING HAS NO CHARGE.** It goes out on the press. DE says only that
the window *"increases its Wind Up Speed"* and publishes no figure, which this
file's own queue had standing as a +100% stand-in on three melee entries.

### What changed

`data/weapons/melee/praedos*.yaml` carried `windup_seconds: 0.7` and a swing of
ZERO, so a heavy cost 0.7 s instead of 1.2. Now 0.4 and 0.8.
`Tennokai::windup_seconds` is 0, `TENNOKAI_WINDUP_SPEED` is gone, and the
stand-in's admission is off the three entries that carried it.

**Not read:** the 0.8 s is the whole swing — its wind-out, the strike and its
recovery together. Where inside it the damage lands is not measured, and the
model settles it at the start.
