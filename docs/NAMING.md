# Naming — what a variable is called, and why it is that long

> A name may be LONG, but it must have STRUCTURE and LOGIC. Never trade
> information away for brevity — that is what makes a codebase unmaintainable.
>

This file is DERIVED, not invented. Every rule in §1–§7 was read off the code
that already existed, and the counts are the evidence for which spelling won.

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

**METRES IS THE MODEL the rest were made to match:** `_m` covered every
distance key and field with **zero** exceptions, so a reader who sees a bare
number knows it is not a distance.

**SECONDS HAD FOUR SPELLINGS**, three of them for ONE concept — `duration_s`,
`duration_secs` and `duration_seconds` meant the same thing. One spelling now.

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

## 4. Words are not abbreviated — AND TWO OF THEM MEANT TWO THINGS

A reader of `bd_eximus_expiry` has to already know that `bd` is base damage
before the name tells them anything at all.

**`ms` MEANT MULTISHOT AND MILLISECONDS.** One two-letter name, two units, one
codebase — `evo_ms` in the engine and `ms_per_run` in `one_fight`.

**`cc` MEANT CRIT CHANCE AND CROWD CONTROL.** `cc_on_headshot` is a crit chance;
`overguard_cc_immunity` is immunity to crowd control, because that is what
Overguard grants. The checker confidently proposed renaming the second one to
`overguard_crit_chance_immunity`, which would have been wrong and silent.

Neither survives. `engine::naming::ABBREVIATED` expands whole
underscore-separated parts, so `cd` is caught in `cd_rel` and left alone in
`cold_stacks`.

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

This is the same rule `engine::board::builds::BUILD_AXES` already states for build
axes: the LIST is shared, the SPELLINGS are per-protocol. What this file governs
is everything else, which is almost everything.

## 7. The ratchet

`engine::naming` holds `forbidden_spellings_never_come_back`, which walks every
Rust field and every `data/` yaml key and refuses the spellings above. It
carries an EXEMPT list, and that list is the frozen wire names of §6 and nothing
else — so it can only shrink, and a new name cannot join it without someone
explaining why the name is durable.

Verified to bite: a struct carrying `cc_on_kill`, `reload_secs` and `ms_bonus`
fails it naming all three, one per rule class.

**AND THE TABLE ITSELF IS GUARDED.** The bulk rename that expanded `ms`
everywhere also rewrote the checker's own row — `("ms", "multishot")` became
`("multishot", "multishot")` — after which every correct name was reported as
needing to become itself, 335 of them. `the_table_is_not_a_fixed_point` refuses
any rule whose two halves are equal, because that failure reads as a codebase
problem when it is a one-row checker problem.

## 8. AN INFRASTRUCTURE RESOURCE IS NAMED AT THE GRANULARITY IT IS PROVISIONED

One resource per kind — one worker, one bucket, one database — holds everything
of its kind for this product; the CONTENT is named one level down, as a table
or a prefix. A name describes the resource, not what happens to be in it. A
second resource of one type needs a boundary (a lifecycle, a write path, a blast
radius) and takes its name from it — `wfsim-staging`, not `wfsim-2`. Neither the
type nor the vendor is in the name: the type lives in the config key
(`D1_DATABASE_ID`, `R2_BUCKET`). Hyphens, not underscores, because an object
store's name must be DNS-compatible. An environment joins the name only when a
second one exists.

## 9. A TIME IS STORED ONE WAY

| what | stored as | column |
| --- | --- | --- |
| an instant | UTC ISO 8601 to the millisecond, `new Date().toISOString()` — `2026-10-09T06:07:00.370Z` | `<event>_at`, `at`, `<event>_until` |
| a calendar day | `YYYY-MM-DD`, UTC | `day` |
| an hour | `YYYY-MM-DDTHH`, UTC — a prefix of the instant, so it compares with one | `hour` |
| a duration | a number with its unit | `_ms`, `_seconds` |

ONE FORMAT, because the stores compare times as strings: `…:31Z` sorts after
`…:31.000Z` of the same second, and an integer sorts before every string, so
two formats in one comparison are wrong in silence. A day is not a truncated
instant: it is for a count by the day, and for the anonymous store, which keeps
the day and nothing finer (docs/BOARD.md) and holds it under the frozen name
`at`. A wire value may be another form — a sync cursor travels as a number —
and converts at the boundary. `scripts/check_time_storage.mjs` refuses the
rest; its exempt lists name the columns not on the standard yet, and only
shrink.

---

## A name may be long; it may not be vague

**A NAME MAY BE LONG; IT MAY NOT BE VAGUE.** `docs/NAMING.md` is the
convention and `engine::naming` enforces it. The shape is
`[scope_]<subject>_<aspect>[_<unit>]` — `falloff_start_m` reads as "the
falloff's start, in metres" to someone who has never opened the file. Never
trade information away for brevity.

A UNIT IS PART OF THE NAME and has ONE spelling: `_m`, `_seconds`, `_deg`,
`_mps`, `_pct`. A dimensionless number declares its ROLE instead — `_chance`,
`_multiplier`, `_bonus`, `_rate`. Words are not abbreviated (`damage` not
`dmg`) except where DE abbreviates them on a card (`crit`, `co`, `aoe`, `dps`).

WHAT IS FROZEN IS THE WIRE. A field inside a saved preset, a share link or a
board record is a durable name and stays as it is — `wf_armor`,
`wf_energy_pct`, `headshot_pct`, `no_resupply` — the same rule
`board::builds::BUILD_AXES` states for axes. `naming::FROZEN` is that list and it may
only SHRINK.
The ratchet walks every yaml key and every engine field rather than a list of
names. A ratchet that cannot fail is not a ratchet; prove it bites.
