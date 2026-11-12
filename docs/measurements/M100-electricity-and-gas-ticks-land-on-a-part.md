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
- **Expiry after a mixed group.** Whether expiry of the latest-applied stack
  changes the group's landing before another proc is applied is not measured.
- **Acuity.** The wiki excludes acuity-like bonuses from the landing, and this
  build has none, so that half is the wiki's alone.

### Follow-up readings — 2026-10-26

The owner tests base-form Laetum with Caput Mortuum's +50% headshot damage,
Secondary Deadhead's +30%, and Lavos +200% Gas. Each pair is direct hit / Gas
tick. Target identity and level were not restated in this batch; the readings
match the damage before armour or other target multipliers.

| hit location | non-critical hit / tick | critical hit / tick |
|---|---|---|
| Body | 480 / 242 | 1058 / 530 |
| Head | 2592 / 2336 | 11424 / 10284 |

Using the quantized base crit damage
`c = round(2.2 * 4095 / 32) * 32 / 4095 = 2.203663003663004`, all readings
match after display rounding:

```
body hit       480                               =   480
body crit      480 * c                           =  1057.758... -> 1058
head hit       480 * 5.4                         =  2592
head crit      480 * 5.4 * 2 * c                 = 11423.789... -> 11424
body tick      (80 + 0.5) * 3                    =   241.5      -> 242
body crit tick (80 * c + 0.5) * 3                =   530.379... -> 530
head tick      (80 * 5.4 + 0.5) * 3 * 1.8        =  2335.5      -> 2336
head crit tick (80 * 5.4 * 2 * c + 0.5) * 3 * 1.8 = 10284.110... -> 10284
```

This confirms the second headshot bracket on the whole Gas tick, including
the accumulator, for both non-critical and critical applying hits. The tick
inherits the hit's crit-headshot seed; these readings do not imply a new crit
roll by the tick. No new neighbour readings were supplied.

Test: `m100_gas_body_head_and_critical_readings_match_to_the_digit`.

### Mixed body/head stacks and lethal attribution — 2026-11-12

The owner tests base-form Laetum, base 160, with Caput Mortuum's +50% and
Secondary Deadhead's +30% headshot damage, and Lavos +200% Electricity.
The readings below were supplied for Electricity; the owner confirms the
same readings and ordering rule for Gas. The imbue contributes the tested
element's +200% bracket. Target identity and level were not restated; the
numbers match damage before armour or other target multipliers. Only the
hits explicitly marked critical below are critical.

Each row lists status applications in order, then the consolidated tick:

| Application order | Observed tick |
| --- | ---: |
| Body, body, critical head | 11148 |
| Body, body, head | 3200 |
| Head, body, body | 1778 |
| Two head applications, one critical, then body | 7249 |
| Head, head, body | 2834 |
| Body, head, head | 5100 |

Controls: two body applications give 482; two non-critical head applications
give 4668. The owner corrected an initial 3220 transcription to 3200. An
earlier 7003 report and the interpretation that mixed applications are
order-independent are withdrawn; the repeated body/head/head reading is 5100.

For this setup, let `P_i` be 1 for a body application and 5.4 for a head
application. Let `C_i` be 1 for a non-critical application, `c` for a yellow
body crit, and `2*c` for a yellow head crit, where
`c = round(2.2 * 4095 / 32) * 32 / 4095 = 2.203663003663004`.
The latest status application's part selects `L`: 1 for body, 1.8 for head.

```
head seed multiplier = 3 * (1 + 0.50 + 0.30) = 5.4
head tick landing   =     (1 + 0.50 + 0.30) = 1.8
per-stack seed      = 0.5 * 160 * P_i * C_i = 80 * P_i * C_i
consolidated tick   = (sum(per-stack seed) + 0.5) * 3 * L
```

The accumulator's 0.5 is paid once for the group (M58). The final `*3` is the
tested element's bracket from Lavos. All six readings match after display
rounding:

| Application order | Calculation | Before rounding | Display |
| --- | --- | ---: | ---: |
| Body, body, critical head | `(80 + 80 + 80*5.4*2*c + 0.5)*3*1.8` | 11148.110109890109 | 11148 |
| Body, body, head | `(80 + 80 + 80*5.4 + 0.5)*3*1.8` | 3199.5 | 3200 |
| Head, body, body | `(80*5.4 + 80 + 80 + 0.5)*3` | 1777.5 | 1778 |
| Two head applications, one critical, then body | `(80*5.4*2*c + 80*5.4 + 80 + 0.5)*3` | 7249.394505494505 | 7249 |
| Head, head, body | `(80*5.4 + 80*5.4 + 80 + 0.5)*3` | 2833.5 | 2834 |
| Body, head, head | `(80 + 80*5.4 + 80*5.4 + 0.5)*3*1.8` | 5100.3 | 5100 |

