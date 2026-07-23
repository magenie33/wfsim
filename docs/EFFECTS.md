# wfsim — Effect System Architecture

How the simulator correctly implements **stateful, event-driven modifiers** —
arcanes, conditional mods, combo — of which there are many and some are complex
(stacking, resets, rate caps, feedback loops). Terminology here follows
[`GLOSSARY.md`](GLOSSARY.md).

## The core split

The 8-layer damage pipeline ([`CORE.md`](CORE.md) §3) is a **pure function**:
given a snapshot of the current modifier state, it computes one Hit's damage.
Statefulness does **not** live in the pipeline. It lives in the timeline
(layer [8]) as an event loop:

```
timeline loop (layer [8]) — stateful, single seeded RNG, fixed tick rate
   │  emits typed Events (Hit{big_crit}, Kill, Headshot, Reload, Tick, ...)
   ▼
Effects subscribe → each updates its own local state (stacks/timers/cooldowns)
   │
   ▼
sum every Effect's Contributions → a modifier-state snapshot
   │
   ▼
pure pipeline [1]–[7] computes this Hit from the snapshot (never mutates Effects)
```

Each Effect is isolated: its state is private, and the pipeline only ever reads
summed `Contributions`.

## The `Effect` trait

```rust
trait Effect {
    fn id(&self) -> &str;
    fn on_event(&mut self, event: &Event, t_secs: f64);
    fn contributions(&self) -> Contributions;
}
```

`Contributions` has one field per modifier bucket the effects layer can touch
(currently `flat_critical_chance`; grows as effects land). Values are additive
within a bucket; mod resolution (layer [1]) combines buckets.

## Declarative first, coded escape-hatch second

Most effects reduce to a small set of primitives:

```
trigger    (OnHit / OnKill / OnHeadshotKill / OnBigCrit / Periodic / ...)
accumulator(per-stack value / max stacks / duration / refresh / decay /
            rate cap Hz / reset condition)
contribution(which bucket: flat crit chance / crit chance multiplier /
            damage / status chance / ...)
```

The plan: describe common effects as **data** (see
`data/arcanes/*.json`) run by a single interpreter. The genuinely weird ones
(e.g. an effect that builds on an Arctic Eximus Snow Globe without resetting)
get a hand-written `impl Effect` behind the same trait — invisible to the
pipeline. `secondary_enervate.rs` is currently a hand-written reference impl; it
will inform the declarative schema.

## What actually guarantees correctness

The architecture does not "prove" an effect right — it makes each effect
**isolated, deterministic, and individually testable** so tests can pin a bug to
one effect. Correctness comes from tests at two levels:

1. **Effect state-machine tests** — pure logic, no game needed: e.g. "stacks
   reset after N big crits", "stack gain capped at 30/s". These lock *behavior*.
2. **Golden tests vs Simulacrum** — the north star. Full sim with the effect
   equipped vs a recorded in-game damage-vs-time trace, matched within rounding.
   Only these move an effect's `verification.status` from `unverified` to
   `verified`.

Supporting rules:

- **Determinism** — one seeded RNG threaded through the whole sim, fixed tick
  rate; no ambient randomness. Makes Monte Carlo reproducible and golden tests
  stable. (Critical here because random big crits can *feed back* into an
  effect's own reset.)
- **`verification.status` is tracked data**, mirrored from the data files, so a
  build using unverified effects can be flagged rather than silently trusted.
- **Trace output** — the sim can emit a per-event log (stacks over time, crit
  tiers, per-bucket contributions) to diff against an in-game trace and localize
  any mismatch. You cannot align what you cannot inspect.
