# M132 — secondary Vermisplicer's status budget covers five tendrils

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md).*

## Observation

On 2026-11-14 the owner relayed an in-game panel reading: secondary Vermisplicer
has five tendrils sharing 39% status chance, 7.8% per tendril. The report included
two opaque image tags; their pixels and the assembly/loadout are unavailable in
this session. The count and stated percentages are the measurement. No damage
pop-up, status frequency or fight score was supplied.

`tests/golden/M132-vermisplicer-secondary-status.json` preserves these values
without assigning unidentified screenshot parts.

## Source-backed reproduction

Client build `2026.10.28.09.54` declares `fireIterations=5` in the secondary
Vermisplicer barrel's `BehaviorOverrides`. Its primary override declares one.
The Flutterfire and Ramflare loader records each declare
`ProcChance=0.38999999` in their `GT_BEAM` override. These are independent
unmodded reproductions of the reported budget; neither loader is identified
as the screenshot's part.

The barrel's 25% budget plus either loader's 14 percentage points gives 39%
for the shot, then division by five gives 7.8% for each tendril. The neutral
loader gives 25% for the shot and 5% per tendril. Status-chance mods apply
after assembly; additional multishot does not divide the per-tendril chance.

`scripts/import_kitgun_status.py` verifies the closed roster and metadata
archives and binds every chamber's native count and every loader's beam
status budget. The compiled regression covers all 400 beam assemblies.
The remaining eleven chamber/slot records have native count one; ordinary
primary/secondary form calibration already converts shot budgets to
per-projectile chances.

## Scope

This settles status normalization and the five-tendril count. It does not
measure damage per tendril, lock-on behavior, headshots, proc frequency or
performance against Ocucor. Existing targeting limitations remain explicit.
