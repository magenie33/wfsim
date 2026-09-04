# M21 — Puncture's Weakened was critting explosions (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

A sweep of the status models against the wiki, prompted by M20. Weakened's
crit-chance grant is real and correctly valued (+5% flat per stack, 5 stacks,
10 s — `Damage/Puncture_Damage`, which the summary `Status_Effect` page omits),
but that page states one exclusion outright:

> This is a flat critical chance buff (like Arcane Avenger), but does not apply
> to Area of Effect damage or Warframe abilities.

The radial stage keeps its own copy of the crit line and that copy added
`weakened_cc`, so an explosion crit off a debuff it is excluded from. The
lingering field never did — the radial's copy was the odd one out. Fixture:
an explosion with zero crit chance of its own dealt 3300 where a never-critting
one deals 3000, a 10% inflation from Weakened alone.

Reach: any AoE weapon that applies Puncture. No roster weapon carries Puncture
and a radial today, so no golden value moves — but a Puncture mod on an
Incarnon explosion is one equip away from it.
`weakened_never_crits_an_explosion` pins it, and asserts the DIRECT hit still
crits off Weakened, which is what the buff is for.

### The rest of the status sweep, checked and unchanged

Every other DoT and debuff matched the wiki exactly:

| | wiki | engine |
|---|---|---|
| Slash | 35%/s, 6 ticks, 1 s delay, bypasses armour | `BLEED_COEFFICIENT 0.35`, `BLEED_DELAY 1.0`, cinematic |
| Toxin | 50%/s, 6 ticks, 1 s delay, bypasses shields | `DOT_COEFFICIENT 0.5`, delayed, toxin share bypasses |
| Heat | 50%/s; strip 15/30/40/50% at 0.5 s; return 50/40/30/15/0 at 1.5 s | same, both ramps |
| Electricity / Gas | 50%/s, no delay (the 6 s event is a dud) | `immediate_ticks` |
| Viral / Magnetic | x2 at 1 stack, +25% each, 10 stacks | `ten_stack_amp` |
| Corrosive | 26% at 1, +6% each, 80% at 10, 8 s | `1 - (0.20 + 0.06n)` |
| Cold | 50% slow; +0.10x crit damage then +0.05x; 10th freezes 3 s, leaves 3; cap 4 under Overguard | all four constants match |
| Blast | 30% per stack on a 1.5 s fuse; the ORIGINAL target takes no AoE | single-target hit only, host excluded |
