# wfsim — In-game Measurement Protocols

Golden tests need real measurements. Each protocol here is written so a
single session produces an unambiguous answer. Add results inline (date +
numbers), then CORRECT the data/docs in place (decision 2026-07-24: no
per-file verification status - the recorded value is always the current
belief).

General setup:
- HUD: enable damage numbers (Enhanced Damage Numbers). Color code:
  **blue = damage to shields**, white = health, yellow/orange/red = crits.
  One number per damage instance; a hit that splits across pools shows the
  pool parts separately (blue vs white).
- Lab: the Simulacrum is fine as a *measurement lab* (mechanics are the real
  game's); our engine just doesn't model the Simulacrum's own toggles.
  Pause AI. Discard any yellow/orange (crit) readings unless the protocol
  says otherwise.

---

## M1 — Is Toxin's shield-bypass damage reduced by the enemy shield gate?

**Question.** When an enemy's shields break, a 0.1 s gate lets only 5% of
damage reach health. Toxin never touches shields — is its direct damage
gated inside that window, or does it pass in full?

**Model assumption (2026-07-24; revised same day):** the gate is the
enemy analogue of the player shield-gate — a 0.1 s protection window on the
unit — so Toxin is **gated to 5% too**. Status: **assumption / unverified**
until this protocol is run. Outcome mapping: target **survives** the
verdict shot → assumption confirmed; **instant death** → Toxin is ungated,
revert MECHANICS.md §8 and the engine model.

**Primary method — kill-threshold discrimination (no recording, no number
reading).** Turn the transient into a persistent binary: pick a level where
target health `H` sits between the gated and ungated toxin damage,
`0.05·T < H ≤ T`. Then the breaking shot either kills on the spot (ungated)
or leaves the target standing (gated).

- **Target.** Corpus **Crewman** (no armor, Head 3.0x; base @L1: 90 HP /
  120 shields). Level ≈ **5** (≈115 HP / ≈148 shields — robust to ±20%
  formula error).
- **Weapons.** *Shield whittler*: any unmodded pure-IPS weapon (bare
  Braton) — IPS never touches health while shields are up. *Verdict shot*:
  **Lex + Pathogen Rounds** (+90% Toxin): panel 180 physical + `T` = 162
  Toxin.
- **Steps.**
  1. Whittle the shield bar visibly low (<20%, eyeball is fine) with body
     shots from the whittler.
  2. One **body** shot with the Lex (never the head — weakspots bypass the
     gate). The 180 physical certainly finishes the shield; the 162 Toxin
     lands the same instant.
  3. Outcome: **instant death** → Toxin ungated (assumption confirmed).
     **Survives the instant** → Toxin is gated (fix MECHANICS.md + engine).
  4. Repeat ≥5×. A crit on the verdict shot cannot flip the result
     (gated 0.05 × 324 ≈ 16 ≪ H). If a Toxin *proc* ticks afterwards
     (green DoT numbers), void that trial — the DoT could kill a
     should-survive target over 6 s.

**Alternative (needs reading numbers).** Same setup at a high level; read
the white (health-pool) damage number of the breaking shot: ≈`T` ungated,
≈`0.05·T` gated. Requires the recording setup below.

---

## Recording setup (for any number-reading protocol)

- **Recorder:** OBS Studio with **Replay Buffer** (hotkey saves the last
  1–2 min; one save per trial). GPU-vendor instant replay (NVIDIA App /
  AMD ReLive) as a lighter alternative; Xbox Game Bar (Win+Alt+G, last
  30 s, 60 fps) as the zero-install fallback.
- **Frame rate:** 60 fps = 6 frames inside a 0.1 s window (sufficient);
  120 fps preferred if the game holds 120+ (uncap the in-game FPS limit).
- **Quality:** hardware encoder (NVENC/AMF) at **CQP 15–18** or ≥50 Mbps,
  native resolution — the payload is small damage-number glyphs; artifacts
  make them unreadable.
- **In-game:** damage numbers = Enhanced; motion blur / screen shake off,
  particles low.
- **Playback:** frame-step in MPC-HC (Ctrl+→) or mpv (`.` / `,`).
- **Window-width probe (M1b):** high fire rate = shots per window:
  Twin Grakatas 20/s (~2 in-window), Soma 15/s (~1–2). Frame-step to count
  how many post-break body hits show 5%-sized white numbers before full
  numbers resume. Keep the M1 verdict shot itself on a slow weapon (Lex).

**Bonus readings from the same session:**
- The physical spill of the breaking shot (white part next to the blue
  break) should be ≈ 5% of the spill → confirms the 5% leak value.
- **M1b — window width:** switch to a high fire-rate weapon (≥10/s), spray
  the body through a shield break: count how many post-break shots show
  tiny (5%) white numbers → confirms the 0.1 s window.
- **M1c — weakspot bypass:** repeat step 2 aiming at the **head**: the
  post-break numbers should be full-value (×3 location) even in-window.

**Result:** _not yet run._

---

## M3 — Corpus Parazon Mercy cap ✅ (informal, 2026-07-24)

**Question.** Does the Corpus Mercy cap reach 100% (Parazon page) or only
with shields removed (Impact page wording)?

**Result (in-game tests, 2026-07-24):**
1. Corpus units **do reach the 100% cap**.
2. **Shields are a hard gate**: at 1 HP behind 10,000 shields there is no
   Mercy prompt — "shields removed" is a prerequisite, not a bonus
   condition. Overguard likewise blocks Mercy entirely (also confirmed by
   the wiki Overguard patch history). Model updated: `can_mercy` requires
   shields = 0 and overguard = 0 before any window math.

---

## M4 — Which health curve do Anarchs use? ✅ (2026-07-24)

**Question.** `Enemy_Level_Scaling` lists Anarchs in two health tabs with
different exponents: "Anarchs, Corrupted" (`0.015·Δ^2.1 / 10.7332·Δ^0.685`)
vs the "Murmur, Sentient, and Unaffiliated" tab whose *text* also names
Anarchs (`0.015·Δ^2 / 10.7332·Δ^0.5`). Engine currently follows the tab
structure (Anarchs = Corrupted curves).

**Method.** Read a plain (non-Eximus, non-Commandeered) Anarchs unit's HP
at a known level and compare (bases @L1: Anarch Arcus 100, Gladius 175):

| unit | level | A: Corrupted curves | B: Unaffiliated curves |
|---|---|---|---|
| Arcus | 50 | 5,415 | 3,702 |
| Arcus | 60 | 7,950 | 5,322 |
| Arcus | 100 | 25,088 | 10,779 |
| Gladius | 60 | 13,913 | 9,313 |
| Gladius | 100 | 43,905 | 18,864 |

A 2.3x gap at level 100 — a health-bar read or a shots-to-kill count
decides it.

**Result (2026-07-24):** **Anarchs = Corrupted curves.** The wiki's own
calculated stat block for Commandeered Ash Prime @L1000 (wiki calculator:
18,275,927.85 HP / 623,680.94 shields / 2,700 armor / 27,531 affinity)
matches our Corrupted health (2.1/0.685) and Corrupted shield (2.0/0.75)
curves **to the cent**; the Unaffiliated pair is 3.6x off. Bonus
confirmations: affinity = base 5,000 × (1 + 0.1425·√level) floored, with
the module's Affinity field being the base value. Pinned as a regression
test (`commandeered_ash_prime_at_1000_matches_wiki_calculator`). The
Murmur-tab text naming Anarchs is a wiki typo.

---

## M5 — Heat Inherit context sync is bidirectional ✅ (informal, 2026-07-24)

**Question.** The Heat singleton's first-proc modifier context (Heat% /
faction brackets): does a strong first proc also elevate later unmodded
contributions (the wiki left this direction unconfirmed)?

