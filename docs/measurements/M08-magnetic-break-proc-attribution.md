# M8 — Magnetic break-proc uses the breaking weapon's context ✅ (owner, 2026-11-12)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

When shields or Overguard break under Magnetic, the event reads the current
Magnetic count and the breaking hit's weapon context. The count is
snapshotted into a body-applied ordinary Electricity proc. That weapon's
Electricity modifiers, status damage, Roar and status duration apply through
the ordinary proc rules. Earlier Magnetic appliers do not supply its context.

The owner confirms attribution by experiment: using a weapon with status
duration to break the pool produces different proc properties from using
another weapon without duration but with an Electricity mod. Exact durations,
weapon identities and additional tick values were not supplied. This is a
qualitative source/context observation.

The owner confirms that the result inherits ordinary Electricity properties.
It joins weapon Electricity in the common pile. Its retained seed does not
prevent a later Electricity application from changing the whole tick's
landing and lethal weak-point attribution. Conversely, this new body proc
changes an existing head-anchored pile back to the body until another head
Electricity application arrives.

M100 records the quantitative controls: Laetum +80% headshot damage, Lavos
+200% Magnetic, stripped Steel Path level50 Corrupted Heavy Gunner Eximus,
10 Magnetic stacks: body/head aim both give 3455, and adding +165%
Electricity gives 9154. The Magnetic count is snapshotted at break. Counts
for 2764 and tentative 1382 remain unconfirmed; M100 retains the raw readings
and the rounding limit.
