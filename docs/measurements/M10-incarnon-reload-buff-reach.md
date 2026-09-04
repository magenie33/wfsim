# M10 — What does a reload-speed buff reach on an Incarnon weapon? ✅ (informal, 2026-07-30)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** Lethal Rearmament grants stacking reload speed on headshot.
On a weapon whose Incarnon form fires charge-backed rounds and never
reloads, does the buff do anything at all in that form — and does it
touch the gauge?

**Result (in-game, user, 2026-07-30):** the buff is **live in BOTH
forms**. What it does *not* affect is the **charge** — building the
Incarnon gauge is not a reload and takes no reload-speed scaling. It
**does** affect **transmute IN and transmute OUT**, consistent with M9's
finding that both directions scale with reload-speed bonuses.

**Consequence for the model.** A reload-speed source joins one bucket and
that bucket drives three things: magazine reloads, transmute-in and
transmute-out. Gauge fill is outside it — the only thing that shortens it
is a charge-rate evolution (Incarnon Efficiency). So on a weapon like the
Laetum, whose 216 charge-backed rounds mean the cycle is transmute-bound
rather than reload-bound, a reload buff still buys back real time. The
sim implements exactly this (`engine::dummy` rescales both transmute
animations by the live bucket); `charges_to_fill` is untouched.