**Result (in-game, 2026-07-24):** **Yes — bidirectional.** The first proc's
brackets apply to every later contribution in both directions. Build
consequence: light the first Heat proc with the best-modded weapon; any
source can then feed the ramp at full value.

---

## M6 — Frozen exit semantics ✅ (informal, 2026-07-24)

**Question.** When Frozen (the 10th-Freeze-stack state) expires, are the
remaining 3 Cold stacks the surviving old ones (carried timers) or a
hard reset?

**Result (in-game, 2026-07-24):** **Hard reset**: Freeze is set to exactly
3 stacks with fresh 6 s timers; pre-Frozen stacks and timers are
irrelevant. Corollary: stack decay during Frozen is moot. Also observed:
the Freeze display stays pinned at 10 throughout Frozen, and Cold procs
are fully inert during it.

---

## M7 — Who owns the 3 fresh post-thaw Freeze stacks? ✅ (2026-07-24)

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

---

## M8 — Magnetic break-proc attribution with mixed appliers

**Question.** The shield/overguard-break Electricity proc scales with the
applier's mods (status-damage ×3.61 double-dip, faction double-dip,
literal Electricity mods). With stacks from MULTIPLE sources, whose mod
context does it read? Hypotheses: A first stack's applier (Heat Inherit
pattern), B last stack's applier (Frozen/M7 pattern), C the
shield-breaking hit's source, D per-stack attribution (each stack's 3%
chunk uses its own applier — suggested by the "per stack" wording).
Stakes: up to ~×8.7 between a modded and a bare applier.
**Prediction: D** (per-stack) — "per stack" wording, the Blast-radial
per-stack-snapshot precedent, and proc instances demonstrably carrying
source context; the counter-precedents (Heat first-proc, Frozen trigger)
both stem from special causes absent here.

**Method.** Magnetic weapons M+ (heavy status-damage + faction mods) and
M0 (bare); high-shield Corpus target. Read the post-break Electricity
tick numbers (6 s window):
1. All stacks M+, break with a non-magnetic weapon → applier baseline.
2. All stacks M0, break with M+ → big ticks = C; small = applier-owned.
3. Mixed 5×M+ then 5×M0, break with a third source → intermediate = D;
   all-M+ value = A; all-M0 value = B. Swap application order to
   separate A/B.

**Result:** _not yet run._

---

## M9 — Incarnon transition timings ✅ (2026-07-26)

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

---

## M10 — What does a reload-speed buff reach on an Incarnon weapon? ✅ (informal, 2026-07-30)

**Question.** Lethal Rearmament grants stacking reload speed on headshot.
On a weapon whose Incarnon form fires charge-backed rounds and never
reloads, does the buff do anything at all in that form — and does it
touch the gauge?

**Result (in-game, user, 2026-07-30):** the buff is **live in BOTH
forms**. What it does *not* affect is the **charge** — building the
Incarnon gauge is not a reload and takes no reload-speed scaling. It
**does** affect **transmute IN and transmute OUT**, consistent with M9's
finding that both directions scale with reload-speed bonuses.

**Consequence for the model.** A reload-speed source joins one bucket and
that bucket drives three things: magazine reloads, transmute-in and
transmute-out. Gauge fill is outside it — the only thing that shortens it
is a charge-rate evolution (Incarnon Efficiency). So on a weapon like the
Laetum, whose 216 charge-backed rounds mean the cycle is transmute-bound
rather than reload-bound, a reload buff still buys back real time. The
sim implements exactly this (`engine::dummy` rescales both transmute
animations by the live bucket); `charges_to_fill` is untouched.

---

## M11 — Is an "on hit" perk judged per trigger pull or per damage instance? ✅ (informal, 2026-07-30)

