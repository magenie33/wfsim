# M24 — a one-run gain screen cannot rank a status mod (2026-08-02)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

From "why does adding status chance LOWER the damage?" It does not. The quick
calc's FIRST pass is one run per candidate, and one run cannot see what a
status mod buys.

Torid, Thrax Lv 300 SP, 300 s, gain over the same build, three seeds:

| candidate | 1 run | 3 | 10 | 30 | 100 | 400 |
|---|---|---|---|---|---|---|
| High Voltage | 30.2% ±39.0 | 30.0 ±39.0 | 24.2 ±28.5 | 30.7 ±12.0 | 30.0 ±20.0 | 29.2 ±5.6 |
| Malignant Force | 31.4% ±38.0 | 42.4 ±3.9 | 44.7 ±2.5 | 44.2 ±6.2 | 43.5 ±8.4 | 42.6 ±2.4 |

The MEAN is roughly right even at one run — this is not a bias. The SPREAD is
the finding: ±39 points, wide enough to print a minus sign in front of a mod
worth +40%. A pure damage mod does not do this (Heavy Caliber: 70.5% at one
run, 69.8% at two hundred) because paired seeds cancel nearly everything about
it. A status mod's payoff is decided by which procs land, which is the one
thing the shared seed cannot hold still once the candidate changes how many
rolls happen.

No run count fixes it cheaply: High Voltage is still ±5.6 at four hundred runs.
So the screen now SAYS it is a screen — a one-run number prints as "≈+12%",
dimmed, with a tooltip explaining the band; only the second pass (the leaders,
at a tenth of the scenario's count) prints a bare number. The two-pass design
was already right; what was wrong was presenting its cheap half with the same
authority as its careful half.
