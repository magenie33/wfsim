# M12 — Do overlapping lingering fields STACK on one target? (Torid) ✅ (2026-07-30)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Answered: they STACK.** Full result — with two other field rules the same
session settled — in **M13** below. The protocol is kept because it records what
was at stake and why the branch is weapon DATA rather than a constant.

**Question.** A Torid grenade sticks to what it hits and leaves a 10 s cloud
ticking once a second. The magazine is 5 at 1.5 shots/s, so five clouds can be
attached to ONE enemy at once. Do they run as five concurrent tick streams, or
does a second grenade refresh a single field?

This is the last undocumented piece of the field model, and it is worth up to
**~5x sustained single-target DPS** — the whole reason to answer it before
implementing. The wiki says stacking clouds is effective (*"dealing large
amounts of damage if the player stacks multiple gas clouds"*, *"Stacking
multiple grenades on an ally allows them to run into groups of enemies"*) but
never says whether that means several streams on ONE enemy or just wider
coverage, and never quantifies it.

**Method** (Simulacrum, one target, unmodded Torid so the numbers are small
and readable):
1. Spawn a single high-EHP target that cannot die inside the test (a Steel Path
   Corrupted Bombard is convenient) and do NOT shoot it in the head — the cloud
   is 1x anyway, but keep the impact clean.
2. **One grenade.** Fire exactly one, then stop. Record the damage-number
   cadence for the full 10 s: expect ~10 ticks, one per second. Note the tick
   VALUE (it should be constant) and count the ticks.
3. **Two grenades, 1 s apart.** Fire, wait ~1 s, fire again. Then watch the
   window where both clouds are alive (seconds ~2–10).
   - **Stacking** ⇒ that window shows ~2 numbers per second, or one number of
     roughly double the value if the game merges simultaneous ticks.
   - **Refresh** ⇒ still ~1 number per second, and the total tick COUNT over
     the whole test is ~10–11, not ~20.
4. **Five grenades** emptied into the same target, then stop firing and count
   ticks until they stop. Stacking predicts ~50 ticks; refresh predicts ~10
   plus the tail. This is the discriminating trial — do it last, it is the one
   worth recording on video.
5. If they DO stack, also check whether the tick TIMERS are independent (ticks
   land at staggered sub-second offsets) or snap to a shared clock. That
   decides whether the sim schedules one timer per field or one shared one.

**Confounds to avoid.** No Toxin/Gas status mods — a Toxin PROC is its own DoT
and would be mistaken for a second tick stream. No multishot (each pellet is
its own grenade and its own cloud, which is the same question asked twice). Do
not stand in the cloud; self-damage is gone but stagger is not worth the noise.

**Outcome mapping.** Stacking ⇒ the field is a LIST of independent instances,
each with its own expiry, exactly like `DebuffState::dots`. Refresh ⇒ a single
field with a reset expiry, like the Heat singleton. The engine shape differs
enough that guessing wrong means rewriting it, which is why nothing is
implemented until this is answered.
