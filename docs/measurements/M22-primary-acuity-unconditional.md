# M22 — Primary Acuity was an unconditional +350%/+350% (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Found from a user question: "the headshot rate is zero, why is there still a
damage bonus?" Its card reads

> +350% **Weak Point** Damage
> +350% **Weak Point** Critical Chance. Multishot cannot be modified.

and the data file had `base_damage_bonus` + `crit_chance_bonus` — both
unconditional, on every shot, plus no multishot lock. Its own pistol twin
(`pistol_acuity`) had been modelled correctly all along, which is what made
the single wrong file easy to miss.

Also, from the same wiki page: "It cannot be equipped on sentinel or companion
weapons — only primary rifles." It was in the Verglas Prime's pool.

After (Torid, Thrax Lv 300 SP, 60 s, 100 runs, seed 7, vs the same build
without it):

| headshot rate | with Acuity |
|---|---|
| 100% | 1.53x |
| 0% | 0.53x |

The 0.53x is correct and worth stating: `disables: [multishot]` cancels Split
Chamber, and at zero weak-point hits nothing comes back for it. A mod that
can lose you damage is what the card describes.

NOT applied: the wiki's note that the weak-point damage bonus lands at 1.5x
the listed value is already in both files' comments but is not implemented —
that needs an in-game measurement, not a wiki sentence.

Two smaller fixes in the same pass, both "the panel and the sim disagreed
about which buffs exist":
- `enumerate_buffs` matched the OUTER effect, so a `WhileAiming`-wrapped buff
  (Argon Scope) produced no card while the resolver ran it. It unwraps now.
- Arcane buff cards were one per GRANT. Frostbite grants crit damage and
  multishot off the same Cold proc — one count by construction — so it is one
  card, and one config now reaches every spec its arcane owns.
