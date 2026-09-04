# M14 — What happens to the ammo remainder when an efficiency buff expires? ✅ (2026-07-30)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** Ammo Efficiency divides the per-shot ammo cost and *"keeps track
of the fractions as well"* (wiki Energized Munitions), so a magazine can sit on
a fractional value — 0.5 rounds left, say. When the buff then lapses and the
cost snaps back to a full 1.0, what does the game do?

Three candidate models, and they differ by up to one shot per magazine:
1. **Fire and overdraw** — the shot happens, the remainder is spent and the
   magazine bottoms out at 0.
2. **Fire and keep the credit** — the remainder is a pre-payment on the *next*
   round, so a 0.5 remainder means the round after costs only 0.5.
3. **Refuse** — a full round is required, so the weapon reloads with 0.5
   showing and that fraction is discarded.

**Result (in-game, user, 2026-07-30).** **Model 1 — and the debt survives the
reload**, which none of the three candidates had anticipated. Run on a 5-round
magazine with a 75% efficiency buff:

| step | internal | UI |
| --- | --- | --- |
| full magazine | 5.00 | 5 |
| buff ON, 3 shots at 0.25 | **4.25** | 5 |
| buff OFF, 5 shots at 1.00 | **−0.75** → reload | — |
| after the reload | **4.25** | 5 |
| buff ON, 1 shot at 0.25 | **4.00** | **4** |

Three separate rules fall out of that one run:

1. **The cost really is divided and the fraction really is kept** — 3 buffed
   shots cost 0.75 of a round, not 1 and not 3.
2. **A partial round fires at full cost, and overdraws.** From 4.25 with the
   buff gone, five full-cost shots all fire; the fifth goes off with 0.25 left
   and lands the counter at −0.75. So the fire gate is "anything left", never
   "enough to pay".
3. **The reload ADDS to the counter rather than setting it** — the fresh
   magazine came back at 4.25, not 5.00.

**How rule 3 is known, given the HUD shows whole numbers.** The **UI displays
the CEILING** of the internal value, which is what makes the last row decisive:
one 0.25 shot dropped the readout from 5 to 4. That only happens from 4.25 →
4.00 (`ceil` 5 → 4). Had the reload handed back a clean 5.00, the same shot
would land on 4.75 and the readout would have stayed at **5**. The UI's rounding
is doing the measuring here.

**Consequence for the model.** `engine::dummy` had rules 1 and 2 right already —
the fire gate is `magazine < 1e-9` and the cost is `1 − efficiency`, so a
partial round fires and the counter is allowed to go negative. Rule 3 was
**wrong**: the reload did `magazine = refill`, silently forgiving the debt and
handing back a free fraction of a round. It is `magazine += refill` now, on both
reload paths (the plain one and the Incarnon cycle's base-form reload).

The fix is a no-op without an efficiency source, since a 1.0 cost lands the
magazine exactly on 0 — which is why nothing else moved. Pinned by
`an_efficiency_overdraw_carries_its_debt_through_the_reload`, built on a 60%
buff (0.4 a shot) precisely because it does NOT divide a 5-round magazine
evenly: carrying the debt gives 25 shots off two magazines, wiping it gives 26.
Verified to fail with `got 26` against the old code.

**Follow-up (same session): a reload draws WHOLE rounds.** Reserve is spent in
whole rounds only, so a reload tops the magazine up by
`floor(capacity − current)` rather than filling it. Measured on the same 5-round
magazine:

| current | draw | after |
| --- | --- | --- |
| 1.50 | `floor(3.50)` = 3 | **4.50**, not 5 |
| 3.25 | `floor(1.75)` = 1 | **4.25** |
| 4.25 | `floor(0.75)` = 0 | **4.25** — the reload is refused outright |

The refusal at 4.25 shows in game as an already-full magazine, because the HUD
ceilings it to 5 — the same rounding that made the main result readable.

This **subsumes the overdraw case rather than competing with it**: a shot cannot
overdraw by a whole round, so after running dry `current` is in (−1, 0] and the
draw is a full `capacity` — which is exactly how −0.75 comes back at 4.25. One
rule, `engine::dummy::reload_draw`, and the earlier `+= capacity` was the
special case of it that happened to be right.

**And it is GLOBAL** (user, 2026-07-30): the auto-reload an Incarnon
**transform** performs runs on the same mechanism, not a separate fill-to-full.
That resolves what this entry previously left open — the transform paths use
`reload_draw` too, so a base magazine sitting on 4.25 comes back on 4.25. (It
still does not arm Renewed Horror; that gate is about reload-from-EMPTY, M13,
and is a separate question from how many rounds move.)