**Question.** Overwhelming Attrition reads "On Hit that is neither Critical
nor applies a Status Effect". Is that ONE evaluation per shot, or one per
damage instance the shot produces? The distinction decides what the perk is
worth on an AoE weapon, and it is the same question as whether a Laetum's
direct hit and its explosion each arm it.

**Result (in-game, user, 2026-07-30):** per **damage instance**, in both
directions the question could be asked.
1. **Across enemies:** fired into a crowd, a SINGLE Laetum shot takes the
   buff from empty to its 3-stack cap. One trigger pull cannot grant three
   stacks under a per-shot reading.
2. **On ONE enemy:** a single shot at a LONE target grants exactly **2
   stacks** — the direct hit and the explosion each arm it, and the count
   is the instance count, not the cap (3).

**Consequence for the model.** Result 2 is the load-bearing one and it is
direct, not inferred: two attack parts landing on the SAME enemy ARE two
instances. It also independently corroborates the verbatim AoE rules
(`Status_Effect`: each enemy hit gets its own status roll; Laetum: "Initial
hit and explosion apply status separately") from the perk side.

That is what `engine::dummy` implements — the trigger is evaluated inside the
attack-stage loop, so each stage judges its own crit and its own proc list.
Pinned by `the_explosion_arms_an_on_hit_buff_of_its_own`, which gives the
radial ZERO damage so the only thing it can contribute is the second arming.

**No residue.** Both readings are measured. Note the 2 (not 3) in result 2 is
itself informative: it rules out "any qualifying shot fills the buff" and
confirms the count tracks instances.

---

## M12 — Do overlapping lingering fields STACK on one target? (Torid) ✅ (2026-07-30)

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

---

## M13 — The lingering field's tick clock, stacking, and Renewed Horror ✅ (informal, 2026-07-30)

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

---

## M14 — What happens to the ammo remainder when an efficiency buff expires? ✅ (2026-07-30)

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

---

## M15 — Does every chain NODE carry a damage sphere, or only the beam's contact point? (Torid Incarnon)

**Question.** The Incarnon beam puts a **2.3 m damage sphere** at its contact
point, and every enemy that sphere catches starts its own chain (5 hops, 7 m
each, ×0.75 per hop). Does a chain HOP also drop a 2.3 m sphere where it lands,
or is there exactly one sphere in the whole attack?

It decides how the weapon scales into a crowd, and therefore what Firestorm is
worth: with one sphere, Firestorm buys more *chain origins*; with a sphere per
node, it multiplies the whole cascade.

**Current model: every node spheres** (`beam.chain.nodes_have_radius: true`).
That is a **decision, not a citation** — the user's in-game read of a clump
lighting up (2026-07-30). It is one line of weapon data precisely so a
measurement can flip it.

**The evidence pointing the OTHER way**, recorded so the decision is not
mistaken for a sourced fact. All four are circumstantial and none is a
statement about chain nodes:
1. the wiki calls the first one *"the **initial** damage radius"* — a qualifier
   that only earns its place if there is exactly one;
2. the sphere is defined *"from the point of impact **against a surface**"*,
   while a chain lands on an **enemy**;
3. the datamined attack table carries **no radius at all** for the Incarnon
   attack, while the Poison Cloud (a real AoE part) carries its falloff;
4. the chain sentence is boilerplate shared with Atomos and Amprex, neither of
   which spheres at a chain node.

Against that: nobody who has played it says otherwise, and the wiki never
addresses the question. Four inferences do not outrank someone with the game
open — hence the decision, and hence this protocol.

**Method — the trap is that a clump cannot tell the two apart.** With several
enemies inside the initial sphere, BOTH models produce a wall of numbers: the
instance count is `1 + 5·Y` in Y, so eight clumped enemies give ~41 instances
with a single sphere. Worse, *"an enemy can be struck by multiple chains"*, so
one enemy shows several numbers either way. Counting damage numbers in a pile
proves nothing.

Force **Y = 1** and put the crowd out of the sphere's reach:

1. One enemy alone, against a wall. Aim so the sphere catches **only** that one
   — Y = 1, so a single chain leaves it.
2. A **tight cluster** (6+ enemies within ~2 m of each other) placed **more than
   7 m** from the impact point, so nothing there can be reached except by a
   chain hop.
3. Fire one tick and read the CLUSTER only:
   - **at most 5 enemies damaged** ⇒ one sphere. A chain hits one enemy per hop
     and there are 5 hops.
   - **the whole cluster damaged** ⇒ nodes sphere. One hop landed and splashed
     its neighbours.
4. Repeat with the cluster at ~15 m (two hops out) to check the answer does not
   only hold for the first hop.

**Confounds to avoid.** Multishot changes nothing here (the sphere and chains
from it take none) but a second enemy drifting into the initial sphere doubles
the chains and ruins the count — check Y is really 1. No Firestorm: enlarging
the sphere is exactly what you are trying to keep out of the test. Watch enemy
COUNT damaged, not damage numbers: the beam ticks 8×/s and the numbers pile up
under either model.

**Outcome mapping.** `true` (current) is already implemented; `false` flips one
line in `data/weapons/primary/torid_incarnon.yaml` and its pinned assertion in
`weapons_data`. Neither value changes a single-target result — the sphere adds
no damage to a target the beam already struck — so this is a multi-target
question and can wait for the 2D model without blocking anything.

---

## M16 — How fast can a bow be TAPPED? (Cernos Prime, uncharged form)

**Question.** A bow is played two ways — hold to full draw, or click as fast as
you can — and the second is a form of its own (`base`, half the damage per
arrow). What is its cadence?

Two readings, and they differ by 2.5x:

| reading | cycle | shots/s | where it comes from |
|---|---|---|---|
| **nock only** (implemented) | 0.65 s | **1.54** | wiki Fire Rate's bow formula, *"Effective Fire Rate = 1 / (Modded Charge Time + Modded Reload Time)"* — no fire-rate term, and a tap pays no charge |
| semi-auto cap + nock | 1/1.0 + 0.65 = 1.65 s | 0.61 | the uncharged attack's own `Trigger = "Semi-Auto"` and `FireRate = 1` in the data module |

The second would make tapping strictly worse than drawing (half the damage at
0.7x the rate), which is not how the weapon is played (user, 2026-07-31), and
the wiki names bows as the exception to the generic charge-weapon formula
precisely because their cadence carries no fire-rate term. So the first is
implemented — but it is an INFERENCE from a formula written for the charged
shot, not a measurement of the tapped one.

**Protocol.** Cernos Prime, no mods (a fire-rate mod would move the answer and
is the second half of the question). Simulacrum, one target, unlimited ammo.
1. Tap-fire as fast as the weapon allows for a fixed window — 30 s on a timer,
   or count against the mission clock — and record the number of arrows.
   Volleys of 3 arrows: count VOLLEYS, not arrows.
2. 30 s should give **~46 volleys** under the nock-only reading and **~18**
   under the semi-auto one. Nothing in between is expected; the readings are
   far enough apart that a rough count settles it.
3. Repeat with Shred equipped (+60% on a bow). Under nock-only the tap does
   NOT speed up (no charge to shorten, and the reload is untouched by fire
   rate); under the other reading it goes to ~22 volleys. **This is the
   cleaner discriminator** — it needs no accurate clock, only "did it change".

**Outcome mapping.** One number in
`data/weapons/primary/cernos_prime_uncharged.yaml`: `charge_seconds: 0.0` is
the nock-only reading. The other reading is not a different value of that
field — it is the generic charge-weapon formula (`charge + 1/fire_rate`),
which the engine does not implement yet and which a non-bow charge weapon
(Opticor, Scourge) would need anyway.

---

## M17 — Do Arch-Guns have an exilus slot, and is Zodiac Shred eligible?

**Question.** Two sources disagree about one Arch-Gun mod, and the answer is
worth a slot of free damage.

| source | says |
|---|---|
| wiki `Module:Mods/data` | `IsExilus: True` for **Zodiac Shred** (+90% Slash) |
| wiki `Category:Exilus_Weapon_Mods` | Zodiac Shred is **not** in the list — and the 74 that are, are ALL utility (ammo, zoom, recoil, silence, speed). Not one grants damage. |
| WFCD export | carries no exilus field for any Arch-Gun mod — silent, not a vote |
| wiki `Arch-Gun` page | does not say whether Arch-Guns have an exilus slot at all |

**Implemented: `exilus: false`**, which is the safer error — a wrong `true`
puts a damage mod in a free slot, a wrong `false` only costs an option. The
module's flag is otherwise perfect (153/153 against our verified rifle and
pistol pools), which is exactly why this one row is recorded rather than
quietly trusted. `verify_mods.py --type Archgun` reports the divergence on
every run BY DESIGN: it is an open question, and it should stay visible.

