# M89 — a Demolisher takes a flat 0.8 inside the FACTION bracket, so a hit is ×0.8 and the status it applies is ×0.64 (group, 2026-09-13)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

An **Aklex Prime with nothing equipped** — base 150, no mods but the one named
— fired at a **Demolisher Devourer**'s head (the page's `Head: 3.0x`). The
number read is the **Slash bleed**, which is the instrument: a bleed ignores
armour, takes no elemental bracket, and carries the crit multiplier of the hit
that seeded it, so a non-critical body-less reading is `0.35 × base × head`
and whatever else is in the way.

```
nothing else equipped        101
Primed Expel Grineer (+55%)  242
```

### What forces the shape

A bleed is one derivation step past the hit that applied it, so faction damage
is squared on it — the depth model this engine already carries (`faction_at`,
`DEPTH_PROC`), and the reason the second reading is worth taking at all.

| | no faction mod | +55% faction |
| --- | --- | --- |
| nothing in the way | 157.5 | 378.4 |
| a 0.8 applied ONCE whatever the depth | 126.0 | 302.7 |
| **a 0.8 INSIDE the bracket, raised with it** | **100.8** | **242.2** |
| measured | **101** | **242** |

`0.35 × 150 × 3 × 0.8² = 100.8` and `0.35 × 150 × 3 × 1.55² × 0.8² = 242.2`.
Only the third row fits both, and it fits to the point.

### …and it is NOT damage attenuation

Attenuation caps what one instance and one second may take off a unit, as a
share of Max Health. This is a plain multiplier that behaves like a Bane —
re-applied at every derivation step — so the patch note that removed
attenuation from Demolishers (`{{Ver|40.0.2}}`, quoted in the unit's own yaml)
does not touch it, and the two must not be read as one fact.

### What is implemented

`TargetParams::faction_bracket_multiplier`, applied in `faction_at_time` to
the FINISHED sum:

```rust
(faction_multiplier + faction_bonus_at(abilities, t)) * target.faction_bracket_multiplier
```

The position is the whole of the entry. Inside the sum it would be one more
Bane and Roar would land on the wrong side of it; applied outside `faction_at`
it would be ×0.8 whatever the payload's depth, which is the second row of the
table above and is measurably wrong. Where it is, two families compose with it
without any member being named: everything that READS the bracket — a Bane,
Roar, a syndicate proc's damage — and every payload that carries a DEPTH, so an
extra hit (Toxic Lash, Resupply) and a spread status take 0.8² and 0.8³ where
they already took `f²` and `f³`.

Nourish is NOT one of them, and the distinction is the point of stating the
list: it adds an ELEMENT, which is its own bracket, so it is scaled by this
multiplier exactly as much as the damage it rides on and no more.

A file states the per-HIT figure (`0.8`) and never the squared one: a yaml
that stated 0.64 would be wrong for every direct hit.

### …and the record names it

`x1.24 faction` with a +55% Bane equipped is a number nobody can trace back to
a card, so the ledger draws the shooter's bracket and the target's multiplier
as two layers — `x1.55 faction`, `x0.8 target's own multiplier`. A status tick
is drawn as the sum it is, each half with the product behind it, so the
`x0.64` and the `x0.8` sit one line apart and the difference is read rather
than asserted.

### What these two readings cannot separate

Both are headshots, and `3.0 × 0.8² = 1.92`. So "a 0.8 in the bracket" and
"this unit's head multiplier is 1.92 rather than 3.0" predict the SAME two
numbers, and the faction mod does not tell them apart because it varies only
the faction term. The head multiplier is taken as 3.0 on the page's own Multis
row and on the owner's word; the arithmetic here does not prove it.

Two readings would, one shot each, and neither has been taken:

| reading | 0.8 in the bracket | head is 1.92 | what the engine says today |
| --- | --- | --- | --- |
| body, non-critical, bleed | 33.9 | 52.9 | 52.9 → **33.9** |
| head, non-critical, DIRECT | 360 | 288 | 450 → **360** |
