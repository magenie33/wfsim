# Naming — what a variable is called, and why it is that long

> A name may be LONG, but it must have STRUCTURE and LOGIC. Never trade
> information away for brevity — that is what makes a codebase unmaintainable.
> — owner, 2026-08-20

This file is DERIVED, not invented. Every rule below was read off the code that
already existed, and the counts are the evidence for which spelling won. The
roster was ~250 weapons and ~1,100 data keys deep when it was written, which is
late enough that the patterns are real and early enough that fixing them cost
one afternoon.

## 1. The shape of a name

```
[scope_]<subject>_<aspect>[_<unit>]
```

| part | what it answers | examples |
| --- | --- | --- |
| `scope` | whose, or in what context | `base_`, `radial_`, `chain_`, `weakpoint_`, `bodyshot_` |
| `subject` | the domain noun | `crit`, `punch_through`, `falloff`, `reload`, `status` |
| `aspect` | which facet of it | `chance`, `multiplier`, `bonus`, `start`, `end`, `max` |
| `unit` | the physical unit, if it has one | `_m`, `_seconds`, `_deg`, `_mps`, `_pct` |

Read `falloff_start_m` as *the falloff's start, in metres*, and
`bodyshot_crit_chance_multiplier` as *on a body shot, the multiplier on crit
chance*. Both are long. Both can be read by someone who has never opened the
file, which is the whole point.

## 2. A UNIT IS PART OF THE NAME, and there is ONE spelling of each

| unit | suffix | never |
| --- | --- | --- |
| metres | `_m` | `_meters`, `_metres`, `_dist` |
| seconds | `_seconds` | `_s`, `_secs`, `_sec`, `_time` |
| degrees | `_deg` | `_degrees`, `_angle` |
| metres per second | `_mps` | `_speed` alone |
| a fraction of a whole | `_pct` | `_percent`, `_frac`, `_ratio` |

**METRES WAS ALREADY PERFECT and is the model the rest were made to match.** At
the survey, `_m` covered 13 data keys and 15 Rust fields with **zero**
exceptions, so a reader who sees a bare number knows it is not a distance.

**SECONDS WAS THE WORST**, with four spellings and — the part that actually
costs — three of them for ONE concept: `duration_s`, `duration_secs` and
`duration_seconds` all existed, in the same engine, meaning the same thing. 465
occurrences across 51 files were renamed to fix it.

### `_pct` is a FRACTION, 0..1

`energy_pct: 1.0` is a full pool, not one percent of one. The one exception is
the WIRE, where `headshot_pct` is 0..100 because that is what a person types
into a box — see §5.

## 3. A DIMENSIONLESS number still declares its ROLE

A ratio has no unit, so the aspect carries the meaning instead. One spelling
each, no abbreviations:

| role | suffix | means |
| --- | --- | --- |
| probability | `_chance` | 0..1, rolled |
| multiplicative | `_multiplier` | `x`, never `_mult` or `_mul` |
| additive fraction | `_bonus` | `+50%` is `0.5` |
| per second | `_rate` | `fire_rate`, `tick_rate` |
| a count | plain plural | `hops`, `pellets`, `stacks` |

`crit_mult` and `crit_multiplier` both existed, ten uses against eight. The
longer one won every time such a pair came up, and that is the tie-break rule:
**when two spellings mean one thing, the one that spells it out wins.**

## 4. Words are not abbreviated

`damage`, not `dmg`. `multiplier`, not `mult`. `seconds`, not `secs`.
`effectiveness`, not `eff`.

The exceptions are words the GAME abbreviates, which are domain vocabulary
rather than shortenings: `crit`, `co` (Condition Overload), `aoe`, `dps`, `mps`.
If DE writes it that way on a card, so do we.

## 5. Booleans are POSITIVE and say what they ask

`takes_multishot`, `is_head`, `can_be_eximus`, `has_reserve`.

Prefixes in use: `takes_` (does this part accept that bucket), `is_` (a property
of the thing), `can_` (a permission), `has_` (possession), `uses_`.

**NEVER A NEGATIVE.** `no_resupply` is the one survivor and it is on this list
as a defect: `if !no_resupply` is a double negative a reader has to unpick every
time. It stays only because it is in stored presets (§6).

## 6. THE WIRE AND STORED PRESETS ARE FROZEN

A field that travels in a saved preset, a share link or a board record is a
DURABLE NAME. Renaming it migrates every stored preset and invalidates every
share link ever posted, so those spellings stay as they are even where they
break a rule above — `wf_armor`, `wf_energy_pct`, `headshot_pct`, `no_resupply`.

This is the same rule `engine::builds::BUILD_AXES` already states for build
axes: the LIST is shared, the SPELLINGS are per-protocol. What this file governs
is everything else, which is almost everything.

## 7. The ratchet

`engine::naming` holds `forbidden_spellings_never_come_back`, which walks every
Rust field and every `data/` yaml key and refuses the spellings above. It
carries an EXEMPT list, and that list is the frozen wire names of §6 and nothing
else — so it can only shrink, and a new name cannot join it without someone
explaining why the name is durable.

Verified to bite: renaming one field back to `crit_mult` fails it by name.
