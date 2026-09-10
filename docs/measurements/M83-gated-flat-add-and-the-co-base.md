# M83 — a Tenno-gated flat base add stays out of the Condition Overload base, exactly as the unconditional half of the same card does (owner, 2026-09-10)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Paris Prime, normal charged shot, +155% base damage, Galvanized Aptitude.**
Evolutions: Guardian's Might (*"+20 Base Damage. With Overshields: +74"*) and
Striking Succession at its 4-stack cap. Galvanized Aptitude at **2 stacks**
against **1 status type**. One reading with overshields up, one without — the
only thing that moves between them is the gated **+74**.

| overshields | reading |
| --- | --- |
| up | **1501** |
| down | **1306** |

### What the two readings solve for

Both hits are `unmodded x (1 + 1.55 + 60 x 2.55 / unmodded + 0.4 x 2 x 1 x C / unmodded)`,
quantized, where `unmodded` is 454 with the overshield and 380 without and `C`
is the absolute the CO term reads.

**Quantization is a flat x1.03125 on this attack and cancels out of the
comparison.** The charged shot's 2.5 / 17.5 / 80 split lands on
0.8 + 5.6 + 25.6 units of `ModdedBase / 32` whatever the bracket is, and rounds
to 1 + 6 + 26 = 33. So the readings are `33/32 x ModdedBase`, and the ratio is
the ratio of the brackets:

| what the CO term reads | predicted readings | ratio |
| --- | --- | --- |
| **180 — the weapon's own base, gate excluded** | **1500.2 / 1305.6** | **1.14905** |
| 254 — the gated +74 feeds it | 1561.2 / 1305.6 | 1.19577 |
| 274 / 200 — every flat add feeds it | 1577.5 / 1322.1 | 1.19317 |

Measured: **1501 / 1306**, a ratio of 1.14931. The first row is right to
**0.9 and 0.4 of a damage point**; solving for `C` and banding it by the
rounding of two integer readings gives `C in [169, 186]`, which contains 180 and
excludes both alternatives.

The same 180 is what `co_base_fraction: 0.5` says, so the pair confirms the
catalog's figure for this row at the same time.

### What changed

`resolve_for` folded a gated flat add with `add_flat_base_damage(flat, flat)` —
the CO base grew with the panel. The unconditional half of the *same card* went
through `evolutions_data::apply`, which asks `EvolutionDef::excludes_co_base`
and, on this `Adding` weapon, feeds nothing. So Guardian's Might's +20 and its
+74 answered the question differently, and picking up an overshield moved a
number that no card says it moves.

The answer now rides on `loadout::GatedTerm::into_co`, recorded where the perk
still exists. `resolve_for` opens the gate long after the evolution is gone, so
that is the only place the two halves can be made to agree.

The old behaviour was never a reading — its comment said so: *"Preserved rather
than decided — no gated perk is on the CO catalog and none has been measured."*
This is that measurement.