**How to settle it.** Equip any Arch-Gun with an Exilus Weapon Adapter (if the
slot exists at all) and try to seat Zodiac Shred in it. One look answers both
halves — whether the slot exists, and whether this mod may enter it.

**Until then** no weapon in the roster is an Arch-Gun, so nothing depends on
it; it becomes live the moment one is added.

## M2 — Simulacrum "Steel Path" toggle: does it still boost armor?

**Question.** The toggle was introduced (U33.5) as "+250% Health, Armor,
and Shields", but U36 removed the armor bonus from Steel Path missions.
Does the Simulacrum toggle still touch armor?

**Sketch.** Armored target (e.g. Grineer Lancer) at a fixed level; compare
damage numbers of the same weapon with the toggle on/off; any change beyond
×(1/2.5) health scaling implies an armor change. (Only affects how we use
the Simulacrum as a lab — missions are authoritative for the engine.)

**Result:** _not yet run._

---

## M18 — Sentinel aiming (answered), and the beam ammo rule (implemented)

Two questions the wiki does not answer, both raised on 2026-08-01, both about
weapons already in the roster.

### (a) Aiming — ANSWERED, and implemented

**A sentinel weapon is ALWAYS aiming** (owner, 2026-08-01). What it cannot do
is TRIGGER the on-headshot half of an aiming mod, because it never aims at the
head.

That is two facts, and the sim already had the second one: `default_headshot_pct`
is 0 for a sentinel, so no headshot lands and no on-headshot buff can fire.
The first is now stated too — `aiming` is forced true for a sentinel weapon and
the request cannot say otherwise, with the box shown ticked and DISABLED, the
same shape as infinite ammo. The state is real; the control is honestly
unavailable.

Why it was worth settling even though it moves no number today: all four
aim-gated rifle mods (Argon Scope, Galvanized Scope, Bladed Rounds, Catalyzer
Link) are CONDITIONAL, so a sentinel's `BaseOnly` policy kills them anyway.
A FLAT aim-gated effect would have been read wrong the moment one could reach
a sentinel weapon — Critical Focus is exactly that, and it is Arch-Gun only by
luck rather than by rule.

Evidence that agrees: Verglas Prime's stat table has no Zoom row and no Recoil
row, which is what "the player never aims it" looks like from the stat side —
the aim STATE is not the same thing as an aim-down-sights optic.

### (b) The 0.5-per-trace beam ammo cost — IMPLEMENTED, needs confirming

`ammo_cost` was read for the first time on 2026-08-01 (it had sat in every
weapon file while the sim spent a flat 1.0). The values come from the wiki:
"Beam Weapons consume 0.5 ammo per trace — unless they are Flamethrowers",
and the Larkspur Prime page states both of its own numbers, "0.5 per primary
tick" against "Alt-fire consumes 10 ammo per shot".

