# M58 — a status tick's accumulator starts at 1, not at 0 ✅ (owner, 2026-08-23)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The answer to the 36/35 M56 left open, and it is not a coefficient, a rounding
rule or a per-weapon quirk. `Damage/Calculation` §Damage Over Time states it:

> "For weapon-generated Heat, Electricity, Toxin, Gas, and Slash status
> effects, **the temporary damage accumulator for each tick group starts at 1
> rather than 0**. The full-precision damage seeds are then added to this
> accumulator before the status coefficient and the remaining applicable
> multipliers are applied."

```
Unrounded Tick Damage = (Σ Sᵢ + 1) × C × M
```

`Sᵢ` is each stored damage seed, `C` is 0.5 (Heat, Electricity, Toxin, Gas) or
0.35 (Slash), and `M` is the elemental, faction and status-damage bonuses. On a
Braton Prime, base 35, that is `(35 + 1) × 0.5 = 18` per tick, and it reproduces
all nine of M56's readings:

| bracket | `35 × 0.5 × b` | `(35 + 1) × 0.5 × b` | measured |
| --- | --- | --- | --- |
| 1.0 | 17.5 | **18** | 18 |
| 1.9 | 33.25 | **34.2** | 34 |
| 3.0 | 52.5 | **54** | 54 |

### Three things it is not, all stated on the page

**Not a flat +1 of damage, and not once per stack.** *"If several seeds are
consolidated into a single tick, they are added to the same accumulator, so its
initial value of 1 is included only once. It is therefore neither a final flat
+1 damage bonus nor a bonus applied once per status stack."* So Heat,
Electricity and Gas — the families that share a clock — count it ONCE per tick
however many stacks fold in, while Slash and Toxin tick independently and each
carries its own.

**Not on every status.** The list is five, and Blast is not in it. **M56's own
capture proves that from the other side**: a detonation read 11 / 21 / 63 across
body, crit and critical headshot, which is `0.3 × 35` times 1 / 2 / 6 exactly.
With an accumulator the crit line would be `0.3 × 36 × 2 = 21.6`, displayed 22.

**Not outside the faction double dip, and not inside it twice.** The page's own
Toxin example is `(40 × 1.55 + 1) × 0.5 × 3.25 × 1.55` — the faction bonus
inside the seed AND in `M`, with the `1` added between them. So it takes exactly
one of the two layers, and a Roar'd bleed is no longer exactly `f²`: at base 100
it is 2.2446 rather than 2.25, approaching 2.25 as the seed grows. Eclipse stays
exactly ×3 at any base, because a FINAL multiplier scales the accumulator and
the seed alike.

### Why a base-35 rifle was needed to see it

The `1` is worth 0.5 damage before multipliers: **2.9% on a base of 35, 0.25% on
a base of 400**. Every fixture this engine had was above the noise floor of its
own tolerances, and the wiki's DoT examples are all on a seed of 40 where the
absolute figures are printed and the ratio is not. It took a small gun and nine
readings across three brackets.

### The harness could not have caught it — and now can

`one_fight`, the cost-and-answer baseline, reported all three shapes **unmoved
to fifteen digits** — including with the accumulator scaled by a thousand,
which is what turned "too small to see" into "never executed". Its default
build is Hellfire + Cryo Rounds and Infected Clip + Stormbringer, which is
BLAST and CORROSIVE: a detonation and an armour strip, and not one of the five
damaging burns. So it ticked no status DoT at all, while its own comment
claimed it exercised them.

The trap underneath is that `dot_damage` is **not** a proxy for "a burn
ticked": that bucket also holds Blast detonations and area hits, so the Torid
reported 29,001 of it with not one burn in the fight. `RunResult::dot_ticks`
already counted the right thing and was never reported anywhere;
`Summary::mean_dot_ticks` is that counter, surfaced.

Fixed twice over. A **fourth shape** — the Braton Prime, 60% Slash, and a
physical type is the one thing an elemental mod cannot combine away — burns
under the unchanged default build, 507.6 ticks a run against zero for the other
three. And the tool now **fails when the whole suite ticks nothing**, so the
next edit to the mod list or the weapon list cannot silently undo it. The mod
list itself is untouched: it is what every saved baseline was measured under.
Both halves verified to bite, and so is the fleet merge carrying the new
counter — a shard that dropped it would report zero burns for a fight full of
them, which is the guard firing on a working engine.

Also noted on the same page and NOT implemented: *"intermediate DoT operations
use binary32 arithmetic"*. Its effect is below the resolution of anything
measured here.
