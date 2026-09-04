# M16 — How fast can a bow be TAPPED? (Cernos Prime, uncharged form)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** A bow is played two ways — hold to full draw, or click as fast as
you can — and the second is a form of its own (`base`, half the damage per
arrow). What is its cadence?

Two readings, and they differ by 2.5x:

| reading | cycle | shots/s | where it comes from |
|---|---|---|---|
| **nock only** (implemented) | 0.65 s | **1.54** | wiki Fire Rate's bow formula, *"Effective Fire Rate = 1 / (Modded Charge Time + Modded Reload Time)"* — no fire-rate term, and a tap pays no charge |
| semi-auto cap + nock | 1/1.0 + 0.65 = 1.65 s | 0.61 | the uncharged attack's own `Trigger = "Semi-Auto"` and `FireRate = 1` in the data module |

The second would make tapping strictly worse than drawing (half the damage at
0.7x the rate), which is not how the weapon is played (user, 2026-07-31), and
the wiki names bows as the exception to the generic charge-weapon formula
precisely because their cadence carries no fire-rate term. So the first is
implemented — but it is an INFERENCE from a formula written for the charged
shot, not a measurement of the tapped one.

**Protocol.** Cernos Prime, no mods (a fire-rate mod would move the answer and
is the second half of the question). Simulacrum, one target, unlimited ammo.
1. Tap-fire as fast as the weapon allows for a fixed window — 30 s on a timer,
   or count against the mission clock — and record the number of arrows.
   Volleys of 3 arrows: count VOLLEYS, not arrows.
2. 30 s should give **~46 volleys** under the nock-only reading and **~18**
   under the semi-auto one. Nothing in between is expected; the readings are
   far enough apart that a rough count settles it.
3. Repeat with Shred equipped (+60% on a bow). Under nock-only the tap does
   NOT speed up (no charge to shorten, and the reload is untouched by fire
   rate); under the other reading it goes to ~22 volleys. **This is the
   cleaner discriminator** — it needs no accurate clock, only "did it change".

**Outcome mapping.** One number in
`data/weapons/primary/cernos_prime_uncharged.yaml`: `charge_seconds: 0.0` is
the nock-only reading. The other reading is not a different value of that
field — it is the generic charge-weapon formula (`charge + 1/fire_rate`),
which the engine does not implement yet and which a non-bow charge weapon
(Opticor, Scourge) would need anyway.