What changed, all exact:

| | before | after |
|---|---|---|
| Larkspur Prime, primary | 500 ticks to dry | **1000** (500 rounds ÷ 0.5) |
| Larkspur Prime, alt-fire | 118 shots / 120 s | **50** (500 rounds ÷ 10) |
| Verglas Prime | 14 reloads / 120 s | **8** (80 magazine ÷ 0.5 = 160 ticks) |

The Torid's Incarnon form keeps 1.0 per tick — that one IS measured (the
charge pool is not ammo, see MECHANICS "Continuous ammo cost").

**What settles it:** fire a full Larkspur Prime magazine on the ground and
count the ticks — 100 rounds should give 200. Then one alt-fire shot and read
the magazine: 100 → 90.

**Result:** _not yet run._

---

## M19 — Do two Deadheads stack? (Primary + Secondary on one weapon)

An Arch-Gun seats a PRIMARY and a SECONDARY arcane, so Primary Deadhead and
Secondary Deadhead can sit on the same weapon. They are the same effect twice:
+120% Damage per stack to 3 stacks, and +30% to the Headshot Multiplier.

**What we model:** two independent buffs. Six damage stacks, not three, and the
headshot bonuses add to +60%. Larkspur Prime at assumed-max, 100% headshots,
and the arithmetic is exact:

| arcanes | ratio | = |
|---|---|---|
| one Deadhead | 5.98x | 4.6 (base-damage bucket) x 1.3 (headshot bracket) |
| both | 13.12x | 8.2 x 1.6 |

**What the wiki supports:** the two rules each bonus obeys, and nothing about
the pair. `Secondary_Deadhead` states "The damage bonus stacks additively with
other damage mods like Hornet Strike" and "Headshot bonus stacks additively
with similar buffs, such as Prowl" — which is why the damage half sits in
Serration's bucket and the headshot half in one additive bracket. It says
nothing about two Deadheads, or about identical buffs from two slots sharing a
cap.

**The open question is the CAP.** If the game treats them as one named buff,
the second arcane refreshes the first and the ceiling stays 3 stacks — worth
2.99x here instead of 8.2/4.6 = 1.78x more. Independent buffs is the reading
we take because they are separate arcanes with separate names, and because a
shared cap would make the second one nearly worthless, which DE tends to say
outright when it is true.

**What settles it:** equip both on an Arch-Gun, get four headshot kills, and
read the buff icons — one stack counter at 3 or two at 3 each. Or compare a
body-shot damage number at full stacks against the same build with one arcane.

**Result:** _not yet run._

## M20 — Primary Frostbite could never earn a stack (2026-08-02)

Found while checking a community claim ("Torid with Deadhead/Merciless crushes
Bulwark/Frostbite"). `ArcTrigger::ColdStatus` was declared and matched in the
data loader, but no `bump_trigger` call in `dummy.rs` ever fired it — Toxin,
Electricity and Heat were wired, Cold was missed. Arcane stacking buffs seed at
`max_stacks`, so Frostbite spent one 12 s window at 40 stacks and then sat at
zero for the rest of every run. It listed, it described itself correctly, and
after twelve seconds it did nothing.

Measured on Cernos Prime (a bow: innate damage is physical, so a Cold mod stays
Cold), Thrax Lv 300 SP, 120 s, 150 runs, seed 7, rank 5:

| | kill score |
|---|---|
| no arcane | 2.722 |
| Frostbite, before | 2.955 (1.085x) |
| Frostbite, after | 5.585 (2.051x) |

The 1.085x is exactly the seeded window and nothing else, which is the
signature of the bug.

NOT a golden-value change: no golden test covered this arcane, and no in-game
measurement is claimed here — the fix makes the sim do what the arcane's own
card says. `every_on_status_trigger_is_fired_somewhere` now fails if any
on-status trigger is left unwired.

Separately, and NOT a bug: on the Torid the same arcane stays at ~1.1x however
it is built, because the weapon's innate Toxin absorbs any Cold mod (into Viral
alone, or into Gas + Magnetic alongside other elements). Cold never survives as
Cold on that weapon, so Frostbite has no trigger there. Primary Bulwark is
`kind: unmodeled` — it scales with the WARFRAME's armour, which a weapon
calculator has no model of — so it reports exactly 1.00x and no comparison
against it means anything.

## M21 — Puncture's Weakened was critting explosions (2026-08-02)

A sweep of the status models against the wiki, prompted by M20. Weakened's
crit-chance grant is real and correctly valued (+5% flat per stack, 5 stacks,
10 s — `Damage/Puncture_Damage`, which the summary `Status_Effect` page omits),
but that page states one exclusion outright:

> This is a flat critical chance buff (like Arcane Avenger), but does not apply
> to Area of Effect damage or Warframe abilities.

The radial stage keeps its own copy of the crit line and that copy added
`weakened_cc`, so an explosion crit off a debuff it is excluded from. The
lingering field never did — the radial's copy was the odd one out. Fixture:
an explosion with zero crit chance of its own dealt 3300 where a never-critting
one deals 3000, a 10% inflation from Weakened alone.

Reach: any AoE weapon that applies Puncture. No roster weapon carries Puncture
and a radial today, so no golden value moves — but a Puncture mod on an
Incarnon explosion is one equip away from it.
`weakened_never_crits_an_explosion` pins it, and asserts the DIRECT hit still
crits off Weakened, which is what the buff is for.

### The rest of the status sweep, checked and unchanged

Every other DoT and debuff matched the wiki exactly:

