# M116 — Forceful Finality pays the burst that empties the magazine ✅ (owner, 2026-10-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**THE READING.** Burston with Forceful Finality ("+5 Base Multishot on final
magazine burst"), Simulacrum, one target, damage numbers counted:

| build | final burst |
|---|---|
| no multishot mod, full burst left | 18 numbers — three rounds at six pellets |
| magazine changed so the last burst is two rounds | 12 numbers — two rounds at six |
| Primary Acuity | 3 numbers — the +5 is gone |
| ammo efficiency (60%, and the other ways of getting it) | only the burst whose rounds force the reload pays; a burst fired while the counter reads two or three but the magazine survives pays nothing |

Dual Toxocyst with Pistol Acuity: no outside multishot source reaches the
weapon, Fevered Frenzy's stacks included.

**WHAT IT SETTLES.**

- The window is the BURST that empties the magazine, every round of it, and
  a partial last burst pays on the rounds it has. `fight::resolve` gates on
  `mag_left <= rounds left in this burst x the round's real cost`, so under
  ammo efficiency the gate is the reload, not the counter.
  `forceful_finality_pays_only_the_burst_that_forces_the_reload`.
- An Acuity's lock takes this "base" multishot too: "Base" names the bracket
  the mods multiply, not the weapon's own value. An Incarnon FORM's multishot
  is a different thing — the form is its own panel, and the lock holds it
  there. M30's reading of the lock stands for Fevered Frenzy and for this.

**CARRIED BY THE OWNER'S RULING, NOT FIRED.** Final Fusillade (Torid) and
Synth Charge read the same gate, so they take the same reading under ammo
efficiency. Rain of Lead's permanent "+2 Base Multishot" (Bronco) is the same
bracket as Forceful Finality's +5, so an Acuity takes it too
(`WeaponBase::evo_base_multishot`): a locked Bronco fires 7, not 9. Firing
either would confirm it.
