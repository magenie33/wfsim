# M8 — Magnetic break damage forces ordinary Electricity ✅ (owner, 2026-11-12)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Breaking shields or Overguard under Magnetic creates an Electricity damage
packet with a separate forced Electricity proc. The current Magnetic count
and broken pool maximum supply its base; the breaking weapon supplies its
modifier context. Earlier Magnetic appliers do not supply that context.

The owner confirms the two-stage status-damage rule: the break packet receives
status damage, and the ordinary proc inherits that scaled source and receives
status damage again. For a common multiplier `S`, the new DoT contribution
therefore contains `S²`. Existing Electricity seeds are unchanged. The proc
inherits the source amount, not the parent's mitigated health damage.

The packet deals body damage independently of the proc. Electricity immunity
can refuse the forced proc while the packet still deals damage. A packet kill
earns ordinary kill effects once, clears the dead body's statuses and does not
seed the fresh spawn or count as a weak-point kill.

The forced proc uses ordinary Electricity: element percentage, faction/Roar,
status duration, area propagation, shared accumulator and the latest-application
landing. It joins an existing pile's clock. Only a fresh inactive pile starts
its first tick immediately. A body application moves the pile to the body;
a later head Electricity application restores its head landing and lethal
weak-point classification (M100).

## Client evidence

Batch `20261112T112832Z-magnetic-break-client-only`, manifest SHA-256
`330502e31d60adb9e78a0b3397a7069482ea34c9afb19712561f2a4256324eba`,
binds client `2026.10.28.09.54`, executable SHA-256
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.
The receiver verified its four payloads and 468 archive members. Its 211 bounded
original-byte cases are producer results, not a complete replay of a live fight.

The effective child coefficients are float32 `0.005` (`0x3ba3d70a`), overriding
the native constructor's `0.03` default. The pool preparation is:

```
Q = trunc_i32(f32(f32(maximum_pool * f32(0.005)) * current_Magnetic_count))
```

`rules::status::magnetic_break_source` matches 28 attainable-count Overguard
vectors from that batch. Normal Electricity applies its own `0.5` coefficient
to the inherited source. A separate synthetic normalized seed 6909 produces
`3454.5 → 3455` and, with +165% Electricity, `9154.42578125 → 9154`.
This consistency case does not establish the owner's live source seed.

The native producer also conditionally divides by a weapon/proxy weakness
factor and rounds before constructing the packet. Its live field bindings are
not reconstructed by the simulator; [UNMODELLED.md](../UNMODELLED.md) records
that boundary. Native status-upgrade operation state remains partial. The
status-damage square is the owner's confirmed integration rule, also stated
by the Magnetic wiki; it is not claimed as a completed native multiplier trace.

## Gameplay controls

The owner confirms breaking-weapon attribution by comparing a weapon with
status duration against another without duration but with an Electricity mod.
The proc properties follow the weapon that breaks the pool. Exact durations,
weapon identities and additional tick values were not supplied.

M100 preserves the quantitative controls: Laetum +80% headshot damage, Lavos
+200% Magnetic, stripped Steel Path level 50 Corrupted Heavy Gunner Eximus,
10 Magnetic stacks: body/head aim both give 3455; +165% Electricity gives 9154.
The count at break is snapshotted. Counts for 2764 and tentative 1382 remain
unconfirmed. The live pool/weakness reconstruction remains open; no target
stats are adjusted to fit those displayed readings.