| | wiki | engine |
|---|---|---|
| Slash | 35%/s, 6 ticks, 1 s delay, bypasses armour | `BLEED_COEFFICIENT 0.35`, `BLEED_DELAY 1.0`, cinematic |
| Toxin | 50%/s, 6 ticks, 1 s delay, bypasses shields | `DOT_COEFFICIENT 0.5`, delayed, toxin share bypasses |
| Heat | 50%/s; strip 15/30/40/50% at 0.5 s; return 50/40/30/15/0 at 1.5 s | same, both ramps |
| Electricity / Gas | 50%/s, no delay (the 6 s event is a dud) | `immediate_ticks` |
| Viral / Magnetic | x2 at 1 stack, +25% each, 10 stacks | `ten_stack_amp` |
| Corrosive | 26% at 1, +6% each, 80% at 10, 8 s | `1 - (0.20 + 0.06n)` |
| Cold | 50% slow; +0.10x crit damage then +0.05x; 10th freezes 3 s, leaves 3; cap 4 under Overguard | all four constants match |
| Blast | 30% per stack on a 1.5 s fuse; the ORIGINAL target takes no AoE | single-target hit only, host excluded |

## M22 — Primary Acuity was an unconditional +350%/+350% (2026-08-02)

Found from a user question: "the headshot rate is zero, why is there still a
damage bonus?" Its card reads

> +350% **Weak Point** Damage
> +350% **Weak Point** Critical Chance. Multishot cannot be modified.

and the data file had `base_damage_bonus` + `crit_chance_bonus` — both
unconditional, on every shot, plus no multishot lock. Its own pistol twin
(`pistol_acuity`) had been modelled correctly all along, which is what made
the single wrong file easy to miss.

Also, from the same wiki page: "It cannot be equipped on sentinel or companion
weapons — only primary rifles." It was in the Verglas Prime's pool.

After (Torid, Thrax Lv 300 SP, 60 s, 100 runs, seed 7, vs the same build
without it):

| headshot rate | with Acuity |
|---|---|
| 100% | 1.53x |
| 0% | 0.53x |

The 0.53x is correct and worth stating: `disables: [multishot]` cancels Split
Chamber, and at zero weak-point hits nothing comes back for it. A mod that
can lose you damage is what the card describes.

NOT applied: the wiki's note that the weak-point damage bonus lands at 1.5x
the listed value is already in both files' comments but is not implemented —
that needs an in-game measurement, not a wiki sentence.

Two smaller fixes in the same pass, both "the panel and the sim disagreed
about which buffs exist":
- `enumerate_buffs` matched the OUTER effect, so a `WhileAiming`-wrapped buff
  (Argon Scope) produced no card while the resolver ran it. It unwraps now.
- Arcane buff cards were one per GRANT. Frostbite grants crit damage and
  multishot off the same Cold proc — one count by construction — so it is one
  card, and one config now reaches every spec its arcane owns.

## M23 — Semi-Rifle Cannonade stated its rules in prose and modelled none (2026-08-02)

The same shape as M22, found the same way — by looking at the one file whose
twin was already right. Its card:

> Only compatible with Semi-Auto Trigger. Fire Rate cannot be modified.
> +240% Damage / +1.5 Punch Through

`semi_pistol_cannonade` has carried `requires: semi_auto` and
`disables: [fire_rate]` since it was written. `semi_rifle_cannonade` had
NEITHER, plus a bare `- kind: fire_rate_bonus` with no value — a reading of
"Fire Rate cannot be modified" as an effect rather than as the lock it is. It
parsed to a zero-valued bonus, so it moved no number, and the lock went
unmodelled: Shred's +30% fire rate applied underneath it, and the mod paid its
+240% on a weapon it cannot go on.

Verified after: Shred is listed as a fire-rate source and the final fire rate
stays at the weapon's base, while the damage bonus pays (100 -> 505 with
Serration).

Values were already right (+240%, +1.5) — the mod-wide value sweep had
compared them against DE's card and found no disagreement. What the sweep
cannot see is a rule stated only in the description, which is why the
condition test from that pass exists.

### Open question, deliberately not changed

`traits_for` gives BOTH forms of a transform group the base entry's trigger,
so the Torid's Incarnon form (continuous) still counts as `semi_auto` and the
mod keeps paying there. That is a documented choice — "traits describe the
WEAPON" — and it is the arsenal's reading: you equip the mod because the
weapon's listed trigger is Semi-Auto. Whether DE keeps the bonus live once the
weapon transforms is UNVERIFIED, and needs an in-game measurement rather than
a guess in either direction.

## M24 — a one-run gain screen cannot rank a status mod (2026-08-02)

From "why does adding status chance LOWER the damage?" It does not. The quick
calc's FIRST pass is one run per candidate, and one run cannot see what a
status mod buys.

Torid, Thrax Lv 300 SP, 300 s, gain over the same build, three seeds:

| candidate | 1 run | 3 | 10 | 30 | 100 | 400 |
|---|---|---|---|---|---|---|
| High Voltage | 30.2% ±39.0 | 30.0 ±39.0 | 24.2 ±28.5 | 30.7 ±12.0 | 30.0 ±20.0 | 29.2 ±5.6 |
| Malignant Force | 31.4% ±38.0 | 42.4 ±3.9 | 44.7 ±2.5 | 44.2 ±6.2 | 43.5 ±8.4 | 42.6 ±2.4 |

The MEAN is roughly right even at one run — this is not a bias. The SPREAD is
the finding: ±39 points, wide enough to print a minus sign in front of a mod
worth +40%. A pure damage mod does not do this (Heavy Caliber: 70.5% at one
run, 69.8% at two hundred) because paired seeds cancel nearly everything about
it. A status mod's payoff is decided by which procs land, which is the one
thing the shared seed cannot hold still once the candidate changes how many
rolls happen.

