# M18 — Sentinel aiming (answered), and the beam ammo rule (implemented)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Two questions the wiki does not answer, both raised on 2026-08-01, both about
weapons already in the roster.

### (a) Aiming — ANSWERED, and implemented

**A sentinel weapon is ALWAYS aiming** (owner, 2026-08-01). What it cannot do
is TRIGGER the on-headshot half of an aiming mod, because it never aims at the
head.

That is two facts, and the sim already had the second one: `default_headshot_pct`
is 0 for a sentinel, so no headshot lands and no on-headshot buff can fire.
The first is now stated too — `aiming` is forced true for a sentinel weapon and
the request cannot say otherwise, with the box shown ticked and DISABLED, the
same shape as infinite ammo. The state is real; the control is honestly
unavailable.

Why it was worth settling even though it moves no number today: all four
aim-gated rifle mods (Argon Scope, Galvanized Scope, Bladed Rounds, Catalyzer
Link) are CONDITIONAL, so a sentinel's `BaseOnly` policy kills them anyway.
A FLAT aim-gated effect would have been read wrong the moment one could reach
a sentinel weapon — Critical Focus is exactly that, and it is Arch-Gun only by
luck rather than by rule.

Evidence that agrees: Verglas Prime's stat table has no Zoom row and no Recoil
row, which is what "the player never aims it" looks like from the stat side —
the aim STATE is not the same thing as an aim-down-sights optic.

### (b) The 0.5-per-trace beam ammo cost — IMPLEMENTED, needs confirming

`ammo_cost` was read for the first time on 2026-08-01 (it had sat in every
weapon file while the sim spent a flat 1.0). The values come from the wiki:
"Beam Weapons consume 0.5 ammo per trace — unless they are Flamethrowers",
and the Larkspur Prime page states both of its own numbers, "0.5 per primary
tick" against "Alt-fire consumes 10 ammo per shot".

What changed, all exact:

| | before | after |
|---|---|---|
| Larkspur Prime, primary | 500 ticks to dry | **1000** (500 rounds ÷ 0.5) |
| Larkspur Prime, alt-fire | 118 shots / 120 s | **50** (500 rounds ÷ 10) |
| Verglas Prime | 14 reloads / 120 s | **8** (80 magazine ÷ 0.5 = 160 ticks) |

The Torid's Incarnon form keeps 1.0 per tick — that one IS measured (the
charge pool is not ammo, see MECHANICS "Continuous ammo cost").

**What settles it:** fire a full Larkspur Prime magazine on the ground and
count the ticks — 100 rounds should give 200. Then one alt-fire shot and read
the magazine: 100 → 90.

**Result:** _not yet run._
