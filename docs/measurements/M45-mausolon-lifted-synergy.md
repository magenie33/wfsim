# M45 — the Mausolon's Lifted synergy, UNMODELLED AND UNMEASURED (2026-08-15)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The weapon's own loop, and the largest thing missing from its number. Its two
forms feed each other, which is the whole reason the cycle is worth simulating:

> `*Primary fire shoots fully automatic rounds.`
> `**Shots explode in a '1.8' meter radius on impact with a surface or enemy.`
> `**Damaging {{D|Lifted}} enemies causes up to 13 additional instances of direct hit damage.`
> `*Getting 5 kills with the Mausolon's primary fire will unlock an [[Alternate Fire]] that discharges a powerful laser that explodes on impact.`
> `**Shots explode in a '8' meter radius on impact with a surface or enemy.`
> `**Guaranteed {{D|Lifted}} proc.`
> `**After using Alternate Fire, additional kills are needed to recharge the laser.`
>
> — wiki `Mausolon`, raw wikitext, §Characteristics

So: the alt-fire lifts, and the primary then deals **up to 14x its direct
damage** into a lifted body. The status itself is modelled as of 2026-08-15
(`independent_procs: [lifted]`, 1 s, counted by Condition Overload); the extra
instances are **not**, and the weapon reads low for as long as the target is
lifted.

### What is missing, and why it is not a guess

**"Up to 13"** publishes a ceiling and no floor, and no rule for what decides
the count. The obvious reading is that the shot strikes a body repeatedly while
it floats, which makes the answer a function of where the body IS — and this
arena has no positions (docs/UNMODELLED.md §"no distance", §"no movement").
Writing 13 would put a 14x multiplier on the board that nobody can reproduce;
writing 1 would be inventing a floor. Neither is a measurement.

Mechanically it is an EXTRA HIT (docs/EXTRA_HIT.md): a second damage instance
beside a hit, worth a percentage of it. The machinery exists and only the
count and its trigger rule are unknown.

### What to measure

1. **Is the count fixed or does it vary?** Fire single shots into a lifted
   Grineer Lancer in the Simulacrum with the damage numbers on. Record the
   instances per trigger pull across ~20 shots. A constant 13 settles it in one
   session; a spread means it is positional and the honest model is a range.
2. **Is each instance the FULL direct hit?** Compare one instance's number
   against the same weapon's number on an unlifted target of the same unit and
   level. Extra Hit members supply a percentage, and this one's is unstated.
3. **Does the radial count, or only the direct?** The line says "direct hit
   damage", which reads as the 180 and not the 72 — worth confirming, because
   it decides whether Primary Compression touches it.
4. **Does it need the Mausolon's OWN Lifted?** Lift with a Warframe ability
   instead and fire. If it works, the synergy is not self-contained and the
   ability layer can feed it.

Until (1) and (2) land, the admission on `mausolon` stands and the board number
is a floor rather than an estimate.

### Sources

Wiki `Mausolon` §Characteristics (raw wikitext, transcribed above), and its
infobox — both columns of which were re-verified field by field on 2026-08-15
and agree with `data/weapons/archgun/mausolon*.yaml` exactly.