No run count fixes it cheaply: High Voltage is still ±5.6 at four hundred runs.
So the screen now SAYS it is a screen — a one-run number prints as "≈+12%",
dimmed, with a tooltip explaining the band; only the second pass (the leaders,
at a tenth of the scenario's count) prints a bare number. The two-pass design
was already right; what was wrong was presenting its cheap half with the same
authority as its careful half.

## M25 — Spectral Serration paid +330% to builds that were not invisible (2026-08-02)

Third of the same shape (M22 Primary Acuity, M23 Semi-Rifle Cannonade). The
card is "+330% Damage **while Invisible**"; the file was a flat
`base_damage_bonus`, so every build collected it.

Invisibility is a WARFRAME state, and the fight now has a Warframe in it:
`condition: while_invisible` is asked of the arena's Tenno. The neutral Tenno
is visible, so the mod contributes nothing and the panel's row says why
("+330%, while Invisible"). Verified: Torid, Thrax Lv 300 SP, 120 s, 100 runs —
0.2865 with no fifth mod, **0.2865 with Spectral Serration**, 0.3437 with plain
Serration; and with `invisible: true` in the scenario the same build pays in
full.

(It first shipped as an unevaluable `CondBuff(BaseDamage)` — full value on the
panel, nothing in the sim. That was the right shape for a calculator with no
player in it, and it stopped being one the moment the player arrived.)

The condition test from M22 walked past it, because it knew the two phrases it
had been written for ("Weak Point", "when/while Aiming"). It now flags ANY
"while/when …" clause on a card whose effects carry no condition and no
trigger — verified to fail on this mod.

## M26 — the two arcanes that read a WARFRAME, and the one fact still missing (2026-08-02)

Primary Bulwark and Primary Overcharge were both `kind: unmodeled`, for the
same stated reason: "the value depends on the Warframe, which a weapon calc has
no model of". It has one now — a fight carries a Tenno — so both are modelled:

| arcane | card | model |
|---|---|---|
| Primary Bulwark | "+1% damage for each unit of armor past 1,000, up to +500%" | `tenno_scaled` off `armor`, `above: 1000`, `per_unit: 0.01`, cap 5.0 |
| Primary Overcharge | "While at or above 90% Energy: gain 35% of Max Energy as Multishot, capped at 350%" | `tenno_scaled` off `max_energy`, `per_unit: 0.0035`, `min_energy_pct: 0.9`, cap 3.5 |

**Checked by construction, not by measurement.** Torid, Thrax Lv 9999 SP, 30 s,
5 runs:

- no frame → both contribute nothing (5,348.9 DPS, identical to the arcane
  slot being empty), which is what "no frame chosen" should mean;
- `wf_armor: 1500` + Bulwark → 32,093.5 DPS, exactly ×6.0 — the cap is +500%
  and it lands in the base-damage bracket;
- `wf_energy: 257` + Overcharge → 15,255.7 DPS, and **Split Chamber at rank 5
  gives 15,255.7 DPS**. 0.0035 × 257 = +90%, which is Split Chamber's number,
  so the arcane demonstrably feeds the same multishot bucket a mod does;
- `wf_energy_pct: 0.5` → back to 5,348.9: the 90% gate holds.

**What is NOT verified, and is the whole of M26's ask**: which multiplier each
bonus JOINS. Both are modelled as additive with their family's mods, because
that is what every other "+X% Damage" / multishot source in this data set does
and what Primary/Secondary Merciless and Primary Plated Round state outright.
Nothing on either card says so. The measurement that settles it is the ordinary
one: a build with a known Serration bonus, in-game panel damage with and
without Bulwark at a known Warframe armor value. If the bonus is an independent
multiplier instead, only the bucket changes — the card's own numbers (1% per
point past 1,000, 35% of max energy, the two caps) are not in question.

## M27 — the buff seed decides nothing, or everything (2026-08-02)

Every stacking buff used to start at full stacks. The replacement rule is in
[`BUFFS.md`](BUFFS.md) §Activation policy: a timed buff starts at 0, a
permanent one starts full. This is what made the change necessary rather than
merely preferable.

Torid + Galvanized Chamber + Galvanized Aptitude + Primary Deadhead, 300 s,
60 runs, seed 7, KPM:

| target | full-start | zero-start | apart |
|---|---|---|---|
| Lv 30 | 524.80 | 520.00 | 0.9% |
| Lv 100 | 58.81 | 49.94 | 15% |
| Lv 300 | 38.82 | 28.63 | 26% |
| Lv 1000 | 22.80 | 7.95 | 65% |
| Lv 9999 SP | 4.87 | 1.95 | 60% |

The seed washes out completely where kills are fast — 0.9% at Lv 30, because
the fight rebuilds the stacks within seconds of the run starting. It dominates
where kills are slow: at Lv 9999 SP the build kills 1.95 times per minute, so
an on-kill stack is essentially never earned, and starting full granted it a
buff the fight cannot produce and then sustained it for the entire 300 s.

So the old default was harmless exactly where it did not matter and wrong
exactly where it did. Engagement length is not the lever people assume: at
300 s the two answers are still 2.5x apart, because nothing is re-earning.

Seven engine tests moved. All seven were asserting what full stacks are worth
or how they decay, and all seven now SEED the stacks they measure
(`arc_stacked`, an explicit `initial_stacks`) instead of inheriting a default —
which is what they should have done from the start, and why they were the
tests that broke. One was rewritten rather than seeded: Primary Crux's
weak-point trigger can now assert that a body-only run is IDENTICAL to no
arcane at all, instance for instance, which is a stronger claim than the
seeded version could make.

## M28 — Primary Frostbite stacked off procs that applied no status (2026-08-02)

