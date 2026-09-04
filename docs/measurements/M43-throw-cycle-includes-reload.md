# M43 — a throw pays for its own reload, so the listed rate is HALF the cycle ✅ (owner, 2026-08-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** The wiki gives the spear throw a fire rate and, separately, the
sentence *"Throwing the spear consumes 1 ammo, then reloads the weapon."* The
entry read the first as a cadence and the second as a note, so the sim threw 40
times between reloads — the primary fire's magazine, on the attack that does not
spend it. The two readings are 1 % apart on a bare build and 60 % apart on a
fire-rate one, because a reload that never happens is a floor that never bites.

**Reported, verbatim:**

> throw的流程是这样的，当按下投掷的时候，先有一个蓄力的时间，然后投掷出去，接着换弹。蓄力的时间和射速有关。默认的蓄力时间是1s。

(*press → a WIND-UP, whose length is set by fire rate and is 1 s at base →
release → RELOAD.*)

**What it settles.** The reload is unconditional, not a magazine running dry, so
the cycle is `wind-up + reload` = 1.0 + 0.6 = **1.6 s** and the throw rate is
0.625/s, not 1/s. That is a `magazine: 1` weapon — the same shape a bow's nock
already has here (`cernos_prime.yaml`, and dummy.rs' "the cycle is charge +
reload however the two are ordered"), so the fix was the magazine and nothing
else. The wind-up being `1 / fire_rate` also makes the second clause fall out for
free: a fire-rate bonus shortens the wind-up and cannot touch the reload.

Scourge Prime, thrown, 180 s against a level-9999 Steel Path Thrax Centurion, no
headshots, finite ammo:

| build | before | after | |
| --- | --- | --- | --- |
| bare | 178 throws, 303 dps | **113 throws, 191 dps** | −37% |
| +Shred | 231 throws, 397 dps | 132 throws, 224 dps | −44% |
| +Primed Shred +Vile Acceleration +Speed Trigger | 400 throws, 596 dps | 194 throws, 281 dps | −53% |
| +Primed Fast Hands | 179 throws, 305 dps | 130 throws, 222 dps | reload went from +0.7% to **+16%** |

The last two rows are the point. Under the old magazine a fire-rate stack bought
its full multiplier and the mode's ceiling was set by fire rate alone; under the
real cycle the reload is a floor, so fire rate buys only the wind-up's share of
1.6 s and RELOAD SPEED becomes a real mod on this weapon — which is the opposite
of what the pre-fix build search would have told a player.

Magazine mods stay inert by construction and correctly so: a reload draws
`floor(capacity − current)` whole rounds, so a 1.66-round capacity still loads
one.

**Pinned by** `a_thrown_speargun_paces_on_wind_up_plus_reload`, which asserts
the cycle against the sim's own shot count and states the fire-rate half as an
inequality (throughput rises by strictly less than the fire rate did). Verified
to bite: restoring `magazine: 40` reproduces the old number exactly — *"178
throws in 180s, but a 1.600s cycle fits 113"*.

### Sources

- [`Scourge Prime`](https://wiki.warframe.com/w/Scourge_Prime) — "Throwing the
  spear consumes 1 ammo, then reloads the weapon"
- the owner's sequence above
