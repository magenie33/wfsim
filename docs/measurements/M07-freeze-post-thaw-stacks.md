# M7 — Who owns the 3 fresh post-thaw Freeze stacks? ✅ (2026-07-24)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** After Frozen's hard reset (M6), the 3 fresh stacks get 6 s
timers — scaled by whose status-duration modifier? Normally each Freeze
stack's duration is `6 s × (1 + status duration)` of its proccing weapon.

**Setup.** Two Cold weapons: **D+** with heavy +status duration (e.g.
+100% → 12 s stacks) and **D0** with none (6 s). Paused target that can
be Frozen (non-boss, no Overguard).

**Trials** (time the 3 stacks' post-thaw lifetime with no further Cold):
1. 9 stacks from D0, the 10th (trigger) from **D+** → if stacks last
   ~12 s: attribution = trigger weapon (hypothesis B).
2. 9 stacks from D+, the 10th from **D0** → if ~12 s here instead,
   attribution = historical stacks (C); if both trials give ~6 s,
   attribution = nobody / flat base (A).
3. Cross-check: all 10 from D+ → A predicts ~6 s, B and C predict ~12 s.

**Result (in-game, 2026-07-24):** **Hypothesis B — the trigger.** The 3
fresh stacks use the status-duration modifier of the weapon that applied
the **10th** stack (the 9→10 proc). Model: the Frozen entity snapshots
its trigger's context and issues the reset stacks from it.
