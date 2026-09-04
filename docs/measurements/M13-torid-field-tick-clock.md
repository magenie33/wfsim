# M13 — The lingering field's tick clock, stacking, and Renewed Horror ✅ (informal, 2026-07-30)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** Three things about the Torid's cloud that no source states, taken
together because one reading settles all three:
1. Does the first tick land WITH the impact, or one full period later?
2. Do overlapping clouds on ONE target run as N concurrent streams, or does a
   second grenade refresh a single field? (This is M12.)
3. Renewed Horror reads *"On Reload from Empty: Lingering damage field duration
   doubles on first shot"* — doubled duration, but how many extra TICKS?

**Result (in-game, user, 2026-07-30):**

1. **The first tick lands WITH the impact.** A hit shows the direct-hit number
   and the cloud's first number together, then nine more over the remaining nine
   seconds: **10 ticks per cloud, not 9.** The wiki's *"Clouds do not instantly
   do damage, so enemies that are quick may run through the cloud without taking
   any damage"* describes the grenade ARMING — reading it as a delayed tick clock
   cost a full tenth of the field's damage. The rule is the plain one:
   **ticks = duration × tick rate, the first at the moment of impact.**
2. **Renewed Horror doubles the NEXT shot's field, and only that one:** the
   post-reload shot reads **"1 direct hit + 20 pod ticks"** against the normal
   10. Note this corroborates result 1 without relying on it — a delayed first
   tick over a doubled 20 s lifetime would read 19.
3. **Overlapping fields STACK** — N concurrent tick streams, one per grenade.
   That answers M12, and it is what makes the cloud the weapon's main damage:
   a 5-round magazine can have all five attached at once.

**Consequence for the model.**

- `engine::dummy` schedules a fresh `FieldState` with `next_tick: t` (was
  `t + 1/tick_rate`) and `ticks_left = duration × tick_rate`. Pinned by
  `one_grenade_leaves_ten_ticks_starting_with_the_impact`.
- `FieldStacking` stays a two-branch DATA field on the weapon rather than
  collapsing to a constant — the Torid stacks, but the branch is per weapon and
  a future one may refresh. Both branches stay unit-tested.
- Renewed Horror stops being `unmodeled` and becomes a real effect kind,
  `field_duration_on_empty_reload: 2`, applied to the field spawned by the first
  shot after a reload from empty. On a 5-round magazine that is one cloud in five
  running 20 s instead of 10 — under `stack` semantics a straight +20% of the
  cloud's total ticks. Pinned by
  `renewed_horror_doubles_only_the_post_reload_field`.
- **Scoping note, not measured:** the sim arms the buff on the magazine-reload
  path only. Reverting out of the Incarnon form also refills the base magazine,
  but a form swap is not a "Reload from Empty", so it does not arm it. A
  modeling choice, recorded here rather than left silent.

Also worth recording: the perk was BROKEN once and fixed in 33.6 (*"Fixed
Torid's Renewed Horror perk not doubling duration on the damage field duration
on first shot as intended"*), so the doubling is live in the current build — the
measurement confirms it directly.
