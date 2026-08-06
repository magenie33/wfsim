# wfsim — Buff System Architecture

How the simulator models **buffs** — the runtime states you *gain* during a
fight (arcane stacks, weapon passives like Frenzy, ability effects, conditional
mods, combo). There are many and some are complex (stacking, resets, durations,
rate caps, feedback loops). Terminology follows [`GLOSSARY.md`](GLOSSARY.md).

## The model: you gain a buff, overlaid on a target

A **buff** is not a static weapon property. When a trigger condition fires, you
**gain a buff** — a live overlay with stacks and (optionally) a duration, shown
in the HUD **buff bar**. Examples: landing a headshot grants Dual Toxocyst's
**Frenzy** buff on that weapon; hitting with Secondary Enervate equipped grows
its **stacking** buff. The arcane and the passive are both *sources* that grant
buffs.

Three concepts, kept distinct:

- **Buff** — the runtime overlay: `{ id, scope, stacks, expiry, contributions }`.
  It is what the buff bar displays.
- **Buff bar** (`BuffBar`) — the single container of all active buffs, mirroring
  the player's HUD. A buff appears here regardless of scope.
- **Perk** (`Perk`) — the held/equipped grantor (arcane / weapon passive /
  Incarnon evolution) that, on a trigger event, applies / refreshes / resets its
  buff in the bar. Holding the perk is what enables the buff. The perk keeps
  private bookkeeping (rate-limit timers, counters) the UI does not show; the
  buff holds the visible stacks/duration.

**Scope.** A buff's `BuffScope` (Weapon / Warframe / Companion / Companion
weapon / Squad — extensible) decides where its contributions actually apply. The
HUD shows every buff regardless of scope, so "shown in the UI" ≠ "applies to this
weapon". We track the precise target and can display **more finely than the game
HUD**. Frenzy and Secondary Enervate are `Weapon`-scoped.

