# M1 — Is Toxin's shield-bypass damage reduced by the enemy shield gate?

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** When an enemy's shields break, a 0.1 s gate lets only 5% of
damage reach health. Toxin never touches shields — is its direct damage
gated inside that window, or does it pass in full?

**Model assumption (2026-07-24; revised same day):** the gate is the
enemy analogue of the player shield-gate — a 0.1 s protection window on the
unit — so Toxin is **gated to 5% too**. Status: **assumption / unverified**
until this protocol is run. Outcome mapping: target **survives** the
verdict shot → assumption confirmed; **instant death** → Toxin is ungated,
revert MECHANICS.md §8 and the engine model.

**Primary method — kill-threshold discrimination (no recording, no number
reading).** Turn the transient into a persistent binary: pick a level where
target health `H` sits between the gated and ungated toxin damage,
`0.05·T < H ≤ T`. Then the breaking shot either kills on the spot (ungated)
or leaves the target standing (gated).

- **Target.** Corpus **Crewman** (no armor, Head 3.0x; base @L1: 90 HP /
  120 shields). Level ≈ **5** (≈115 HP / ≈148 shields — robust to ±20%
  formula error).
- **Weapons.** *Shield whittler*: any unmodded pure-IPS weapon (bare
  Braton) — IPS never touches health while shields are up. *Verdict shot*:
  **Lex + Pathogen Rounds** (+90% Toxin): panel 180 physical + `T` = 162
  Toxin.
- **Steps.**
  1. Whittle the shield bar visibly low (<20%, eyeball is fine) with body
     shots from the whittler.
  2. One **body** shot with the Lex (never the head — weakspots bypass the
     gate). The 180 physical certainly finishes the shield; the 162 Toxin
     lands the same instant.
  3. Outcome: **instant death** → Toxin ungated (assumption confirmed).
     **Survives the instant** → Toxin is gated (fix MECHANICS.md + engine).
  4. Repeat ≥5×. A crit on the verdict shot cannot flip the result
     (gated 0.05 × 324 ≈ 16 ≪ H). If a Toxin *proc* ticks afterwards
     (green DoT numbers), void that trial — the DoT could kill a
     should-survive target over 6 s.

**Alternative (needs reading numbers).** Same setup at a high level; read
the white (health-pool) damage number of the breaking shot: ≈`T` ungated,
≈`0.05·T` gated. Requires the recording setup below.
