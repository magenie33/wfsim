# M82 — enemy Overguard carries its excess into health (2026-09-07)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** A hit larger than the Overguard bar in front of an enemy: does the
excess reach the shield and health under it, or is it discarded with the bar?

### MEASURED: an Overguard holder dies to one shot ✅

**In game** (owner, 2026-09-07): units carrying Overguard are killed outright by
a single shot, and the bar reads as an external health bar rather than as a
wall. Damage that outruns it lands.

### SOURCED: the depletion protection belongs to the player, not the enemy

Wiki `Overguard`, on the PLAYER's bar:

> It additionally has a **0.5** second invulnerability gate when fully depleted,
> preventing any excess damage from leaking into their shield or health pool.

…which Update 33.6 introduced, in its own words:

> Similar to Shields, Player Overguard will now offer a brief moment of
> invulnerability (currently 0.5s) once depleted. This window will prevent
> players with Overguard from being one-shot, as damage to Overguard will no
> longer carry over into your Health or Shield pool once depleted.

…and immediately below that patch note:

> NOTE: This Overguard Depletion Protection applies only to Player Overguard,
> not Overguard seen on enemies.

The enemy paragraph says the same thing from the other end:

> Enemies do not have any invulnerability gating when their Overguard is
> depleted.

So carrying over is the DEFAULT and the gate is the exception; players were
one-shot through Overguard until 33.6 gave them the window, and enemies were
explicitly left without it.

### The model

The instance spends `overguard / (raw × overguard column × Disrupt amp)` of
itself on the bar; the rest re-enters the ordinary route — the faction column,
the shield gate, Toxin's bypass, armour and the Viral amp — because the two
pools read different columns and the carried share never met Overguard's.

Secondary Fortifier's multiplier does not travel with it: the card says "to
Overguard" (M38), so the carried share arrives unfortified.

**An enemy now has exactly two places to waste damage** — the health bar, where
a killing blow's excess is overkill, and the shield bar, whose gate passes only
5% of the breaking hit's overflow (M61). Overguard wastes none.

### What it moved

Every fight against an Overguard holder, which is every ruler: the official
enemy is a Thrax Centurion and the whole board is scored on it. On the
group-clear ruler, a Ballistica Prime build that scored 3973 KPM scores 3961
with the carry-over, and the same build with Primed Heated Charge instead of
Heated Charge goes from 3929 to 4110 — the two swap places. The old model
threw away 27% of everything that build dealt, and it threw away more of it the
harder the build hit, which is what inverted the ranking.