**Same name, two things.** A perk and the buff it grants often share a name — the
*Frenzy perk* (Dual Toxocyst's passive) grants the *Frenzy buff*. Keep them
distinct in speech and code.

## Debuffs: the same machinery, pointed at the target

Status effects (procs) are **not** a separate system. Per the core
philosophy (2026-07-24): *a proc is only a trigger event; the entity is a
**debuff** applied onto the target*, symmetric to perks granting buffs:

```
player side:  Perk   --trigger-->  Buff    in the player's BuffBar
enemy  side:  proc   --trigger-->  Debuff  in the target's DebuffBar
```

The container on the target is named **`DebuffBar`** (decision 2026-07-24)
— same machinery as `BuffBar` (stack tracking, per-stack expiry,
contribution snapshots), rendered in the arena UI as the enemy's status
icon row.

**Every actor carries both bars** (decision 2026-07-24) — the structure is
symmetric across sides; only the contents differ:

|            | BuffBar (gained boons)                          | DebuffBar (suffered afflictions) |
|------------|--------------------------------------------------|----------------------------------|
| player     | arcane stacks, Frenzy, ability buffs             | enemy procs on the player (Magnetic drain, Toxin DoT, Heat armor strip) |
| enemy      | Ancient Healer 90% DR aura, Guardian Eximus overguard regrant, Shield Osprey shields, Eximus auras | our procs: Stagger, Corrosive strip, Viral, DoTs |

Mitigation and damage layers on *either* side read the same two snapshots:
the attacker's `BuffBar` and the defender's `DebuffBar` (+ the defender's
`BuffBar` for protective auras like the Healer's DR).

- A **debuff** has the same shape as a buff: stacks, per-stack duration,
  overflow policy (e.g. Stagger: 5 stacks, 6 s each, 6th proc replaces the
  oldest), per-stack modifiers, caps, conditions. Stored in
  `data/debuffs/` as standalone files — unlike player-side buffs (inlined at
  their granting item), a debuff is shared by EVERY source of its status
  type, so it keeps its own file.
- The target-side pipeline layers read a **contribution snapshot from the
  target's `DebuffBar`** exactly like the weapon pipeline reads the player's
  `BuffBar`:
  armor reduction (Corrosive), damage-taken multipliers per pool (Viral →
  health, Magnetic → shields/overguard), slow / crit-received (Cold),
  Parazon threshold (Impact/Stagger), etc.
- **DoT debuffs** (Heat, Toxin, Slash, Gas, ...) are debuffs whose stacks
  emit damage events on the timeline (layer [8]) — each stack ticking on
  its own clock.
- **The provenance principle** (2026-07-24): *every trigger has a
  source, and every debuff instance records its full applier context.*
  A stack = `{ application timestamp (the FIFO key), applier context
  snapshot (the hit-formula inputs), expiry, payload values }`. This is
  what keeps 4 players × 100 weapons on one target coherent: each Bleed
  ticks at its own value, each Magnetic stack's break-chunk reads its own
  mods, FIFO replacement swaps whole instances with their provenance.
  System-issued effects derive their context from instances (per-stack
  sum — Blast radial, expected for the Magnetic break-proc) or from a
  designated trigger instance (Frozen's reset stacks), never from thin
  air. Heat's singleton accumulator is the lone exception with a single
  shared context slot (locked to its first proc).
- CC components (stagger, knockdown) are debuff properties with per-unit
  immunities (Ospreys/Bosses/Tenno ignore Stagger's CC while still
  carrying its stacks); Overguard grants blanket CC immunity.

## The core split

The 8-layer damage pipeline ([`CORE.md`](CORE.md) §3) is a **pure function**:
given a snapshot of the current modifier state, it computes one Hit's damage.
Statefulness lives in the timeline (layer [8]):

```
timeline loop (layer [8]) — stateful, single seeded RNG, fixed tick rate
   │  emits typed Events (Hit{big_crit}, Kill, Headshot, Reload, Tick, ...)
   ▼
Perks react → apply/refresh/reset their Buff in the BuffBar
   │  (BuffBar also expires duration-based buffs as time advances)
   ▼
BuffBar.total_contributions() → a modifier-state snapshot
   │
   ▼
pure pipeline [1]–[7] computes this Hit from the snapshot (never mutates buffs)
```

## `Perk` and `Buff`

```rust
trait Perk {
    fn id(&self) -> &str;
    fn on_event(&mut self, event: &Event, t_secs: f64, bar: &mut BuffBar);
}

struct Buff { id, scope, stacks, expiry_secs: Option<f64>, contributions }
```

`Contributions` has one field per **additive** bucket (currently
`flat_crit_chance`; grows as buffs land). Multiplicative buckets (e.g. an
independent fire-rate multiplier) are *not* summed here — the mod-resolution
layer (layer [1]) combines buckets by their real rules. `expiry_secs = None`
means "until reset/removed" (Secondary Enervate); `Some(t)` means a timed buff
(Frenzy's 3 s).

## Declarative first, coded escape-hatch second

Most perks reduce to a small set of primitives:

```
trigger    (OnHit / OnKill / OnHeadshot / OnBigCrit / Periodic / ...)
accumulator(per-stack value / max stacks / duration / refresh / decay /
            rate cap Hz / reset condition)
contribution(which bucket: flat crit chance / crit chance multiplier /
            fire-rate multiplier / injected element / ...)
scope      (Weapon / Warframe / Squad)
```

**Where the data lives (decision 2026-07-27): INLINE at the source.** A
perk+buff pair is written as ONE `kind: buff` block inside the yaml of the
thing that grants it — the mod, the arcane, the weapon (its `passives:`).
There are no standalone `data/perks/` / `data/buffs/` files: a triggered
buff is 1:1 with its granter, so the pair is declared together (the block's
trigger/condition IS the perk; the rest IS the buff). The block may carry a
`perk: <id>` field when the mechanic needs a hand-written stateful
implementation (`engine::perks::<id>` behind the `impl Perk` trait) — the
data still records the trigger, values, and wiki-verified boundary notes;
the code implements the state machine (Enervate's ramp/reset, Frenzy's
multiplicative fire rate + injection). A buff would GRADUATE back to a
standalone file only when it stops being 1:1 — shared by several granters,
or granted by a non-item source (abilities, team buffs) — neither exists in
the pool today.

## Mod data: triggered effects are `kind: buff`

A mod's conditional/triggered effect is written as one declarative buff (the mod
is the perk that grants it). UNCONDITIONAL modifiers stay as their plain bucket
(`base_damage_bonus`, `multishot_bonus`, …); only triggered ones use `buff`.
A "permanent + triggered" mod (the Galvanized family) is simply BOTH — a plain
bucket effect **and** a `kind: buff` effect.

```yaml
- kind: buff
  trigger: on_kill        # on_kill | on_headshot | on_headshot_kill | on_ability_cast | on_reload | on_hit | passive
  condition: while_aiming # optional
  grants: multishot       # the bucket it feeds (multishot | condition_overload | crit_chance | crit_damage | status_chance | fire_rate | accuracy | …)
  rank0: 0.027            # per-stack value at rank 0
  rankMax: 0.30           # per-stack value at max rank
  max_stacks: 4           # 1 for a non-stacking triggered buff
  duration: 20            # seconds; omit = until reset
  decay: lose_one_and_reset   # lose_one_and_reset | per_stack_expiry | all_drop

### The decay families, and which are real

`lose_one_and_reset` is the Galvanized rule and was for a long time the only
TIMED one implemented — so every stacking buff decayed that way whether or not
it was its rule. `all_drop` is the on-status arcane family (Cascadia Flare).

`per_stack_expiry` became real on 2026-08-07: each stack keeps its OWN clock and
expires on it, oldest first. Stormburst is the first perk that needed it (owner,
observed in game: "3个层走FIFO，每个2s，上限就3层"), and at the cap a new stack
evicts the oldest rather than being dropped.

**The difference is not cosmetic.** Under `lose_one_and_reset` a single hit
inside the window refreshes the whole pile, so one hit every 2 s holds three
stacks. Under `per_stack_expiry` that same hit holds exactly one — sustaining
three needs three hits per window. On the Furis that moved Stormburst's extra
pellets from 11 to 7 over the same engagement.
  one_stack_per_instance: true  # optional, arcanes: cap the GRANT (see below)
```

### `one_stack_per_instance:` — capping the grant, not the decay

`decay:` says how stacks LEAVE; this says how fast they ARRIVE. Default false =
one stack per trigger event, so a status-triggered buff gains one per PROC, and
a multishot volley that procs on five pellets grants five.

Cascadia Flare states otherwise and is the only entry in its family that does —
verbatim: *"Only one stack can be added per damage instance; applying multiple
Heat status effects, such as via Multishot or Archon Vitality in a single hit
will not generate multiple stacks."* So the instance is the **trigger pull**,
not the pellet and not the proc: `ArcRuntime::next_instance` opens one per pull,
and separately one per lingering-field tick and per syndicate blast, because
those are their own instances at their own times.

Measured on a 2.8x-multishot Laetum Incarnon: 40 stacks in **3.0 s** per proc
against **4.8 s** per pull. Over a 120 s benchmark the DPS barely moves — the
ceiling is reached either way and then held — so this is a RAMP correction, and
it is worth having exactly where a fight is short or a build is thin.

The three other 40-stack on-status arcanes (Primary Blight, Primary Frostbite,
Conjunction Voltage) do NOT carry the flag: their pages do not state the rule,
and absence is not evidence of it. `arcanes_data` asserts them false so a
copy-paste cannot spread it quietly.

### `condition:` — the state that has to hold

`condition:` gates ANY effect, not only a `kind: buff` one: put it on a plain
bucket and the bucket only counts while the state holds.

Every value names a state of the fight's TENNO (`data/tenno/`), and they all
resolve to `ModEffect::WhileTenno(TennoCondition, …)`, which
`loadout::resolve_for` evaluates against the Tenno it was handed:

| value | `TennoState` field | means |
|---|---|---|
| `while_aiming` | `aiming` | aiming down sights — Galvanized Crosshairs / Scope, Argon Scope, … |
| `while_invisible` | `invisible` | Spectral Serration's "+330% Damage while Invisible" |
| `while_airborne` | `airborne` | the Aero set |

`while_aiming` is one of these rather than a case beside them: it was a bool
threaded through the resolver while the other states lived on the Tenno, which
is two homes for one kind of fact (user, 2026-08-02).

The neutral Tenno is aiming and doing nothing else, so a while-Invisible mod
contributes nothing until a scenario says otherwise — and the panel labels the
row with the condition rather than hiding it. An unrecognised `condition:`
gates NOTHING, which `mods_data`'s card-vs-model test catches as "the card
states a condition and the model has none".

## Arcane data: a Warframe stat is `kind: tenno_scaled`

An arcane whose value comes from the PLAYER rather than from anything the
weapon does reads it off the fight's Tenno:

```yaml
- kind: tenno_scaled
  stat: armor           # armor | max_energy
  above: 1000           # the first N units pay nothing (default 0)
  per_unit: 0.01        # bonus per unit past `above`
  min_energy_pct: 0.9   # optional gate: at or above this fraction of the pool
  grants: base_damage   # base_damage | multishot — the bucket it joins
  rank0: 2.5            # the CAP at rank 0
  rankMax: 5.0          # the CAP at max rank
```

It resolves to a passive one-stack `ArcBuffSpec` (`ArcTrigger::Passive`,
pinned): a Warframe stat does not decay mid-fight and no event grants it, so it
rides the grant machinery the on-kill arcanes already feed correctly instead of
adding a static bucket to the damage path. A bonus of zero produces NO buff at
all — a zero-value stack would still list in the picker and invite someone to
turn it up.

`engine::mods_data` maps the modeled `(trigger, grants)` combos to the buff
`ModEffect` variants at max rank (`OnKillMultishot`, `ConditionOverload`,
`OnHeadshotCritChance`, `OnHeadshotKillCritChance`); triggers not yet modeled
keep their uniform data and resolve to a no-op until the generic interpreter
below lands. Replaces the old ad-hoc `stacking_buff` / `on_headshot_*` kinds.

## Activation policy: what a buff is worth at t = 0

A conditional/stacking buff can be evaluated under three policies
(recorded 2026-07-24; see [`OPTIMIZER.md`](OPTIMIZER.md) §3):

1. **`assumed_max`** — full stacks, 100% uptime. What the **PANEL** shows: a
   build's ceiling, which is the question the panel answers.
2. **`configured`** — explicit per-buff stack counts/uptimes (the buff cards).
3. **`emergent`** — the timeline grants and decays stacks itself. What the
   **SIM** runs.

The policy changes results, so it is part of any evaluation cache key.

### "No timeout" OVERWRITES the duration

The buff card carries two knobs, and they answer different questions:

- **stacks** — what the count is at t = 0;
- **no timeout** — whether a stack can ever expire.

Locking removes the **expiry and nothing else** (user, 2026-08-02). The count
still starts where the card sets it and still climbs on every trigger.

**It is not a flag. It is the duration** (user, 2026-08-04). `apply_buff_config`
writes `loadout::NO_TIMEOUT` (`f64::INFINITY`) into the buff's own `duration`,
and that is the entire implementation — every clock in the sim is
`expiry = now + duration`, so an infinite duration is a buff that earns
normally and never falls off. Nothing downstream knows the concept exists:
there is no `pinned`/`locked` field on any buff spec any more, and no read site
has a branch for it.

That shape is the point. The flag it replaced had to be honoured wherever a
stack count was read, and it was missed at enough of them that "no timeout"
came to mean its own opposite — the stacks decayed anyway while the trigger was
skipped, so a locked buff decayed to zero and could never come back. It was
wrong in three of the five families at once (Galvanized on-kill stacks, Lethal
Rearmament, Overwhelming Attrition), and a player reported the worst of them
(2026-08-03: 选无限持续后直接不生效). A duration cannot be forgotten, because
it is the thing the clocks already read.

Two consequences worth knowing:

- **`AssumedMax` is `initial_stacks = max` plus `NO_TIMEOUT`** — "full, and
  nothing can take it away", said once instead of as a second flag.
- **A buff with no clock states it as `NO_TIMEOUT`, never as `0.0`.** The
  `tenno_scaled` arcanes (a Warframe stat does not decay mid-fight) carried
  `duration: 0.0` beside the flag; a zero duration in a decay loop is an
  infinite loop waiting for a reader.

### A buff card is THREE lists, and they have to agree

- `DummyParams::buff_roster` — what exists in the run;
- `enumerate_buffs` / `evo_buffs` (webapi) — what is drawn as a card;
- `DummyParams::apply_buff_config` — what the run actually OBEYS.

Deadly Efficiency was in the first two and missing from the third, so its card
was drawn, set, and dropped — for as long as it had existed. Nothing in the UI
could show that: a knob that does nothing looks exactly like a knob whose buff
is not currently up. Its clock was seeded from a literal `0.0` too, where its
three siblings seed from their card.

`every_buff_the_roster_offers_is_actually_read` now closes it generically: it
sets one roster id at a time and asserts the params CHANGED, so a buff added
later is covered without anyone remembering this note. The one exemption is
`frenzy`, applied outside these params (the api builds a `locked_buffs` entry).

### Every timed buff starts EARNED, at zero

**A buff starts full only if it is neither timed NOR consumable. Everything
else starts at 0.**

- **timed** — it has a duration, so a lull empties it;
- **consumable** — it is SPENT by being used, whatever its duration says. A
  "next shot deals X" buff is the clear case, and DEACTIVATION counts as
  consumption (user, 2026-08-03). An infinite duration does not save it: the
  question is whether the fight can hand it to you at t = 0, and a buff you
  have already spent is not one you are holding.

The `permanent` flag on a buff card is exactly this distinction, and it is the
only input the rule takes.

**Secondary Enervate (次要·失活) is the worked example of the consumable half.**
Untimed and UNCAPPED — a hit adds a stack of +10 flat crit chance with no
ceiling — but a big crit wipes the pile, so it starts at 0 like everything else
that can be spent. It lives in a PERK rather than in `arcane.buffs`, which is
why it had no card at all until 2026-08-03: the arcane whose entire point is a
stack count was the one you could not set. `BuffMeta.uncapped` says there is no
maximum, the card shows `/ ∞` and its input takes no `max`, and
`DummyParams::enervate_stacks` carries the configured pile into the run. In the whole data set today, exactly one buff
qualifies: **Fevered Frenzy** (the Dual Toxocyst evolution).

The modelled fight is therefore: *you have been at it a while, but you have not
been in contact for the last few seconds and are about to be.* Whatever
survives a lull is up; whatever expires in one is not (user, 2026-08-02).

This REPLACES the 2026-08-02 decision that every buff starts full. That one was
made to keep the buff cards uniform, and uniform they stay — the number they
open on is what changed. The reason it had to change is not the scenario, it is
that the old default asserted stacks the fight could not produce:

| target | full-start | zero-start | apart |
|---|---|---|---|
| Lv 30 | 524.80 | 520.00 | **0.9%** |
| Lv 100 | 58.81 | 49.94 | 15% |
| Lv 300 | 38.82 | 28.63 | 26% |
| Lv 1000 | 22.80 | 7.95 | **65%** |
| Lv 9999 SP | 4.87 | 1.95 | **60%** |

(Torid, Galvanized Chamber + Galvanized Aptitude + Primary Deadhead, 300 s,
60 runs, KPM.) Where kills are fast the seed is irrelevant — the fight rebuilds
the stacks in seconds and the two answers converge to under a percent. Where
kills are slow it is *everything*: at Lv 9999 you kill twice a minute, so
on-kill stacks never accumulate, and seeding them full handed the build a buff
it can never earn and then held it there for five minutes. Zero-start is not a
more pessimistic guess in that case; it is the correct one.

Which buffs stay full, in the whole data set: **Fevered Frenzy** (`on_ability_cast`,
20 stacks, no duration, not consumed). That is the list. Everything else — every Galvanized
mod, every on-kill/on-status arcane, Argon Scope, Sharpened Bullets, and the
Dual Toxocyst's own **Frenzy** passive — has a 3–30 s timer and is earned.

Two things carry no duration but are NOT permanent, and must never be treated
as such by inferring the rule from `duration == 0`: **Secondary Enervate** (a
ramp/reset perk) and **Secondary Surge** (affects the next shot only). They end
by their own rule, not by a timer.

`tenno_scaled` arcanes (Primary Bulwark, Primary Overcharge) are not buff cards
at all. Their value is a Warframe stat, not a stack anyone earns or loses; they
ride the buff machinery to reach their bucket and that is an implementation
detail. Their control is WF Armor / WF Energy in the Tenno block.

## What actually guarantees correctness

The architecture does not "prove" a buff right — it makes each perk
**isolated, deterministic, and individually testable** so tests can pin a bug to
one perk. Correctness comes from tests at two levels:

1. **Perk state-machine tests** — pure logic, no game needed: e.g. "buff resets
   after N big crits", "stack gain capped at 30/s", "duration buff expires".
2. **Golden tests vs Simulacrum** — the north star. Full sim with the buff active
   vs a recorded in-game damage-vs-time trace, matched within rounding. A
   mismatch means the data or engine gets corrected in place.

Supporting rules:

- **Determinism** — one seeded RNG threaded through the whole sim, fixed tick
  rate; no ambient randomness. Makes Monte Carlo reproducible and golden tests
  stable. (Critical here because random big crits can *feed back* into a buff's
  own reset.)
- **Trace output** — the sim can emit a per-event log (buff bar over time, crit
  tiers, per-bucket contributions) to diff against an in-game trace and localize
  any mismatch. You cannot align what you cannot inspect.
