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

**Result (in-game, 2026-07-26):** transmute-in = the weapon's reload time
(2.35 s unmodded); **revert-out has its own base of 1.0 s** (measured
exactly 1.3 s under −30% reload speed). Both directions scale with
reload-speed bonuses. Full cycle downtime = 2.35 + 1.0 = **3.35 s**
unmodded; the pseudo-reload model updated accordingly. Minor residue:
the exact scaling formula direction (time × (1+penalty) vs
base/(1+bonus)) — one more mod value would pin it.

**Confirmed as the roster-wide rule (owner, 2026-08-10).** The Phenmor's page
says mode switching *"has an animation equal to weapon's reload speed"*, which
reads like a 2.8 s revert on that weapon and a contradiction of the 1.0 s above.
It is not: **that sentence is about ENTERING the Incarnon form only**, and there
is no official figure for the way back on any weapon. So M9's measured 1.0 s
stays the revert everywhere, and the two facts are about two different halves of
the cycle rather than in conflict. Settled — do not re-derive a reload-length
revert from that sentence.
