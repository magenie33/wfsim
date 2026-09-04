# M25 — Spectral Serration paid +330% to builds that were not invisible (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Third of the same shape (M22 Primary Acuity, M23 Semi-Rifle Cannonade). The
card is "+330% Damage **while Invisible**"; the file was a flat
`base_damage_bonus`, so every build collected it.

Invisibility is a WARFRAME state, and the fight now has a Warframe in it:
`condition: while_invisible` is asked of the arena's Tenno. The neutral Tenno
is visible, so the mod contributes nothing and the panel's row says why
("+330%, while Invisible"). Verified: Torid, Thrax Lv 300 SP, 120 s, 100 runs —
0.2865 with no fifth mod, **0.2865 with Spectral Serration**, 0.3437 with plain
Serration; and with `invisible: true` in the scenario the same build pays in
full.

(It first shipped as an unevaluable `CondBuff(BaseDamage)` — full value on the
panel, nothing in the sim. That was the right shape for a calculator with no
player in it, and it stopped being one the moment the player arrived.)

The condition test from M22 walked past it, because it knew the two phrases it
had been written for ("Weak Point", "when/while Aiming"). It now flags ANY
"while/when …" clause on a card whose effects carry no condition and no
trigger — verified to fail on this mod.
