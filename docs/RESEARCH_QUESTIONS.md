# Client research questions

Open questions for the Windows research machine. Each question identifies the
missing behavior, the existing evidence and the result needed for integration.
[UNMODELLED.md](UNMODELLED.md) owns the simulator's current coverage.

## Cross machine handoff

This file in WFSim Git owns the question definitions and their review status.
The shared Drive folder `My Drive/WFSim/warframe-data/requests/` carries closed
snapshots naming the source commit, file path and SHA-256. A snapshot is an
input for the research session; it does not supersede this file.

On Windows, open `E:\src\github.com\magenie33\warframe-data`, read `AGENTS.md`
and `docs/EXCHANGE.md`, then select an open question from the request snapshot.
Partial deliveries may answer individual question ids. Leave the request
unchanged and reference its id in a new result batch.

Return results through a new immutable `batches/<batch-id>/` using the existing
manifest and READY protocol. For each answered id, include the conclusion,
exact source paths or native RVAs, branch conditions, a reproducible check,
and remaining unknowns. Pin the client build, executable/input hashes and
tool/source commits. An unresolved question remains an explicit unknown.
If another build is installed, identify the new build and rebind addresses.

The WFSim receiver verifies hashes and interpretation, writes a separate
receipt, then updates this file. The review stages are Open, Evidence received,
Verified and Integrated. A research result alone does not mark a model as
integrated. Game observations, if needed, are separate experiments with their
own setup and observations; they are not implied by an offline trace.

## Coda Bubonico questions

Request id: `coda-bubonico-runtime`.

| Question | Priority | Status | Needed result |
| --- | --- | --- | --- |
| CB-01 | First | Open | Shot-count progression and reset rules for fire-rate spool |
| CB-02 | Next | Open | Inverse spread state at each projectile emission |
| CB-03 | Next | Open | Additional multishot spread and random-seed ownership |
| CB-04 | Later | Open | Projectile trajectory, collision and explosion cover rules |

The matching source client is `2026.10.28.09.54`; executable SHA-256:
`dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb`.
The weapon owner is
`/Lotus/Weapons/Infested/InfestedLich/LongGuns/CodaBubonico/CodaBubonicoCannon`.
Behavior 0 is primary fire; behavior 1 is alternate fire.

Start from these closed exchange batches, verifying READY and their manifests:

- `20261109T053558Z-weapon-roster-dossiers`: `weapon-roster-dossiers.zip`,
  `weapons/coda_bubonico/dossier.json` and `shared/objects.jsonl`.
- `20261107T231054Z-companion-cadence-aiming`: `companion-evidence.zip`,
  `native/automatic-spool-rate--00.asm.txt` and its original bytes.
- `20261109T025857Z-spread-sampling`: `spread-sampling-evidence.zip`, the
  report, original native spans and bounded sampler cases.

The exact primary tokens and archive hashes are retained in
[the scoped fixture](../tests/fixtures/client/coda-bubonico-scope-2026-10-28.json).
[The primary receipt](../tests/fixtures/client/primary-calibration-2026-10-28.json)
binds the adopted weapon fields. These existing source values need no fresh
bulk export unless a consumer requires additional records.

### CB-01 Fire rate progression and reset

The primary state is `WeaponAutomaticBurstStateBehavior`.
`ContinuousFireRateAtten={0.30000001,1}`, `ContinuousMaxShots=12` and
`SpindownTime=0`. WFSim currently uses the wiki's seven-shot interpretation:
the first shot starts at 30%, with full rate from the seventh shot.

Trace the effective virtual method at slot `+0x200` called by the native rate
getter at RVA `0x83beb0`, then the state writes advancing and resetting its
progress. Establish whether updates count rounds, firing iterations, state
transitions or another event, and whether they occur before or after firing.
Keep trigger release, a short pause, partial and empty battery recovery,
mode switching and holstering distinct.

Completion requires reproducible successive-shot rates and reset cases from
the effective Coda state, explaining how the serialized 12 relates to emitted
shots. A reread of the field or a stubbed progress value is not that result.
An in-game rate comparison may independently check the interpretation.

### CB-02 Inverse spread during firing and recovery

Primary fire has `SpreadInverse=1`. Hip and aimed firing select
`/Lotus/Types/Game/WeaponProperties/Accuracy/Weapon/InfArmCannonAccuracyInfo`.
The existing parsed profile and sampler evidence do not close the per-form
update and emission order. WFSim currently samples the static minimum spread.

Trace profile selection, shooting-channel growth/recovery, inverse handling,
smoothing and the sampler input at actual emission. Preserve the update clock,
first-shot state, continuous-fire sequence, recovery after a pause, and hip
versus aimed branches. Distinguish a raw channel value from a smoothed sampler
input and from the resulting projectile angle.

Completion requires a deterministic sequence of emitted spread inputs and
angles with the selected branch and time steps, covering a first shot,
sustained firing and firing after recovery.

### CB-03 Additional multishot spread

Primary fire declares `SpreadExtraProjectiles=1`, `ExtraSpread=2`,
`SpreadMultiplier=3` and `MultiShotProjectileType=CodaInfArmCannonBoltoProjMS`.
Its initial and maximum fire iterations both equal seven; those values do
not describe a pellet-count ramp.

Trace which emitted projectiles use each override, the ordering relative to
the CB-02 spread input, and whether native seeds are initialized or shared
per shot, pellet or sampler call. Keep the ordinary and additional-multishot
projectile branches distinct even when their damage packets match.

Completion requires a reproducible projectile list and angle/seed sequence
for the baseline seven-pellet shot and a controlled increased-multishot case.
State the exact effective multishot input; a stubbed upgrade boundary remains
labelled as such. Replaying the generic sampler alone does not close this path.

### CB-04 Projectile trajectory and collision

Resolve both behaviors' projectile records through the owner's references.
The alternate projectile declares speed 25 and gravity 0.5. Its explosion
declares `ExplosionIgnoreCover=0`, `ExplosionIgnoreDynamicCover=1` and
`ExplosionCoverMinDistance=20`. These tokens require their consumers before
they establish trajectory units or a wall-penetration rule.

Trace gravity integration, launch direction, flight/collision timing,
enemy and world punch through, impact location and the explosion cover test.
Separate primary, additional-multishot and alternate projectile branches.
Keep static cover, dynamic cover and the distance threshold distinct.

Completion requires an executable rule or bounded cases giving travel and
impact behavior plus cover decisions on both sides of the native threshold.
If offline evidence cannot settle a case, return the missing branch and a
narrow proposed game observation. WFSim's current straight-flight geometry
and lack of world cover remain separate implementation limits.
