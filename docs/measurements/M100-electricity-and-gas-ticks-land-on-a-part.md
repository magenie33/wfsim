# M100 — an Electricity or Gas tick lands on a body part of its own, and a Tesla arc carries the hit's head ✅ (owner, 2026-09-17)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Laetum, base form, no mods: `64 Impact + 96 Slash`, base 160. Lavos adds
**+200%** of one element at a time. Headshot damage **+80%** in total: the
Evolution IV perk Caput Mortuum (+50%) and Secondary Deadhead (+30%), no
temporary buffs. No crits. Target: ordinary Corrupted Heavy Gunners, Steel
Path level 210, so armour sits at the 2700 cap (×0.1) unless stripped.

| element | body hit | head hit |
| --- | --- | --- |
| Electricity, the struck body | 24 | 234 |
| Electricity, a neighbour's body | | 130 |
| Gas | 24 | 234 |
| Toxin | 24 | 130 |
| Blast | 5 | 26 |
| Heat, armour stripped 50% | 88 | 472 |

And the count: with ten bodies, every one of them on its own spot, the arc
lands on a neighbour's head **10 times in 189** (21 placements, 9 neighbours
each). A placement that does not move gives the same answer every time.

### The arithmetic

A tick is `(0.5 × 160 × part + 0.5) × 3.00` before armour (M58, M91), and the
head ladder is `3 × (1 + 0.3 + 0.5) = 5.4`.

```
body               (240   + 1.5)             × 0.1    =  24.15  →  24
head seed only     (1296  + 1.5)             × 0.1    = 129.75  → 130
head + landing     (1296  + 1.5) × 1.8       × 0.1    = 233.55  → 234
Blast              0.3 × 160 × part          × 0.1    =   4.8 / 25.9
Heat at 50% strip  (240 / 1296 + 1.5)        × 0.364  =  87.9 / 471.8
```

### What it settles

- **Two layers, and only Electricity and Gas have the second.** The SEED takes
  the part the hit struck, with its whole ladder (M91). Where the TICK lands
  is a second layer: on a head it is the headshot brackets over a 1x base.
  Toxin, Blast and Heat read 5.4× the body tick, and Electricity and Gas read
  9.75×. That is the wiki's own list, `Enemy_Body_Parts` §Notes: "always
  defaulting to the 1x multiplier, unaffected by acuity-like bonuses but
  affected by deadhead-like bonuses: … Electricity and Gas status procs".
- **The landing multiplies the whole tick.** Scaling the seed alone gives
  `1296 × 1.8 + 1.5` → 233.43 → 233, and the reading is 234.
- **A Tesla arc keeps the hit's head in its seed.** 130 on a neighbour's body
  is the struck body's head seed with no landing on top.
- **A neighbour's own landing is a head about 5% of the time.** 10/189 = 5.3%,
  95% interval about 3% to 9.5%. One head in six parts (16.7%) is outside it.
  Per round the counts of 0, 1 and 2 heads (12, 8, 1) sit on the binomial's
  12.8, 6.5 and 1.7.

### What it leaves open

- **A body hit's arc.** Every neighbour count was taken off head hits. Whether
  a body hit's arc still lands on a neighbour's head 5% of the time is not
  read. The engine assumes it does, because where the arc lands is a matter
  of placement.
- **Gas on a neighbour.** Only the struck body's gas tick is read, so a
  cloud's neighbours keep a body seed and a body landing.
- **A mixed group.** A body carrying head-landed and body-landed Electricity
  seeds at once pays one tick. The engine scales each seed by its own landing
  and the accumulator by the landing of the seed that brought it.
- **Acuity.** The wiki excludes acuity-like bonuses from the landing, and this
  build has none, so that half is the wiki's alone.

### What is implemented

`lands_on_a_part` names the two families, and `Dot::landing` carries the
second layer over the whole tick. The Tesla arc posts its seed unchanged, and
`drain_area_procs` draws each neighbour's landing at
`TESLA_HEAD_LANDING_CHANCE` from its own stream. `m100_*` in `engine::fight`
replays the body, head and neighbour readings on the real unit and checks the
rate.
