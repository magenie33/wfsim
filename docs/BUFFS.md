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
  `data/debuffs/`, mirroring `data/buffs/`.
- The target-side pipeline layers read a **contribution snapshot from the
  target's `DebuffBar`** exactly like the weapon pipeline reads the player's
  `BuffBar`:
  armor reduction (Corrosive), damage-taken multipliers per pool (Viral →
  health, Magnetic → shields/overguard), slow / crit-received (Cold),
  Parazon threshold (Impact/Stagger), etc.
- **DoT debuffs** (Heat, Toxin, Slash, Gas, ...) are debuffs whose stacks
  emit damage events on the timeline (layer [8]) — each stack ticking on
  its own clock.
- **The provenance principle** (user, 2026-07-24): *every trigger has a
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

The plan: describe common perks as **data** (`data/perks/*.yaml` +
`data/buffs/*.yaml`) run by a single interpreter. Genuinely weird ones (e.g. a perk
that builds on an Arctic Eximus Snow Globe without resetting) get a hand-written
`impl Perk` behind the same trait. `secondary_enervate.rs` is currently a
hand-written reference perk; it will inform the declarative schema.

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
```

`engine::mods_data` maps the modeled `(trigger, grants)` combos to the buff
`ModEffect` variants at max rank (`OnKillMultishot`, `ConditionOverload`,
`OnHeadshotCritChance`, `OnHeadshotKillCritChance`); triggers not yet modeled
keep their uniform data and resolve to a no-op until the generic interpreter
below lands. Replaces the old ad-hoc `stacking_buff` / `on_headshot_*` kinds.

## Activation policy: assumed, configured, or emergent

A conditional/stacking buff can be evaluated under three policies
(recorded 2026-07-24; see [`OPTIMIZER.md`](OPTIMIZER.md) §3):

1. **`assumed_max` (default)** — full stacks, 100% uptime. Today's
   `LockedBuff` in `engine::dummy` (Frenzy locked, Fevered Frenzy at 20
   stacks) is the seed of this; it generalizes to a per-buff map.
2. **`configured`** — explicit per-buff stack counts/uptimes.
3. **`emergent` (future)** — no assumption; the timeline itself grants
   and decays stacks (on-kill grants, Galvanized one-stack decay,
   death-clears).

The policy changes results, so it is part of any evaluation cache key.

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
