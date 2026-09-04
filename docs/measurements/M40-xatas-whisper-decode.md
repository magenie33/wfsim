# M40 — Xata's Whisper decodes exactly, and two of its clauses are still open (2026-08-09)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

### CONFIRMED A SECOND TIME by the wiki's own worked example (2026-08-09)

The owner supplied the `Xata's_Whisper` §"Interaction with Blast" section
verbatim, and it carries a four-line worked chain rather than a formula — which
is the strongest citation this interaction has, because each line checks the
next:

> A gun deals 100 damage per bullet, and we have Thermite Rounds, Rime Rounds,
> Stormbringer, Primed Bane of Grineer, and Xata's whisper at base strength:
>
> - the initial hit: `100 × (1 + 0.6 + 0.6 + 0.9) × (1 + 0.55) = 480.5`
> - its extra hit: `0.26 × 480.5 × (1 + 0.55) = 193.6415`
>   — *"the Faction Damage Bonus is applied again"*
> - the Blast detonation: `0.3 × 100 × (1 + 0.55)² = 72.075`
>   — *"Elemental Damage doesn't apply to Blast detonations and the Faction
>   Damage Bonus is applied again"*
> - the extra hit off the detonation:
>   `0.26 × 72.075 × (1 + 0.55) × (1 + 0.6 + 0.6 + 0.9) = 90.0433`
>   — *"the Faction Damage Bonus is applied YET again, and the Elemental Damage
>   Bonus is applied even though Blast detonations don't scale off Elemental
>   Damage Bonuses"*

Both oddities are visible in the last line alone, and the whole faction ladder
is visible across the four: `f¹` on the hit, `f²` on its extra hit AND on the
detonation, `f³` on the extra hit off the detonation.

`the_wiki_worked_example_reproduces_to_the_digit` runs it. **The relations are
exact; the absolute figures are not, and that is quantisation** — DE rounds each
element of the vector down to a step of the base, so the example's 310 is
300.3125 here. An illustration written to show a formula has no reason to carry
it, and this engine has every reason to. The test therefore asserts the four
RELATIONS, where the quantised total cancels, and states the one number that
differs and why.

It also rules out the two near-misses by name, because both are what a careful
reader would expect instead: the extra hit off a detonation WITHOUT the
elemental bracket (a detonation takes no elemental bonus, so why would the hit
off it), and WITHOUT the third faction layer (two is what every other status
gets). Neither is the number.


**Question.** What is an EXTRA HIT worth, and specifically what happens when one
fires off a Blast detonation — the interaction the owner named as the reason to
implement the ability at all ("注意这个和blast的联动").

**Answer: measured, and the model reproduces every number.** The owner supplied
a player's capture with video (2026-08-09). Per
[owner-supplied numbers are measurements](../AGENTS.md), it is used as one.

### The capture

A Magnus (98 base) with two 60/60 mods making Blast (+120% elemental) and a
Primed Bane of Grineer (+55%), Xata's Whisper at 100% strength, body shot:

| what popped | on screen | formula |
| --- | --- | --- |
| the hit | 323 | `98 × 2.2 × 1.55` = 334.18 |
| its extra hit | 135 | `× 0.26 × 1.55` = 134.68 |
| the Blast detonation | 71 | `0.3 × 98 × 1.55²` = 70.63 |
| the extra hit off the detonation | 63 | `× 0.26 × 1.55 × 2.2` = 62.62 |

Three of the four are exact. The hit reads 323 rather than 334 through the
Anatomizer's own modifiers, and its extra hit — which is Void and neutral
there — reads the full 135, which is itself a small confirmation that the extra
hit is a SEPARATE instance taking its own vulnerability column rather than a
share of the weapon's.

**The poster then adds an Electricity mod on camera and the extra hit moves.**
That is the direct demonstration of the strangest clause: a Blast detonation
takes no elemental bonus at all, and the extra hit copied from it takes the
whole bracket.

### What it settles

