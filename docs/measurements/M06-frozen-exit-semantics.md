# M6 — Frozen exit semantics ✅ (informal, 2026-07-24)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** When Frozen (the 10th-Freeze-stack state) expires, are the
remaining 3 Cold stacks the surviving old ones (carried timers) or a
hard reset?

**Result (in-game, 2026-07-24):** **Hard reset**: Freeze is set to exactly
3 stacks with fresh 6 s timers; pre-Frozen stacks and timers are
irrelevant. Corollary: stack decay during Frozen is moot. Also observed:
the Freeze display stays pinned at 10 throughout Frozen, and Cold procs
are fully inert during it.