Each stack retains its applying hit's part and critical multiplier in its
seed. A later application updates the landing of the whole consolidated
tick, including its accumulator. Head/head/body and body/head/head contain
the same seeds; their final part selects 2834 versus 5100. The tick does not
roll a new crit, and a later non-critical body application preserves an
earlier head crit's seed, as the 7249 reading demonstrates.

The owner also measures headshot-kill classification by application order:

| Application order before the lethal tick | Headshot kill / Deadhead eligibility |
| --- | --- |
| Body applications, then a head application | Yes |
| Many head applications, then a final body application | No |

The latest status application's part supplies the tick's headshot-kill
classification as well as its landing multiplier. Earlier head-applied
stacks retain their damage seeds when the latest application is to the body;
that remaining damage does not make the lethal tick a headshot kill. The
owner confirms the same behavior for Electricity and Gas.

These tests concern the struck body's pile from one weapon. They do not
settle an ordinary final hit that applies no status, multiple appliers,
neighbouring recipients, expiry of the latest stack, or the Electricity proc
generated by Magnetic breaking shields or Overguard.

### Magnetic break Electricity follow-up — 2026-11-12

The owner tests base-form Laetum with Caput Mortuum's +50% and Secondary
Deadhead's +30% headshot damage, and Lavos Valence Formation +200% Magnetic.
The target is an armour-stripped Steel Path level 50 Corrupted Heavy Gunner
Eximus. This is Magnetic-generated Electricity, separate from the earlier
Lavos Electricity measurements.

| Aim throughout the trial | Magnetic stacks at break | Observed Electricity tick |
| --- | ---: | ---: |
| Body | 10 | 3455 |
| Head | 10 | 3455 |

Adding +165% Electricity gives 9154 at 10 Magnetic stacks; aim was not
restated for this reading. `9154 / 3455 = 2.64949...` supports the ordinary
Electricity-mod bracket. Multiplying the displayed baseline by 2.65 gives
9155.75, so the unrounded numerical reconstruction is not yet closed.

The owner also reports 2764 at an unconfirmed Magnetic stack count and a
tentative 1382 reading at an unconfirmed count. Neither count is assigned
from the damage ratio. The owner observes that the Magnetic count when the
break proc triggers is snapshotted: increasing or decreasing that count
afterwards does not change the existing Electricity damage.

The body/head comparison establishes equal isolated break-proc damage under
this setup despite the breaking weapon's +80% headshot bonus. The target
survived, so this batch contains no separate lethal-event observation.

The owner confirms that this is an ordinary Electricity proc, inheriting
its properties and joining weapon Electricity in the same pile. It is
applied to the body with a seed from the broken pool and the Magnetic count;
Electricity modifiers, status damage, Roar and duration use the breaking
weapon's ordinary proc context. The Magnetic count snapshot fixes this
contribution, not the later tick landing. A body-applied break proc resets
the whole pile's landing to body; another head-applied Electricity proc
before the next tick changes the whole pile back to the head.

The owner confirms the source rule by another experiment: breaking with a
weapon with status duration produces different proc properties from breaking
with another weapon without duration but with an Electricity mod. Exact
duration values and additional damage numbers were not supplied. M8 records
this breaking-weapon source rule; the earlier Magnetic appliers are not the
source to follow. The owner states this calibration batch is complete.

### Implementation

`lands_on_a_part` names the two families, and `Dot::landing` carries the
second layer over the whole tick. The Tesla arc posts its seed unchanged, and
`drain_area_procs` draws each neighbour's landing at
`TESLA_HEAD_LANDING_CHANCE` from its own stream. `m100_*` in `engine::fight`
replays the body, head and neighbour readings on the real unit and checks the
rate.

`DebuffState::proc_landings` keeps the latest Electricity/Gas application
part; the consolidated tick uses it for every seed, its accumulator and the
lethal weak-point trigger shared by Deadhead and Galvanized Scope/Crosshairs.
`m100_latest_proc_part` checks the six mixed-stack readings through both tick
schedulers, the lethal head/body order, precision without a damage bonus,
non-precision status controls and the record's landing flags. Magnetic's
pool-derived seed enters `settle_procs` as a body-applied ordinary Electricity
proc. It shares weapon Electricity's source modifiers, duration, area path,
clock, accumulator, landing and lethal attribution. M8 regressions cover
source scaling, count snapshot and body/head anchor transitions.