**The claim** (user): a Cold proc landing on an already-FROZEN target does not
refresh the arcane. Correct, and the repo said so before the code did —
`data/debuffs/frozen.yaml` has carried `refreshable: false` with the note
"cannot be extended: Cold procs are inert" since it was written, and
`apply_cold_proc` has always honoured it for the DEBUFF.

**The bug.** The arcane did not read that answer. The trigger was bumped
BEFORE the proc and unconditionally:

```rust
arc.bump_trigger(&params.arcane.buffs, ArcTrigger::ColdStatus, at);
debuffs.apply_cold_proc(at, sd, ...);          // ← may be inert
```

So an arcane whose card reads "On Cold Status Effect" earned a stack from a
proc that applied no status. `apply_cold_proc` now returns whether a status
LANDED and the bump is gated on it. A capped stack list still counts —
pushing past a cap replaces the oldest, which is an application; only Frozen
returns false.

**OPEN, and it decides most of what Frostbite is worth here.** That last
sentence is an INFERENCE, not a source: nothing says a replace-oldest at the
cap counts as "a Cold Status Effect applied" for an arcane trigger. The
alternative reading is that the trigger needs the stack COUNT to rise, and the
two disagree exactly where it matters most (user, 2026-08-02).

An OVERGUARD holder caps Freeze at 4 and can never be Frozen (sourced —
`data/debuffs/freeze.yaml`, "Bosses and Overguard holders cap at 4 stacks").
So against one, the fix above never fires: Frozen never happens, and under our
reading every Cold proc keeps stacking the arcane forever, pinning it at 40.
Measured — Cernos Prime + Primed Cryo Rounds + a crit set, Thrax Lv 300 SP,
300 s, 40 runs:

| arcane | DPS |
|---|---|
| none | 19,951 |
| Primary Frostbite | 51,684 (**x2.59**) |

Under the other reading the arcane would stall at 4 triggers' worth and then
decay on its 12 s all-drop timer, and that x2.59 largely collapses. On a normal
enemy the two readings barely differ — the 10-stack cap is reached and Frozen
takes over. On an overguard holder they differ by everything, and the roster's
only enemy is an overguard holder.

**The measurement**: in the Simulacrum, on an overguard enemy, apply Cold until
the stack display pins at 4, then keep applying it and watch the Frostbite
counter. Still climbing/refreshing ⇒ this model is right. Stuck ⇒ the trigger
needs the count to rise, and `apply_cold_proc` has to report "the count went
up" rather than "a status landed".

**Measured impact: inside the noise floor.** Cernos Prime + Primed Cryo Rounds
+ Serration + Split Chamber + Point Strike + Vital Sense + Primary Frostbite,
Thrax Steel Path, 300 s, 120 runs, seed 11 (KPM):

| level | before | after |
|---|---|---|
| 300 | 10.32916 | 10.30742 |
| 1000 | 5.62010 | 5.63318 |
| 9999 | 1.81099 | 1.81099 |

±0.2%, and not consistently in one direction — fewer stacks shift kill timing,
which re-aligns the RNG stream. M24 puts a status build's one-scenario spread
far wider than this. Frozen lasts 3 s against a 12 s all-drop buff and takes
nine Cold stacks inside their own 6 s window to reach, so few procs are ever
wasted. Lv 9999 is identical because Frozen is never reached there at all.

This is a CORRECTNESS fix, not a number fix, and it is worth having as one: the
model now says the same thing in both places, and a build that does keep a
target frozen no longer collects an arcane it is not earning.

**Worth knowing while testing Frostbite**: on a weapon with innate Toxin, a
Cold mod does not give you Cold. The Torid + Primed Cryo Rounds is Toxin +
Cold = **Viral**, so there are no Cold procs at all and Frostbite never
triggers — the two runs were byte-identical before this was noticed. It is the
same fact behind Frostbite measuring ~1.1x on the Torid earlier.

## M29 — Reified Bane starts at the reload, not at the end of it (2026-08-03)

**The claim** (user, in game): Boar Prime's REIFIED BANE evolution grants its
conditional +14 base damage **the moment an empty reload begins** — "换弹的那一
刻就有了，不需要等待换弹完成". The wiki says the opposite: the bonus is
"applied after finishing a reload while the magazine is empty".

A measurement beats the wiki, so the measurement is what the repo records.

**Why it is not pedantry.** We model this half as permanently HELD — a
`stacking_buff` of one stack, permanent, open from t = 0
(`data/evolutions/boar_prime_reified_bane.yaml`, `EvoBdBuff`). Whether that is
EXACT or an OVERSTATEMENT is decided entirely by this timing:

| reading | the buff during a reload | held is |
|---|---|---|
| measured — up at reload START | up | exact |
| wiki — up at reload END | **down for 2.75 s of every magazine** | too generous, every cycle |

Boar Prime empties 20 rounds and reloads for 2.75 s. Under the wiki's reading
the gap is a real fraction of every cycle and "held for the whole run" would
inflate the build on a schedule. Under the measured one there is no gap at
all: the magazine empties, the reload starts, the buff is already back, and it
then "lasts indefinitely until a manual reload is initiated while the magazine
is not empty" — which the sim never does, because the sim only ever reloads
empty.

**What this constrains.** The day the buff becomes earned rather than granted —
the sim currently cannot express "starts off, turns on at the first reload" —
its trigger fires at reload START. Anything that waits for the reload to
complete is reproducing the wiki's error, and the engine doc comment on
`EvoEffect::FlatBaseDamageOnEmptyReload` says so at the point where it would
be written.

**Also from the same page, and also a correction to what the game shows**: the
in-game card MISPRINTS this half as +10 ("Reload From empty bonus is
incorrectly listed as +10 in game"). The effect is +14, which is the value in
the data.
