# M75 — a stance swing under 100% still earns a combo point, and every hit and body earns its own (owner, 2026-09-03)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Sovereign Outcast, Rogue Edict, the first input** — `200%` then `5x 50%`, six
instances in one press. The counter moves by **7**.

| reading | what it gives |
| --- | --- |
| the wiki's proportionality alone | 2.0 + 2.5 = 4.5 |
| a flat point an instance | 1 + 5 = 6 |
| **the multiplier rounded up, per instance** | 2 + 5 = **7** |

*"Stance attacks add combo points, scaling with the attack's stance damage
multiplier (100% = 1 point)"* is the wiki's half and was the whole of the
implementation, so a 50% swing earned half a point. It earns one.

**AND EVERY BODY EARNS ITS OWN.** A swing landing on five enemies earns five
times as much, which is why reach and attack speed build the counter as fast as
they do. That half was already modelled.

### Rounded up, not floored at one

They agree everywhere they can be told apart, and rounding is ONE rule where a
floor is two. **Not separable with this roster**: every stance multiplier in it
is whole except `0.5`, which is below the line the two readings differ above. A
stance publishing 150% would settle it.
