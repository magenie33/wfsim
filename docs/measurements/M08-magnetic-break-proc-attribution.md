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
vectors from that batch. The selected proxy is the first strictly greatest
positive native Weakness. With weapon WeaknessBonus `W`, proxy Weakness `V`
and generic DamageAtten `A`, the retained source is:

```
normalized = floor(f32(f32(Q / f32(1 + f32(max(W,0) * V))) + 0.5))
seed = f32(normalized * A)
```

Batch `20261112T165824Z-magnetic-pool-proxy`, manifest SHA-256
`ccdba60c8d17b97e85010305b3f692640cfc0c9bdce8979f3ab8ae905435b3c6`,
binds the generic modifier before the HEAD-only gate. The actual Magnetic
packet keeps ANY_PART6 and retains that modifier without becoming a headshot.
The receiver verified four payloads,580archive members and215native spans;
portable checks cover57producer result rows, not a receiver emulator rerun.

For the fixed owner target, the native integer pool103623 produces Q5181.
Laetum W0.5, selected GrineerHead V1 and A2 give normalized3454 and seed6908.
The fresh ordinary Electric group starts at1: `(6908+1)*0.5=3454.5 ->3455`.
Own Electricity+165% yields native float32 `9154.42578125 ->9154`. Both body
and head aim match. Supplied +80%headshot and +200%Magnetic percentage entries
do not alter this ANY_PART source/own-Electricity route.

The fixture and importer `scripts/import_magnetic_proxy.py` preserve exact
native weapon owners, ordered target proxies and original tokens. Unknown
native defaults or generic proxy coefficients remain absent rather than
receiving an inferred normalization. `UNMODELLED.md` records that coverage
and the unbound live installation/recipient boundaries. Status damage once
upstream and once in the child remains the owner's confirmed rule.

## Gameplay controls

The owner confirms breaking-weapon attribution by comparing a weapon with
status duration against another without duration but with an Electricity mod.
The proc properties follow the weapon that breaks the pool. Exact durations,
weapon identities and additional tick values were not supplied.

M100 preserves the quantitative controls: Laetum +80% headshot damage, Lavos
+200% Magnetic, stripped Steel Path level 50 Corrupted Heavy Gunner Eximus,
10 Magnetic stacks: body/head aim both give 3455; +165% Electricity gives 9154.
The count at break is snapshotted. Counts for 2764 and tentative 1382 remain
unconfirmed. The bounded native pool/proxy chain above reproduces both controls without
adjusting target stats or fitting a universal multiplier. Full live spawn,
mode/proxy installation and radial/lethal reconstruction remain partial.

## Shield maximum and cross-level controls (owner, 2026-11-13)

Setup: regular Corrupted Crewman, Steel Path, Kuva Nukor, Lavos+200%Magnetic,
10Magnetic at Shield break. The owner confirms every tick is958 at level100.
Additional owner observations arrived in the research-side conversation;
the weapon/count setup carries forward rather than being restated in each
short message. Other modifiers and the measurement client build were not
individually reconfirmed. Only level100 explicitly confirms every tick.

| Level | Shield Disruption | Measured Electric tick | Native maximum Shield |
| --- | --- | ---: | ---: |
| 80 | absent | 807 | 16128 |
| 100 | absent | 958 | 19157 |
| 120 | absent | 1098 | 21948 |
| 80 | −18% | 662 | 13225 |

The level80/120 predictions preceded those observations. The aura's late
installation comparison was calculated after the662 observation. Native
derivation and owner comparisons remain separate in the supplied archives.

The Shield controller joins the Orokin RifleSpacemanAvatar to the Corpus
AIWeek/RifleSpacemanDamageController, MaxShield120, native base level1.
The Orokin Shield curve is evaluated in binary32. HardModeEnemyLevelAura
declares AVATAR_SHIELD_MAX140/MULTIPLY1.58114, represented as
1.5811400413513184; this is a native value, not a fitted square root.
The integer maximum getter applies that operation before and after level
writeback. At level100, with binary32 multiplication and half-up rounding:

```
initial = round(120 * 1.58114) = 190
curve = 63.77061462402344
stored = round(190 * curve) = 12116
maximum = round(12116 * 1.58114) = 19157
Q = trunc(f32(f32(19157 * f32(0.005)) * 10)) = 957
normalized = 957                         # Kuva Nukor W0
retained source = 957 * 2 = 1914          # selected proxy generic modifier
ordinary tick = (1 + 1914) * 0.5 = 957.5 -> 958
```

Raw-source truncation is unchanged. The discrepancy was the maximum Shield
preparation, not an extra point added to Electricity. At level80 the player
Shield Disruption STACKING_MULTIPLY factor enters the final maximum query,
after level writeback and after the Steel operation, before its integer
rounding: `round(f32(f32(10200*1.58114)*f32(1-0.18)))=13225`.
Early installation instead gives a543 tick and fails the owner662 control.

Pinned batches, with READY/manifests and every outer/archive member verified:

| Batch | Manifest SHA-256 | Native spans |
| --- | --- | ---: |
| `20261113T060047Z-magnetic-shield958` | `94ee4b13562d197ce9df284873049a95117e029efcbff064206612b5fd9e9ddc` | 182 |
| `20261113T062548Z-shield-aura-crosslevel` | `370bb31c91a401efded87e3a08c8fc189ab3ecfaa6feff67ba30a5ae5af3deb6` | 180 |
| `20261113T065338Z-mode-enemy-scaling` | `c586f197c5534ac1c9ed31974dec0e4fc368bcc4abd17e68190da0ccfceee78b` | 198 |

Together these contain13outer files and1643archive members. The receiver
checked raw span hashes, assembly-byte correspondence and reported nonstub
instruction coverage. Each batch reports one executed declared allocation
stub; those entries are not archived original executable instructions.
The receiver did not rerun an emulator or have the original executable.
The public Shield query fixture preserves38conditional native pool vectors.
The real engine's level100 test failed957 before correction; corrected
full-constructor controls cover807/958/1098/662, spawn pools and neighboring
bodies. Existing Overguard controls remain unchanged.

### Adopted Empowered ordering

The mode batch recovers a shared OnAgentCreated script block. The ordinary
branch supplies full MULTIPLY `2+0.5*N`; enabled EmpoweredEnemies complication
supplies `3+N`. Shield uses stat140. The same block's armor stat20 is separately
guarded by missionInfo.tier>0. The block has no additional count clamp; this
does not establish live squad limits. Full mode activation, membership and
callback order relative to level writeback remain unbound.

The owner explicitly adopts the same before-level installation order as
Steel Path. WFSim therefore applies each full mode Shield factor in both
maximum queries, approximately its square before integer rounding. This is
an implementation assumption supported by the shared consumer, not a new
mode-specific gameplay measurement. Supplied native before/after cases stay
distinct in the fixture. Health/armor paths are outside this Shield correction.


## Historical external cross-check

The [2024 forum report](https://forums.warframe.com/topic/1409188-how-does-magnet-status-work-after-update-36)
records Crewman maximum shield14789 and Electric ticks38 with Lanka versus74
with Gammacor, and Elite Crewman maximum18486 with ticks47 versus93.
With one Magnetic stack, the native pool source truncates to73 or92.
The independently parsed Lanka WeaknessBonus1 versus Gammacor0, with the
selected head proxy Weakness1/DamageAtten2, gives exactly those four displayed
ticks after normalization, generic modification and the ordinary accumulator.
This is consistency with a historical third-party observation, not new owner
calibration or a reconstruction of that old live spawn/ability chain.
