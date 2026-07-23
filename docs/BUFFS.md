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

**Scope.** A buff's `BuffScope` (Weapon / Warframe / Squad) decides where its
contributions actually apply — the HUD shows every buff regardless of scope, so
"shown in the UI" ≠ "applies to this weapon". Frenzy and Secondary Enervate are
`Weapon`-scoped.

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
`flat_critical_chance`; grows as buffs land). Multiplicative buckets (e.g. an
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

The plan: describe common perks as **data** (`data/arcanes/*.json`, weapon
`innate_effects`) run by a single interpreter. Genuinely weird ones (e.g. a perk
that builds on an Arctic Eximus Snow Globe without resetting) get a hand-written
`impl Perk` behind the same trait. `secondary_enervate.rs` is currently a
hand-written reference perk; it will inform the declarative schema.

## What actually guarantees correctness

The architecture does not "prove" a buff right — it makes each perk
**isolated, deterministic, and individually testable** so tests can pin a bug to
one perk. Correctness comes from tests at two levels:

1. **Perk state-machine tests** — pure logic, no game needed: e.g. "buff resets
   after N big crits", "stack gain capped at 30/s", "duration buff expires".
2. **Golden tests vs Simulacrum** — the north star. Full sim with the buff active
   vs a recorded in-game damage-vs-time trace, matched within rounding. Only
   these move a perk's `verification.status` from `unverified` to `verified`.

Supporting rules:

- **Determinism** — one seeded RNG threaded through the whole sim, fixed tick
  rate; no ambient randomness. Makes Monte Carlo reproducible and golden tests
  stable. (Critical here because random big crits can *feed back* into a buff's
  own reset.)
- **`verification.status` is tracked data**, mirrored from the data files, so a
  build using unverified buffs can be flagged rather than silently trusted.
- **Trace output** — the sim can emit a per-event log (buff bar over time, crit
  tiers, per-bucket contributions) to diff against an in-game trace and localize
  any mismatch. You cannot align what you cannot inspect.
