# M133 — continuous beam ramp scales DoT seeds and CentralBeam does not scale with multishot ✅ (owner, 2026-11-26)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

## Observations

Phantasma Prime (base 15 per beam, six innate beams, 15% beam ramp floor over 0.6 s)
with `Incendiary Coat` (+90% Heat), body hits on an unarmoured target:

| Setup | Observed Heat tick | Model |
| --- | ---: | ---: |
| Tap (1st tick at 15% ramp, unmodded multishot 6) | **14** | `(90 × 0.15 + 1) × 0.5 × 1.9 = 13.775` → **14** |
| Same + `Hell's Chamber` (+120% multishot) | **28** (`2.0×`) | `5 × 2.2 + 1 = 12` beams (`2.0×`, vs `6 × 2.2 = 13.2`) |
| Sustained fire to 10 Heat stacks across ramp-up | **669–690** | 10 stacks across 16–18 ticks (`avg_ramp` 0.782–0.806): **669.2–690.0** |
| Sustained fire to 20 Heat stacks (next 10 at full ramp) | **1542** | `687 + 10 × 85.5 = 1542` (`+85.5`/stack at 100% ramp) |

## Rules settled

1. **Innate multi-beam weapons merge into the DoT seed.** Twenty stacks reaching 1542
   adds `+85.5`/stack over the 10-stack reading (matching `90 × 0.5 × 1.9 = 85.5`),
   ruling out single-beam DoTs (`14.25`/stack) and confirming all six innate beams merge
   into `ModifiedBase = 90`.
2. **Beam damage ramp multiplies status DoT `ModifiedBase` and resets per target.**
   A first-tick tap drops from `(90 + 1) × 0.5 × 1.9 = 86.45` to `13.775 → 14`, and ten
   stacks across the 0.6 s ramp average ~78–81% of full (`669–690` instead of `856`).
   Every continuous weapon tracks its ramp per target generation, including beams
   without an `attack.beam` splash/chain block.
3. **`CentralBeam = 1` with `fireIterations > 1` is excluded from multishot scaling.**
   Client calibration (`tests/fixtures/client/primary-calibration-2026-10-28.json`) gives
   `fireIterations = 5, CentralBeam = 1` for Phantasma / Phantasma Prime and
   `fireIterations = 6, CentralBeam = 1` for Phage. Multishot mods and live relative
   multishot grants scale the outer beams only: `(base - 1) × (1 + MS) + 1`.