1. **Faction twice on an ordinary hit.** `0.26 × 1.55`, not `0.26`.
2. **Faction three times off a detonation.** The detonation is a status payload
   already at `faction_at(f, 2)`; its extra hit is at 3. Nothing in the engine
   hardcodes a 3 — `fire_extra_hits` applies one layer and the depth of the
   thing that triggered it supplies the rest.
3. **The elemental bracket applies to the detonation's extra hit.** ×2.2 here,
   out of a payload that has none.
4. **No second body-part factor off a detonation.** Stated by the CN card
   ("弱点倍率只会被计算一次") and consistent with the capture, which is a body
   shot and so cannot distinguish it — taken from the card.

`an_extra_hit_fires_off_a_blast_detonation_at_the_third_faction_layer` asserts
all four against these numbers.

### The EN and CN pages disagree, and CN wins on both counts

| | EN `Xata's Whisper` / `Extra_Hit` | CN 真理密语 | taken |
| --- | --- | --- | --- |
| rank ladder | 17 / 23 / 23 / 26 % | 17 / 20 / 23 / 26 % | irrelevant — max rank is 26% either way, and only max rank is modelled |
| duration ladder | 20 / 30 / 30 / 35 s | 20 / 25 / 30 / 35 s | same; 35 s |
| body part | the `Extra_Hit` formula shows it ONCE, inside `Weapon Hit Damage` | "同理，弱点倍率也会被计算两次" | **CN: twice** |

The body-part row is the one that matters, and it is not really a
contradiction — the EN ability page says "The ability double dips on faction
damage, **and body part weaknesses**" in the same sentence the formula page
elides. Two EN statements and one CN statement against one EN formula that does
not mention it either way. Modelled as TWICE.

### Two clauses still OPEN

**(a) Does a lingering FIELD tick trigger an extra hit?** Neither page says. The
EN mechanic page's rule is "most non-standard weapon hits", with an explicit
exclusion list (Bursting Mass's absorbed damage, Pathocyst's maggots) that a
cloud tick is not on; against that, a cloud tick is on its own clock long after
the shot. **Modelled as NO**, which is the reading that does not invent a
trigger, and it is the conservative direction for exactly one weapon in the
roster — the Torid, where the cloud is most of the output.

*What settles it:* Simulacrum, Torid + Xata's Whisper, one grenade into a
stationary target. Count the numbers per tick. Two numbers a tick = yes.

**(b) Does the extra hit's status roll read the weapon's LIVE status chance or
its modded listing?** The card says "based on the weapon's total status chance".
The direct-hit path passes the live per-instance chance (arcane stacks
included); the Blast-detonation path has no instance in scope and passes
`ap.status_chance`, the modded listing. The two differ only on a build carrying
Primary Crux or Sentient Surge, and only for the detonation's own Void proc,
which is worth one CO stack.

*What settles it:* a Primary Crux build at full stacks, counting Void procs off
detonations against Void procs off hits over a long engagement.

### Why the Void proc is worth anything at all

It deals no damage — a Bullet Attractor is a 2.5 m field for 3 s that redirects
fire, and this arena has one target and nobody shooting back. But Condition
Overload's own page lists the procs that count and **Void is on it**, so an
extra hit that procs buys a CO stack. That is the whole of its value here, it is
tracked exactly like Radiation's Confusion, and
`the_void_proc_pays_condition_overload_and_no_damage` pins both halves.

### Sources

- [`Extra_Hit`](https://wiki.warframe.com/w/Extra_Hit) — the general formula
- [`Xata's Whisper`](https://wiki.warframe.com/w/Xata%27s_Whisper) — EN card, and
  the Blast clause under Bugs
- 真理密语 (CN wiki, via the API) — three worked examples, the IPS-distribution
  rule and the body-part clause
- [`Damage/Void_Damage`](https://wiki.warframe.com/w/Damage/Void_Damage) — the
  Bullet Attractor's radius and duration
- the supplied capture above
