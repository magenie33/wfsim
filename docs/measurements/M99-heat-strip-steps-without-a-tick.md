# M99 — Heat's armour strip climbs to 50% on a burn too short to tick, in the wiki's four steps, on the capped 2700 (owner, 2026-09-16)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Braton Prime at +165% base damage (Impact 4.6, Puncture 32.5, Slash 55.7 on
the arsenal), then with +200% Heat added by Lavos. Status
duration -87.5% from Lavos, so one proc's burn lasts `6 × 0.125 = 0.75 s` and
never reaches its +1 s tick on its own. Target: an ordinary Corrupted Heavy Gunner, Steel Path
level 210, body hits, held fire.

| build | readings |
| --- | --- |
| no Heat | 11, crit 22 |
| +200% Heat | settles "almost at once" on 107, crit 214; seen on the way: 30, 70, 90, crit 103 |
| +200% Heat, held fire | a Heat tick of 1164, at about twenty-odd stacks |

### The arithmetic

Quantized against `92.75 / 32` (M57, M80): Impact 5.80, Puncture 31.88,
Slash 55.07, Heat 185.5. Orokin column Puncture ×1.5 (Heat neutral), so the
hit is 108.72 without Heat and 294.22 with it. The unit's armour is far past
the cap, so it spawns at 2700 (DR 90%) and a strip multiplies that.

| strip | armour | DR | no Heat | +200% Heat | seen |
| --- | --- | --- | --- | --- | --- |
| 0% | 2700 | 90.0% | 10.87 | 29.42 | 11 · 30 |
| 15% | 2295 | 83.0% | | 50.08 (crit 100.17) | crit 103 |
| 30% | 1890 | 75.3% | | 72.67 | 70 |
| 40% | 1620 | 69.7% | | 89.10 | 90 |
| 50% | 1350 | 63.6% | | 106.97 (crit 213.94) | 107 · 214 |

**The tick.** One proc's tick is `0.5 × 92.75 × (1 + 200%) = 139.13`, and at
the full strip (DR 63.6%) it lands as 50.59. The consolidated burn pays that
once per proc folded in, so 1164 is `23 × 50.59 = 1163.6`.

### What it settles

- **The strip does not wait for a tick.** It reaches 50% within the first
  0.25 s, well before any burn could tick.
- **A burn shorter than a second still ticks while it is refreshed.** Each proc
  resets the one entity's expiry to 0.75 s ahead, so held fire keeps it alive,
  it ticks on its +1 s beat and the tick carries every proc folded in — 23 of
  them in the 1164. Below -83.33% a single proc never ticks; a weapon that
  lands a Heat proc inside every burn window still does.
- **The strip is the four steps, on the capped value.** The settled 107/214
  and the 11/22 match to the digit, and the glimpses sit near the steps, not
  anywhere in between.

### What it leaves open

- **Where the ramp starts.** A held trigger keeps the burn alive past +1 s,
  so a ramp starting at the first tick would also settle on 107, about one
  second later. "Almost at once" favours the proc; the wiki's cited capture
  (imgur.com/a/V2Oc6hl, 15% at +0.51 s) says the proc. A frame count of the
  30s before the first 107 after a reload decides it: 1-2 on the proc, ~10 on
  the tick.
- **70 and crit 103** are 2.7 and 2.8 off their steps (73, 101), more than
  display rounding explains. Glimpses, not readings.
- **Display rounding.** 29.42 showed 30 and 89.10 showed 90, which reads as
  rounding UP. Two glimpses do not make a rule.

### What is implemented

Already the engine's rule, so nothing moved: `data/debuffs/ignite.yaml`
anchors the ramp to the proc and scales its steps by status duration, and
`rules::scaling::ARMOR_CAP` clamps spawn armour before any strip.
`m99_heat_strip_climbs_without_a_tick_on_capped_armour` in `engine::fight`
replays this build on the real unit and fails if the ramp waits for a tick;
`m99_a_refreshed_short_burn_ticks_every_second` holds the fire, fails if a
refresh does not keep the burn alive, and pins one proc's tick at 50.59.
