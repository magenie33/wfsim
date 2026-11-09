# M9 — Incarnon transition timings ✅ (2026-07-26)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** (a) Duration of the on-empty revert (fire the 270th round →
first base-form shot possible); (b) whether transition animations scale
with modded reload speed. Known: manual transmute-in = the weapon's
reload time (confirmed in-game).

**Method** (deterministic animation — 1 trial suffices, do 3): record at
60 fps, frame-step in PotPlayer (F/D).
1. **Calibrate the marker method** on a plain base-form reload: R-press →
   magazine UI refill frame; expect ≈141 frames (2.35 s unmodded).
2. **Manual switch-back** (alt-fire with charge left): transform-start
   frame → first base-form muzzle flash while SPAMMING fire (±50 ms click
   error acceptable). Secondary marker: the ammo UI swapping from the
   charge display back to 12/72 (record both; a mismatch is itself a
   finding).
3. **Empty revert**: 270th round's muzzle flash → same T1 markers.
4. Repeat all three with a large reload mod (Quickdraw +48% → predict
   1.59 s if animations scale with modded reload speed).
5. Cross-check timings against the audio track in Audacity (transform /
   reload sound onsets are ms-sharp edges).

**Outcome mapping**: revert ≈ 2.35 s → the 4.7 s pseudo-reload model
stands; ≈ 0 → pseudo-reload becomes 2.35 s; other → independent constant.
Scaling result updates `transition_animation_seconds` semantics.

**Earlier result (in-game, 2026-07-26; retained observation):** transmute-in = the weapon's reload time
(2.35 s unmodded); **revert-out has its own base of 1.0 s** (measured
exactly 1.3 s under −30% reload speed). Both directions scale with
reload-speed bonuses. Full cycle downtime = 2.35 + 1.0 = **3.35 s**
unmodded; the pseudo-reload model updated accordingly. Minor residue:
the exact scaling formula direction (time × (1+penalty) vs
base/(1+bonus)) — one more mod value would pin it.

**Earlier roster-wide interpretation (owner, 2026-08-10).** The Phenmor's page
says mode switching *"has an animation equal to weapon's reload speed"*, which
reads like a 2.8 s revert on that weapon and a contradiction of the 1.0 s above.
It is not: **that sentence is about ENTERING the Incarnon form only**, and there
is no official figure for the way back on any weapon. So M9's measured 1.0 s
stays the revert everywhere, and the two facts are about two different halves of
the cycle rather than in conflict. Settled — do not re-derive a reload-length
revert from that sentence.

## Package calibration (2026-11-07, owner-directed)

The owner selects the parsed client state timers for simulator entry and exit,
including where they conflict with the earlier reload-time interpretation.
The earlier observation above remains provenance, not the current timer input.
No new input-to-first-shot measurement is claimed.

Client build `2026.10.28.09.54`; extraction batch
`20261106T160514Z-animation-incarnon-timing`, supplementing
`20261106T150853Z-client-metadata-offline`. Receiver checks verified both
manifests, outer files and archive members, and a separate property-block
reader matched all 79 exported mode profiles. All 69 roster firearm forms join
by internal path.

| source | SHA-256 |
| --- | --- |
| executable | `dae100202115919c89440c9ef320f691113ee8dcaf01205cbd180072c35aabdb` |
| decoded Packages.bin | `e74b5477f645ff5298ba9e8678aa68e2b1e3f58e9a31b855891bf2ab5f353f2c` |
| timing manifest | `2d12018a10e8bd302c70bfc5ca09bfa04d8448d852fcbcada539a0faf7839e1e` |
| incarnon-switching.json | `a314823bf485865dcaabc38024322c6cb271352b60c08e75fc9003ac55a324fa` |

The native `WS_MODE_SWITCH` getter at RVA `0x1389e00` reads `ModeSwitchTime`,
applies `WEAPON_RELOAD_SPEED` then `WEAPON_MODE_SWITCH_SPEED`, and divides by
the speed result with a 0.01 minimum. Imported `FullClipModeSwitchTime` values
are zero. Resource clip lengths are kept separate from this state duration.

| profile | entry seconds | exit seconds |
| --- | ---: | ---: |
| Dual Toxocyst | 1.4 | 0.6 |
| Torid | 1.4 | 0.6 |
| Dera / Dera Vandal | 1.4 | 0.7 |
| Telos Boltor / Latron Wraith | 1.4 | 1.0 |
| Laetum | 2.0 | 1.0 |
| Phenmor / Felarx / Onos | 3.0 | 1.0 |
| Strun Prime | 2.0 | 1.0 |

The complete source-token table is pinned in
`tests/fixtures/incarnon/switching-2026-10-28.json`. The arena uses those
per-profile inputs for both transitions; its locked-gauge downtime is their
sum. Runtime source/destination selection, automatic-exit paths, first-fire
readiness and tick rounding still require separate observation. The original
cache files and executable were not transferred to the receiver, so their
original-input hashes were not independently recomputed there.
