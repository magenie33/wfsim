# M32 — the Incarnon's explosion fired on every base-form shot (2026-08-07)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

From "两个benchmark存的东西是不对的… torid的数据是完全不对的" (owner,
2026-08-07). The board was the symptom; this is what was under it.

A cycle fires TWO weapons in turn, and the shot loop switches to the active
form's params (`ap`) for damage, crit, status and forced procs. Two lines did
not: `radial_stage` and `co_mult_radial` read `params.radial` — the OUTER
params, which are the Incarnon form's — so a weapon whose Incarnon detonates
threw that explosion on every BASE-form shot as well.

### The measurement

Burston Prime, Serration only, Thrax Centurion 100, 4 s, 400 runs, **zero
headshots** — so a weak-point-charged gauge never fills and both sides report
**zero transforms**. The fight is base form from end to end in both.

| | DPS | sources |
|---|---:|---|
| pinned `base` | 1738 | direct 6713 · Slash 239 |
| `incarnon_cycle` | **2470** | direct 6210 · **radial 2584 (Heat)** · Heat 758 · Slash 384 |

**+42%**, and the whole of it in a radial dealing HEAT — an element the base
form has nowhere in its vector. After the fix the two are identical to the
digit: 1738.0 against 1738.0, same sources.

### What it cost the boards

Rescored, and the size of each correction is the share of the engagement spent
in the base phase — which is exactly what a leak from the other form should
look like:

| board | weapon | before → after |
|---|---|---|
| aimed | Burston Prime | −1.3% … **−2.7%** (10 rows) |
| aimed | Laetum | −0.2% … −0.8% (10 rows) |
| **no aim** | Burston Prime | 0.9572 → **0.5858 (−38.8%)** |

Only those two: they are the roster's Incarnon forms that carry a radial. At a
100% headshot rate the weapon is in its Incarnon form for most of the fight, so
the leak had little room; on the no-aim board it never transforms at all and
the explosion was the whole difference between a real score and a fiction.

### The near miss, which is the part worth keeping

The board records a `mode`, and eight of the nine Incarnon forms never
transform at a 0% headshot rate. So the obvious reading was "a cycle row that
never transformed IS a base row" — file it there, and the no-aim board stops
claiming a form nobody saw. That change was written, and the measurement above
is what stopped it: relabelling re-scores, and the published Burston Prime
moved 0.9572 → 0.5858 under it. The right conclusion was not that the label was
wrong but that **the two fights should have been equal and were not**.

Pinned as `a_cycle_that_never_transforms_is_its_base_form`, over a fixture whose
Incarnon declares a radial and whose base form declares none — with body-only
aim, so the gauge can never fill. It bites: 3500 against 500 before the fix.
