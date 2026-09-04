# M26 — the two arcanes that read a WARFRAME, and the one fact still missing (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Primary Bulwark and Primary Overcharge were both `kind: unmodeled`, for the
same stated reason: "the value depends on the Warframe, which a weapon calc has
no model of". It has one now — a fight carries a Tenno — so both are modelled:

| arcane | card | model |
|---|---|---|
| Primary Bulwark | "+1% damage for each unit of armor past 1,000, up to +500%" | `tenno_scaled` off `armor`, `above: 1000`, `per_unit: 0.01`, cap 5.0 |
| Primary Overcharge | "While at or above 90% Energy: gain 35% of Max Energy as Multishot, capped at 350%" | `tenno_scaled` off `max_energy`, `per_unit: 0.0035`, `min_energy_pct: 0.9`, cap 3.5 |

**Checked by construction, not by measurement.** Torid, Thrax Lv 9999 SP, 30 s,
5 runs:

- no frame → both contribute nothing (5,348.9 DPS, identical to the arcane
  slot being empty), which is what "no frame chosen" should mean;
- `wf_armor: 1500` + Bulwark → 32,093.5 DPS, exactly ×6.0 — the cap is +500%
  and it lands in the base-damage bracket;
- `wf_energy: 257` + Overcharge → 15,255.7 DPS, and **Split Chamber at rank 5
  gives 15,255.7 DPS**. 0.0035 × 257 = +90%, which is Split Chamber's number,
  so the arcane demonstrably feeds the same multishot bucket a mod does;
- `wf_energy_pct: 0.5` → back to 5,348.9: the 90% gate holds.

**What is NOT verified, and is the whole of M26's ask**: which multiplier each
bonus JOINS. Both are modelled as additive with their family's mods, because
that is what every other "+X% Damage" / multishot source in this data set does
and what Primary/Secondary Merciless and Primary Plated Round state outright.
Nothing on either card says so. The measurement that settles it is the ordinary
one: a build with a known Serration bonus, in-game panel damage with and
without Bulwark at a known Warframe armor value. If the bonus is an independent
multiplier instead, only the bucket changes — the card's own numbers (1% per
point past 1,000, 35% of max energy, the two caps) are not in question.
