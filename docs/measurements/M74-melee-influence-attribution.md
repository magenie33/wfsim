# M74 — Melee Influence pays the body it came from (owner, 2026-09-03)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**One target, a melee with Shocking Touch, Melee Influence equipped.** The
arcane's spread damage lands on the struck enemy as well as on everything within
the radius, and force-procs there like anywhere else.

The wiki's example counts *"every other enemy within Melee Influence's range"*,
which is the half of it that sentence is about — not a statement that the host
is skipped.

### What changed

`spread_from_influence` skipped the epicentre body, so on a single target the
arcane was worth exactly nothing and a test asserted that on purpose. It matters
most where it is least visible: a single-target ruler ranked every Influence
build as if the arcane were not equipped.
