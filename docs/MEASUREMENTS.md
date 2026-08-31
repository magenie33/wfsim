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

**Confirmed as the roster-wide rule (owner, 2026-08-10).** The Phenmor's page
says mode switching *"has an animation equal to weapon's reload speed"*, which
reads like a 2.8 s revert on that weapon and a contradiction of the 1.0 s above.
It is not: **that sentence is about ENTERING the Incarnon form only**, and there
is no official figure for the way back on any weapon. So M9's measured 1.0 s
stays the revert everywhere, and the two facts are about two different halves of
the cycle rather than in conflict. Settled — do not re-derive a reload-length
revert from that sentence.

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

**Current model: ONE sphere, at the beam's contact point**
(`beam.chain.nodes_have_radius: false`). Flipped 2026-08-06 from the `true` this
carried since 2026-07-30 — the earlier value was the user's in-game read of a
clump lighting up, and the user retracted it on the falloff argument below.
**Still not a citation, and this protocol still stands**: an argument narrows
which default is defensible, it does not measure anything.

**The falloff argument** (user, 2026-08-06), the one that moved it. Unlike the
four signals below it is structural, not circumstantial:

- An explosion in this engine is *a separate damage instance with linear falloff
  from epicentre to edge* (MECHANICS §Area of Effect). Every `radial:` in `data/`
  carries a falloff — Torid base 1.0, Burston Incarnon 1.0, Phantasma charged
  0.5, Laetum Incarnon 0.2 — and Detonate has to declare `falloff: none` out loud
  to be the exception. **This sphere carries none**, because the wiki denies it
  what falloff attaches to: *"The damage radius is not a separate damage instance
  from the beam."*
- So the sphere is not an explosion at all — it is the beam's **hit-detection
  volume**, widening what the single instance touches. Which is exactly why a
  directly struck target *"is still only hit once."*
- A sphere at a chain node could not belong to the beam, whose contact point is
  elsewhere. It would have to be the node's **own** damage instance — an
  explosion, needing a falloff nothing in the wiki or the datamine documents.
  `true` implied five undocumented explosions per attack.

Third-party agreement, not a citation: `malurth.github.io/AoE-simulator` — the
author of the Torid/Primed Firestorm mechanics video — chains node to node with
no sphere at any node.

**The four circumstantial signals**, recorded when the value was `true` and now
pointing the same way as it. None is a statement about chain nodes:
1. the wiki calls the first one *"the **initial** damage radius"* — a qualifier
   that only earns its place if there is exactly one;
2. the sphere is defined *"from the point of impact **against a surface**"*,
   while a chain lands on an **enemy**;
3. the datamined attack table carries **no radius at all** for the Incarnon
   attack, while the Poison Cloud (a real AoE part) carries its falloff;
4. the chain sentence is boilerplate shared with Atomos and Amprex, neither of
   which spheres at a chain node.

What held `true` in place for a week was that four inferences do not outrank
someone with the game open. What moved it was not a fifth inference but the
player retracting the read — so the count never decided this, and it should not
decide it now either. The wiki still never addresses the question.

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

**Outcome mapping.** `false` (current) is what is implemented; `true` flips one
line in `data/weapons/primary/torid_incarnon.yaml` and its pinned assertion in
`weapons_data`. Neither value changes a single-target result — the sphere adds
no damage to a target the beam already struck — so nothing is blocked today.
It stops being free the moment the 2D model reads this line, which is why the
default was corrected before that model exists rather than after.

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

### The SHOTGUN one was still wrong, a day later (2026-08-03)

"By looking at the one file whose twin was already right" found two of three.
`semi_shotgun_cannonade` had neither `requires` nor `disables` and still
carried the bare zero-valued `fire_rate_bonus` — so the card rendered
"+0% Fire Rate" under a sentence that forbids modifying it, on a mod that Boar
Prime (full-auto) could equip and the optimizer could return as a winner
(user: "半自动野猪是装不了的").

The lesson is about the METHOD, not the mod: comparing a file against its twin
finds a difference between two files and stops there. The family invariant is
now a test — every Cannonade states its equip rule, its calc gate and its lock,
and carries no fire-rate EFFECT under a fire-rate LOCK — which is a question
about all three at once and cannot be answered by reading any one of them.

Two more rules landed with it. `requires_weapon: semi_auto` is an EQUIP rule
and removes the mod from the pool entirely, which is the layer that matters
for the optimizer: `requires` only makes an equipped mod inert, and a build
that cannot be assembled in the arsenal should never be offered at all. And
the lock is symmetric — verified in both directions, a fire-rate bonus and a
fire-rate drawback (Critical Delay's -20%) both vanish under it, so the mod is
worth MORE on a build carrying a negative, not less.

### The equip rule is asked of EVERY firing mode (2026-08-04)

The wiki states the rest of it on the mod's own page: "Weapons with an Incarnon
mode must have Semi-Auto trigger type for **both firing modes** in order to
equip this mod, such as Bronco / Lato / Lex Incarnon Genesis." Dual Toxocyst,
Laetum and the Torid are all semi-auto and all transform into something that is
not (full-auto, full-auto, a held beam), so all three lose the Cannonade the
moment the Genesis goes in — and keep it while it does not (user, 2026-08-04:
"只要没点第一个 evo 就视为还是纯半自动，那就可以带，如果装上了就不可以带").

So the pool is a question about the BUILD, not about the weapon:
`mods_data::pool_for_build(weapon, evolutions)` is the rule and
`pool_for_weapon` is that function with nothing installed. A firing MODE is the
weapon's own trigger plus that of any form an evolution UNLOCKS — a CHARGED
form is not one, because charged vs uncharged is chosen on every trigger pull
and the weapon comparison lists a single trigger for such a weapon (Cernos
Prime is "Charge", Larkspur Prime "Held"). That is the line
`FormKind::is_gauge_switched` already draws.

It also settles the `continuous` case the same way, without changing it: the
Torid's Incarnon form IS a beam and the weapon still cannot take Sinister Reach,
because its other firing mode is a grenade launcher.

Both modules obey it from the same call. The simulator resolves its build
against `pool_for_build` with the fight's evolutions — which includes the one
the requested FORM implies, so asking for the Incarnon cycle is asking for the
weapon that has it — and the optimizer, where evolutions are a search
DIMENSION, vetoes the (subset, variant) PAIR rather than narrowing the scope:
the same eight mods are a legal build under a set that leaves tier 1 out.

### Open question, deliberately not changed

`traits_for` gives BOTH forms of a transform group the base entry's trigger, so
the Torid's Incarnon form (continuous) still counts as `semi_auto` for the CALC
gate (`requires`) and the mod keeps paying there. That is a documented choice —
"traits describe the WEAPON" — and it is separate from the equip rule above,
which has been settled: a build that reaches the calc has already been ruled
equippable. Whether DE keeps the bonus live once the weapon transforms is
UNVERIFIED, and needs an in-game measurement rather than a guess in either
direction. (In practice the two now rarely meet: a build that can wear a
Cannonade has no Incarnon form to transform into.)

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

**It is the EXCEPTION, and that is the part worth writing down** (user,
2026-08-03). A reload-triggered effect fires on reload COMPLETION by default.
Reified Bane needs BOTH halves of an unusual trigger — the magazine EMPTY, and
the reload's first frame rather than its last — and no other evolution is known
to work this way. So it keeps a narrow variant of its own
(`FlatBaseDamageOnEmptyReload`) instead of becoming a general "on reload" with
a flag: the next reload effect should inherit the default, not this.

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

## M30 — a stat LOCK stopped at the mod bucket (2026-08-04)

From "Pistol Acuity 这个计算是不是有问题，应该要锁定的，好像没锁" (user,
2026-08-04). Five mods in the data carry a `disables:` lock, in two families
that say the same sentence:

| mod | card | locks |
|---|---|---|
| Primary Acuity / Pistol Acuity | "Multishot cannot be modified." | `multishot` |
| Semi-Rifle / Semi-Shotgun / Semi-Pistol Cannonade | "Fire Rate cannot be modified." | `fire_rate` |

And both pages state the same rule under it: **"Equipping this mod will set
weapon's <stat> to its default ignoring other bonuses, EVEN NEGATIVE
EFFECTS"** (wiki, Primary_Acuity / Semi-Rifle_Cannonade).

The implementation read that as "zero the MOD bucket". It is not what the
sentence says, and four layers of this model never pass through that bucket:

| source | stat | where it lives |
|---|---|---|
| an evolution's permanent stacks (Fevered Frenzy) | multishot | `WeaponBase::buff_multishot_bonus` |
| Final Fusillade's last-round add | multishot | `WeaponBase::multishot_on_last_round` |
| an arcane's live stacks (Primary Overcharge, Conjunction Voltage) | multishot | added per shot in the sim |
| an evolution's fire-rate bonus | fire rate | `WeaponBase::evo_fire_rate_bonus` |
| the weapon's Frenzy passive (×2.5) | fire rate | the BUFF BAR, in the sim |

All five survived the lock. The largest is the last: Dual Toxocyst + a
Semi-Pistol Cannonade kept Frenzy's ×2.5 cadence, so the sim reported roughly
two and a half times the shots the game can fire — on exactly the build the
Cannonade exists for.

The fix states the rule once and in two halves, because the panel is not the
last word on either stat. `resolve` shadows the out-of-bucket layers it can see
and publishes `ResolvedPanel::locked`; `DummyParams::locks()` is the sim's one
reader for the live ones. A locked row on the panel now says `locked_by` — base
== final with no sources is also what a build that bought nothing looks like,
and the difference is worth a line. Buff CARDS for a locked stat are gone too:
a control that moves no number is worse than no control.

### What this deserves a measurement for

Whether the lock really eats the WEAPON'S OWN PASSIVE. Frenzy is a mod in DE's
data (`/Lotus/…/FireRateOnHeadshotPistolMod`, a `default_upgrade`), so it goes
through the same stat pool the sentence says is ignored — which is why it is
modelled that way here. But "ignoring other bonuses" was written about mods you
choose to equip, and nobody has fired the combination and counted. Dual
Toxocyst + Semi-Pistol Cannonade, headshots, 60 s: the shot count settles it.

---

## M31 — a riven's two elements enter the hierarchy backwards, and a combined element may block the chain (2026-08-07)

From "带元素紫卡的最终元素适配可能有点问题" (owner, 2026-08-07), with one
weapon, one riven and two slot arrangements:

- riven card, top to bottom: **Multishot 87.9 · Toxin 71.2 · Electricity 72.9 ·
  Crit Damage −55.3** (owner, asked and answered: 先毒再电，从上往下)
- **A** — Magnetic / Cold / riven / Electricity → in game **磁力 / 毒 / 辐射**
- **B** — the Cold and the Magnetic swapped → in game **腐蚀 / 冰 / 辐射**

### What A settles

The wiki says it in the Damage page's own words: **"the hierarchy priority will
be given to the LAST elemental stat listed on the Riven mod"**, worked through
a riven with "+100% Electricity damage first and +90% Toxin damage last" where
the Toxin combines UP and the Electricity down. So a mod's own elements enter
the hierarchy in REVERSE of how its card prints them, and the engine was
entering them in print order.

The owner's card prints Toxin first and Electricity last, so the Electricity is
the one that reaches up to the Cold above it:

| | before | after |
|---|---|---|
| A | Viral + Electricity + Magnetic + Radiation | **Magnetic + Toxin + Radiation** |

which is his 磁力/毒/辐射 exactly. Only a riven can carry two elemental stats —
no mod under `data/mods/` has more than one, and one element reversed is itself
— so this changes the reading of riven builds and nothing else; the board did
not drift and no row on it carries a riven. The wiki's other half needed no
code: "if no other elemental damage mods are present, the elements on the Riven
mod will combine with itself" — reversed or not the pair stays adjacent, and
`/api/simulate` on the riven alone returns Corrosive.

### What B does NOT settle — the open question

Under the model in MECHANICS §3, **A and B are the same build**. Magnetic
Strafe grants an already-combined element, rule 7 keeps it outside the primary
hierarchy, and a thing outside the hierarchy cannot change where the Cold sits
relative to the riven. The engine returns Magnetic + Toxin + Radiation for
both. The game did not.

One model explains both readings, and it is a small change to rule 7: **a
combined element occupies its slot in the hierarchy and FLUSHES the pending
primary above it** — it does not combine, but a primary above it can no longer
reach a primary below it.

| | order | walk | result |
|---|---|---|---|
| A | Magnetic(c), Cold, Elec, Toxin | Magnetic passes · Cold+Elec = Magnetic · Toxin alone | 磁力/毒 ✓ |
| B | Cold, Magnetic(c), Elec, Toxin | Cold flushed pure · Elec+Toxin = Corrosive | 腐蚀/冰(/磁力) ✓ |

It is plausible as an implementation — one ordered list of every elemental
entry, walked once, keeping at most one primary pending — and it is
UNVERIFIED. It also predicts a Magnetic in B that the owner's list does not
name, which is either the list being partial (both arrangements carry a
Magnetic mod, so it is in both) or the model being wrong.

The engine is NOT changed on this. A hypothesis that fits one report is not a
measurement, and this one rewrites how every build with a 60/60 combined-element
mod reads.

### The experiment that settles it

Phantasma Prime, three mods, nothing else, in this slot order:

**Cold · Magnetic Strafe · Electricity**

| model | panel reads |
|---|---|
| today's engine | Magnetic + Radiation |
| the flush model | Cold + Electricity + Magnetic + Radiation |

There is no overlap, so one arsenal screenshot decides it. The control is the
same three mods as **Magnetic Strafe · Cold · Electricity**, which both models
read as Magnetic + Radiation.

---

## M32 — the Incarnon's explosion fired on every base-form shot (2026-08-07)

From "两个benchmark存的东西是不对的… torid的数据是完全不对的" (owner,
2026-08-07). The board was the symptom; this is what was under it.

A cycle fires TWO weapons in turn, and the shot loop switches to the active
form's params (`ap`) for damage, crit, status and forced procs. Two lines did
not: `radial_stage` and `co_mult_radial` read `params.radial` — the OUTER
params, which are the Incarnon form's — so a weapon whose Incarnon detonates
threw that explosion on every BASE-form shot as well.

### The measurement

Burston Prime, Serration only, Thrax Centurion 100, 4 s, 400 runs, **zero
headshots** — so a weak-point-charged gauge never fills and both sides report
**zero transforms**. The fight is base form from end to end in both.

| | DPS | sources |
|---|---:|---|
| pinned `base` | 1738 | direct 6713 · Slash 239 |
| `incarnon_cycle` | **2470** | direct 6210 · **radial 2584 (Heat)** · Heat 758 · Slash 384 |

**+42%**, and the whole of it in a radial dealing HEAT — an element the base
form has nowhere in its vector. After the fix the two are identical to the
digit: 1738.0 against 1738.0, same sources.

### What it cost the boards

Rescored, and the size of each correction is the share of the engagement spent
in the base phase — which is exactly what a leak from the other form should
look like:

| board | weapon | before → after |
|---|---|---|
| aimed | Burston Prime | −1.3% … **−2.7%** (10 rows) |
| aimed | Laetum | −0.2% … −0.8% (10 rows) |
| **no aim** | Burston Prime | 0.9572 → **0.5858 (−38.8%)** |

Only those two: they are the roster's Incarnon forms that carry a radial. At a
100% headshot rate the weapon is in its Incarnon form for most of the fight, so
the leak had little room; on the no-aim board it never transforms at all and
the explosion was the whole difference between a real score and a fiction.

### The near miss, which is the part worth keeping

The board records a `mode`, and eight of the nine Incarnon forms never
transform at a 0% headshot rate. So the obvious reading was "a cycle row that
never transformed IS a base row" — file it there, and the no-aim board stops
claiming a form nobody saw. That change was written, and the measurement above
is what stopped it: relabelling re-scores, and the published Burston Prime
moved 0.9572 → 0.5858 under it. The right conclusion was not that the label was
wrong but that **the two fights should have been equal and were not**.

Pinned as `a_cycle_that_never_transforms_is_its_base_form`, over a fixture whose
Incarnon declares a radial and whose base form declares none — with body-only
aim, so the gauge can never fill. It bites: 3500 against 500 before the fix.

---

## M33 — what base a Primary Debilitate split burns off (2026-08-08; base DECIDED, exponent OPEN)

The owner brought a community formula for a Primary Debilitate build, with an
in-game number beside it, and asked whether the case it describes generalises
(2026-08-08: "我们是否可以反推到一般情况呢").

```
Damage x Cyte% x Dot% x (1+Elemental) x (1+DotElemental) x Bane^4 x Elementalist

350 * 0.5 * 0.5 * (1+6+0.6+0.6) * (1+6+0.6) * (1+0.3)^4 * (1+0.9)
= 29591.20        in game: 29551   (-0.14%)
```

### It decodes exactly, which is why it is worth taking seriously

Every term is identifiable, and the one that pins it is the shard bracket. The
Violet Archon Shard reads "+30% (+45%) Primary Electricity Damage. Gain an
additional +10% (+15%) per Crimson, Azure, or Violet Archon Shard equipped."
Five Tauforged Violet: `5 x (45% + 15% x 5)` = **600%** — the bare `6` in both
brackets, and the count includes the shard itself (owner, 2026-08-08).

| term | what it is |
|---|---|
| `350` | Vectis Prime base |
| `Cyte%` `0.5` | Cyte-09's **Resupply** — "triggering an Extra Hit of … 50% for Sniper Rifles … that procs a guaranteed status effect from the selected element", at 100% Strength, on a sniper, with **Corrosive** selected |
| `Dot%` `0.5` | the elemental DoT coefficient |
| `(1+6+0.6+0.6)` = 8.2 | the **CORROSIVE** bracket — shards + the Toxin 60/60 + the Electricity 60/60, i.e. both components |
| `(1+6+0.6)` = 7.6 | the **ELECTRICITY** bracket — the component the split landed on |
| `(1+0.3)^4` | a normal faction Bane, four layers |
| `(1+0.9)` | Rifle Elementalist |

Read as a chain it is not two rules but one, applied at each link — the Extra
Hit is a damage instance that guarantees a Corrosive status, the target is
saturated so Debilitate splits that status into Electricity, and the split's DoT
is what the 29551 is:

```
Extra Hit      = 350 x 0.5 x 8.2          <- Resupply, sniper, Corrosive
Debilitate split instance                  <- guaranteed status, saturated -> Electricity
split DoT      = (that) x 0.5 x 7.6        <- the number on screen
                                     x Bane^4 x Elementalist
```

**THE FOURTH BANE LAYER IS RESUPPLY'S OWN HIT**, and the source says so
outright (owner, 2026-08-08, relaying it):

> WeaponInitialHit -> ResupplyInitialHit -> DeliberateInitialHit -> DeliberateDoT
> (Bane/Roar reapplies itself every other layer of damage)

Which reconciles with the wiki's `f^3` exactly: drop Resupply and the chain is
WeaponInitialHit -> split instance -> split DoT, three links. The `^4` is the
`^3` with one more producer in it, which is the answer to the question as
asked — the exponent is a COUNT, and `faction_at(f, depth)` already is that
count.

### Two laws, and only one of them generalises

**FACTION IS ALREADY GENERAL HERE.** `faction_at(f, depth)` is `f^depth` and the
depth composes by recursion, so nothing is hardcoded: a hit is 1, a status is
its parent + 1, and Debilitate's split reaches 3 because it goes through an
extra instance. The video's **4** is not a different rule — it is this rule with
one more producer in the chain. So the answer to "can we generalise from the 4x
case" is that the 4x case IS the general case; the wiki's 3 and this 4 are the
same law counted over different chains.

**THE BASE IS NOT.** The formula carries BOTH brackets — the parent's 8.2 and
the child's 7.6 — and the engine carries only the child's:

```
ours:  0.5 x ModifiedBase       x (1 + child bracket) x f^3
video: 0.5 x [the parent's hit] x (1 + child bracket) x f^4
       where the parent's hit already includes its own 8.2
```

The gap is a whole elemental bracket: **x8.2 on that build**, ~x2.8 on an
ordinary two-mod Corrosive one. It is not a correction, it is a different
weapon — which is exactly why the next section is careful about what the
formula does and does not demonstrate.

### THE CRUX — and it is narrower than it looked (2026-08-08)

Shipped for one commit, then reverted, because the owner put his finger on what
the evidence actually covers: **"那个resupply的例子就是说明，类似toxic lash的例子
啊，不是常规武器的"**.

The source's own analogy is the reason. Toxic Lash's page carries the worked
example:

> "with an unmodded weapon whose damage sheet says it hits for 200 damage, a
> Rank 3 Toxic Lash, and a Rank 5 Intensify, Toxic Lash will deal:
> 200 x 0.3 x 1.3 = **78** direct Toxin damage, and always trigger a Toxin proc
> that ticks for **78 x 0.5 = 39** Toxin damage per second"

39 is half of **78** — the ABILITY's own damage — not half of the weapon's 200.
So "base damage" is not a property of the weapon. It is whatever applied the
status, and DE has two rules for it:

| who applied the status | its base |
|---|---|
| the WEAPON's own hit | `ModifiedBase` = unmodded x (1 + BaseDamageBonuses) — **elements excluded** |
| an ABILITY / an instance | that thing's own damage number — **elements included** |

**The formula that decodes the 29551 is entirely in the second row.** Its chain
is `WeaponInitialHit -> ResupplyInitialHit -> DeliberateInitialHit ->
DeliberateDoT`, and the number the DoT burns off is Resupply's Extra Hit —
`350 x 0.5 x 8.2`, an ABILITY's damage. That is Toxic Lash's rule, demonstrated
on Toxic Lash's case. It says nothing about a plain weapon shot.

So the open question is exactly one thing, and only one:

> With no ability in the chain, does Debilitate's split DoT read the weapon's
> `ModifiedBase` (elements excluded, DE's weapon-status rule), or the weapon's
> whole modded hit (elements included, the instance rule)?

Both readings survive everything known. For `ModifiedBase`: the parent is still
a weapon shot, and DE's weapon-status rule is exactly the special case that
exists to keep a DoT scaling with its OWN element rather than with all of them.
For the hit: the arcane's intermediate instance "has no damage", so the DoT
reads THROUGH it to whatever is above — which in the video is an ability hit
including elements, and in the plain case would be a weapon hit including
elements. The `x f^3` proves the intermediate link exists either way.

**The engine keeps the `ModifiedBase` reading** — the one it has always had —
and `a_debilitate_split_burns_off_modified_base_not_the_hit` pins it so the
question cannot be settled by accident. Flipping that assertion from 1.0 to 2.0
and passing the hit's damage into the recursion is the whole of the change.

What the reverted commit cost is the argument for not shipping it: it moved
published board rows by up to **+112%** (Torid, no-aim) on an inference.

### More material, and what it changed (2026-08-08)

Asked to collect more (owner: "你再多搜集点资料"), and the sources changed the
shape of the question rather than answering it.

**The weapon-status rule is documented to the digit, and we match it.** The
Toxin page: `Toxin Proc Damage per Tick = 0.5 x Modified Base Damage x (1 +
Toxin Damage Bonuses) x (1 + Status Damage Bonuses) x (1 + Faction Damage
Bonuses)`, with `Modified Base Damage = Un-modded Weapon Damage x (1 + Base
Damage Bonuses) x (1 + Faction Damage Bonuses) x Additional Multipliers` and
the note "**Note the lack of elemental bonuses in the Modified Base Damage
formula**". The Electricity page says it twice as plainly: "Modded Base Damage
is not the same as normal damage calculations, **ignoring physical and elemental
damage bonuses**". Its worked example — 100 Puncture, Serration, Infected Clip,
Rifle Elementalist, Bane of Infested — comes out at `0.5 x 344.5 x 4.693 =
808.37`, which is this engine's arithmetic exactly.

**MELEE INFLUENCE SAYS A THIRD THING — AND IT IS FILED SEPARATELY** (owner,
2026-08-08: "melee influence是传染比较特别，你单记"). It is a SPREAD: the damage
it names is dealt to OTHER enemies, as the price of contagion, and a mechanic
that moves an effect sideways is not the same kind of thing as one that splits a
status on the target in front of you. Recorded here as a data point about how DE
scales derived elemental damage, NOT as the precedent for Debilitate. Its page:

> "When an elemental Status Effect is spread by Melee Influence, affected
> enemies are also dealt damage equal to **that element's damage from the
> original attack**" … "based on the amount of matching elemental damage after
> quantization, including effects such as Condition Overload and critical
> multiplier" … "Faction Damage Bonuses … are applied **twice** on damage done
> by Melee Influence"

Not `ModifiedBase`, and not the whole hit either — **that element's own damage
on that hit**. A player thread on Debilitate says the same thing from the other
end ("It scales based on the damage value of the element not the mods. So if you
have 100 gas damage the heat and toxin procs would be calculated against that"),
though nobody there measured anything — and a thread is not a page.

### DECIDED: (a), the weapon's own algorithm (owner, 2026-08-08)

"a版本吧，我觉得是对的，先按照a来设计". The weapon is the SOURCE, so the base is
computed the weapon's way — which is also the only one of the three that is
documented for a weapon-applied status, matched to the digit on the Toxin page's
own worked example, and already what ships. Nothing changes; what changes is
that the question is now closed by decision rather than left open, and the two
rivals below are what a measurement would have to overturn it with.

The three candidates for the plain weapon case were:

| | the split's base | on the M33 build |
|---|---|---|
| **(a)** what ships | `ModifiedBase`, elements excluded | 350 |
| **(b)** the reverted attempt | the whole modded hit | 350 x 8.2 = 2870 |
| **(c)** the Melee Influence rule | the COMBINED element's damage on the hit | 350 x 7.2 = 2520 |

**(b) and (c) are indistinguishable in the video's chain**, which is why it
decodes under both: Resupply's Extra Hit is entirely of the selected element, so
"the whole hit" and "that element's damage" are the same number there. The
video therefore rules out (a) for the ABILITY case and separates nothing else.

### THE EXPONENT IS NOW THE OPEN ONE — 3 or 2 (2026-08-08)

Choosing (a) puts a second question in relief, and the owner raised it in the
same breath: "我们已经多吃一次bane加成了，理论应该是只有2的，而不是3". If the
split's base is the WEAPON's `ModifiedBase` — the same base an ordinary weapon
status uses — then the arcane's instance is not acting as a damage layer, and
an ordinary weapon status double-dips faction, `f^2`. Charging `f^3` while
reading the weapon's base looks like having it both ways.

**The counter-argument, and it is the sources', not mine.** The wiki states the
three outright: "applied as a separate damage instance, causing Faction Damage
Bonuses to multiply the Damage over Time effect of Heat, Electricity, and Toxin
status **three separate times**". And the video's own description says the
instance "**has no damage**". Those two together are consistent in exactly one
way: the instance is real enough to add a faction layer and carries no damage of
its own, so the DoT's MAGNITUDE has to come from somewhere else — the weapon —
while the extra layer is the only trace the instance leaves. Which is also the
only thing it predicts that anyone can see, and is what this file has said since
M-notes were first written for this arcane.

**We are not double-counting it.** `ModifiedBase` here carries no faction at all
(`base_vector.total() x (1 + bd)`), and `fm2 = faction_at(f, depth)` supplies
every layer: `f^2` for an ordinary weapon DoT — which is the wiki's double dip,
exactly two — and `f^3` for the split. Three is a deliberate one-more, not a
stray multiply.

It is also the cheapest thing on this page to measure: the exponent is a RATIO,
so it needs no absolute numbers and no theory about the base at all.

### What decides it — three tests, in this order

On any weapon, in the Simulacrum, with a Corrosive build saturated to 10 stacks.
It needs no frame, no shard and no exalted weapon, and it reads as a RATIO, so
every mitigation, faction column, crit and body-part factor cancels out of it.

A pure Corrosive build cannot proc plain Toxin or Electricity at all — both are
combined into Corrosive — so **any Toxin or Electricity DoT on screen is the
split**. That is a clean signal, and it is what makes this cheap.

**TEST 0 — the exponent, and it settles `f^3` vs `f^2` on its own.** Take one
build, saturate Corrosive, read the split's tick with a Bane mod OFF, then with
it ON. Nothing else changes, so the ratio IS the exponent:

| reading | tick with Bane / tick without, for a +30% Bane |
|---|---|
| `f^2` (an ordinary weapon status) | 1.3^2 = **1.69** |
| **`f^3`** (what ships, and the wiki's number) | 1.3^3 = **2.197** |

30% apart, and it needs no absolute number, no unarmoured target and no view on
the base question. Do this one first.

**TEST 1 — does the base include the element at all?** Watch the **Electricity**
split's tick while adding a **Toxin** 60/60. Toxin is not in the Electricity
bracket, so under (a) nothing can move; under (b) and (c) the Corrosive the hit
carries grew, and the split grew with it.

| reading | with the Toxin mod added |
|---|---|
| **(a)** what ships | **no change** |
| (b) / (c) | up, by the Toxin mod's share of the Corrosive |

**TEST 2 — the whole hit, or just the element?** Only if test 1 moved. Add ONE
**Heat** mod in the LAST slot. Heat is the odd element out, so Corrosive still
forms and its value does not change — only the hit's total does.

| reading | with the Heat mod added |
|---|---|
| (a) / **(c)** | **no change** |
| (b) | (1+0.6+0.6+0.9)/(1+0.6+0.6) = **+41%** |

An IPS mod does test 2's job with a smaller swing and no second DoT colour on
screen, which may read more cleanly.

A single absolute reading works too, for a base-100 weapon with Serration:
`ModifiedBase` = 265, the Corrosive on the hit = 265 x 1.2 = 318, the whole hit
= 583, a Toxin split's bracket 1.6 — so **212** (a) against **254** (c) against
**466** (b). Against an unarmoured target, with no Bane, no crit and no
status-damage mods, those are far enough apart to tell by eye.

### Also settled by this: the split deals no damage of its own

The owner's other half — "殴打的那一下，是没有伤害的…就是直接上dot（电还是会立刻
电一下）" — is what the engine already does, and only the DATA said otherwise.
`settle_procs` applies the split as a status and never calls `target.apply`; the
Electricity tick that lands immediately is the DoT's own first tick (delay-0),
not a hit. `primary_debilitate.yaml` opened with "IT DEALS AN INSTANCE", which
reads as a damage number even though the paragraph below it says the opposite;
that is now stated once, in the direction the code goes.

---

### CLOSED 2026-08-09 — it was an EXTRA HIT all along

The wiki's `Extra_Hit` page names this arcane outright — *"a 0-damage Extra Hit
that applies a guaranteed status effect"* — and states the general rule its
status follows: *"Damage over Time status effects created by an Extra Hit will
use the Extra Hit Damage as Modded Base Damage"*, which is also why such a
status takes the ELEMENTAL bonuses an ordinary one is denied.

Read literally that gives ZERO here. The rule that covers both members (owner,
2026-08-09: "如果为0，那么就找上一级去找base") is that an Extra Hit **replaces**
the base its status would have used — so a 0% one replaces nothing and the level
above stands, which is the `ModifiedBase` this engine already used.

It also explains why the third reading — the full modded hit — decoded the 29551
above and still moved published board rows by +112% when shipped: that reading
is CORRECT, for an Extra Hit with damage. The Cyte-09 chain it came from is a
10–25% Extra Hit, and *上一级被 resupply 替换了*. Debilitate is the one member
with nothing to replace. See docs/EXTRA_HIT.md.

The exponent is closed with it: the same page derives `f³` rather than asserting
it, so the "理论应该是只有2的" doubt is answered — the missing rung is the Extra
Hit itself, which carries the bonus twice before its status carries it again.

## M34 — Primary Debilitate was dead on Blast, and only a run could tell ✅ (2026-08-08; threshold generalised 2026-08-10)

From "还有blast是可触发冰和火的" (owner, 2026-08-08). It is a one-line statement
of fact about a combination the arcane is supposed to cover, and the engine did
not cover it — not by omitting Blast from a table, but by making it unreachable.

`components_of(Blast)` has always answered `(Cold, Heat)`, and
`combined_stacks(Blast)` has always counted `debuffs.blast`. The unit test on
the split function passed for Blast the day it was written. What never happened
is the split.

**The reason is Blast's own cap.** Every other combination sits at ten stacks
and waits for an eleventh application — which is what "if an enemy HAS 10
stacks, inflicting the same Status Effect AGAIN" describes, and why the check
reads the count BEFORE this application. Blast does not sit anywhere: reaching
ten DETONATES and drains every stack (`detonate.yaml`: "reaching 10 detonates
everything early"). So the count a later application reads is 0..=9 forever,
and the condition can never be true.

**The tenth APPLICATION is therefore the moment the condition is met** — the
only instant a Blast target has ten, and so the only reading of that sentence
under which Blast is eligible at all.

That was implemented for Blast and for nothing else, on the reasoning that for
the other five the pre-application count is right and counting the new stack
would fire the arcane one application early. **That reasoning was wrong, and the
exemption was the rule.** See below.

### ✅ RESOLVED, and it was never a Blast rule (2026-08-10)

> 如果当前是9层，下一发是10层的话，就可以立刻触发其中一个（根据等级），而不是要
> 等到10级后，再打才触发。这样爆炸实际上是可以触发火或者冰的，而且并不像wiki说
> 的那么rarely
>
> 全部都是这种情况的，我实际测试了

— owner, 2026-08-10, on all six combinations.

So the threshold is the count the target is AT **including the stack this
instance is applying**, for every combined element. DE's card text ("if an enemy
HAS 10 stacks, inflicting the same Status Effect AGAIN") is one step late, and
the `if proc == Blast` branch that stood here for two days is gone:
`debilitate_split` takes `stacks_with_this` and the caller passes
`stacks_before + 1` unconditionally.

**What it costs the other five: one application.** They cap at ten and stay
there, so under either reading every subsequent application splits — the only
difference is the shot that reaches ten, which now splits too. On Blast the same
one-line rule is the entire mechanic, which is why the bug was visible there and
nowhere else.

**And the wiki's "rarely" is wrong about Blast for the same reason.** Under the
card's own reading Blast could never split at all; under the measured one it is
an ordinary member with no special case anywhere in the engine.

This is the shape the repo keeps running into (see
[derive triggers, don't list them]): a fix written as "…and also Blast" was the
general rule with five exceptions no one had tested. The way it got caught was
the owner testing the other five.

### What would have falsified it

A Blast build with a saturating status chance, a Bane, and the arcane at rank 5,
against a target that survives detonation. If Cold and Heat statuses never
appeared, DE would be evaluating the arcane strictly before the stack lands and
Blast would be genuinely exempt. Kept here because the same experiment now runs
the other way: it is `a_blast_build_actually_reaches_the_debilitate_threshold`,
and `the_tenth_application_is_the_one_that_splits` is its non-Blast twin —
nine applications must pay nothing, ten must pay. Both were re-run against the
old reading and both fail on it.

### Why it needed an end-to-end test

`debilitate_splits_only_a_saturated_combination` asks the split FUNCTION what it
does at ten stacks. It answered correctly for Blast the whole time. The gap was
that nothing ever handed it ten, which is invisible to any test that calls the
function directly — so `a_blast_build_actually_reaches_the_debilitate_threshold`
runs a saturating Blast build and asserts the arcane adds damage at all. It
bites: with the pre-application count it reports "added 0". The lesson
generalises past this arcane — a threshold and the thing that produces the
threshold are two different claims, and a unit test on the first says nothing
about the second.

The table test now walks all six combinations rather than Corrosive alone. Five
of six passing is exactly the shape of failure nobody checks for.

## M35 — which riven stats a weapon can roll is not derivable, so it was counted (2026-08-08)

**Reported** (owner, 2026-08-08, relaying a player): *"紫卡负面没投射速度。我们的
紫卡解析，还应该考虑灵化情况"* and then *"这个是盗贼的紫卡哈，灵化吃这个，所以就
可以装备"* — a real Furis riven carries Projectile Speed, the editor would not
offer it, and the reason given is that the Incarnon form uses it.

### What the editor was doing

`rivens_data::excluded_for` derived a weapon's rollable pool from two rules: the
wiki's *"weapons without more than 25% of a physical damage type usually cannot
roll that respective attribute… Exceptions exist on a case by case basis"*, and
"a stat the weapon does not have" (no ammo pool, nothing that flies, a sentinel
weapon nobody aims). GAUGE-SWITCHED forms were excluded from both, on the
argument that an Incarnon form is paid for with evolutions while a riven's pool
is fixed when it drops.

### The measurement

A survey of every riven family in the roster, from warframe.market's public
auction search — 26 families, up to 500 live listings each, ~12 000 real cards
(`scripts/survey_riven_pools.py`, output `data/rivens/pools.yaml`). A riven
carries 2-3 of ~24 class stats, so a stat that CAN roll appears in roughly 55 of
500 listings. Measured, a stat that rolls landed at **30-70** and a stat that
does not at **0-4**. Nothing real came near the floor, so the verdict is
three-way: rollable, never, or unclear — and unclear falls back to the rules.

**The derivation was wrong in both directions, on six of 26 families:**

| family | rules said | 500 cards say |
|---|---|---|
| Ocucor | no Impact/Puncture/Slash (9% Puncture, 91% Radiation) | all three roll (49/46/39) |
| Phantasma | Projectile Speed rolls (the plasma bomb flies at 25 m/s) | 0 of 500 |
| Phantasma | Zoom rolls | 0 of 500 — it has no scope, and no field says so |
| Boar | Zoom rolls | 1 of 500 |
| Phenmor | Puncture rolls (30%, over the line) | 0 of 500 |
| Karak Wraith | no Slash (7.75 of 31 is EXACTLY 25%, not "more than") | 45 of 424 |
| Sicarus | Puncture and Slash roll (Sicarus Prime is 30% each) | 0 and 0 |

Two entries at exactly 25% settle nothing between them: Karak Wraith's Slash is
25.00% and rolls, Vasto's Impact is 25.00% and does not. There is no threshold
that fits both, which is the point — DE's table is not a formula.

### What was decided

Three sources, in order: **a real card** → **a count over live listings** →
**the derivation**.

**WHICH FILE DECIDES:** the RULES do. `data/rivens/exceptions.yaml` overrides
them per family with each entry naming its evidence, and `data/rivens/pools.yaml`
— the survey — is read by a TEST and by nothing else (owner: "抓取只是来当验证
才对"; DATA_SOURCES §"Riven pools"). Everything below is what the survey FOUND,
and every finding became an exception entry carrying its count.

Why the survey does not decide: a re-run of the scrape came back "nothing rolls
anything" for all 26 families. Data the engine reads at calculation time would
have emptied every pool in the app without failing anything.

### On the Incarnon question specifically

**Counting the Incarnon form would have been wrong.** The Latron, Lex and Atomos
Incarnon forms each fire a literal travelling projectile, and their families show
**0, 4 and 0** Projectile Speed listings out of 500. The gauge-switched form stays
out of the derivation — but now because it was counted, not because of an
argument about when rivens drop.

The Furis is the mirror case and is why the exception list exists: hit-scan in BOTH
forms, 13 of 500 in the survey's unclear band, and a player's card carries it.
The likely reason is that DE's Incarnon form is a projectile internally whatever
the beam looks like — the wiki's Condition Overload catalog row for it reads
"Furis | Incarnon Mode | **Projectile**", and the Ocucor, the same held 12-tick
beam shape and hit-scan in our data, rolls Projectile Speed on 47 of 500 cards.
That is an explanation, not a rule: the Phantasma's bomb genuinely flies and
still rolls none.

### The negative half of the report

Projectile Speed **can** be the malus — the wiki's positive-only list is the four
elements plus Punch Through, and the survey finds negative Projectile Speed on
real cards. `data/rivens/*.yaml` already had it right (`malus: true`); it was
missing from the negative slot only because it was missing from the stat list
entirely.

### What would falsify it

Any in-game card carrying a stat this file marks `never`. Absence in 500 listings
is strong evidence and not a proof — one card beats the count, which is exactly
what `exceptions.yaml` is for.

## M36 — the Felarx's +2000% and Gun CO multiply ✅ (owner, 2026-08-08)

**Measured in game** ("我已经测试过了"). Devastating Attrition's 50% chance of
+2000% on a non-critical hit and CONDITION OVERLOAD are two independent
multipliers on this weapon; they do not share a bracket.

Both sources agree with the measurement, which is why it was worth checking
rather than assuming — agreement is cheap and a shared bracket would have been
invisible:

- the perk's own wiki note: *"Damage bonus is multiplicative to base damage
  bonuses such as Serration"* — so it is not in the base-damage bucket;
- the CO catalog lists the **Felarx** among the Multiplying entries, and the
  owner confirmed the answer is the same on BOTH of its modes, so CO is not in
  that bucket either on this weapon.

Two terms, neither in the bucket, nothing to share.

### What it pins

`raw = qtotal x part x crit x CO x faction x arcane x ATTRITION x ramp x ...` —
the two are separate factors in one product, and
`devastating_attrition_multiplies_with_gun_condition_overload` asserts it as a
RATIO OF RATIOS: whatever the perk is worth alone and whatever CO is worth
alone, having both must be worth their product. It also asserts the product is
nowhere near the additive reading, which is the answer being ruled out. The
perk's own 50% is replaced by 1.0 inside the test — a coin flip in the middle of
a measurement would need thousands of runs to say anything, and the question is
about the bracket rather than the odds.

### Why this weapon and not a rule

The CO half is per-weapon: the catalog lists the anomalies, and a weapon absent
from it is Adding. On a weapon where CO is Adding, its share of the damage joins
the base-damage bucket and this multiplication does not arise — the Attrition
term still multiplies, but there is no second free-standing factor for it to
multiply with.

## M37 — a Debilitate DoT eats Attrition TWICE, and it is a BUG ✅ (owner, 2026-08-08)

**Reported, measured, then explained.** A player asked the owner whether the
elemental hit that triggers Primary Debilitate can also trigger the Felarx's
20x, and the calculator said no:

> 大佬游戏里衰弱触发的dot可以再次触发逐枭凤歿的外围20倍但是网站里的计算器
> 显示不出来

He tried it:

> 刚刚试了一下，确实是可以触发的！直伤一次，附加伤害一次，dot一次一共3次牛吼，
> 强袭损耗在吃两次，最终的触发的dot会吃到三次方牛吼和441倍强袭损耗的伤害加成

> 大概测一下三次牛吼，两次强袭损耗，元素师，元素mod都能吃到。用凤殁测了一下，
> 只有牛吼增伤的情况下，dot跳一下，爆破使就没了

So the chain is **直伤 → 附加伤害 → dot** with a Roar layer at each step (the
`f³` this engine already applied as `DEPTH_DERIVED_PROC`, see M33), and
**Devouring/Devastating Attrition applies twice**: 21 x 21 = **441**.

### It is not a design — it is a leak, and the leak names the rule

The owner's reading, and the reason this is filed as DE's bug rather than as an
interaction (2026-08-08: "有bug，你理解吗，这个+21好像还会作用在由衰弱产生的dot
上面（非本意）"):

> 衰弱触发成功后，会进行一次伤害为0的伤害，但是是 0×bane乘区×以及概率的21倍乘区
> （50%概率触发，因为这个也没暴击），但是在算这个伤害产生的dot的伤害的时候，0的
> 部分被替换为上一级，但是又把这2个乘区也带进来了

The split fires a damage instance whose damage is **zero** — which is why the
wiki can call it a separate instance and say it "has no damage" in the same
breath. Zero still gets multiplied, by that instance's own faction bracket and
its own Attrition roll. Then the DoT is computed off it, the zero is **replaced
by the parent hit's value**, and the two multipliers already applied to the zero
are left in. One instance's multipliers on another instance's magnitude.

Being able to name the mechanism is what makes the third ruling below
predictable rather than a second measurement.

### What it pins

1. **A hit's Attrition roll travels with the statuses it applied.** A proc's
   magnitude is the applying instance's — that is why `crit_mult` was already
   carried into `settle_procs` — and Attrition is a per-instance multiplier of
   the same shape. This engine was passing 1.0.
2. **The split rolls a second one of its own**, on the zero.
3. **The split's roll lands even when the parent hit CRIT.** The zero instance
   has no crit of its own — there is nothing to crit — so "on a hit that is
   neither Critical nor…" is satisfied whatever the parent did ("50%概率触发，因
   为这个也没暴击"). A critting build therefore gets 1 x 21 here, never 1. This is
   the ruling that matters in practice, since the parent hit on a real Felarx
   build usually crits and its own roll is then worth nothing.

Two rolls and not three: the DoT itself never rolls, because a DoT is not a hit.
Had it rolled the number would have been 21³ = 9261, and had only the split
rolled it would have been 21.

`the_debilitate_dot_carries_two_attrition_layers` asserts all three, and each
fails on its own when removed: an ordinary Slash DoT must come out at **x21.0**,
a split's Toxin DoT at **x441** (±25 over 200 runs), and a fight that crits every
shot must still show **x21** on the split's DoT. The perk's own 50% is forced to
1.0 for the same reason as M36 — the question is which layers apply, not the
odds. Two details the test has to work around: turning the perk on consumes an
extra RNG draw per instance, so a single pair of runs compares two different
fights and only an average means anything; and Puncture is made immune, because
Weakened is a flat crit-chance buff on the victim that would otherwise set the
crit rate the test is trying to control.

### Why the explanation is believed, and what it does not settle

It is not a fit to 441 — it PREDICTS things that were not measured, and they
hold:

- **It predicts the 3-vs-2 asymmetry.** Faction lands at every step of the chain
  because a status always carries it one layer more than what caused it;
  Attrition lands only where an instance ROLLS, and the DoT does not roll
  because it is not a hit. Three and two fall out of one story. Any account
  where the split "just gets a bonus layer of everything" has to explain why
  Attrition is not also three.
- **It predicts the crit ruling**, which is the counter-intuitive one, and it
  was implemented from the prediction rather than from a measurement.
- **It is consistent about Cold** (owner, 2026-08-08): the roll picks one of the
  two components, and "万一roll到是冰，那就是一个带441倍率的冰（没有效果），要是
  是其他的毒/火/电，那就是个441的dot". A 441x multiplier on a status that has no
  damage payload is worth nothing — which is what this engine does anyway, since
  only a DoT type reaches `push_dot`. A theory that has to special-case Cold
  would be a worse theory.
- **It explains the wiki's two sentences at once** — "separate damage instance"
  and "has no damage" are contradictory until the instance's damage is a literal
  zero.

**THE CARRIER IS NOT GENERALISED, and that is a decision rather than an
omission** (owner, 2026-08-08: "我确信目前就这个21是非本意的，其他的还是按照之前
的建模来做"). The obvious next question was whether every free-standing final
multiplier double-dips — **Condition Overload** being the candidate M36 already
established is its own bracket on this weapon. It is not asked, and CO stays
CO¹: the owner plays this weapon and the 21 is the only term he has seen behave
this way. Should that change, the measurement is one run — hold the status count
fixed, compare the DoT with and without CO — and the term to add sits next to
`attrition` in the same struct.

The same shape showed up in M33's Cyte-09 chain (owner: "昨天的cyte-09的resupply
好像也有类似的情况，感觉有个东西被层层传递了"). That is what makes "a carrier
passed down the chain" worth treating as the model rather than as a story about
one arcane.

### Reproduced on the shipping site

A Felarx Incarnon cycle, eight shotgun mods making Corrosive
(`primed_charged_shell` + `shell_shock` + `toxic_barrage` + `contagious_spread`,
with `shotgun_elementalist` alongside), Primary Debilitate at rank 5, against a
level-9999 Steel Path eximus Corrupted Heavy Gunner, 60 runs of 30s through
`/api/simulate` in the shipping wasm build:

| build | direct | split DoT (Toxin+Electricity) |
|---|---|---|
| Debilitate, no Attrition | 1.28 M | 0.18 M |
| Debilitate + Devastating Attrition | 7.48 M | **7.86 M** |

The split's DoT grows **44x** while the direct damage grows 6.2x — the gap is
the second layer — and it ends up **larger than every direct hit in the fight
combined**, which is the shape of the owner's "dot 跳一下，爆破使就没了".

THE TARGET HAS TO SURVIVE TO 10 STACKS. On a level-150 gunner the same build
shows NO split at all with Attrition equipped and a healthy one without it: the
21x kills it before the tenth Corrosive stack lands, and the arcane's condition
is never met. That is the model working, not failing, but it means the
interaction is invisible in any scenario where the weapon simply wins.

### THE SPLIT'S ROLL IS ITS OWN COIN ✅ (owner, 2026-08-10)

> 衰弱自己再判定一次是否触发21倍伤害（自己的）……只有直击先触发21，衰弱自己的那个
> 0 伤害 extra hit 要自己再判断一次

Which is what the engine does, and now what a test says. The forced-chance runs
above cannot see it — with the perk pinned at 1.0 every roll succeeds, so "rolls
its own" and "copies the hit's" produce the same 441 — so the claim is made at
the perk's REAL 50%, where the two readings are far apart:

| | expectation of the DoT's multiplier |
| --- | --- |
| two independent coins | `E[hit] × E[split]` = 11 × 11 = **121** |
| the split copying the hit | `E[hit²]` = ½·441 + ½·1 = **221** |

`the_debilitate_dot_carries_two_attrition_layers` reads **x121**. The joint
distribution of the two rolls, instrumented while writing it, comes out
25/25/25/25 across (1,1) (1,21) (21,1) (21,21) — four equal cells, which is the
whole of what "independent" means.

**So a 21× hit does NOT guarantee a 441× DoT.** The four outcomes are ×1 a
quarter of the time, ×21 half, ×441 a quarter — and the ×441 the owner measured
is the top of that spread rather than the rule.

The same independence is why a COLD split is worth nothing however it rolls: it
takes its own coin like any other, and then has no damage payload to spend it on
(owner, 2026-08-08: "万一 roll 到是冰，那就是一个带 441 倍率的冰（没有效果）").

**A note on how this was nearly mis-read.** The first version of the test
compared 400 runs against a 200-run baseline and reported x243 — close enough to
221 to look like the engine was copying the roll. It was a ratio between two
different numbers of fights. The instrumented joint distribution is what settled
it, and it is worth remembering that a suspicious factor of ~2 is usually a
bookkeeping error rather than a mechanic.

### A CRIT COSTS THE SPLIT A COIN ✅ (owner, 2026-08-10)

> 如果直击是暴击的，但是后面的衰弱 dot 还是可以 roll 出 21，那么此时会带着前面的
> 各种 multiplier（暴击伤害，弱点暴击）……因为衰弱永远不暴击

Both halves are true and they pull opposite ways:

- **a critical hit is not eligible for Devouring Attrition**, so the HIT's coin
  is gone — one coin instead of two;
- **the split instance never crits**, so ITS coin is always live, and the DoT
  still inherits the hit's crit multiplier and its body part.

So the answer to "could it be more than 21x" is yes — it is `crit_mult × 21`
when it rolls. But the comparison that matters is between builds, and it is
arithmetic:

|  | expectation of the DoT's multiplier |
| --- | --- |
| not critting | `E[hit] × E[split]` = 11 × 11 = **121** |
| critting | `crit_mult × 11` |

**They cross at a crit multiplier of 11.** Measured, 200 runs a cell:

| build | Attrition is worth | split DoT total |
| --- | --- | --- |
| no crit | **×120.6** | 1.97e10 |
| always 3× | ×11.0 | 9.44e9 |
| always 11× | ×11.0 | 3.46e10 |
| always 21× | ×11.0 | 6.61e10 |

The ×11 is the SAME at 3×, 11× and 21×, which is what shows it is the hit's coin
that went missing rather than a scaled version of it. And a 3× crit build's
split DoT comes out at **half** a non-critting one's, despite the crit.

**This is the DoT bucket alone.** The direct damage still wants crits by a wide
margin and no real Felarx build gives them up — but it is the one place in this
model where two of the weapon's own perks pull against each other, and
`a_crit_costs_the_split_a_coin_and_pays_it_back_in_multiplier` pins both ends of
it.

### ✅ CONFIRMED IN GAME on a second weapon (owner, 2026-08-10)

> 就是 Debilitate 的那个 0 的 extra hit，是永远视为不暴击的，而不是一个可能暴击
> 的 0 伤害。我刚刚用凤殁的暴击提升到了必爆，debilitate 触发的 dot 伤害还是有几
> 率 ×21 的

Everything above rests on the split instance being **permanently non-critical**
rather than **a zero-damage hit that happens to roll crit against a zero**. The
two readings deal identical damage — zero either way — and are opposite about
eligibility: under the second, a build that crits every shot disqualifies the
split too and the whole extra layer disappears.

The Phenmor at guaranteed crit separates them, and it comes out the first way.
It is also a second weapon and a second perk: the original deduction came off
the Felarx's **Devastating** Attrition, this is the Phenmor's **Devouring**
Attrition, so the behaviour belongs to the SPLIT and not to one card. Nothing
changed — the engine passes a literal tier `0` at
`dummy.rs` `attrition: attrition * noncrit_mult(ap.noncrit_bonus, 0, rng)`, and
claim 3 of `the_debilitate_dot_carries_two_attrition_layers` already asserted a
fight that crits every shot still takes ×21. This upgrades that claim from a
reading of "on a hit that is not critical" to a run.

### What is still open

- **This is a bug, so it can be patched.** Nothing here is a designed
  interaction, and a DE hotfix that stops the zero from carrying its multipliers
  removes both extra layers at once. That is a reason to keep it in one place
  (the split's `InstanceScale`) rather than to generalise it — and the reason
  the arcane's card SAYS SO: `live_bugs:` on `primary_debilitate.yaml` is a
  fourth kind of admission, the only one that is not a shortfall (owner: "我要建
  立啊，但是标记可能非本意，我要忠实原本游戏，如果修了那我就改"). The other
  three tell a player the number is lower than the card promises; this one tells
  them it is right today and rests on something DE can take away.
- **The lingering FIELD's ticks** (Torid's cloud) roll their own crit tier, so by
  the ordinary argument they are instances and should roll Attrition. Left at
  1.0: no weapon in the roster carries both, and this measurement does not reach
  it.
- ~~**元素师**~~ — CLOSED. It is `shotgun_elementalist` (霰弹枪元素师), an
  ordinary elemental-damage mod, and it already reaches the split's DoT the way
  every elemental mod does: the split runs the normal proc path and picks up its
  own element's bracket. Nothing to change.
- **Whether an ordinary status DoT double-dips faction** remains M33's question.
  This entry changes the Attrition term only.

## M38 — Secondary Fortifier: the RULE is settled, the NUMBER is not (2026-08-09)

Audited against the whole wiki page at the owner's request. Everything
transcribed is right — x3/x4/x5/x6/x7/x8 by rank, Overguard only (and this
engine sends the WHOLE instance to Overguard while it holds, with no carry-over
to shields or health, so multiplying the instance is right), lost the moment the
pool breaks, and the steal half deliberately unmodelled and disclosed as such
(`secondary_fortifier :: overguard on damage`).

### MEASURED: a status tick takes it, and takes exactly what the hit takes ✅

**In game** (owner, 2026-08-09): Ocucor, 220 base + 225 Heat, into a Techrot
Babau Eximus's body. Left column is that tick's damage, right column is each
Heat DoT's:

| | without the arcane | with it (rank 3, "x6 Extra") |
|---|---|---|
| | 64 – 34 | 384 – 202 |
| | 103 – 53 | 672 – 346 |
| | 36 – 20 | 535 – 277 |
| | 74 – 39 | 725 – 372 |

**The DoT is 52% of its hit in BOTH columns** — 0.531 / 0.515 / 0.556 / 0.527
without, 0.526 / 0.515 / 0.518 / 0.513 with. That ratio is the whole
measurement, and it is the one number in this table that four uncontrolled
samples CAN pin, because it is taken within each shot rather than across the two
runs.

Half of ModifiedBase is what a Heat tick is, so 0.52 is the tick unmultiplied
relative to its own hit. **Under the old model it would have read 0.52 ÷ 7 =
0.075 with the arcane on.** It reads 0.52. The tick takes the same multiplier
the hit takes, once — which is what the reasoning below had already concluded
and is now a measurement rather than a reading.

### The reasoning it confirms

The wiki's own two sentences:

> "The Overguard steal effect can be 'inherited' if the first source of Heat
> status applied to an enemy was from a secondary with this Arcane active."
>
> "Extra damage to Overguard is **not inheritable and is dynamically applied**,
> so the effect is lost entirely after depleting the Overguard from an enemy."

**"Dynamically applied"** is the phrase that settles it: the bonus is not baked
in anywhere, it is checked when damage LANDS — which is exactly what a DoT tick
is. The card says "Deals x8 Extra Damage to Overguard" with no qualifier about
hits, and the same page says DoTs trigger the steal half.

**"Not inheritable" is not evidence against it.** It names `Heat_Inherit` — the
mechanic that attributes later Heat damage to whoever applied the first Heat
status — and says the damage bonus does not travel down THAT path. The owner
read it first (2026-08-09: "这里的继承应该是说……在 warframe 引擎看来，还是这把枪
造成的，所以战甲的 heat 伤害也可以吃到加成"), and it is the reading that makes
both sentences say something rather than one of them contradicting the card.

**ONCE, not squared** (owner: "那dot也是9倍，而不是9*9倍率吧"). Faction damage is
re-applied per derivation step because DE re-applies it — `faction_at(f, depth)`
— and nothing says that here. A tick is not a derivation; it is the same
instance's payload landing later.
`the_arcane_multiplies_a_status_tick_exactly_once` reads x8.0 and would read
x64 if it were treated like faction.

### MEASURED: "x8" is the EXTRA, so the total is ×9 ✅ (owner, 2026-08-09)

DE's card says "Deals **x8 Extra** Damage to Overguard"; the wiki's stats table
column is headed "Overguard Damage Buff" with the value "x8". Those two
phrasings disagree: "extra" reads as +8x on top of the hit (**x9 total**), the
table reads as the total (**x8**). There is no worked example on the page and no
datamined figure to hand.

This engine read it as the TOTAL until now (`rank0: 2.0` … `rankMax: 7.0`).
**The owner's call is ×9** ("应该是9倍，你先执行"), on the plain reading of the
word DE chose: `x8 Extra` is eight times extra, on top of the hit. The ladder
moves with it — `x3 Extra` … `x8 Extra` is ×4 … ×9 — so the stored bonus is now
the number DE prints rather than one less than it.

**DECIDED: ×9 at max, ×7 at rank 3** (owner, 2026-08-09 "应该证明了，就是*7",
reaffirmed 2026-08-10 "是*9"). The rows are NOT four matched pairs — they are
eight independent samples of a beam whose ramp and crit tier move under it, so
nothing here is meant to be divided row by row.

The arithmetic that survives unpaired samples is thin but points the same way.
Dividing the buffed column by each candidate and looking for the unbuffed
column's own values:

| ÷ | gives | against the unbuffed column (36, 64, 74, 103) |
|---|---|---|
| ÷6 | 64, 112, 89.2, 120.8 | 64 exactly, nothing else |
| **÷7** | 54.9, 96, **76.4**, **103.6** | **74 and 103**, both within 3% |
| ÷8 | 48, 84, 66.9, 90.6 | nothing |
| ÷9 | 42.7, 74.7, 59.4, 80.6 | 74.7 against 74 |

Two near-hits for ×7 against one exact for ×6, on four values and four
candidates, which is suggestive rather than conclusive on its own. It agrees
with the plain reading of the word DE chose, and the owner ran it.

**What would overturn it,** kept because a decision is not a measurement and
this one is worth 12.5% on every Overguard hit: hold the beam on a fresh Eximus
until the ramp tops out and the number stops moving, read the ordinary
(non-crit) tick with the arcane on and off, Overguard bar still up both times.
`with ÷ without` is the multiplier exactly, and the one line to change is
`rank0`/`rankMax` in `data/arcanes/secondary/secondary_fortifier.yaml`. Nothing
else moves with it.

Shipped ladder: ×4 / ×5 / ×6 / ×7 / ×8 / ×9 by rank.

One consequence worth knowing about: DE's card prints the EXTRA here while
`fill_x`'s "xX" convention exists because DE usually prints the TOTAL over a
stored bonus. The card text is therefore un-converted for this one effect rather
than the data being bent to fit a formatting rule, and the panel's own line says
both numbers ("×8 extra damage to Overguard (×9 in total)").

## M39 — Secondary Fortifier's value is LEVEL-SHAPED, and can be negative ✅ (2026-08-09)

Audited because the owner did not feel the arcane in game: *"我打200级的eximus的
堕落重型机枪手感觉没那么强啊，你是不是什么地方多算了"*. **Nothing is
over-counted** — at his level the model agrees with him.

Ocucor, the board's own top build, against a Corrupted Heavy Gunner, 40 runs of
300 s per cell:

| target | without | with (max rank) | gain |
| --- | --- | --- | --- |
| level 200 Eximus | 8.62 | 8.38 | **−2.8%** |
| level 200, no Eximus (no Overguard) | 27.01 | 27.01 | **0.0%** |
| level 60 Eximus | 66.17 | 61.94 | **−6.4%** |
| level 9999 Eximus | 2.16 | 2.83 | **+31%** |

The no-Overguard row is the control: **exactly** zero, so the "only while the
pool is up" gate is doing its job and nothing leaks past it.

### Why +31% at the ruler and nothing at 200

The official ruler is level 9999, where an Eximus carries **12.4 M** Overguard
and this weapon only gets through ~104 M of damage in 300 s — so the Overguard
is 12% of everything the gun does in the fight, and the arcane turns that into
1.3%. At level 200 the same pool is 366 k against a target that costs ~424 k to
kill *in total*: there is nothing left to save.

### Why it goes NEGATIVE, which is the interesting half

Monotonic in the multiplier — at level 60 it is −2.5% at ×4, −5.2% at ×6, −6.4%
at ×9 — so it is caused by the bonus rather than by noise. Decomposed:

| | without | with | |
| --- | --- | --- | --- |
| direct | 19.0 M | 22.6 M | **+19%** |
| DoT | 9.1 M | 4.6 M | **−50%** |
| total | 28.1 M | 27.2 M | −3.4% |

**Overguard carries no armor and a DoT ticks on its own clock.** So the window
in which the Overguard is up is a window where FREE ticks land unmitigated on a
unit whose health would keep 10% of them (2700 armor). The arcane shortens that
window from 1.9 s to 0.25 s and throws the windfall away — and at low level the
windfall is worth more than the direct damage the arcane adds.

Two modelled facts, both correct, producing a result neither of them announces.
It is also exactly why the arcane feels weak in the owner's own play and strong
on the board.

### The premise that had to be checked, and was ✅

The whole result rests on damaging statuses applying while the Overguard is up.
They do — owner-confirmed in game (2026-08-09: "可以在敌人身上啊"). Overguard
blocks CROWD CONTROL, not damage. Had it blocked damaging status too, the model
would have been over-crediting every DoT weapon against every Eximus in the
roster, which is a far larger error than the arcane this audit started from.

`a_dot_ticks_into_a_full_overguard_bar` pins it, and says in its own comment
that M39 flips with it.

### Not caused by the M38 tick change

Checked by removing it: the level-60 loss reads −6.5% / −6.7% / −6.2% across
three seeds with the tick multiplier in, and −6.6% / −6.7% / −6.5% with it out.
The behaviour predates 2026-08-09.

---

## M40 — Xata's Whisper decodes exactly, and two of its clauses are still open (2026-08-09)

### CONFIRMED A SECOND TIME by the wiki's own worked example (2026-08-09)

The owner supplied the `Xata's_Whisper` §"Interaction with Blast" section
verbatim, and it carries a four-line worked chain rather than a formula — which
is the strongest citation this interaction has, because each line checks the
next:

> A gun deals 100 damage per bullet, and we have Thermite Rounds, Rime Rounds,
> Stormbringer, Primed Bane of Grineer, and Xata's whisper at base strength:
>
> - the initial hit: `100 × (1 + 0.6 + 0.6 + 0.9) × (1 + 0.55) = 480.5`
> - its extra hit: `0.26 × 480.5 × (1 + 0.55) = 193.6415`
>   — *"the Faction Damage Bonus is applied again"*
> - the Blast detonation: `0.3 × 100 × (1 + 0.55)² = 72.075`
>   — *"Elemental Damage doesn't apply to Blast detonations and the Faction
>   Damage Bonus is applied again"*
> - the extra hit off the detonation:
>   `0.26 × 72.075 × (1 + 0.55) × (1 + 0.6 + 0.6 + 0.9) = 90.0433`
>   — *"the Faction Damage Bonus is applied YET again, and the Elemental Damage
>   Bonus is applied even though Blast detonations don't scale off Elemental
>   Damage Bonuses"*

Both oddities are visible in the last line alone, and the whole faction ladder
is visible across the four: `f¹` on the hit, `f²` on its extra hit AND on the
detonation, `f³` on the extra hit off the detonation.

`the_wiki_worked_example_reproduces_to_the_digit` runs it. **The relations are
exact; the absolute figures are not, and that is quantisation** — DE rounds each
element of the vector down to a step of the base, so the example's 310 is
300.3125 here. An illustration written to show a formula has no reason to carry
it, and this engine has every reason to. The test therefore asserts the four
RELATIONS, where the quantised total cancels, and states the one number that
differs and why.

It also rules out the two near-misses by name, because both are what a careful
reader would expect instead: the extra hit off a detonation WITHOUT the
elemental bracket (a detonation takes no elemental bonus, so why would the hit
off it), and WITHOUT the third faction layer (two is what every other status
gets). Neither is the number.


**Question.** What is an EXTRA HIT worth, and specifically what happens when one
fires off a Blast detonation — the interaction the owner named as the reason to
implement the ability at all ("注意这个和blast的联动").

**Answer: measured, and the model reproduces every number.** The owner supplied
a player's capture with video (2026-08-09). Per
[owner-supplied numbers are measurements](../AGENTS.md), it is used as one.

### The capture

A Magnus (98 base) with two 60/60 mods making Blast (+120% elemental) and a
Primed Bane of Grineer (+55%), Xata's Whisper at 100% strength, body shot:

| what popped | on screen | formula |
| --- | --- | --- |
| the hit | 323 | `98 × 2.2 × 1.55` = 334.18 |
| its extra hit | 135 | `× 0.26 × 1.55` = 134.68 |
| the Blast detonation | 71 | `0.3 × 98 × 1.55²` = 70.63 |
| the extra hit off the detonation | 63 | `× 0.26 × 1.55 × 2.2` = 62.62 |

Three of the four are exact. The hit reads 323 rather than 334 through the
Anatomizer's own modifiers, and its extra hit — which is Void and neutral
there — reads the full 135, which is itself a small confirmation that the extra
hit is a SEPARATE instance taking its own vulnerability column rather than a
share of the weapon's.

**The poster then adds an Electricity mod on camera and the extra hit moves.**
That is the direct demonstration of the strangest clause: a Blast detonation
takes no elemental bonus at all, and the extra hit copied from it takes the
whole bracket.

### What it settles

1. **Faction twice on an ordinary hit.** `0.26 × 1.55`, not `0.26`.
2. **Faction three times off a detonation.** The detonation is a status payload
   already at `faction_at(f, 2)`; its extra hit is at 3. Nothing in the engine
   hardcodes a 3 — `fire_extra_hits` applies one layer and the depth of the
   thing that triggered it supplies the rest.
3. **The elemental bracket applies to the detonation's extra hit.** ×2.2 here,
   out of a payload that has none.
4. **No second body-part factor off a detonation.** Stated by the CN card
   ("弱点倍率只会被计算一次") and consistent with the capture, which is a body
   shot and so cannot distinguish it — taken from the card.

`an_extra_hit_fires_off_a_blast_detonation_at_the_third_faction_layer` asserts
all four against these numbers.

### The EN and CN pages disagree, and CN wins on both counts

| | EN `Xata's Whisper` / `Extra_Hit` | CN 真理密语 | taken |
| --- | --- | --- | --- |
| rank ladder | 17 / 23 / 23 / 26 % | 17 / 20 / 23 / 26 % | irrelevant — max rank is 26% either way, and only max rank is modelled |
| duration ladder | 20 / 30 / 30 / 35 s | 20 / 25 / 30 / 35 s | same; 35 s |
| body part | the `Extra_Hit` formula shows it ONCE, inside `Weapon Hit Damage` | "同理，弱点倍率也会被计算两次" | **CN: twice** |

The body-part row is the one that matters, and it is not really a
contradiction — the EN ability page says "The ability double dips on faction
damage, **and body part weaknesses**" in the same sentence the formula page
elides. Two EN statements and one CN statement against one EN formula that does
not mention it either way. Modelled as TWICE.

### Two clauses still OPEN

**(a) Does a lingering FIELD tick trigger an extra hit?** Neither page says. The
EN mechanic page's rule is "most non-standard weapon hits", with an explicit
exclusion list (Bursting Mass's absorbed damage, Pathocyst's maggots) that a
cloud tick is not on; against that, a cloud tick is on its own clock long after
the shot. **Modelled as NO**, which is the reading that does not invent a
trigger, and it is the conservative direction for exactly one weapon in the
roster — the Torid, where the cloud is most of the output.

*What settles it:* Simulacrum, Torid + Xata's Whisper, one grenade into a
stationary target. Count the numbers per tick. Two numbers a tick = yes.

**(b) Does the extra hit's status roll read the weapon's LIVE status chance or
its modded listing?** The card says "based on the weapon's total status chance".
The direct-hit path passes the live per-instance chance (arcane stacks
included); the Blast-detonation path has no instance in scope and passes
`ap.status_chance`, the modded listing. The two differ only on a build carrying
Primary Crux or Sentient Surge, and only for the detonation's own Void proc,
which is worth one CO stack.

*What settles it:* a Primary Crux build at full stacks, counting Void procs off
detonations against Void procs off hits over a long engagement.

### Why the Void proc is worth anything at all

It deals no damage — a Bullet Attractor is a 2.5 m field for 3 s that redirects
fire, and this arena has one target and nobody shooting back. But Condition
Overload's own page lists the procs that count and **Void is on it**, so an
extra hit that procs buys a CO stack. That is the whole of its value here, it is
tracked exactly like Radiation's Confusion, and
`the_void_proc_pays_condition_overload_and_no_damage` pins both halves.

### Sources

- [`Extra_Hit`](https://wiki.warframe.com/w/Extra_Hit) — the general formula
- [`Xata's Whisper`](https://wiki.warframe.com/w/Xata%27s_Whisper) — EN card, and
  the Blast clause under Bugs
- 真理密语 (CN wiki, via the API) — three worked examples, the IPS-distribution
  rule and the body-part clause
- [`Damage/Void_Damage`](https://wiki.warframe.com/w/Damage/Void_Damage) — the
  Bullet Attractor's radius and duration
- the supplied capture above

---

## M41 — a hitscan Incarnon's explosion fires ONCE PER TRIGGER PULL ✅ (owner, 2026-08-11)

**Question.** The wiki's CO catalog attaches one sentence to the Braton, Burston
and Zylok Incarnon radial rows — "AoE does not scale off multishot" — and the
engine has carried it as `takes_multishot: false` since those weapons went in,
on that sentence alone. A Notes column is not a measurement, and the sentence
admits more than one reading: it could be a statement about the TABLE's own
arithmetic (don't multiply the AoE when computing a theoretical total) rather
than about what the game does, and the Opticor's copy of it lives under **Bugs**
rather than under Notes, so it could also have been hotfixed away since.

**Measured** (Braton Prime, Incarnon form, +150% multishot ⇒ multishot 2.5).
The form fires at 5.67/s and the trigger cannot be released fast enough for a
single round, so the shortest burst obtainable is TWO rounds. Two rounds at 2.5
produce ~5 pellets (2 guaranteed per pull, 50% for a third). Observed:
**exactly 2 explosions.**

That is one explosion per TRIGGER PULL, not one per pellet, and the gap it has
to clear is categorical rather than statistical — 2 against ~5. `takes_multishot:
false` is what the game does.

### What it rules out

Two readings were live before this and are now dead:

- **"The form has innate multishot 2."** It does not. The wiki's stat block
  gives the Braton Prime's Incarnon form AND its radial `Multishot: 1 (70.00
  damage per projectile)` each, and the Burston Prime's both `Multishot: 1
  (13.00 damage per projectile)`. Had the 2 explosions come from an innate pair
  of pellets, the conclusion would have been the OPPOSITE one — the radial
  scaling normally off a base of 2.
- **"The note is about the table's arithmetic, not the game."** It is about the
  game. The forum reports' summary — "multiplied in arsenal, but not in reality"
  — is the right way round, and the arsenal is the half that lies.

### What it does NOT settle

The measurement is the BRATON's. The Burston and Zylok families carry the same
sentence in the same catalog, keyed the same way (one row per family), and this
confirms that reading the sentence literally is correct — but their own rows are
still wiki-sourced. Worth one shot each if the weapons are to hand; the Zylok is
the cheapest to read, being charge-fired with a 500 IPS direct hit against a 700
Heat explosion.

It also says nothing about WHY. The correlation across the catalog is perfect —
every row carrying the sentence is a hitscan attack (six say "Hitscan" in the
attack name outright; Mausolon's says "Based on hitscan damage"), and no
Projectile-typed AoE row carries it — and the plausible mechanism is that a
hitscan shot has no projectile entity to hang an explosion on, so the radial is
spawned by the fire event instead. That remains an inference. Per the catalog
rule it is NOT what the engine acts on: `takes_multishot` is declared per entry
from the row that names it, never derived from a weapon being hitscan.

### Sources

- [`Condition Overload (Mechanic)`](https://wiki.warframe.com/w/Condition_Overload_(Mechanic))
  — the three Incarnon radial rows and every other row carrying the sentence
- [`Braton Prime`](https://wiki.warframe.com/w/Braton_Prime) — the Incarnon
  form's and its radial's `Multishot: 1` stat blocks
- [`Trumna`](https://wiki.warframe.com/w/Trumna) — "Explosion is unaffected by
  multishot" (Notes)
- [`Opticor`](https://wiki.warframe.com/w/Opticor) — "Explosion isn't affected by
  multishot" (**Bugs**)
- the owner's run above

---

## M42 — the Scourge's field dies when the NEXT THROW STARTS, not when it lands (owner, 2026-08-14)

**Question.** The wiki states the exclusivity and not its timing: *"Only one
field can be deployed at a time. Throwing the spear will remove existing
fields."* Removal at the new spear's IMPACT and removal at the throw's
initiation look identical on a single throw and are opposite mechanics on a
build that throws continuously — the first hands a spam build ~100% uptime, the
second hands it the least uptime of any way to play the weapon.

**Reported, verbatim:**

> 然后我测试发现，之前投掷过留下的东西，会在我投掷发起的那一刻消失

(*"then I tested it and found that what a previous throw left behind disappears
the instant I initiate a throw"* — the removal is keyed to the throw ACTION.)

**What it settles.** The field's own clocks are ceilings a throw build never
reaches. *"The field lasts for 20 seconds"* and *"pulses immediately on impact
then once every 5 seconds"*, so a build that throws every second gets **the
impact pulse and nothing else** — the every-5-s pulses require not throwing for
5 seconds, and the 20 s duration requires not throwing for 20. And because the
old field goes at the throw's START rather than at the new one's impact, there
is a DEAD BAND on every throw — the whole travel time, plus whatever the throw
animation costs before release — where no field exists at all.

So the FIELD ENTITY is worth least to the build that throws most, and the way
to hold one for its full 20 s is to throw ONCE and then fire the primary.

**Then the second half, which reverses that for the part that matters** (owner,
same day, answering the question this measurement had left open):

> 消失的只是立场，消失前被附加的立场是不影响的，这个立场效果就是虚空的特效

(*only the FIELD disappears; what it had already applied is unaffected — and the
field effect IS the Void effect.*)

Two things at once. The debuff on an enemy is **not** taken back with the field
that applied it, so a build throwing every 1.6 s re-applies a 4.7 s attractor on
every impact and the TARGET carries one continuously — the opposite conclusion
from the field entity's, and the one that decides whether a spam build has
attractor at all. And it is the Void Bullet Attractor, i.e. `DebuffState::
attractor` — the debuff the engine already had, reachable until now only from
Xata's Whisper.

**What it does NOT settle.** The headshot rate the field is worth: unchanged and
still the blocker (docs/UNMODELLED.md §Bullet Attractor). This pair of reports
settles the field's UPTIME and its identity; neither says what easier aiming is
worth, and the wiki refuses to ("does not guarantee a headshot").

**Status: WIRED, for exactly what it is worth here** (`attractor_seconds: 4.7`
on both thrown entries). One line in the Condition Overload counter and nothing
else, which is all `DebuffState::attractor` has ever been worth in this arena.
Three consequences fall out of the two clocks rather than being modelled:

- 4.7 s against a ≤1.6 s cycle means it is simply UP from the second throw on;
- the field is planted AFTER the throw's own pellets land, so a throw never
  counts the field its own impact planted — the ordering that claims least;
- the field's every-5-s pulses can never fire in a throw-only fight (the cycle
  is shorter than the interval), so omitting them is exact here, not an
  approximation.

Pinned by `a_thrown_speargun_plants_a_bullet_attractor_that_counts`, which
measures it through the CO count — a build with a CO bracket must beat the same
build that cannot see the field — and carries the negative control that the two
thrown entries are the only planters in the roster. Verified to bite: dropping
the plant makes the two builds identical to the last point.

### Sources

- [`Scourge Prime`](https://wiki.warframe.com/w/Scourge_Prime) — Characteristics:
  the 2 m field on heads within 14 m, 4.7 s on an enemy, a pulse every 5 s, 20 s
  of field, one field at a time
- the owner's test above

---

## M43 — a throw pays for its own reload, so the listed rate is HALF the cycle ✅ (owner, 2026-08-14)

**Question.** The wiki gives the spear throw a fire rate and, separately, the
sentence *"Throwing the spear consumes 1 ammo, then reloads the weapon."* The
entry read the first as a cadence and the second as a note, so the sim threw 40
times between reloads — the primary fire's magazine, on the attack that does not
spend it. The two readings are 1 % apart on a bare build and 60 % apart on a
fire-rate one, because a reload that never happens is a floor that never bites.

**Reported, verbatim:**

> throw的流程是这样的，当按下投掷的时候，先有一个蓄力的时间，然后投掷出去，接着换弹。蓄力的时间和射速有关。默认的蓄力时间是1s。

(*press → a WIND-UP, whose length is set by fire rate and is 1 s at base →
release → RELOAD.*)

**What it settles.** The reload is unconditional, not a magazine running dry, so
the cycle is `wind-up + reload` = 1.0 + 0.6 = **1.6 s** and the throw rate is
0.625/s, not 1/s. That is a `magazine: 1` weapon — the same shape a bow's nock
already has here (`cernos_prime.yaml`, and dummy.rs' "the cycle is charge +
reload however the two are ordered"), so the fix was the magazine and nothing
else. The wind-up being `1 / fire_rate` also makes the second clause fall out for
free: a fire-rate bonus shortens the wind-up and cannot touch the reload.

Scourge Prime, thrown, 180 s against a level-9999 Steel Path Thrax Centurion, no
headshots, finite ammo:

| build | before | after | |
| --- | --- | --- | --- |
| bare | 178 throws, 303 dps | **113 throws, 191 dps** | −37% |
| +Shred | 231 throws, 397 dps | 132 throws, 224 dps | −44% |
| +Primed Shred +Vile Acceleration +Speed Trigger | 400 throws, 596 dps | 194 throws, 281 dps | −53% |
| +Primed Fast Hands | 179 throws, 305 dps | 130 throws, 222 dps | reload went from +0.7% to **+16%** |

The last two rows are the point. Under the old magazine a fire-rate stack bought
its full multiplier and the mode's ceiling was set by fire rate alone; under the
real cycle the reload is a floor, so fire rate buys only the wind-up's share of
1.6 s and RELOAD SPEED becomes a real mod on this weapon — which is the opposite
of what the pre-fix build search would have told a player.

Magazine mods stay inert by construction and correctly so: a reload draws
`floor(capacity − current)` whole rounds, so a 1.66-round capacity still loads
one.

**Pinned by** `a_thrown_speargun_paces_on_wind_up_plus_reload`, which asserts
the cycle against the sim's own shot count and states the fire-rate half as an
inequality (throughput rises by strictly less than the fire rate did). Verified
to bite: restoring `magazine: 40` reproduces the old number exactly — *"178
throws in 180s, but a 1.600s cycle fits 113"*.

### Sources

- [`Scourge Prime`](https://wiki.warframe.com/w/Scourge_Prime) — "Throwing the
  spear consumes 1 ammo, then reloads the weapon"
- the owner's sequence above
## M44 — the sniper combo and the scope, IMPLEMENTED AND UNMEASURED (2026-08-14)

Both sniper mechanics now reach the number
(MECHANICS §7 §"THE SNIPER RIFLE"), and both are implemented from the wiki
alone. **No in-game capture backs either of them**, which is why this entry
exists: the repo's rule is that a faithful-looking implementation without a
measurement is not correct, and nothing here should be read as calibrated
until it is.

### What a capture would settle

Four questions the wiki does not answer, in the order they matter:

1. **Does the counter reach the DoT?** The multiplier is applied where every
   other final multiplier is, so a Slash proc off a combo'd hit inherits it —
   the same treatment Roar gets, and consistent for that reason rather than
   measured.
2. **Does an Incarnon form keep it?** The Vectis forms declare no combo and
   say so on their cards. A single scoped shot in Incarnon form with the
   counter visible under the reticle answers it.
3. **Do two Multishot pellets in one target really count as two?** The wiki
   says so outright; it is the one clause with a cheap in-game check (fire a
   Split Chamber'd Vectis Prime and see whether the counter goes up by 1 or 2).
4. **Does the second pellet of a shot pay the first pellet's increment?**
   Modelled as yes (each pellet reads the counter as of itself, like every
   other on-hit roll in the loop). Unknowable from the page.

### What it is worth today

Vectis Prime, base form pinned, Thrax Centurion lv 9999 Steel Path, 60 s,
100 runs, 100% headshots, eight mods (Serration / Split Chamber / Point Strike
/ Vital Sense / four elementals):

| combo | mean DPS | vs a counter earned from zero |
|-------|---------|-------------------------------|
| earned from 0 | 420,157 | 1.00x |
| held at 5 (x1.5) | 431,378 | 1.03x |
| held at 45 (x2.5) | 514,726 | 1.23x |
| held at 135 (x3.0) | 615,952 | 1.47x |
| held at 405 (x3.5) | 715,872 | 1.70x |

The interesting row is the first: **a fight long enough does not need the
card.** 76 shots at ~2.0 multishot is ~152 landing hits, so an earned counter
is already past the fourth tier by the end of a 60 s engagement, and over 180 s
(226 shots, ~452 hits) it passes 405 and the run's biggest hit lands at the
full x3.5. The card is for the short fight and for stating what a player walks
in holding — it is not how the multiplier is normally reached.

Played as its Incarnon CYCLE the same weapon gains far less (1.27x at a held
x3.5 against 1.70x in base form), because most of a cycle's damage is dealt in
a form that declares no combo. That gap is a claim about question 2 above and
nothing more.

### Sources

Wiki `Sniper Rifle` §Shot Combo Counter / §Zoom Buffs, `Vectis`,
`Vectis Prime`, `Vectis Incarnon Genesis` — cached under `vendor/wiki/`.


## M45 — the Mausolon's Lifted synergy, UNMODELLED AND UNMEASURED (2026-08-15)

The weapon's own loop, and the largest thing missing from its number. Its two
forms feed each other, which is the whole reason the cycle is worth simulating:

> `*Primary fire shoots fully automatic rounds.`
> `**Shots explode in a '1.8' meter radius on impact with a surface or enemy.`
> `**Damaging {{D|Lifted}} enemies causes up to 13 additional instances of direct hit damage.`
> `*Getting 5 kills with the Mausolon's primary fire will unlock an [[Alternate Fire]] that discharges a powerful laser that explodes on impact.`
> `**Shots explode in a '8' meter radius on impact with a surface or enemy.`
> `**Guaranteed {{D|Lifted}} proc.`
> `**After using Alternate Fire, additional kills are needed to recharge the laser.`
>
> — wiki `Mausolon`, raw wikitext, §Characteristics

So: the alt-fire lifts, and the primary then deals **up to 14x its direct
damage** into a lifted body. The status itself is modelled as of 2026-08-15
(`independent_procs: [lifted]`, 1 s, counted by Condition Overload); the extra
instances are **not**, and the weapon reads low for as long as the target is
lifted.

### What is missing, and why it is not a guess

**"Up to 13"** publishes a ceiling and no floor, and no rule for what decides
the count. The obvious reading is that the shot strikes a body repeatedly while
it floats, which makes the answer a function of where the body IS — and this
arena has no positions (docs/UNMODELLED.md §"no distance", §"no movement").
Writing 13 would put a 14x multiplier on the board that nobody can reproduce;
writing 1 would be inventing a floor. Neither is a measurement.

Mechanically it is an EXTRA HIT (docs/EXTRA_HIT.md): a second damage instance
beside a hit, worth a percentage of it. The machinery exists and only the
count and its trigger rule are unknown.

### What to measure

1. **Is the count fixed or does it vary?** Fire single shots into a lifted
   Grineer Lancer in the Simulacrum with the damage numbers on. Record the
   instances per trigger pull across ~20 shots. A constant 13 settles it in one
   session; a spread means it is positional and the honest model is a range.
2. **Is each instance the FULL direct hit?** Compare one instance's number
   against the same weapon's number on an unlifted target of the same unit and
   level. Extra Hit members supply a percentage, and this one's is unstated.
3. **Does the radial count, or only the direct?** The line says "direct hit
   damage", which reads as the 180 and not the 72 — worth confirming, because
   it decides whether Primary Compression touches it.
4. **Does it need the Mausolon's OWN Lifted?** Lift with a Warframe ability
   instead and fire. If it works, the synergy is not self-contained and the
   ability layer can feed it.

Until (1) and (2) land, the admission on `mausolon` stands and the board number
is a floor rather than an estimate.

### Sources

Wiki `Mausolon` §Characteristics (raw wikitext, transcribed above), and its
infobox — both columns of which were re-verified field by field on 2026-08-15
and agree with `data/weapons/archgun/mausolon*.yaml` exactly.

---

## M46 — the chill ladder, walked one stack at a time ✅ (owner, 2026-08-16)

**Setup.** Laetum, BASE form (crit multiplier 2.2), evolutions chosen not to
move damage, Lavos's +200% Cold infusion (forced procs), every shot on the
TORSO of a Demolisher — a target that cannot be frozen, so the ladder can be
walked all the way to ten instead of converting at the top. Non-crit held at
**192** throughout (192.3 before the display rounded it).

| stacks | crit | crit / 192.3 | implied bonus | ladder |
|---|---|---|---|---|
| 0 | 423 | 2.20 | 0.00 | — |
| 1 | 442 | 2.30 | 0.10 | 1st rung |
| 2 | 452 | 2.35 | 0.15 | 2nd |
| 3 | 462 | 2.40 | 0.20 | 3rd |
| 5 | 481 | 2.50 | 0.30 | 5th |
| 10 | 529 | 2.75 | 0.55 | **10th** |

Every row lands within half a point of `2.2 + 0.10 + 0.05 x (n - 1)`. Three
things fall out of it at once.

### 1. THE LADDER HAS TEN RUNGS

+0.55x at ten, one past the published table — the page stops at nine because on
everything it describes the tenth stack IS Frozen, whose own +1.0x replaces the
ladder anyway. Only a target that reaches ten WITHOUT freezing can show it.

A NINE-RUNG CAP SHIPPED FOR ONE COMMIT and this is what removed it. The
inference was the wiki's `Demolisher` line — *"will not freeze at 10 procs,
instead their movement will be Slowed by 90%"* — read across from the SLOW
table, whose ninth rung is 90%. The slow does cap at 90% (owner, confirmed);
the crit ladder does not, and a measurement beats a reading of a neighbouring
table.

### 2. A HIT IS SCALED BY THE STACKS ALREADY ON THE TARGET, NOT BY ITS OWN

The rows are labelled *before -> after*: the 423 was the shot that took the
target from 0 to 1, and it was scaled by **zero**. The Cold status a hit
applies does not pay that hit.

The engine already worked this way — `cd_abs` is read at the top of the pellet
body, before `settle_procs` applies that pellet's status — and now the ordering
is measured rather than incidental. Earlier pellets of the SAME pull do count,
because they landed first.

### 3. IT EXPLAINS A READING THAT LOOKED LIKE A FAULT

The same weapon on the same target alternated between **529 and 423** with an
unchanged non-crit of 192, which read as a bonus flickering on and off. It is
not: 423 is the first shot into a fresh target (0 stacks) and 529 is a shot
once the ladder is full (10). One rule, both numbers.

### Also measured

**Lavos's +200% Cold is x3.2** on this weapon against this target (60 -> 192
non-crit). The arithmetic checks: 160 base (64 Impact + 96 Slash) plus 320 Cold
is x3.0 before the target's own damage-type column and x3.2 after it.

### How to measure here

TAKE THE DIFFERENCE AT A FIXED NON-CRIT. Armour, faction, level and the
infusion are common factors of both crits and cancel; the absolute ratio does
not behave as cleanly (an earlier pair on this target, without the infusion,
gave 141/60 = 2.35, which fits no rung). The clean form is

    (crit_at_n - crit_at_0) / non_crit = the nth rung

which is how +0.55x was separated from +0.50x: `(529 - 423) / 192 = 0.552`
against a nine-rung prediction of 0.495.

---

## M47 — a body is 0.2 m across the floor, measured by walking into one ✅ (owner, 2026-08-16)

Walking into an enemy stops at **0.4 m centre to centre**. Two bodies of the
same size touching at 0.4 m makes each of them **0.2 m**, and that is the whole
derivation — the closest approach IS twice the radius, so the one quantity a
player can read off the game gives it directly.

`space::BODY_RADIUS_M` is 0.2 and `CONTACT_RANGE_M` is 2r = 0.4.

**IT REPLACES A GUESS OF 0.25 m** that shipped for one day. That number came
from taking the circle of the same AREA as a 0.6 x 1.8 m humanoid silhouette —
an attempt to put a body's HEIGHT back into a flat model, on the reasoning that
a real spread cone spends half its deviation vertically. It was wrong twice:
the plane is the model (owner, 2026-08-15), and `headshot_pct` already answers
where on a body a landed pellet went. The owner's original 0.2 m was right and
now has a measurement under it rather than a shrug.

**WHAT IT MOVES.** Nothing at contact, and everything past it:

- CONTACT IS INVARIANT under the radius. The hit test at contact is
  `r / 2r = 0.5` for any r, so both boards, every golden value and the two
  entries whose aimed cone is wide enough to miss at contact (the Mandonel's
  uncharged 60 degrees, the Cryotra's 40) are exactly where they were.
  `one_fight` reports every answer unchanged.
- BEYOND CONTACT a smaller body is a harder target. The same 2 degree cone that
  missed a 0.25 m body past about 7 m misses a 0.2 m one past about 5.7 m.

### AMENDED 2026-08-20 — the radius is 0.25 after all, and there is only one number

The owner: the Tenno's radius and an enemy's are both **0.25 m**, and
`BODY_RADIUS_M` and `BODY_MATERIAL_M` *"就应该是一个数字"* — should be one number.
They now are: the material is `2r`, the diameter, because a body is a circle.

**THE PENETRATION TABLE IS WHAT DECIDES IT, and it was in the repo the whole
time.** `a_body_costs_what_the_wiki_table_says` brackets a humanoid's material to
`(0.4, 0.5]` across thirteen published cells — 0.4 m fails on three independent
mods, 0.5 m works on Vigilante Offense. Under one constant the material IS the
diameter, so 0.2 m of radius gives 0.4 and is **excluded by that table**, while
0.25 gives exactly 0.5. The table was being read as evidence about a separate
constant when it is evidence about this one; that reading is what forced the
split in the first place.

**WHAT THIS MEASUREMENT ASSUMED.** The derivation above is "two bodies of the
same size touching at 0.4 m makes each of them 0.2 m" — which requires the walk-in
stop distance to be exactly the sum of two radii, with no push-out margin between
the capsules. Nothing measured that step. Two independent sources now say 0.25
against one derivation that needed an assumption.

**WHAT IT MOVES: still nothing at contact.** `one_fight` reports every answer
unchanged on all three shapes and every golden value holds, for the reason this
entry already gives — the hit test at contact is `r / 2r` for any radius. Past
contact the effect is this entry's own arithmetic read the other way: a 2 degree
cone reaches a body to about 7 m again rather than 5.7 m.

**STILL OPEN: whether the HIT TEST should read this radius.** What is measured
is how much FLOOR a body occupies — which is what decides where two of them can
stand. That the same number decides whether a pellet reaches one is the model's
choice, and DE publishes nothing to check it against: the wiki's `Area of
Effect` gives zone shapes and never says whether a radius is measured to a
body's centre or its surface, `Hit Mechanic` covers only the player's side, and
`Line of Sight` describes an enemy as three rays to head, torso and feet — a
vertical segment with no width at all.

The experiment that would settle it is unchanged: stand a known distance from
one stationary enemy, fire a counted number of pellets from a weapon of known
spread (the per-attack `MinSpread` from the wiki's weapon module), and count
what lands. Two ranges and two spreads over-determine it.

---

## M48 — the Burston Prime's CO reads 13 of its 55 on the DIRECT hit too ✅ (owner, 2026-08-16)

**Setup.** Burston Prime, Incarnon form, ONLY Forceful Finality (+42 base
damage) and Galvanized Aptitude equipped, every shot on the TORSO, unarmoured
target.

| Aptitude stacks | status types on target | direct (crit) | radial |
| --- | --- | --- | --- |
| 1 | 1 | **181** | — |
| 2 | 1 | **196** | **65** |
| 2 | 2 | **227** | **76** |

…and four on the **BASE form**, which is the independent confirmation — another
attack, another crit multiplier (1.8), another fraction (46/88 = 0.523 against
the Incarnon's 0.236), and one reading that is the REFERENCE the others are
divided by:

| stacks | status types | direct (crit) | / bare |
| --- | --- | --- | --- |
| — | 0 (bare crit) | **188** | 1.0000 |
| 1 | 3 | **306** | 1.6277 |
| 2 | 3 | **423** | 2.2500 |

    1 + 0.4 x 1 x 3 x f = 1.6277  ->  f = 0.5231
    1 + 0.4 x 2 x 3 x f = 2.2500  ->  f = 0.5208
                                      46/88 = 0.5227

Both within 0.4%, on a form whose fraction is a different number entirely.

The target was a Corpus **Crewman**, which is where the x1.19 comes from: the
damage-type column is the faction's, and Corpus is `puncture: 1.5`. Our column
puts the bare crit at `(26.4 + 26.4x1.5 + 35.2) x 1.8 = 182.2` against a
measured **188** — a 3.1% gap that is a SEPARATE and much smaller question (the
shield pool has its own column, and which pool a Crewman's first hits land on
depends on its shields) and that the ratios above are immune to.

**TAKE THE RATIO TO A BARE HIT, NEVER THE ABSOLUTE.** The target's damage-type
column multiplies everything and cancels in the ratio — here it is x1.19 on the
base form's IPS mix (`188 / (88 x 1.8) = 1.187`) and about x1.0 on the Incarnon
form's pure Heat, which is the only reason the Incarnon absolutes fit on the
nose. Working from absolutes without the bare reading made the base-form
readings look like FOUR status types when three were reported and three were
right; the bare crit dissolved that immediately. The same lesson as M46's
`(crit_at_n − crit_at_0) / non_crit`.

### What was wrong

The catalog's 24% was read as belonging to the **radial alone** — the row names
"Incarnon Form Radial Attack" — so the direct hit computed its CO term on the
full evolved 55. The error is multiplicative in the CO term, so it grows with
the build:

| reading | game | engine before | overstated by |
| --- | --- | --- | --- |
| 1 stack, 1 type | 181 | 231 | +28% |
| 2 stacks, 1 type | 196 | 297 | +52% |
| 2 stacks, 2 types | 227 | 429 | **+89%** |

**THE EXCLUSION IS THE PERK'S, NOT THE ATTACK PART'S.** Once stated that way it
needs no new machinery: `co_base_excludes_this_evolution` already sets the
weapon's `co_base_fraction`, and the roster already carries the flag on eleven
other perks (docs/CATALOGS.md). It is now on both tier-2 +42 options of both
Burston variants — Forceful Finality and Fortress Salvo, flagged together the
way the catalog already flags the Atomos's two tier-2 options.

### Consequence for the board

The Burston Prime's published score was computed on the overstated direct hit
and will FALL when the board rescores. That is the correction working: the
weapon was ranked on damage the game does not deal, and the size of the drop is
a function of how much CO the build was carrying.

## M49 — the Dual Toxocyst computes CO on a flat 75, and Carnage Reign's +33% is GATED, not dead ✅ (owner, 2026-08-16; resolved 2026-08-26)

Two findings from one session on 毒囊双枪, and each overturns something this
repo had written down. Galvanized Shot throughout (40% a stack per status type).

**THE READINGS, verbatim:**

> 啥没有，125，250
> 原来是75
> 3层是，+120%
> 3层，2debuff，305（+50base）
>
> 实际是，125*（1+2*1.2*（75/（125）） = 305
>
> 3层，2debuff，315 630
> 3层，1debuff，225 450
> 1层，1debuff，165
>
> 135*（1+2*(0.4*3*（75/135）+0.33)） = 315
> 135*（1+1*(0.4*3*（75/135)） = 225
>
> 2*1.53*75/135==1.33
>
> 大概意思是2个evo实际上计算gunCO的时候，都不考虑灵化带来的加成，而且+60的evo
> 的那个33%也完全不生效。我试过卸下gunCO的mod去打带状态的敌人，也没有加成，伤害
> 和原来一摸一样。并且平常和incarnon都是生效的

(The `1.53` in the last line is a slip for `1.2`; the `1.33` it produces is
right. The `+0.33` inside the 315 line is the term being ruled OUT — carried
through it gives 404, and the reading is 315.)

**ONE EXPRESSION FITS ALL FOUR:**

```
damage = panel + 75 x 0.4 x stacks x types
```

| panel | stacks | types | computed | measured |
|---|---|---|---|---|
| 125 (Fevered Frenzy) | 3 | 2 | 125 + 180 = **305** | 305 |
| 135 (Carnage Reign)  | 3 | 1 | 135 + 90 = **225** | 225 |
| 135 | 1 | 1 | 135 + 30 = **165** | 165 |
| 135 | 3 | 2 | 135 + 180 = **315** | 315 |

### 1. The CO base is the unevolved 75 under EITHER tier-2 option

The catalog names only Carnage Reign for this weapon:

```
Dual Toxocyst | Incarnon Mode | Projectile
  Attack Unmodded Damage ......... 75 or 135 (with Evolution II Perk 1)
  Actual CO Bonus at +100% ....... 75
  Notes .......................... CO-bonus does not use base damage increase Evolution
```

and CATALOGS' standing rule is that **ABSENCE MEANS ORDINARY**, so Fevered
Frenzy's +50 was modelled as feeding the CO term in full. It does not, and the
gap is not subtle: a CO term on the full 125 gives `125 + 125 x 2.4 = 425`
against a measured **305**.

**The rule is not repealed.** It has held for every other row and the five
remaining negative controls in
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages` still
assert it. What is now known is that the catalog's silence is silence rather
than a statement *here*, and that on this weapon the exclusion belongs to the
WEAPON rather than to one perk. The flag stays per PERK because the Despair
still needs that granularity — one of its two tier-2 options is excluded and
the other measurably is not.

### 2. Carnage Reign's "+33% Direct Damage per Status Type" pays NOTHING

Confirmed two independent ways:

* **In the table above.** The 135 panel reads 225 at one status type and 315 at
  two, which is the expression with no room for a 33% term — carrying it gives
  250 and 364.
* **With the GunCO mod removed entirely.** A status-afflicted enemy takes
  exactly the panel. This clause is then the only CO source on the build, and it
  adds nothing at all.

Modelled as a **LIVE BUG** rather than as a gap: DE's own CO-source list carries
this perk, the card states the clause, and a hotfix restores it. The distinction
is the reader's action — an unmodelled line says wait for us, this one says do
not pick the perk for that half, because nobody here is going to implement what
DE has not shipped. It is the first EVOLUTION to carry one (`live_bug:` beside
the effect it kills, so a two-clause perk can have one working half; the +60
base damage works and is untouched).

**A CANDIDATE CAUSE, UNVERIFIED.** The wiki notes an unlisted requirement of
**energy max ≥ 200** on this clause, recorded in the yaml since the file was
written and never modelled. If it is real and the measuring frame was under it,
the perk is CONDITIONAL rather than broken and the engine should learn the gate
instead of zeroing the clause. Nothing has measured the same weapon on a frame
above 200, which is the one experiment that would tell the two apart — and it is
cheap: same build, same target, a frame with ≥ 200 max energy, read the panel
against a status-afflicted enemy with the GunCO mod off.

### RESOLVED — it was the gate (owner, 2026-08-26)

**The measurements above stand and the conclusion does not.** The unlisted
requirement is real: the clause pays at **energy max ≥ 200**, and it is
CONDITIONAL rather than broken.

Nothing above needs re-reading, because the gate explains it exactly instead of
contradicting it — the neutral Tenno this repo ships has **150** max energy, so
every run that found nothing was made UNDER the threshold. The experiment this
section asked for ("a frame with ≥ 200 max energy") is what the owner supplied.

Modelled as `gated_by_tenno` with `condition: "energy_max >= 200"` and
`grant: condition_overload`, which feeds the same CO term
`innate_co_per_type` does — so the behaviour, the base exclusion and
direct-damage-only all keep coming from where they already came from. Measured
across the threshold on a Dual Toxocyst Incarnon with four status mods:

| max energy | DPS |
| --- | --- |
| 150 (the neutral Tenno) | 33,384 |
| 199 | 33,384 |
| **200** | **53,383** |
| 300 | 53,383 |

The step is at 200 and not 201, which is why the engine gained an
`EnergyMaxAtLeast` gate rather than reusing `EnergyMaxOver(199)`: a threshold
written one off so the operator comes out right reads as a typo and is wrong the
moment a frame lands between the two.

**WHAT THE READER GETS BACK.** `live_bug` said *do not pick this perk for that
half, and a hotfix will change it*; a gate says *it pays on the right frame*,
which is a build decision rather than a warning. Saying the first when the second
is true costs a player 60% of the weapon's damage.

It was also the roster's ONLY evolution live bug, so `check_disclosure` lost its
sample. The three assertions there now run against an INJECTED one, the way
`check_board_link` injects a second mode — the claim is that the machinery can
say it, and that claim should not depend on which perks happen to be broken this
patch.

### What it moves

The first finding still pushes the Dual Toxocyst DOWN and the board rescores it:
the weapon was being credited with a CO term on up to 125/135 of base where the
game computes 75. The second no longer does — the 33%-per-status source exists,
on a frame that can reach 200 energy, and the board's neutral player cannot, so
board rows are unchanged by the correction.

## M50 — the Torid Incarnon's CO reads a flat 51, and the default flipped ✅ (owner, 2026-08-16)

The measurement M49 asked for, on the weapon that was picked because it would
settle the most: the most-played Incarnon in the game, and one whose catalog
rows this repo had read as an explicit "100%".

**THE READINGS.** Torid, Incarnon form, Galvanized Aptitude only.

| perk | panel | bare crit | stacks x types | measured |
|---|---|---|---|---|
| Final Fusillade (+51) | 102 | 316 | 1 x 1 | **380** |
| Plentiful Mayhem (+31) | 82 | 254 | 0 x 0 | **254** |
| Plentiful Mayhem (+31) | 82 | 254 | 1 x 1 | **318** |
| Plentiful Mayhem (+31) | 82 | 254 | 2 x 1 | **381** |

**THE FORM IS IDENTIFIED BY THE CRIT.** 316/102 = 3.098 and 254/82 = 3.098,
which is the Incarnon form's 3.1; the base form's is 2.0 and would have printed
302 off its own 151 panel. No second run was needed to pin which was measured.

**THE DECISIVE SHAPE IS NOT THE FRACTION — IT IS THAT THE BASE IS CONSTANT.**
Solved as an absolute rather than a ratio, `co_base = (hit/bare - 1) / (0.4 x
stacks) x panel`:

| reading | panel | solved CO base |
|---|---|---|
| Final Fusillade, 1 stack | 102 | 51.65 |
| Plentiful Mayhem, 1 stack | 82 | 51.65 |
| Plentiful Mayhem, 2 stacks | 82 | 51.25 |

All three land on the unevolved **51**, off two different panels and two
different perks. A CO term that fed on the evolution would have solved to 102
and 82 — two numbers — and would not have agreed with itself across the pair.
A ratio alone cannot say that, which is why the second perk was worth measuring.

If the +51 fed the term the first reading would have been 442 against 380, and
the Plentiful Mayhem pair 356 and 457 against 318 and 381.

### And the CO term is not multiplied by base damage — the class, measured

The same weapon and perk again with a **+165% base-damage mod** on the build.
This is the experiment that separates `Adding` from `Multiplying`, and it needs
that mod present: without one the two are algebraically identical.

| | bare | 1 stack | 2 stacks | increment |
|---|---|---|---|---|
| no Serration | 254 | 318 | 381 | **+64 / +127** |
| +165% | 674 | 737 | 801 | **+63 / +127** |

**The absolute increment does not move.** `Multiplying` would have scaled it by
2.65 to +170 and +337. `Adding` predicts exactly what was read, because the CO
chunk joins the base-damage bucket and the whole bucket is then divided by the
same `(1 + bd)` — so the term lands as a flat addition:

```
damage = ( panel x (1 + base_damage_mods) + 51 x 0.4 x stacks x types ) x crit
```

Six readings, worst error 1 point (0.1%, the display's rounding). The Torid
Incarnon's `Adding` class was catalogued; it is now measured.

**AND THE RESIDUAL IS NOISE, NOT A PATTERN.** With three readings it looked
systematic — all +1 above a CO base of 51.0. Six readings solve to 51.6 / 51.2 /
50.8 / 51.2, scattering both sides. It is display rounding and the base is 51.

### And the default flipped

Two weapons, four perks, four exclusions. The reading of the catalog that
produced the old default did not survive it, and the reason is in the table's
own columns:

* Its **"Attack Unmodded Damage"** column prints a DOUBLE value — "100 or 124
  (with Evolution II)" — on exactly **eleven** rows. Those are the only rows
  where anyone measured the weapon with an evolution installed. **All eleven are
  excluded.**
* Every other row prints a single number, and that number is the UNEVOLVED
  base. "Torid | Main-fire | 100 | 100%" says the CO bonus equals the base of a
  Torid with no evolution on it, which is true by construction and answers a
  different question. This repo read it as "the evolution feeds in full".
* So the score on the question actually asked is **15 to 0**: eleven catalog
  rows plus four owner measurements, all excluded, and **nothing anywhere
  measured an evolved weapon and found its evolution fed the term.**

**THE DEFAULT FLIPPED FOR `Adding` ENTRIES ONLY** (owner, 2026-08-16). An
undeclared perk on an Adding entry now keeps its flat damage out of the CO term;
238 weapon+perk pairs moved, by 37% on average at two Galvanized stacks against
two status types. The board rescores on the push and every Adding Incarnon
carrying a base-damage perk falls.

**`Multiplying` IS UNTOUCHED — 24 pairs — because nothing has measured one.**
All four owner measurements are Adding entries, and the owner stopped the
version of this change that covered both ("don't extrapolate"). The rule may
well be the same on both sides, since which base the term reads sits upstream of
how it combines, but that is an argument and not a reading.

> **SUPERSEDED THE SAME DAY BY M51**, and kept because the reasoning is the
> record. The argument in the last sentence was WRONG, not merely unproven: a
> `Multiplying` entry reads its FULL evolved base, so the two classes disagree
> and "upstream of how it combines" was the wrong picture. Refusing to
> extrapolate is what stopped that argument from being written into 24 entries
> as a fact — the flip would have been backwards on every one of them.

**AND A DECLARATION IS SCOPED TO THE FORM IT WAS MEASURED ON.** A perk reaches
both entries of its transform group while a reading comes off one of them, and
the Torid is where that bites: `co_base_excludes_only_form: incarnon` on both
its perks, so recording the measurement does not silently assert the base form
nobody fired.

**THE ERROR IS ASYMMETRIC**, which is why the Adding half is the right call at
this sample size. The old default OVERSTATES, and for a calculator whose promise
is matching in-game measurements that is the worse direction — it ranks weapons
on damage the game does not deal. One measurement finding an INCLUDED Adding
perk reverses it, and
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages` is the
loop that would lose a line.

### Still open

* ~~**THE TORID'S BASE FORM**~~ and ~~**a Multiplying entry, any Multiplying
  entry**~~ — both ANSWERED the same day, by the same readings. See **M51**.
  The four-hypothesis table this section printed came back on its first row.

## M51 — a `Multiplying` entry reads its FULL evolved base, and the two CO classes disagree ✅ (owner, 2026-08-16)

The experiment M50 §Still open asked for, on the weapon it named: the Torid's
**base form**, which is `Multiplying` where the form of M50 is `Adding` — the
same two tier-2 perks, the same Galvanized Aptitude, one reading answering both
open questions.

**THE READINGS.** Torid, base form, +165% base damage and +90% Electricity
(Corrosive), as `grenade impact / toxin cloud`. `1 x 2` is one Galvanized stack
against two status types.

| perk | 0 x 0 | 1 x 1 | 1 x 2 |
|---|---|---|---|
| Final Fusillade (+51) | 763 / 460 | **1068 / 644** | **1373 / 827** |
| Plentiful Mayhem (+31) | 662 / 359 | **926 / 502** | **1191 / 646** |

**THE MULTIPLIER DOES NOT MOVE WHEN THE PERK DOES.** 1068/763 = 1.3997 and
926/662 = 1.3988; 1373/763 = 1.7994 and 1191/662 = 1.7991. The CO term is scaled
by NOTHING — the entry reads its full evolved base. Fed on the unevolved 100 it
would print 1.265 under the +51 and 1.305 under the +31, which are neither each
other nor what was read.

**AND THE ANSWER IS THE OPPOSITE OF M50's**, on the same weapon and the same two
perks. That is the finding: which base the term reads is decided BY THE CLASS,
not upstream of it. The M50 paragraph guessing that "the rule may well be the
same on both sides" was wrong.

| | class | the CO term reads |
|---|---|---|
| Torid Incarnon form (M50) | `Adding` | the UNEVOLVED 51 |
| Torid base form (M51) | `Multiplying` | the FULL evolved 151 / 131 |

**THE DECISIVE SHAPE IS THE TWO COLUMNS, and it needs only the +51.** The same
flat +51 lands on both attack parts, so the impact's evolved base is 151 and the
cloud's is 91. Any term reading something other than the evolved base has a
different fraction in each column — 100/151 = 0.662 against 40/91 = 0.440 — and
must therefore print two DIFFERENT multipliers, 1.265 against 1.176, 7.5% apart.
It printed **1.3997 and 1.4000**. The +31 pair is a second, independent
confirmation rather than the argument.

**THE WHOLE SET IS CONSISTENT TO THE DISPLAY'S ROUNDING.** Solving all twelve
readings for the one multiplier every build factor and the target's own column
collapse into, against bases of exactly 100 and 40:

`763/151, 662/131, 460/91, 359/71, 1068/151/1.4, …` → **5.049 to 5.056**, a
spread of 0.14%. Twelve readings, three known inputs, no residual.

**AND THE CLOUD TAKES CO AT ALL** — the doubly-discrepant catalog row confirmed
from the other side. An AoE part is not supposed to receive the bonus, and this
one receives it as `Multiplying`, at the same rate as the main fire:

```
Torid | Toxin AoE Cloud | AoE | 40 | 40 | 100% | Multiplying
```

**SO THE RULE GENERALISED TO ALL 26 `Multiplying` ENTRIES** (owner, 2026-08-16),
deliberately ahead of the catalog and on one weapon's reading: the wiki prints a
fraction for a minority of attacks, this rule beats that table, and a
measurement that contradicts it edits ONE weapon's yaml. The class now answers
BEFORE a perk's declaration on a `Multiplying` entry, so a reading taken off an
`Adding` form cannot reach across a transform group and dilute one —
`no_evolution_dilutes_a_multiplying_co_base` asserts the property roster-wide
rather than the 26 numbers, and so covers a weapon nobody has entered yet.

**IT MOVED NO NUMBER TODAY.** Every `Multiplying` entry already computed on its
full evolved base, because the class default said so and the Torid's two
declarations were scoped away from it. What changed is that it is now MEASURED
rather than defaulted, and structural rather than a coincidence of which
declarations happen to exist.


## M52 — a chain's path is FIXED, and its rule is not in the formation ✅ (owner, 2026-08-17)

The first measurement of chaining, and the only part of `engine::chain` that
rests on something fired in game rather than on a wiki line.

**THE SETUP.** A **5 x 4** formation in the Simulacrum, bottom-left `(1,1)` and
top-right `(5,4)`. Torid Incarnon.

**THE READINGS.**

```
shooting (1,1):   1,1 - 1,2 - 1,3 - 1,4 - 2,4 - 3,4
shooting (2,1):   2,1 - 3,1 - 4,1 - 4,2 - 3,2 - 2,2
```

Both are five hops, both repeat exactly, and **hitting (1,1) and (2,1) at the
same time perturbs neither** — two seeds, two independent paths, each the same
one it walks alone.

> 我发现如果生成的敌人，是规整在这几个位置的，那么无论什么敌人，都是这样的规律。
> 但是如果模型不是人形，或者展位稍微错位，那么路径也会不一样。

### What it CONFIRMS: nearest

**All ten hops went to an orthogonal neighbour.** Never a diagonal, never past
a nearer body. On a square lattice that is exactly "the nearest viable target",
and it is now measured rather than assumed.

### What it REFUTES: any tie-break made of relative geometry

Nine of the ten hops were exact ties. Every candidate rule was scored against
all ten:

| rule | fits |
| --- | --- |
| entity index — row-major or column-major, lowest or highest | 4–7 / 10 |
| a fixed compass priority, best of all 24 orderings | **8 / 10** |
| a turn preference (straight / left / right / back), best of all 96 | **8 / 10** |
| nearest to the seed / farthest from the seed | 5–6 / 10 |

Nothing fits, and the reason is visible in one pair of steps: arriving at
`(3,1)` heading `+x` the path went STRAIGHT, and arriving at `(4,1)` heading
`+x` it TURNED. Same heading, same shape of choice, two answers — so no
function of the formation's own geometry can be it.

### The explanation, and it is the owner's own clue

**A non-humanoid model changes the path while every relative position stays
identical.** What changes with the model is the COLLIDER. So the order is the
game's spatial query handing back bodies in world-space broadphase order —
which cell each body falls into — and that is not a function of the formation
at all. It explains all three observations at once: a fixed layout gives fixed
cell assignments and therefore a fixed path; nudging a body across a cell
boundary changes it; and so does a collider of a different size.

The owner's guess (*"猜测和怪的坐标有关系？"*) was right, with one correction:
the ABSOLUTE world coordinates, not the positions within the formation.

### What the model does instead

Not reproduce it (owner, 2026-08-17): *"我们不要求100%还原，但是思路是一致的
… 如果多个敌人是永远不动的，那么这个链接的路径是永远固定的，就做到这点就可以了。"*

So `chain::resolve` breaks ties by the lowest body index — arbitrary, and
STABLE, which is the honest pair when the real rule is unknowable. A formation
that does not move always chains the same way, and a test asserts it a hundred
times over, with a body walked off the map as the negative control.

**AND THE UNKNOWABLE PART DOES NOT REACH THE ANSWER.** The total is invariant
to tie-breaking — `seeds x (1 + f + … + f^hops)` — so what nobody can know
moves damage BETWEEN bodies without changing how much the formation took. It
decides which one dies first and nothing else.

## M53 — the Burston Incarnon PUNCHES THROUGH, and its blast lands behind you ✅ (owner, 2026-08-20)

Verbatim, both messages, in the order they arrived:

> 我刚刚测试了一下burston是可以punch through的，子弹会传过去，然后爆炸（也就是会在别人的身后爆炸，就是穿过后飞行距离达到极限），也就是说纯单体伤害可能还会降低，你顺便也实现了

> 我们给burston这种的面板带上aoe，但是实际上可以穿透的得定义一种类型（他的aoe算法还不吃多重，我认为是一种假aoe，你应该可以归类一下，这样有些计算更好处理，名字你来定）

### What it overturns

The night before, I told the owner that punch-through mods pay nothing on a
Burston Prime Incarnon and that this was CORRECT — the form carries a `radial:`,
and the punch-through page's class rule says *"weapon projectiles with an area
of effect (AoE) component will not Punch Through enemies or level geometry at
all"*. The card showed `+0m` and I called it honest.

It is not. He went and shot one. The round passes through the enemy and
detonates BEHIND it, at the point where the flight ends.

**AN INFERENCE FROM A CLASS RULE LOST TO A MEASUREMENT**, which is the whole
reason this file exists. The page's own sentence begins *"With a very few
exceptions"* and never says which — so a weapon being in the exception set is
exactly the thing the rule cannot tell you.

Two pieces of published evidence agree with him, and both were reachable the
night before:

- The `Incarnon` page's changelog carries evolution perks that fire **on punch
  through hit** — *"Paris Incarnon's Ardent Trigger Evolution (on punch through
  hit: + 40% Fire Rate for 6s)"*, and Braton Incarnon's Evolution III is *"On
  Punch Through Hit: 20% chance for 10% Ammo restored"*. DE does not build
  perks around a thing the weapon cannot do.
- The **Tenet Ferrox** states the whole mechanic in words, on its own page:
  *"Shots explode in a 4 meter radius after reaching maximum punch through
  distance."* It went into the roster the same night, with that sentence
  transcribed into a comment, and I did not connect the two.

### The classification he asked for

He named the smell before the mechanic: the blast *"不吃多重"* — takes no
multishot — so it is a **假 AoE**, a fake one, and it should be a TYPE rather
than a pile of per-weapon exceptions.

`weapons_data::BlastKind`, two values:

| kind | detonates | punch-through mods | example |
| --- | --- | --- | --- |
| `contact` (default) | on the first thing it touches | **refused** — a true AoE | Tenet Envoy, Kuva Ogris, every grenade |
| `terminal` | where the FLIGHT ends, after the punch-through budget is spent | **allowed** | Burston (Prime) Incarnon, Tenet Ferrox |

### What it does to the number, and why it can go DOWN

**THE BUDGET IS SPENT ON MATERIAL** (owner, 2026-08-20), which is the mechanic's
own definition — *"the total distance of material (object or enemy) that a
weapon's projectile, bullet or beam can pass through before dissipating"*. Air
costs nothing. `space::dissipation_point` therefore crosses `BODY_MATERIAL_M`
per body and detonates in whichever one the round cannot get out of, which is
the same accounting `struck_along` does for the direct hits, read one step
further.

When it clears every body on the line the arena has to answer a question the
game answers with a WALL. There is no geometry here, so nothing would ever stop
it; the leftover budget is spent as flight instead. That is the one place the
model is a stand-in rather than the mechanic, and it is bounded by the weapon's
own punch through rather than by a number invented for it.

Measured on the wire, level 100, 100–200 runs, with the server's own standard
error. **One standing enemy** — the blast moves back and the damage drops:

    Burston Prime Incarnon   Serration      16584.5 ± 38.4
                             + Metal Auger  16357.6 ± 38.2     -1.4%, about 4σ
    Tenet Ferrox             Serration       2659.9 ± 17.8
                             + Metal Auger   2651.6 ± 17.8     no measurable change

**A line of seven, 1.5 m apart** — which is where the accounting actually shows,
and where a distance-based reading gets it wrong:

    Burston Prime Incarnon   Serration      16565.7 ± 50.7    1 body
                             + Metal Auger  53618.9 ± 141.8   5 bodies
                             + Primed Shred 66125.5 ± 264.2   5 bodies

Five is `1 + floor(2.1 / 0.5)` exactly, and the detonation lands on the FIFTH
body rather than 2.1 m past the first.

**TWO READINGS WERE TRIED AND BOTH ARE WORTH RECORDING.** The first sent a round
that crossed every body off the field entirely — which fits the Burston and
*killed the Tenet Ferrox's radial* against a lone target with no mod equipped at
all (2674 DPS to 2416), because its 1.5 m of INNATE punch through cleared the
only body there was. The second read the budget as a flight DISTANCE, which
saved the Ferrox and got the CROWD wrong: it put the blast a fixed 2.1 m past
the first body instead of on the fifth. Only the material accounting fits the
measurement, the Ferrox and a line of enemies at once.

**THE DIRECTION IS MEASURED; THE MAGNITUDE IS NOT.** The owner reported that
single-target damage *"可能还会降低"* — may even decrease — and gave no number, so
what is pinned here is that it drops and that the weapon's own blast radius
decides by how much. A figure from in game would tighten it and nothing in the
model would have to move to accept one.

### The four Braton Incarnons — DEFAULTED to this, not measured

The same evidence points at exactly four more entries and no others. A sweep for
a form carrying BOTH a `radial:` and an evolution whose own text reads *"On
Punch Through Hit"*:

| entry | evolution |
| --- | --- |
| `braton_incarnon` | Gunsmoke Pick Up — *"On Punch Through Hit: 20% chance for 10% Ammo restored"* |
| `braton_prime_incarnon` | the same |
| `braton_vandal_incarnon` | the same |
| `mk1_braton_incarnon` | the same |

Every other weapon with such a perk — the Paris family, the Ballisticas, the
Felarx, the Onos — carries no radial at all, so it already takes punch-through
mods normally and there was nothing to decide.

They were left `contact` for one night as a known-wrong state, on the grounds
that guessing between `terminal` and `contact` plus `punch_through_mods: true`
is the CO rule's own mistake in another mechanic. **The owner then chose the
default (2026-08-20): the Burston's answer.** A form whose own evolution rewards
punch-through hits cannot be a weapon that refuses punch through, and of the two
fixes the Burston's is the one with a measurement behind it.

It is a DEFAULT and the files say so. One shot settles it: fire a Braton
Incarnon with a punch-through mod at a single enemy and watch whether the
explosion lands on it or behind it. Measured at 200 runs the choice currently
costs that weapon nothing detectable on a lone target (9629.3 ± 74.5 against
9602.7 ± 74.5), so what it really buys is the crowd — which is the case worth
checking in game.

## M54 — a BLAST detonation carries the weak point ×3 to everything its sphere reaches, and a TOXIN DoT carries nothing ✅ (owner, 2026-08-22)

Burston Prime, no Incarnon, a Lavos syndicate mod at +200% of the matching
element. Numbers as typed, `direct — status`:

```
BLAST                       TOXIN (on a Runner)
打身体  115 — 11            打身体  115 — 107
        231 — 21            打头    346 — 159
打头    346 — 32
        1385 — 126
```

> 打身体/头，爆炸会有10条（相当于原本身上的10层都爆了，而不是一个）
> 周围会受到1次伤害
>
> 打身体的时候10层
> 周围1260 （暴击过1次）
> 周围1050 （完全没暴击）
>
> 打头 (暴击情况各异）
> 周围3675
> 3150
> 3380

### What the direct column pins first

`231/115 = 2.01` is the crit multiplier, `346/115 = 3.01` the head multiplier,
and `1385/346 = 4.00` is the HEADCRIT — `1 + (2−1)×3 = 4`, the game's own rule,
arriving unasked. That is what makes the rest of the numbers trustworthy: four
samples reproduce three published multipliers before anything about Blast is
read off them.

### The single-target detonation takes both

`11 → 32` is ×2.9 for the head and `32 → 126` is ×3.94 for the crit, i.e. a
blast stack is stamped with the multipliers of the hit that applied it. The
engine already did this.

### THE AoE TAKES THEM TOO, AT EXACTLY ×3

The clean pair is the two runs with **no crits at all**:

| where the 10 stacks were applied | a neighbour takes |
| --- | --- |
| body | **1050** |
| head | **3150** |

`3150 / 1050 = 3.000`. No remainder. The weak-point multiplier of the hit that
applied the stack reaches every body the 5 m sphere catches.

And `1050 / 10.5 = 100`, i.e. ten stacks at 300% each against the same ten at
30% each — the published 10× between the radial and the single-target halves,
confirmed rather than assumed.

The shape is confirmed too: *"爆炸会有10条…周围会受到1次伤害"* — ten separate
numbers on the host, ONE combined instance on the neighbours, which is the
wiki's *"The radial damage of all procs will be combined into one damage
instance"*.

**WHAT IT SETTLED.** `data/benchmarks/group_clear.yaml` said "Nothing a chain, a
blast or a cloud reaches can be a weak point hit" and the engine had never
implemented that for a blast. The measurement says the ENGINE was right and the
RULE TEXT was wrong, so the text changed. Worth 14.8× on the Larkspur Prime's
board row (914.8 → 62.0 with headshots off), which is why it was worth measuring
rather than reasoning about.

### A TOXIN DoT DOES NOT TAKE THE WEAK POINT

> 我确定毒不吃爆头

This contradicts the wiki's Toxin page, which lists *"Enemy Body Parts
multipliers"* among the additional multipliers on a Toxin tick. A measurement
beats the wiki (docs/DATA_SOURCES.md), so `dot_takes_weakpoint` returns false for
Toxin and true for everything else — the others are UNMEASURED and keep the
wiki's answer rather than inheriting a rule from one case.

The two lines quoted above do not settle it on their own — `159/107 = 1.486` is
neither 1 nor 3, and the samples differ in crit state and carry a faction bonus
that applies twice to a status — which is why this rests on the owner's own
in-game reading and says so.

### Electricity and Gas tick on ONE clock

> 电也是一个大dot的模式，无论多少层，只会跳一下…毒气也是这种，伤害频率和第一次上dot的时候保持一致

Confirmed on the wiki for Electricity with a dated patch note — Update 33.6,
*"multiple procs on an enemy no longer deal their respective damage separately,
like current Slash statuses, but once per second, similar to Heat status.
However, they still maintain each own timer and will not refresh, unlike Heat"*
— and confirmed in game for Gas, which the wiki does not state.

So there are THREE DoT models and the engine had two:

| status | model |
| --- | --- |
| Slash, Toxin | per instance, own clock, own timer |
| **Electricity, Gas** | **one clock per body, own timer, no refresh** |
| Heat | one clock per body, one timer, every proc REFRESHES all of it |

`push_dot_capped` moves a joining instance onto the family's clock. It is
tick-count neutral by arithmetic — an instance with `k` ticks joining a clock
`φ < 1` ahead fires `ceil(k − φ) = k` times — so this is fidelity, not a
rebalance, and every golden value held.

## M55 — the Soma Prime's GunCO reads its own 12, and neither Incarnon perk raises it ✅ (owner, 2026-08-22)

The `additive_with_base_damage` default — *a perk's flat base add raises what
the attack PRINTS and not what the CO term computes on* — was GENERALISED ahead
of the wiki's catalog rather than read off it. Nine readings across four builds
of one weapon now certify it, and they certify the shape as well as the number:
a perk that adds through the PANEL and a perk that adds through a BUFF both stay
out.

**THE READINGS**, as typed. Soma Prime, base form, one 40% source unless the
build says otherwise:

```
3.0爆伤                      3.0爆伤                      1层/2buff- 158-475
36基伤（12+12）+6*2          24基伤（12+12）
200腐蚀                      200腐蚀（40天赋）
0层0buff: 108-324            1层-86
1层2buff：137-411            2层-101-302
                             3层-115-346！
```

> 这个是选了40的那个直伤天赋，加上卡的一层的co（那么应该是80的加成）
>
> 确实是3个异常是158，这是实测的

### The three-point set is what settles it

`24基伤` with one 40% source, at one, two and three status types. Solved as an
absolute — `co_base = (hit/E − base) / (0.40 × types)`, with `E = 3.00` fixed by
`24 × 3 = 72`:

| types | measured | implied `co_base` |
| --- | --- | --- |
| 1 | 86 | 11.67 |
| 2 | 101 | 12.08 |
| 3 | 115 | 11.94 |

**12** — the weapon's own unevolved base, not the 24 on the panel. Three points
over-determine two unknowns, which is why this set decides and a single reading
cannot: the first attempt at this measurement gave one point with a MISCOUNTED
stack, and `(24 + 0.40×24×1)×3 = 100.8` is the same 101 as
`(24 + 0.40×12×2)×3`. Two errors cancelling produced a clean-looking answer of
24 and nearly moved the data. A stack count is the easiest thing in this
measurement to be wrong about; a slope is not.

### Every reading, against `co_base = 12`

`(base_total + 0.40 × 12 × types) × 3`:

| build | base | types | predicted | measured |
| --- | --- | --- | --- | --- |
| Fortress Salvo + Fresh Havoc | 36 | 0 | 108.0 | 108 |
| Fortress Salvo + Fresh Havoc | 36 | 2 | 136.8 | 137 |
| Fortress Salvo | 24 | 1 | 86.4 | 86 |
| Fortress Salvo | 24 | 2 | 100.8 | 101 |
| Fortress Salvo | 24 | 3 | 115.2 | 115 |
| Fortress Salvo, 40% talent + 40% card | 24 | 3 | 158.4 | 158 |

The last row is the one that was PREDICTED before it was confirmed. It arrived
labelled `2buff` and no combination of two status types reaches it — two gives
129.6, and reading the CO base as 24 gives 187.2. Only `0.80 × 3 × 12` lands on
both halves at once, 158.4 and 475.2 against a measured 158 and 475; asked
about it, the target had three.

### The two perks add through different doors and neither reaches the term

**Fortress Salvo** (tier 2, `+12`) is a panel add: `add_flat_base_damage(12, 0)`,
so `base_vector.total()` becomes 24 while `co_base` stays 12 and
`co_base_fraction` is 0.5. **Fresh Havoc** (tier 4, `+6` ×2) never touches the
panel at all — it is a `stacking_buff` granting `FlatBaseDamage`, which lands in
the live base-damage bucket at fire time. Two mechanisms, one answer, and the
36-base rows are what show the second one: the buff moves the hit from 108 to
what a 36 base deals and moves the CO term not at all.

Nothing changed. The entry is here because a rule that was inferred and a rule
that was measured are not the same rule to the next person deciding whether to
trust it — and because the near-miss above is the record of how close a single
reading came to overturning a correct one.

## M56 — a BLAST detonation takes NO elemental bonus, and Lavos can imbue Gas as its own element ✅ (owner, 2026-08-23)

Two mechanics measured in one sitting on a **Braton Prime, base 35**, and they
are opposite answers to the same question — *does an element bonus reach the
status damage?* — which is why they are recorded together.

### The reports, verbatim

```
90冰mod+90火mod
爆炸98-11

200爆炸+90冰mod+90火mod
爆炸337-21
168-11
1011-63（爆头）
```

```
200毒气+90毒mod
毒137-34 / 毒气137-54 / 火137-18

200毒气+90毒mod+90火mod
毒168-34 / 毒气168-54 / 火168-34

90毒mod+90火mod
毒气98-18
```

The `200<element>` source is **Valence Formation** (效价炼成), Lavos's passive
augment: +200% of one element, and the ONLY source that can add a COMBINED
element as its own.

### Blast: the element bracket is nowhere in it

| build | direct | detonation |
| --- | --- | --- |
| 90% Cold + 90% Heat | 98 = 35 × 2.8 ✓ | **11** |
| …+200% Blast | 168 = 35 × 4.8 ✓ | **11** |

The hit moved by 71% and the detonation did not move at all: `0.3 × 35 = 10.5`
both times, displayed as 11. That is the wiki's own sentence, measured —
*"Unlike other damaging statuses, adding more elemental damage (Heat and Cold)
will not increase the Blast proc damage"* (`Damage/Blast_Damage`) — and it is a
CONTROLLED PAIR rather than a single reading: the same run proves the imbue
landed.

The other two lines of the second block are the same shot at higher
multipliers, and they confirm the two the detonation DOES take:

- `337-21` — a critical hit. `168 × 2` and `10.5 × 2`, the crit multiplier
  reaching both halves.
- `1011-63` — a critical **headshot**, exactly `3.000 ×` the line above in both
  columns. Head 2 and crit 2 give a critical-headshot multiplier of
  `1 + (2 − 1) × 2 = 3`, and it is the same 3.000 M54 measured on the AoE half.

So: crit and weak point yes, elements no. The engine was already right and for
the right reason — a stack reads `modified_base`, which is the Serration bucket
alone, while elements are a bracket applied at the hit — but nothing asserted
it, and every OTHER status wants that bracket there.
`elemental_damage_moves_the_hit_and_never_the_blast_detonation` is that
assertion, both halves, verified to bite (607.5 against 202.5).

### Gas: the element bracket is the whole of it, and only a LITERAL source counts

The opposite answer, on a DoT. A tick reads `1 + Σ THAT ELEMENT's own bonuses`,
and only a source naming that element literally is in the sum:

| build | Gas direct | Gas DoT | reads |
| --- | --- | --- | --- |
| 90% Toxin + 90% Heat | 98 = 35 × 2.8 | 18 | bracket **1.0** |
| +200% Gas, +90% Toxin | 137 = 35 × 3.9 | 54 | bracket **3.0** |

The two mods that CREATE the Gas contribute nothing to the Gas burn — they are
Toxin and Heat sources, and the burn is Gas. Only Valence Formation, which adds
Gas *as Gas*, moves it. The wiki states the mechanism from the other side:
*"Bonus Elemental Damage will be added parallel to the weapon's Elemental
Damage, meaning it will NOT combine with elements on the weapon."* DE's own card
says 附加, not 合成.

The split rows are the same rule seen three times over: adding a 90% Heat mod to
the second build moved the HEAT split DoT 18 → 34 and left the Gas DoT at 54,
because a Heat mod is in Heat's sum and not in Gas's.

### The 36/35 on every tick — RESOLVED, see M58

Every DoT tick in both blocks came back **×1.0286 (= 36/35)** above what the
engine computed, across three independent brackets:

| bracket | computed | measured |
| --- | --- | --- |
| 1.0 | 17.5 | 18 |
| 1.9 | 33.25 | 34 |
| 3.0 | 52.5 | 54 |

The direct hits pin the base at exactly 35, so the DoT half behaved as though
the base were 36. It was left open here because a rounding rule, a per-weapon
quirk and a wrong `DOT_COEFFICIENT` all fit the nine rows, and the coefficient
reaches every elemental DoT in the app.

It is none of those: the status formula's accumulator **starts at 1 rather than
0**, stated on `Damage/Calculation` §Damage Over Time. `(35 + 1) × 0.5 = 18`.
M58 is the whole of it.

## M57 — quantization divides by ModdedBase, not by the vector's total ✅ (owner, 2026-08-23)

Four direct-hit readings on a **Braton Prime, base 35** (1.75 Impact / 12.25
Puncture / 21 Slash), each under a different element bonus. They were taken as
part of the Blast and Gas work in M56 and are separated here because what they
settle is neither of those mechanics: it is the **denominator of damage
quantization**, which is under every damage number this app produces.

| build | raw total | measured pop | ours (before) | ours (after) |
| --- | --- | --- | --- | --- |
| 90% Toxin + 90% Heat | 98 | **98** | 101.06 → 101 | 98.4375 → 98 |
| +200% Corrosive | 105 | **105** | 105.0 → 105 | 105.0 → 105 |
| +200% Gas, +90% Toxin | 136.5 | **137** | 132.23 → 132 | 136.7188 → 137 |
| +200% Blast, +90% Cold, +90% Heat | 168 | **168** | 162.75 → 163 | 168.4375 → 168 |

### What the page actually says

`Damage/Calculation` §Quantization states it as two formulas, and both name the
same quantity:

```
Scale = ModdedBase / 32
x     = TotalDamageTypeValue / ModdedBase
Quantized(x) = sign(x) × floor(|x| × 32 + 0.5) / 32
```

**ModdedBase** is `base × (1 + damage mods)` with the elemental portions
excluded — the number this engine already carried as `dot_modified_base` for
status payloads, one line below the call that needed it. Elements are in the
numerator only.

`DamageVector::quantized()` divided by `self.total()` instead, which includes
them. On the first row that snaps the four components to **33** units of a
larger scale rather than 32 of the right one, and the hit comes out 3.1% high.

### Why it survived, and why four readings were needed

The only test on the function is the page's own worked example — 30 Impact / 30
Puncture / 40 Slash with **no mods at all**, so `ModdedBase == total == 100` and
the example passes under either reading. It cannot distinguish them, and neither
could anything else: quantization is invisible on a physical-only weapon and
this is a calculator whose calibration cases were physical.

MECHANICS §Quantization even contains the sentence that names the case —
*"the two descriptions differ only when elemental mods change the vector's
composition"* — written in July as a note about a "pseudo-conflict" flagged on
the wiki. The reasoning was right and nobody ran it against the code.

The **second row is why one measurement would not have done it**: +200%
Corrosive on this weapon agrees under both denominators (32 units either way). A
single reading that happened to be that build would have confirmed the bug.

### What moved

`one_fight` reports all three shapes moved, in both directions, which is what
quantization does — the page's own note says mixed-type damage is *"frequently
gained or lost by the conversion"*:

```
torid        kill progress 0.185578 -> 0.186538   (+0.52%)
gotva_prime  kill progress 0.223337 -> 0.217810   (-2.47%)
scourge      kill progress 0.053459 -> 0.053732   (+0.51%)
```

One test changed with it, and it was already documented as the exception: the
Xata's Whisper worked example (M40) is written as `98 × 2.2`, but 117.6 Blast is
38.4 steps of the `98/32` scale and snaps to 38, so the vector a hit deals is
214.375 and the bracket a status burns off is 2.1875 rather than 2.2. Its final
assertion also had a tolerance of ±0.002 around a ratio read off **two whole
numbers popped in game** (63 and 71) — a precision two integers cannot carry.
The band is `[62.5/71.5, 63.5/70.5]` now, which is what the capture actually
pins, and both readings sit inside it.

### Not settled by this

The DoT tick gap from M56 is untouched: every tick in those blocks is 36/35
above what the engine computes, and quantization does not explain it — a mono
DoT instance of 17.5 on a ModdedBase of 35 is exactly 16 units and is lossless
under the corrected rule too.

## M58 — a status tick's accumulator starts at 1, not at 0 ✅ (owner, 2026-08-23)

The answer to the 36/35 M56 left open, and it is not a coefficient, a rounding
rule or a per-weapon quirk. `Damage/Calculation` §Damage Over Time states it:

> "For weapon-generated Heat, Electricity, Toxin, Gas, and Slash status
> effects, **the temporary damage accumulator for each tick group starts at 1
> rather than 0**. The full-precision damage seeds are then added to this
> accumulator before the status coefficient and the remaining applicable
> multipliers are applied."

```
Unrounded Tick Damage = (Σ Sᵢ + 1) × C × M
```

`Sᵢ` is each stored damage seed, `C` is 0.5 (Heat, Electricity, Toxin, Gas) or
0.35 (Slash), and `M` is the elemental, faction and status-damage bonuses. On a
Braton Prime, base 35, that is `(35 + 1) × 0.5 = 18` per tick, and it reproduces
all nine of M56's readings:

| bracket | `35 × 0.5 × b` | `(35 + 1) × 0.5 × b` | measured |
| --- | --- | --- | --- |
| 1.0 | 17.5 | **18** | 18 |
| 1.9 | 33.25 | **34.2** | 34 |
| 3.0 | 52.5 | **54** | 54 |

### Three things it is not, all stated on the page

**Not a flat +1 of damage, and not once per stack.** *"If several seeds are
consolidated into a single tick, they are added to the same accumulator, so its
initial value of 1 is included only once. It is therefore neither a final flat
+1 damage bonus nor a bonus applied once per status stack."* So Heat,
Electricity and Gas — the families that share a clock — count it ONCE per tick
however many stacks fold in, while Slash and Toxin tick independently and each
carries its own.

**Not on every status.** The list is five, and Blast is not in it. **M56's own
capture proves that from the other side**: a detonation read 11 / 21 / 63 across
body, crit and critical headshot, which is `0.3 × 35` times 1 / 2 / 6 exactly.
With an accumulator the crit line would be `0.3 × 36 × 2 = 21.6`, displayed 22.

**Not outside the faction double dip, and not inside it twice.** The page's own
Toxin example is `(40 × 1.55 + 1) × 0.5 × 3.25 × 1.55` — the faction bonus
inside the seed AND in `M`, with the `1` added between them. So it takes exactly
one of the two layers, and a Roar'd bleed is no longer exactly `f²`: at base 100
it is 2.2446 rather than 2.25, approaching 2.25 as the seed grows. Eclipse stays
exactly ×3 at any base, because a FINAL multiplier scales the accumulator and
the seed alike.

### Why a base-35 rifle was needed to see it

The `1` is worth 0.5 damage before multipliers: **2.9% on a base of 35, 0.25% on
a base of 400**. Every fixture this engine had was above the noise floor of its
own tolerances, and the wiki's DoT examples are all on a seed of 40 where the
absolute figures are printed and the ratio is not. It took a small gun and nine
readings across three brackets.

### The harness could not have caught it — and now can

`one_fight`, the cost-and-answer baseline, reported all three shapes **unmoved
to fifteen digits** — including with the accumulator scaled by a thousand,
which is what turned "too small to see" into "never executed". Its default
build is Hellfire + Cryo Rounds and Infected Clip + Stormbringer, which is
BLAST and CORROSIVE: a detonation and an armour strip, and not one of the five
damaging burns. So it ticked no status DoT at all, while its own comment
claimed it exercised them.

The trap underneath is that `dot_damage` is **not** a proxy for "a burn
ticked": that bucket also holds Blast detonations and area hits, so the Torid
reported 29,001 of it with not one burn in the fight. `RunResult::dot_ticks`
already counted the right thing and was never reported anywhere;
`Summary::mean_dot_ticks` is that counter, surfaced.

Fixed twice over. A **fourth shape** — the Braton Prime, 60% Slash, and a
physical type is the one thing an elemental mod cannot combine away — burns
under the unchanged default build, 507.6 ticks a run against zero for the other
three. And the tool now **fails when the whole suite ticks nothing**, so the
next edit to the mod list or the weapon list cannot silently undo it. The mod
list itself is untouched: it is what every saved baseline was measured under.
Both halves verified to bite, and so is the fleet merge carrying the new
counter — a shard that dropped it would report zero burns for a fight full of
them, which is the guard firing on a working engine.

Also noted on the same page and NOT implemented: *"intermediate DoT operations
use binary32 arithmetic"*. Its effect is below the resolution of anything
measured here.

## M59 — the Laetum's Incarnon form doubles Secondary Irradiate's echo ✅ (owner, 2026-08-24)

A **Laetum**, base damage 220, read across both forms. Most of what was
measured confirms what the engine already computed; one number does not, and it
is the reason this entry exists.

### The reports, verbatim

```
220基伤
512/10752

2/2 768 160*（1+2.2+0.4*2*2）
2/1 640 160*（1+2.2+0.4*1*2）

灵化后
0基伤   100/300
220基伤 320/960
3/1-440/960
3/2-560/960
100*(1+2.2+1*3*0.4)=440
100*(1+2.2+2*3*0.4)=560
300*(1+2.2)=960

照射测试
灵化前 1536 / 隔壁 2764.8
       (可以受到这个伤害，并且也可以 x21，互相独立不影响，
        相当于旁边单独一次 1.8x)
灵化下 320/960   隔壁 1152/24192
       960/2880  隔壁 3456/24192
```

### What confirms the model

**Devouring Attrition is ×21 on the instance**, and it is INDEPENDENT of the
echo. `10752 / 512 = 21.000`, and the owner's note beside the echo says the
same from the other side: the echo "can also take the ×21, independently and
without affecting each other". That is what `noncrit_mult` already does — a
roll per non-critical damage instance, in its own multiplicative bracket.

**The Incarnon form's radial is 3× its direct**, and the stack bonus reaches
the direct hit only: `320/960` and `960/2880` at 220% base, with the AoE
sitting at 960 across `3/1-440/960` and `3/2-560/960` while the direct climbs.
`300 × (1 + 2.2) = 960` is the whole of the AoE's arithmetic.

**Only a DIRECT hit triggers the echo; an AoE hit never does** — stated
outright by the owner. The engine had this right and could not easily have had
it wrong: `spread_from_echo` is called from the direct path alone.

### What does not: the echo is 3.6× on one form and 1.8× on the other

Secondary Irradiate deals `1.8 × the hit` at max rank, and the owner measured
several pure single-target weapons at exactly that. Not here:

| form | direct | echo | ratio |
| --- | --- | --- | --- |
| base | 1536 | 2764.8 | **1.80** |
| Incarnon | 320 | 1152 | **3.60** |
| Incarnon | 960 | 3456 | **3.60** |

Twice, on two different direct-hit sizes, and the base form of the same weapon
is ordinary — so it is the FORM and not the gun.

**THE OWNER'S READING**, offered as a hypothesis: the game sees TWO damage
components on this attack — a direct hit and a radial — so it computes an echo
for each, `1.8 + 1.8`, while only the direct one actually fires. That sits
exactly on top of the other measurement in the same session (an AoE hit never
triggers the echo), and it gives the number a meaning: **one per damage
component**, rather than a magic 2.

### What is implemented, and what is deliberately not

`WeaponSpec::echo_multiplier` is that coefficient, `2.0` on
`laetum_incarnon` and 1.0 everywhere else — a per-ENTRY figure, not a rule
about AoE weapons. Every direct+radial weapon in the roster is a candidate for
the same doubling and **none of the others has been read**; generalising one
measurement to a class is what `docs/CATALOGS.md` forbids, and the owner's own
framing was "other weapons with an AoE seem to have a bit of this problem",
which is a lead rather than a finding.

`only_the_measured_entry_carries_an_echo_coefficient` is the note to come back
to: it asserts the roster holds exactly one, so the day a second weapon is
measured the test fails, names both, and forces the decision to be made on
purpose instead of by a default.

### …and an EXTRA HIT does not roll the ×21 again

Same session, and it closes a question this file had left open. Xata's Whisper
fires a second damage instance worth a percentage of the hit that triggered it.
Devouring Attrition's own rule is "per damage instance that did not crit", and
an extra hit IS a second instance — so a second roll was the reading a careful
person would have argued for, and it would reach **×441**.

It does not. **"真理密语不能再继续触发那个 x21，从而达成 x441。只能简单的 x21，
就是原本的实现"** (owner, 2026-08-24). The extra hit inherits the ×21 the
trigger already took, through the `raw` it is a percentage of, and stops there
— which is what this engine already did, on the strength of a comment about
crit and the body part rather than about this perk. Now measured rather than
inherited.

**The one thing that DOES reach ×441 is Primary Debilitate's DoT**, and for an
unrelated reason: its zero-damage instance leaks its multipliers into the burn
it leaves. That is a LIVE BUG in the game, declared as one on the card, and
recorded in M37.

## M60 — headshot bonuses ADD, and the crit-tier ladder holds ✅ (owner, 2026-08-25)

An unmodded **Laetum in its BASE form**, every shot to the head of a **Techrot
Babau** (wiki: *"Head: 1.5x"*, faction Techrot, 10,000 health, 500 shield, no
armour). Two numbers per line, the crit tier the pop-up was:

```
全无
爆头 1058/1876
50%爆头伤害
爆头 1587/2812
50%爆头伤害+死首
爆头 1904/3376
```

and a second capture on a built weapon — *"死首3层+50%爆头伤害 / 187%爆率+镀层
液压准星满层+复仇者(45%暴击加算)"*:

```
15529（橙色）/22300（红）
```

### What it settles: the two headshot bonuses are ONE ADDITIVE BUCKET

`+50% Headshot Damage` is the Laetum's tier-4 evolution **Caput Mortuum**;
Secondary Deadhead's rank-5 passive is **`+30% to Headshot Multiplier`**. The
two could add into one bucket or multiply, and the arithmetic tells them apart:

| | additive `1+0.5+0.3` | multiplicative `1.5 × 1.3` | measured |
|---|---|---|---|
| step from +50% to +50%+Deadhead | **×1.200** | ×1.300 | **1904/1587 = ×1.1998** |

So they ADD. The engine already summed them
(`headshot_multiplier_bonus + streak_bonus + headshot_damage_bonus`, with
`headshot_bonus_multiplicative` false for everything but Cernos Prime) — but
that came from reading the wiki, and this is the first time it has been
measured. Reproduced end to end: 1.4999 / 1.2000 / 1.7998 against the
measurement's 1.5000 / 1.1998 / 1.7996.

### …and the CRITICAL HEADSHOT ladder is the wiki's formula

`Headshot Crit Tier Multi = Headshot Multi × (1 + Tier × (2 × CD − 1))` (wiki
`Critical_Hit`), which at CD 2.2 gives tier multipliers **1 / 4.4 / 7.8 / 11.2**:

| step | formula | measured | off by |
|---|---|---|---|
| yellow → orange | 7.8/4.4 = 1.77273 | 1876/1058 = 1.77316 | +0.024% |
| orange → red | 11.2/7.8 = 1.43590 | 22300/15529 = 1.43602 | +0.009% |

The first capture's pair is therefore **yellow/orange**, not white/yellow: white
→ yellow would be ×4.4 and the numbers are nowhere near it.

### The 0.19% that is NOT explained

All six numbers of the first capture sit **+0.14% to +0.21%** above
`160 × 1.5 × tier` — one uniform factor of about ×1.0019, not a structural
error: every RATIO above is exact to three decimals. The three published inputs
were checked against the wiki and all match what `data/` holds (160 = 64 Impact
+ 96 Slash, head 1.5x, crit multiplier 2.20x), so the factor is not in any of
them. Left open deliberately (owner, 2026-08-25: *"有小误差就有，不用管"*).

What would close it is three pop-ups from an unmodded gun on the same target: a
body white (should be 160), a body yellow (352), and a head white (240). Between
them the factor is pinned to one layer.

### A note on which number to compare

The perk's own multiplier is **exactly 1.500000** on the DIRECT hit (480 → 720
white, 2112 → 3168 yellow). A dps RATIO gives 1.49988 and does not converge with
run count, because dps is the whole engagement and about 0.025% of it is status
DoT, which does not take the headshot bonus (M54). Comparing a perk's multiplier
through dps measures the perk plus everything the perk does not touch.

**Techrot Babau is not in `data/enemies/`**, so this capture cannot be replayed
in the app as it stands.

---

## M61 — a shot that BREAKS a shield keeps killing through it (owner, 2026-08-27) ⚠ OPEN

An unmodded **Laetum in its BASE form** (the same gun as [M60](#m60--headshot-bonuses-add-and-the-crit-tier-ladder-holds--owner-2026-08-25): 160 = 64 Impact + 96 Slash, crit multiplier 2.20x) against a **level 1 Corpus Crewman, no Steel Path — 120 shield, 90 health**. Two numbers where the pop-up showed two.

The owner's report, verbatim:

> 刚刚我发现了一个问题，那就是敌人的超短暂的破盾保护好像很多时候是不触发的，或者选择性触发（例如震地的出场在会触发）。但是打枪的时候，例如我造成1w伤害，这个人只有100盾100血，那么这一枪会直接秒了。

```
奏凯普通
96slash+64impact=160
critical damage 2.2x

1级crew man无钢铁加成
shield 120
health 90

无mod
爆头
暴击 120+1776=1896
无暴击 120+620=740

无盾爆头
暴击 2015
无暴击 860

身体
爆击 341+12
无暴击 158+2

无盾身体
无暴击 160
暴击 353

有+220+165基伤
爆头
暴击  120+ 9920
无暴击 120+4316

无盾爆头
暴击 10160
无暴击 4556

身体
爆击 1630+80
无暴击 743+33

无盾身体
无暴击 776
暴击 1710
```

### What it settles, and it does not need a model to settle it

**A HIT THAT BREAKS A SHIELD PAYS THE REST INTO HEALTH, IN THE SAME INSTANT.**
Every one of the four headshot lines shows the shield's `120` beside a health
number of 620 to 9,920 — one trigger pull, two pop-ups, and the target has 90
health. It dies to that shot.

`dummy::TargetState::apply` does neither half:

```rust
if self.shield > 0.0 {
    shield_part = rest * mit.disrupt_amp;   // the WHOLE non-Toxin hit
} else { … }
…
self.shield = 0.0; // no spill
self.gate_until = now + 0.1;
```

so a 10,000-damage hit on a 120-point shield is charged **entirely** to the
shield, 9,880 of it is discarded, health takes **nothing**, and every instance
for the next 0.1 s is multiplied by 0.05 unless it is a direct weakpoint hit.
The same shot that kills in game leaves the target at full health here, and the
follow-up shot is quartered twenty times over. Two separate faults:

1. **NO SPILL.** The excess past the shield's remaining points is thrown away.
2. **THE GATE IS NOT THE ONE THE GAME APPLIES.** M1 asked whether Toxin's
   shield-bypass damage is reduced by the gate and never resolved; this says
   the gate does not stop the instance that broke the shield at all.

**IT HAS NEVER SHOWN UP ON THE BOARD**, and that is why it survived: all three
entries in `data/enemies/` — `thrax_centurion`, `corrupted_heavy_gunner`,
`demolisher_devourer` — carry `shield: 0`. Every ruler, every golden test and
every board row is fought against a target with no shields, so the entire
Corpus half of the mitigation model is unexercised.

**AND THE GAME SHOWS IT AS TWO NUMBERS**, which is the shape
`crate::record` was built for on the same day: one row per number the game pops,
each with its own pool and its own mitigation ledger. The Toxin split
(`Pool::Shield` / `Pool::Health`) is already that mechanism; this is a second
member of it, reached by overflow rather than by bypass.


### The second capture: the same fight with the shield ALREADY DOWN 41

The owner's follow-up, on the same Laetum and the same level 1 Crewman with
`+220% +165%` base damage, after knocking 41 points off the shield — so 79
remained:

```
220+165基伤
打掉41的盾
爆头
暴击  79+10002
无暴击 79+4398

身体
爆击 1628+82
无暴击 741+35
```

**It settles the body rule outright.** `health = 0.05 × (damage − shield
remaining)` and `shield shown = damage − health`, at TWO different shield
values and both crit tiers:

| | damage | shield 120 | `0.05 ×` | shield 79 | `0.05 ×` |
|---|---|---|---|---|---|
| white | 776 | 743 + **33** | 32.80 | 741 + **35** | 34.85 |
| crit | 1710 | 1630 + **80** | 79.50 | 1628 + **82** | 81.55 |

**And it settles the head rule's shape**, which the first capture could only
state as a constant: the shield shows its REMAINING POINTS and the hit is
charged **twice** them.

| | damage | shield 120 | `− 240` | shield 79 | `− 158` |
|---|---|---|---|---|---|
| white | 4556 | 79 + **4316** | 4316 | **4398** | 4398 |
| crit | 10160 | **9920** | 9920 | **10002** | 10002 |

Twelve points across two mod levels, two crit tiers and two shield values. The
cost tracks the shield exactly — 240 at 120 points, 158 at 79 — so the `2` is a
property of the rule and not a coincidence of the first capture.

### …and it moves the open question OFF the shield entirely

Fitting the four "no shield" headshot readings against their body counterparts
gives, for both crit tiers:

```
white:  head = 6.0000 × body − 100.0
crit:   head = 6.0022 × body − 103.8
```

A slope that is the same at both tiers, and a flat offset that does not scale
with the damage mods. **That offset is `2 × 50`** — which under the head rule
above is a shield of 50 points, so those readings were taken with the shield
NOT at zero. Adding it back:

| | corrected reading | wiki `Head: 3.0x` + [M60](#m60--headshot-bonuses-add-and-the-crit-tier-ladder-holds--owner-2026-08-25)'s ladder |
|---|---|---|
| head crit | 2,115 / 10,260 | **2,112 / 10,243** — 0.14% |
| head white | 960 / 4,656 | 480 / 2,328 — **exactly ×2** |

So the CRITICAL headshot reproduces the wiki's multiplier and M60's ladder to
inside M60's own unexplained +0.19%, and the WHITE headshot is exactly twice
what the same two say it should be. This engine computes 480 for that shot
(`160 × 3.0`, verified through `/api/simulate`'s hit account), so if the ×2 is
real it is wrong on every white headshot in the app — which on a low-crit build
is most of its damage.

**M60 ASKED FOR THIS EXACT NUMBER AND NEVER GOT IT.** Its own closing paragraph
names the three pop-ups that would close its 0.19%: *"a body white (should be
160), a body yellow (352), and a head white (240)"*. Its capture turned out to
be yellow/orange, so no white headshot was ever measured — and this is the
first one.

### What would close it

**ANSWERED: neither.** The readings were taken through the unit's HELMET — see
the section above. The engine's `Head: 3.0x` is right and nothing in the head
path needed to move. The BODY rule is
implemented (`ENEMY_SHIELD_GATE_LEAK`, `dummy::TargetState::apply`) and
reproduces all eight of its numbers; the head path still charges the shield once
rather than twice, and is therefore known to be off by one shield pool on a
weakpoint hit against a shielded target.

### RESOLVED: the headshot readings were on the HELMET

A Crewman wears one, it is its own destructible hitbox, and while it is on it
takes MORE than the head beneath it — destroy it and headshots read the
ordinary `Head: 3.0x` (owner, 2026-08-27). Every head reading above was taken
through a helmet, and this engine has one head per body and no way for a part
to be destroyed and reveal another, so it aims at the bare head from the first
shot.

**The engine's head multiplier was never wrong.** An unmodded body shot is 160,
this sim computes `160 × 3.0 = 480` for the head, and the capture read 860.

WHAT THE HELMET ACTUALLY IS, IS NOT MEASURED (owner, 2026-08-27). Its
multiplier, whether it has a health pool of its own, how much, and what
destroying it costs are all unknown; the readings are consistent with something
near 6x and that is an inference from four numbers taken through it, not a
figure anybody has read off a page. Nothing should be built on it — the entry
here exists so the next person does not re-derive the anomaly from scratch.

**AND THE CRIT READING'S APPARENT AGREEMENT WAS A COINCIDENCE**, which is worth
recording because it nearly bought a wrong conclusion. `3.0 × 4.4` (the wiki's
head multiplier under M60's critical-headshot ladder) and `6.0 × 2.2` (a 6x
helmet under a plain crit multiplier) are **both 13.2**, so the critical
headshot number cannot tell the two apart and it matched the ladder to 0.14%.
Only the WHITE headshot separates them — 480 against 960 — and it says helmet.

The gap is admitted on the unit (`data/enemies/crewman.yaml`, `unmodeled:`).
Modelling it would need two things this engine does not have — a part with its
own health that can be destroyed, and a part REVEALED by another one breaking —
and a measurement of both, which is the part that does not exist yet.

### Still open: a flat ~100

`head = 6.00 × body − 100` fits all four helmet readings, across two mod levels
and both crit tiers, and the `−100` does not scale with a +385% damage bucket.
It is not a multiplier, not the shield (those readings are single numbers, and
a shielded hit pops two), and not rounding — it is 11.6% of the smallest
reading. It appears only on the head; the four body numbers are exact.

The likeliest explanation is that it is not real: the unmodded capture and the
modded one were separate sessions, and a line fitted through two different
loadouts has an intercept that belongs to neither. **One capture would settle
it** — three damage levels in ONE session, same evolutions and arcanes, only
the mods changed, white headshots. Three points on a line through the origin
means the `−100` was an artifact of the fit.

It changes no model either way while the helmet is unmodelled, so it is a loose
end rather than a bug.

### What is NOT settled — three arithmetic facts that need the owner

The body lines and the head lines do not fit one model, and the mismatch is not
noise:

**(a) The head is exactly 6.0x the body, at BOTH crit tiers.** Fitting
`head = S × B + c` across the unmodded and the +385% base-damage captures
(`B` = 1 and 4.85) gives, for both:

| | S | S ÷ matching body number | c |
|---|---|---|---|
| no crit | 960 | 960/160 = **6.00** | **−100** |
| crit | 2,116 | 2116/353 = **5.99** | **−101** |

M60 measured this weapon's headshot multiplier at **1.5x** on a Techrot Babau
and confirmed the wiki's critical-headshot ladder, under which a head CRIT
should be `1.5 × 4.4 = 6.6x` the base while a body crit is `2.2x` — a head/body
ratio of **3.0 for crits and 1.5 for whites**, not 6.00 for both.

**(b) There is a constant −100 on the head lines and none on the body lines.**
The body numbers scale by exactly 4.85 between the two captures
(160→776, 353→1710); the head numbers scale by 5.30 and 5.04, and only a flat
−100 reconciles them. Nothing in the target's sheet is 100.

**(c) The body lines lose nothing to the shield and the head lines lose 120.**
Comparing TOTALS: `158+2 = 160` and `341+12 = 353` are exactly the no-shield
body numbers, while `120+620 = 740` is exactly 120 below the no-shield 860 (and
the same 120 for the other three head lines). Under any single spill model the
shield should cost the same on both.

**ANSWERED by the second capture above** (owner, 2026-08-27): the pair is
`shield + health` in that order, the body lines were fired at a full 120, and
the head lines' apparent inconsistency was the shield being charged TWICE its
points rather than anything about an evolution. What is left open moved off the
shield entirely — see "What would close it".

## M62 — a volley settles pellet by pellet, and every instance re-reads the target ✅ (owner, 2026-08-27)

A **Laetum** — 100 direct, 300 explosion, the card's own 1:3 — carrying one
Viral damage mod that takes each instance to twice its base, an effect forcing
a Viral proc on every hit, and 110% multishot. Fired at a **body** with no
mitigation at all: no shield, no armour, no vulnerability column, no headshot.
Four numbers popped and the target finished on **four Viral stacks**.

The owner's report, verbatim:

> 还有一个问题 奏凯 100直击 300范围 200%病毒加成，每下强制一下病毒 顺序 200 450 1200 1500 最终4层病毒 你可以推测一下顺序吗 是弹头1和弹头2 你帮我推理一下，是怎么样的顺序

> 就是纯伤害加成200%，你可以假设带了一张200的病毒mod，同时还有个特效是让没一下伤害必定触发病毒，还有概率再出发病毒（因为武器自己有tsatus chance，只是这次没有）。mod只带了110多重，目标完全没减伤，打的是身体。那你推理顺序

```
200   450   1200   1500      (4 Viral stacks at the end)
```

### The order is FORCED by the arithmetic

The four numbers are given SORTED, not in the order they appeared — which is
the whole puzzle. Viral is +100% on the first stack and +25% on every stack
after it, so the only multipliers an instance can read are

```
0 stacks x1.00    1 stack x2.00    2 stacks x2.25    3 stacks x2.50
```

Divide the four numbers by the two instance bases (200 and 600, after the mod)
and exactly one assignment survives:

| # | instance | stacks BEFORE it | Viral | damage |
|---|---|---|---|---|
| 1 | pellet 1 direct | 0 | x1.00 | 200 |
| 2 | pellet 1 explosion | 1 | x2.00 | 1,200 |
| 3 | pellet 2 direct | 2 | x2.25 | 450 |
| 4 | pellet 2 explosion | 3 | x2.50 | 1,500 |

200 cannot be an explosion — that would need x0.667, and a stack count only
climbs. 450 cannot be one either: 450/600 = 0.75, below the x2.00 the second
number has already used. So the two small numbers are the COLLISIONS and the
two large ones the EXPLOSIONS, and from there only one ordering has
multipliers that climb.

The owner's own question was which of the two middle instances comes first —
*"我就在纠结是范围1先还是直击2先"*. It is **explosion 1**: if it were direct 2
the second number would be `200 x 2.00 = 400`, which is not among the four,
while `600 x 2.00 = 1,200` is.

### What it establishes, and it is three separate things

1. **A VOLLEY IS PELLET-MAJOR.** A pellet resolves its own explosion before the
   next pellet's collision — `P1 direct, P1 blast, P2 direct, P2 blast` —
   rather than every collision and then every explosion.
2. **AN INSTANCE DOES NOT AMPLIFY ITSELF.** The first collision reads x1.00:
   its own forced proc lands after it has been settled.
3. **EVERY INSTANCE RE-READS THE TARGET** — not every shot, and not even every
   pellet. Pellet 1's explosion already reads the stack pellet 1's collision
   left one instant earlier.

### It found a real bug, and (3) is the one that was wrong

The engine took its mitigation snapshot **once per pellet**, above the stage
loop, and both halves of a pellet settled against it. Against this fixture it
produced

```
200   600   450   1350        (engine, before the fix)
```

— the same ORDER, and each explosion a step behind, sharing its collision's
stack count. It is a few per cent on any status build, always in the direction
of "this build is good", and invisible in every aggregate this engine reports,
because it is already inside the mean.

`DebuffState::amps` is now read inside the stage loop, once per INSTANCE.
Pruning stays where it was — once per pellet, since the whole volley is at one
instant `t` and pruning again could only be a no-op — which is what keeps the
fix free: measured **-1.5%** on `one_fight` alongside the `Replay.pops`
deletion, every answer unchanged on all four shapes.

The golden test is
`a_volley_settles_pellet_by_pellet_and_each_instance_re_reads_the_target`, and
it pins the four numbers rather than the rule, so any of the three properties
regressing reddens it.

### Why the combat record is what found it

Every other output this engine has would have hidden it. The four numbers are a
mean of 850 either way once they are summed, and 837.5 before the fix — 1.5% on
a Monte Carlo whose own standard error is larger. What made it visible is that
a record ROW states the stacks it read, beside the number they produced, so
"600 at 0 stacks" and "1,200 at 1 stack" are two different sentences instead of
one average.

## M63 — the Grimoire's orb is six unaimed strikes at ×0.8, and one of them is not a shot ✅ (owner, 2026-08-28)

The report opened as a data question and turned out to be a mechanic. Verbatim,
in the order it arrived — the last three messages CORRECT the first, and the
corrections are kept rather than folded in, because two of them caught a wrong
model that had already been built:

> 我发现一个武器和wiki写的完全不一样，那就是grimorie，次要射击完全就不是面板上
> 写的样子，例如我测试实际上次要射击的点球是每下280而不是350，最好的爆炸也是另
> 外一个伤害，我很有理由怀疑，里面的百分比还是不对的

> 实际会有6下直击加最后一个爆炸 … 数值上就是官方的*0.8，直击的时候会强
> 制触发电，但是爆炸的时候没有这个强制触发

> 然后那个球，完全不吃GunCO，直击部分以及爆炸部分都不吃，因为这个算范围直击
> （也就是变相的范围，类似于field）

> 还有这个球无法multishot，永远只有一个

> 这个球实际上碰到以后马上开始电第一下，这个第一下就是field啊，和后面的5下是
> 一摸一样的，然后结束爆炸（有正常falloff），range_m这个没有错，标识的是自己的攻
> 击范围，飞行6m/s和总共飞6s也都是没错的数据

> 我修正一下，电球实际上是选半径6m随机一个人射一下，只有一条chain，chain默认是2个，
> 后续的multishot加成是1*multishot+2，也就是如果面板的multishot面板是2.6，那么就
> 说明稳定4个，概率5个意思
>
> 后续爆炸才是范围内的全部（因为有falloff），电球的射程和最终爆炸的范围都是
> 6m，受范围增益影响

The wiki page agrees with all of it and adds the two lines nobody had read:
*"Orb will shock 1 enemy within 6 meters of it every 1 second. Each enemy hit
chains to an additional 2 enemies within 6 meters"*, *"Every strike from the
alternate fire has a forced Electricity status effect. The strikes and the
forced Electricity proc can hit weakspots"*, *"Tick rate is not affected by
Fire Rate"*, *"Number of chains is affected by Multishot"*.

### The numbers

`Module:Weapons/data/secondary` gives the active attack one hit of 350
Electricity and one blast of 250. Both are the module's own value ×0.8:

| part | module | measured | ratio |
| --- | --- | --- | --- |
| strike | 350 | 280 | 0.800 |
| blast | 250 | 200 | 0.800 |

TWO RATIOS, ONE MULTIPLIER. That is why this was transcribed as one fact rather
than as two corrected numbers: had the second come back at anything but 0.800,
the two halves would be independent slips and each would need its own evidence.

The measurement does NOT settle whether the ×0.8 lives in the weapon or in the
module's column — `350 × 0.8` and a published 280 are indistinguishable under
every later multiplier — and nothing downstream depends on which.

### The shape

The orb is not a shot with an explosion. It lives 6 s and STRIKES six times —
one random body inside 6 m each second — then detonates. It flies at 6 m/s and
drops to 2 m/s once it touches something, which changes WHERE the later strikes
happen and not how many there are. `range_m: 6.0` is the strike's reach, not a
flight distance.

**All six strikes are the same thing.** The owner said it twice, the second time
to correct a model that had made the first one special: *"碰到以后马上开始电第
一下，这个第一下就是field啊"*. That is the property the engine now has to hold
rather than a description of it.

### Four rules, each a different mechanism

**Every strike forces Electricity; the final explosion does not.** One attack
answering the same question both ways, which is why `forced_procs` is declared
per part — the Astilla splits the same way between its collision and its
radial, the Scourge the other way round.

**Nothing here takes Condition Overload** — not the strikes, not the blast. The
owner's reason is what a strike IS: the orb's rather than the gun's, a ranged
strike that behaves like a field. "An AoE part takes no CO unless its own row
says so" is the standing rule, and the wiki's catalog was re-read on the PAGE
the same day with **no Grimoire row of any kind**.

**Multishot does not add orbs — it adds CHAIN TARGETS.** `multishot + 2` enemies
a strike, so a panel reading ×2.6 is four for certain and a fifth 60% of the
time. The chain is not modelled, so the bucket is pinned at the weapon's default
(`locks: [multishot]`), which is the right answer against one target and an
understatement against a crowd — and the weapon says both halves on its own
page, because a padlock with no explanation reads as "worthless everywhere".

**The strikes can find a weak point, and so can the Electricity they force.**
How often is **assumed at a flat 10% each** — the owner's number, and an
assumption rather than a measurement, on the page as well as in the yaml.

### How it is modelled: an ENTITY, not a field

The first two attempts filed the orb as a lingering FIELD, and the owner
rejected the type rather than the numbers:

> 我觉得这个不能算是field，因为field是殴打范围内全部的，有falloff的。这个应该是
> 其他类型，是一个实体有范围的，打击范围内一个目标的，前6下伤害都是一样。以及严
> 谨我们发射的时候，发射点是圆心你应该搞一个更准确的

He is right, and the distinction is not cosmetic. A `lingering:` field is an
AREA: it sits where it landed and burns everyone standing in it, each at their
own falloff distance. An orb has a PLACE OF ITS OWN, it moves, and every strike
reaches exactly one body — so who is in reach is a question about where the orb
is, and a field cannot ask it.

`weapons_data::OrbSpec` is that type. It carries geometry and a clock and
nothing about damage: a fuse, a strike interval, a reach, the two speeds, and
the chain. What a strike DEALS is the attack's own `damage:`, and what the fuse
ends in is the attack's own `radial:` — the same division `beam:` already makes.
An attack with an `orb:` settles no collision and no explosion when it is fired;
it deploys, and everything it deals is delivered later and elsewhere.

THE ORB LEAVES THE MUZZLE, which is the accuracy the owner asked for. It starts
at `space::muzzle(player, aim)` — a point on the shooter's own circumference,
the same place every other shot in this arena leaves from — travels along the
aim ray at 6 m/s, and drops to 2 m/s at the first body it touches without
turning. Its reach is measured from ITSELF, so "within 6 metres" is finally a
statement about a real position rather than about the target.

THE STRIKE COUNT IS NO LONGER WRITTEN DOWN. Six ticks over a six second fuse,
and a tick with nobody inside the reach strikes nobody and is spent. `ceil(6 -
flight)` — the owner's rule — falls out of that: a throw that connects in under
a second loses none, one that takes 2.5 s lands four, and one thrown at nothing
lands none. `a_strike_with_nobody_in_reach_is_spent` asserts both ends and the
ladder between them.

The strike itself is settled by the same function a cloud's tick is, because the
arithmetic of a damage instance on a clock of its own does not depend on what
produced it — and sharing it is what stops the two drifting apart. What is NOT
shared is who it lands on. The record tells them apart: `Origin::Orb`.

### The chain count and the headshot rate, measured off the Invocation mods

The four Invocation mods gain a stack per HIT, which makes a strike's body count
readable off a buff instead of inferred from a damage total. Against twenty
enemies:

| multishot | 1.0 | 1.6 | 2.1 | 2.7 | 3.6 | 3.9 |
| --- | --- | --- | --- | --- | --- | --- |
| bodies a strike reaches | 3 | 4 | 6 | 8 | 10 | 11 |
| `floor(3 × multishot)` | 3 | 4 | 6 | 8 | 10 | 11 |

**`floor(3 × multishot)`**, the struck body included, and a hard floor rather
than a rolled remainder — x2.1 hits six every time, not six-or-seven.

THE ENTRY HAD `multishot + 2` UNTIL THIS. The wiki's two sentences support it
just as well — *"chains to an additional 2 enemies"* and *"Number of chains is
affected by Multishot"* — and the two readings agree at x1.0 and part company
immediately after: at x2.1 the sum says 5 and the product says 6. Both were
consistent with everything known; only the measurement separates them, and it is
worth 47% more bodies at x3.9.

`the_orbs_chain_reaches_three_bodies_per_point_of_multishot` pins the whole
table for exactly that reason — a test at the unmodded count alone passes on the
reading it replaced. Verified to bite: restoring the sum reddens it at x1.6.

**And the headshot rate is measured too, at about 10% per body hit** — five weak
points in 48 hits, counted as hits/heads over six strikes of eight bodies:

```
8/3   8/0   8/0   8/0   8/0   8/2
```

`unaimed_headshot_chance` is declared on the ATTACK rather than on any of its
parts, because the orb picks its own body and the scenario's `headshot_pct` — a
statement about the player's aim — is the wrong number for every strike. Each
body a strike reaches rolls its own, chained ones included, which the sample
shows directly: three of the eight in one strike, two in another.

It is a small sample, so the weapon says "about 10%" rather than 10.4%, and it
is an AVERAGE over a crowd rather than geometry — a fight where the enemies line
up beats it and one against a single tall target may not reach it. The owner's
own framing: *"因为视觉上chain很少，但是实际上应该是这么计算的"* — the chain looks
rare and is not.

Both the collision path and the orb path draw their body part through one helper
(`unaimed_part`), so the six strikes cannot answer differently. A head strike
takes the critical-location fold-in like any other hit on an eligible weak point
— no exception was invented for it, because inventing one would be a claim with
no measurement behind it.

### What the position model found: six is not reachable

**Four strikes land on a lone standing enemy, and no throw distance buys six.**
It is arithmetic rather than a simulation result. A stationary body is inside
the reach for a bounded window:

```
  approach   (reach + body radius) / launch speed   6.25 / 6 = 1.04 s
  departure  (reach + body radius) / slowed speed   6.25 / 2 = 3.13 s

  at contact                          (no approach)            3.13 s  ->  4
  thrown from beyond the reach                                 4.17 s  ->  5
```

Six strikes a second apart need the body in reach for more than five seconds,
and 4.17 s is the most these numbers can buy. The owner proposed that a
mid-range throw would fix it — *"如果是有一定距离，例如10m，那么飞行4m以后，就会
开始第一下（因为半径是6m），那应该就可以完整打完"* — and the model says it does
not: the approach is worth at most another second, so the count goes 4, 4, 5, 4,
5, 4… across the whole range and never six. Measured every metre from contact to
30 m.

WHAT WOULD BUY SIX, measured rather than derived, so the bound above is a
statement about these two numbers and not about the model:

| post-contact speed | reach | strikes at contact |
| --- | --- | --- |
| 2.0 m/s | 6.0 m | 4 |
| 1.5 m/s | 6.0 m | 5 |
| **1.2 m/s** | 6.0 m | **6** |
| 2.0 m/s | 7.5 m | 5 |
| 2.0 m/s | 8.0 m | 5 |
| **0.0 m/s** (it stops) | 6.0 m | **6** |

So the measured six needs the post-contact speed at **1.25 m/s or below**, or a
reach of **9.75 m or more**, or an orb that stays near what it touched.

### …and it is the third one, for a reason the arena cannot have

> 我确定这是对的，之前可以打6个是因为有墙，碰见墙就反弹

**The orb bounces off walls.** Every number in the entry is right and the model
is right; what produced six in game is a room. An orb thrown at a body at
contact meets a wall or the floor within a metre or two, comes back, and spends
its whole fuse near what it was thrown at — so it strikes six times and
detonates on the target. In an open field it drifts twelve metres and does
neither.

THIS ARENA HAS NO WALLS, which is a standing limitation rather than anything new
(the same sentence `ricochet_terrain` has carried since the Latron Incarnons
landed). What is new is a weapon where it is worth a great deal: measured on the
same fight, an orb held near its target against one that drifts away is
**12,611.6 DPS against 8,105.7 — +55.6%**, and the difference is four strikes
becoming six plus a detonation that lands at all.

So the single-target number this app reports for the Grimoire's alt fire is a
FLOOR, and an unusually loose one. It is on the weapon's own page in both
languages rather than left in a yaml, because a player comparing this weapon
against another needs to know that one of them is being measured in a field and
played in a corridor.

The three-row table above stays because it is what MADE the answer findable: it
turned "your six and my four disagree" into three numbers, each checkable in
game, and the owner recognised the mechanism from the third row. That is the
useful thing a position model produces and no aggregate could.

`an_orb_that_drifts_leaves_a_lone_target_behind` pins the whole finding: four at
contact, five as the ceiling over thirty metres, and six at 1.2 m/s and at a
standstill. Whichever of the three turns out to be right, that test says what
the old answer was worth.

### The chain, settled

*"不受增益"* meant the RANGE bucket, not the damage one:

> 这里的增益，是指范围增益，就是跳的距离永远是6m，那些其他的什么暴击等等的都是
> 正常加成的 … 你就认为是chain起来没有衰减的beam chain那种方式就可以，并且存在
> multishot增加跳数的机制

So a hop deals the strike in full — a beam chain with no falloff — and takes
crit, status and every damage mod normally. What it does NOT take is a range
mod: **the jump is always six metres**, while the orb's reach and its detonation
radius both grow with Fulmination. Three distances on one attack, all six metres
unmodded, and only two of them move.

`a_range_mod_widens_an_orbs_reach_and_not_its_chain_hop` asserts the asymmetry
rather than the three numbers, because a test that only read them apart would
pass on an engine that scaled none of them — the mod has to be seen to bite on
two before "and not the third" says anything. Verified to bite: scaling the hop
again reddens it at `7.44 against 6`.

### A bug the question found

Asking it was what exposed the chain's share riding the wrong bracket. It was on
`damage_multiplier`, which is Plentiful Mayhem's and is documented as leaving
the status payload OUT — so a hop at 0.31 of the strike still seeded a full-size
Electricity DoT, and the two readings of the ambiguity came out 4.6% apart when
one of them should have been half the other.

`chain::Instance::share` says which bracket a chain belongs in, and it is
explicit: *"a beam with a smaller base damage, so it scales the hit AND the
status base that hit computes its DoTs from"*. Scaling the part's own
`modified_base` is that, and it put the two readings 1.92x apart (146,590 DPS
against 76,306, three bodies, Hornet Strike) — which is what made the question
worth asking out loud rather than guessing at.

The answer is the first reading, so nothing in the entry changed. The bug did,
and it would have been silently wrong on every chaining orb in a crowd.

### Still open

**`AttackSpec::locks` came and went.** It was added on *"这个球无法multishot，永
远只有一个"* and removed on the correction two messages later: multishot does not
add orbs, it adds CHAIN TARGETS (`multishot + 2` bodies a strike). Pinning the
bucket would have been the right answer to the wrong question, and would have
told a reader the mod is worthless where it is in fact most of what a crowd
build buys. The orb path gives one orb by construction — a deploying shot fires
no pellets — so nothing was needed in its place, and the count goes where the
game puts it.

## M64 — a Tome's meter is a clock you spend, and a kill leaves ammo on it ✅ (owner, 2026-08-28)

The Grimoire's alt fire has never been fireable at will, and the roster said so
in the loudest admission it had: *"everything below for this form is the ceiling
rather than the average, and by a wide margin"*. This is that sentence replaced
by a number.

The page, verbatim:

> Requires a fully filled meter beneath the reticle in order to fire. The meter
> takes 45 seconds to completely recharge. Hitting enemies with the primary fire
> reduces recharge time by 1 second per hit. Picking up secondary or universal
> ammo reduces recharge time by 10 seconds. … Radial damage does not count an
> additional hit. Multishot will count as an additional hit.

### It is a third kind of gate, and it gets its own type

This roster already had two: a MAGAZINE you spend and reload, and an INCARNON
GAUGE you fill with hits. A meter is neither — it fills with TIME. The owner's
call on where to put it (2026-08-28):

> 这个机制目前不多的，你完全可以单独一个类型，等我们真的全部做完，再思考可不可
> 以重构为一种类型

So `MeterSpec` is its own type rather than a bent `GaugeSpec`. One weapon has it;
guessing the shared shape from one is how a wrong abstraction gets built.

WHAT IT DID NOT NEED WAS NEW MODE MACHINERY. `play_modes` reads "does entering
this form cost something you must earn" off the ENTRY rather than off its name,
which was written for the Mausolon and says so in its own comment (owner,
2026-08-07). Declaring a meter therefore moves the form from a sustainable
`alternate` a ruler may rank to a `transformed` mode showing the form's own
numbers, and adds the `cycle` that is how a Tome is played — none of it decided
here. `WeaponSpec::has_gauge` is the one question and now knows both gates.

### The cycle is not a transformation

An Incarnon cycle puts the weapon in the other form. A Tome never leaves its
primary fire — it THROWS the other form's orb, and the orb has been an entity
rather than a state since M63. So `tome_cycle_from_panels` is the base form's
params with the orb and the meter laid on top, and nothing in the shot loop has
to switch.

One thing had to move for it: `unaimed_headshot_chance` was on the ATTACK, and a
cycle fires two forms at once that disagree about it — you POINT the primary
fire and the orb picks its own body. It rides `ResolvedOrb` now, declared once in
the yaml and read only by the strikes.

### What a kill leaves, and why no enemy needed a drop table

The owner asked how the ten catalogued enemies implement ammo drops. They do
not, and they do not have to:

> *"Chance to drop Primary or Secondary Ammo scales with squad size"* — solo 45%
> (60% in Landscapes) … *"Eximus are guaranteed to drop either a Primary or
> Secondary Ammo, each having the same chance of dropping. This does not
> overwrite the enemies normal chance of dropping an Ammo pickup."*
> (wiki `Pickups`)

Ammo is a property of the SQUAD and the place, not of the body — a Lancer and a
Crewman drop it at the same rate. `engine::ammo` is that table; the only
per-enemy term is the Eximus guarantee, which is ADDITIONAL to the ordinary roll
(1.45 expected pickups solo, not 1.0) and which this engine already knows.

Only SECONDARY counts for a tome's meter, and universal packs are placed in a
Simulacrum rather than dropped, so a kill contributes through half its roll:
`0.45 × 0.5 × 10 = 2.25` seconds a kill.

EVERY DROP ARRIVES INSTANTLY (owner: *"我们的场景就假设怪物死掉以后所有的pickup
立刻马上到"*) — no vacuum radius, no walking back. And INFINITE AMMO does not
remove it: the house rule is about the reserve, a real fight is under its cap
almost always, and the pack is on the floor either way (owner, 2026-08-28).

### One orb, and a throw costs an animation

Two corrections that arrived after the meter did (owner, 2026-08-28):

> 同一时间只能有一个球，如果在前一个球存在的期间，再放，原来的球立刻消失。并且这
> 个球是有一个前摇时间的（类似投掷类武器那样），这个前摇时间是可以被fire rate降低）
>
> 主要应该是点击以后0.1s后射出去，间隔反正完美对应射击rate，次要是0.15s后射出去，
> 接着0.85s硬直，才可以继续主要模式。射速mod可以加速这两个动作

**One orb at a time.** A new throw makes the old one vanish — no detonation, no
strikes it had left. In the cycle this is free, because the meter puts throws
tens of seconds apart and a fuse is six; where it bites is anything that throws
faster than the fuse, where six strikes an orb becomes one.

**A throw is 0.15 s of wind-up and 0.85 s of recovery**, both shortened by fire
rate, and their sum is this form's listed fire rate of 1 — the animation IS the
cadence, which is the same fact the module states twice. In the cycle it is the
only price beyond the meter: the primary fire stops for a second every time an
orb goes out, measured at 256 pellets against 271.

**And the primary's own 0.1 s wind-up is modelled too**, which it nearly was
not. It was written off here as latency on the reasoning that the interval
*"corresponds exactly to the fire rate"*, so a sustained engagement fires the
same rounds and the mean does not move. The owner's answer:

> 为啥不建模啊，其他的枪械类武器都是0s子弹出膛，但是这个是0.1s啊，不也是变量吗 …
> 我们要严谨肯定要建模的

He is right, and the reason is the one this app is built on: the combat record's
claim is that a row can be laid beside a recording and checked number for
number, and a stream whose every timestamp is 0.1 s early fails that test. It
also reaches time-to-first-kill and the opening of the DPS curve — the two
figures a short engagement is read by — and it is a VARIABLE, shortened by fire
rate, not a constant to be waved away.

The implementation is one line, because the cadence being exact is what makes it
one: shot `k` lands at `windup + k / rate`, so the engagement STARTS at the
wind-up rather than each shot being delayed one at a time. Coming back from a
throw costs it again — an interval only corresponds to the fire rate while you
are holding the trigger down.

`a_round_leaves_after_the_windup_and_the_interval_is_still_the_rates` asserts the
TIMES rather than a total, which is the whole point: at 2/s it pins
`0.0, 0.5, 1.0 …` against `0.1, 0.6, 1.1 …`, and no aggregate can tell those
apart.

### Two modes, and `transformed` is not one of them

> transformed注意一点，不能套用！！！这个是灵化模式专属的 … 这本书我们应该有2个
> mode，一个是只使用主要射击模式，另外一个是使用主要射击，次要槽满了，再使用次
> 要，然后继续使用主要射击，就这两种

`Transformed` is a state you are IN — an Incarnon window, a form that fires its
own magazine for a few seconds — and the builder shows its numbers because
"while you are in it" is a real thing to ask. A metered form is not a state: you
throw one orb and you are back on the primary before it lands. So `play_modes`
emits only the CYCLE for it, and a Tome has exactly two ways to be played:

* `base` — the primary fire alone
* `cycle` — the primary fire, an orb whenever the meter fills, then the primary
  again

### What it is worth

Solo, unmodded, 180 s, the neutral Tenno:

| mode | what it means | DPS | orbs |
| --- | --- | --- | --- |
| `base` | the primary fire alone | 1,419 | — |
| `cycle` | the weapon | **1,795** | **10** |

The cycle throws ten orbs where the clock alone would give four: the primary's
hits take the 45 second meter down to about 18. Against a killable target the
ammo term shows on top of that — 43 kills bought two more orbs. The throw
animation is the other direction, and visible: 256 primary pellets rather than
the 271 the base form fires.

For scale, what this replaced: the alt fire used to be simulated as though you
could throw an orb every second forever, and reported **8,113 DPS**.

### Still not modelled, and one of them on purpose

* **Health and energy orbs.** The same page lists them and publishes no drop
  chance for either, and they would pay nothing here — this arena has no ability
  economy and the player has no health.
* **Resources.** The owner asked (*"甚至可不可以模拟素材掉落啊"*). A per-enemy
  table, and it feeds none of BUILD, SIMULATE or SOLVE — a farming calculator is
  a different product, which is the rule AGENTS.md states for anything new.
* **Heavy ammo**, the one ammo kind that IS per enemy (5.01% on specific heavy
  units). No Arch-Gun here reads a pickup yet.

## M65 — the eight Tome mods, and two readings that were wrong about all of them ✅ (owner, 2026-08-28)

All eight were transcribed months ago and all eight were filed as paying
nothing. Four of them pay.

### The two readings that were wrong

**"Allies within Affinity Range" excludes you.** It does not. Lohk Canticle and
Fass Canticle were out of scope on the reasoning that this arena has one Tenno
and every point of those cards is spent on other people; the owner plays the
weapon and says the wielder gets it:

> 因为这个有个是可以增加射速的，我们是可以吃到的，因为我们现在已经存在tenno了

The wiki settles nothing either way — it says "ally" and does not say whether
the caster is one — so the measurement decides. Lohk is +7.5% to +30% fire rate
for 15 s on kill, and it is worth **+11.7%** in a fight with kills in it.

**"A drop is not a damage model."** It is now. `engine::ammo` (M64) turns a kill
into what it leaves on the floor, so Khra Canticle's Universal Orb is no longer
refused for being a drop. What refuses it is what the orb CONTAINS: health and
energy, and this arena gives the player neither.

### What each of the eight is worth

| card | what it does | here |
| --- | --- | --- |
| **Lohk Canticle** | +30% Fire Rate to allies on kill, 15 s | **+11.7%** |
| **Jahu Canticle** | −5% Armor and Shields of enemies in range, on kill | **+15.8%** |
| **Vome Invocation** | +4% Ability Strength per hit, 15 stacks | **+10.5%** with Roar |
| **Ris Invocation** | +4% Ability Duration per hit, 15 stacks | **+7.6%** with a 30 s Roar |
| Netra Invocation | +4% Ability Efficiency per hit | nothing — no ability is CAST here |
| Xata Invocation | +1 Energy Regen/s per hit | nothing — no energy pool |
| Fass Canticle | ally shield recharge on kill | nothing — nobody shoots back |
| Khra Canticle | 12% Universal Orb on death | nothing — no health or energy to fill |

Measured solo against eleven level-25 Corrupted Heavy Gunners, cycle mode, one
card at a time. The four that pay nothing measure exactly zero, which is the
control the table needs to mean anything.

### Jahu, and where Affinity Range is measured from

*"Killing enemies reduces the Armor and Shields of other enemies within Affinity
Range"* — and Affinity Range is a 50 m radius **around the squad** (wiki
`Affinity`), not around the corpse. That is what makes it cheap: which body died
does not matter, only that one did, so it is a count rather than a position.

THE SHARES COMPOSE, each kill taking 5% of what is LEFT — the rule every other
strip in this engine follows and the only one under which repeated kills cannot
take armour past zero. UNCONFIRMED against the game: the card states a
percentage and no stacking rule, and the flat reading would make it worth almost
nothing. It compounds with itself, which is most of the +15.8%: a stripped body
dies sooner, and a kill strips again.

The SHIELD half is admitted rather than modelled — `Mitigation` carries an
armour multiplier and nothing that shrinks a shield POOL mid-fight.

### Vome and Ris, and the seam they cross

A mod belongs to the BUILD and an ability to the FIGHT, and these two are a mod
that raises the fight's own knob. They meet in `DummyParams::from_panel`, which
is the one place that holds both — so the Arena now carries the unresolved
PICKS and the strength they were resolved at, and `from_panel` resolves them
again when a card asks.

RE-RESOLVED RATHER THAN RESCALED, for a reason that is easy to miss:
`abilities_data::resolve` settles the same-family contest BY the resolved value,
so a bonus big enough to make a Helminth Roar beat a Rhino's has to be in hand
before the winner is picked. Nothing re-resolves without a card asking, so every
other fight takes the arena's list byte for byte.

TAKEN AT THE CAP, and on this weapon that is nearly exact rather than a
convention: one orb strikes six times and each strike reaches
`floor(3 × multishot)` bodies, so a bare build lands 18 hits with the FIRST orb
against a 15-stack cap, and the meter throws another about every 18 s.

### Still open

**A buff can depend on the weapon being in your hands.** The Invocations' stacks
pay only while the tome is out — swap away and they go dormant, swap back and
they pay again (owner). This engine has one weapon and it is always out, so the
question cannot arise; it is recorded in docs/UNMODELLED.md because it becomes a
real decision the day weapon swapping lands.



## M66 — the charge multiplier is (1 + progress), and a full charge is the ×2 end of it ✅ (owner, 2026-08-30)

Eight readings on a **Ballistica Prime**, all with **Headcracker installed**
(+3 base damage, and it cannot be uninstalled). They settle three things the
published stats do not say, and they confirm two the engine already did.

### The readings

Uncharged panel **43** (2.2 Impact / 23.6 Puncture / 17.2 Slash), charged panel
**158**. "Near full" is the highest the owner could reach by hand without the
bar filling.

| build | no charge | near full | full |
| --- | --- | --- | --- |
| no damage mods | **44** | **85** | **160** |
| +220% base damage | **142** | **273** | **512** |

GunCO, at +220% base damage, written `mods × status types`:

| | 3×2 | 2×2 | 1×3 | 3×1 | 2×1 |
| --- | --- | --- | --- | --- | --- |
| Normal Shot | **241** | **208** | **191** | | |
| Charged Shot | **709** | **643** | | **610** | **577** |
| Incarnon Form | | | | **5864** | |

### 1. ON THIS WEAPON, THE CHARGE MULTIPLIER IS (1 + PROGRESS) — THE BAR, NOT THE CLOCK

**ONE WEAPON, NOT A CLASS.** Everything in this entry is the Ballistica
Prime's, and it is the only weapon this has been found on (owner). No other
charge weapon has been measured for it, and none may be given it without its
own reading — a bow's drawn shot is calibrated against golden values that this
would break.

Releasing early fires: a shot at 50% of the bar deals 1.5× the Normal Shot, and
the number tracks the PROGRESS rather than the time held (owner). The two "near
full" readings are the same release point under both builds — 85/44.34 = 1.917
and 273/141.9 = 1.924 — and a single p = 0.92 reproduces both to the digit:

```
43    × 1.92 × 33/32 =  85.1  ->  85
137.6 × 1.92 × 33/32 = 272.5  -> 273
```

At the top of the ramp the shot becomes the CHARGED attack, which is where the
discontinuity is: 40 × 2 = 80 against the 160 a full charge actually deals.

### 2. A FULL CHARGE DEALS TWICE THE PUBLISHED CHARGED DAMAGE

The wiki's infobox and DE's export both give 76 per projectile. The game deals
**152**: with Headcracker's +3 that is a base of 155, and

```
155   × 33/32 = 159.8  -> 160
155   × 3.2 × 33/32 = 511.5  -> 512
```

The ×2 is on the PUBLISHED base and not on the evolution's flat add — 152 + 3
reproduces both rows where (76 + 3) × 2 = 158 misses both. Neither of this
weapon's other two attacks is doubled: the Normal Shot's 44 is 43 × 33/32 and
the Incarnon form's 5864 is 833 × 3.2 × 2.2 exactly.

### 3. THE CO TERM'S BASE IS DOUBLED WITH IT — 80, NOT 76

Two unknowns fall out of the four charged GunCO rows on their own, because each
row is `(B + 0.4·a·b·C) × 33/32`:

```
(3×2) - (2×2):  0.8·C = 687.5 - 623.5 = 64   ->  C = 80.0
back-substituted:                             ->  B = 496 = 155 × 3.2
```

B confirms §2 independently of the two full-charge pops. C = 80 is the
UNCHARGED 40 with the same ×2 on it, which is the catalog's own sentence
("uses uncharged damage value") surviving the multiplier. As a fraction of this
attack's 152 that is 40/76 = **0.5263**, and the catalog's `50%` is it rounded —
so `co_base_fraction` moved off the published number and onto the measured one.

### What was already right

**Quantization** (M57), on two independent readings: this weapon's 5/55/40
split lands on 2 + 18 + 13 = 33 units of the ModdedBase/32 scale, a flat +3.1%,
and 43 → 44.34 → **44** and 137.6 → 141.9 → **142** are both exact. The
Incarnon form is mono-Slash, so it sits at 32/32 and its 5864 confirms the
other end: a quantization that gained anything there would have missed it.

**The `Adding` class reads the UNEVOLVED base** (M50). Headcracker's +3 is out
of the CO term on all three attacks, which is the class default and needs no
per-perk declaration: 137.6 + 2.4×40 = 233.6 → **241**, and 43 in place of 40
would have printed 248.

**`independent` on the Incarnon form.** 833 × 3.2 × 2.2 = 5864.3 → **5864**,
the CO term as a free-standing final multiplier and the +3 inside the base.

### Not settled by this

**Nothing about how far this reaches, except where it does NOT.** The base
Ballistica and the Rakta Ballistica were measured for the same thing and
**neither has it** (owner) — their charged shots are exactly as published, and
the ×2 is one weapon's, not the family's. Every other charge weapon in the
roster is untested and stays as published: a bow's drawn shot is calibrated
against golden values this would break. `data/notes.yaml`
`charge_x2_is_the_primes_alone` is what the three entries carry.

**The ramp itself is not modelled.** A partial charge is a shot this engine
cannot fire — the entry says so in `unmodeled:`. It is dominated at both ends
on this weapon (a 50% charge is 66 damage per 0.7 s against 44 per 0.3 s and
160 per 1.1 s), so the two ends are the two builds worth ranking, which is what
the roster carries.

**The arsenal's charged panel reads 158**, i.e. (76 + 3) × 2, while the damage
is computed from 152 + 3. The panel doubles the evolution's flat add and the
hit does not; ours shows what the hit uses.


## M67 — the Ballistica's Incarnon form pierces bodies, and its two tier-2 perks keep different clocks ✅ (owner, 2026-08-30)

Three readings on the **Ballistica Prime**, all of them about things the stat
block does not carry.

### THE INCARNON FORM HAS INFINITE BODY PUNCH THROUGH, and its stat reads 0

**Five enemies in a line, all struck, no falloff and no stop.** The arsenal
shows `Punch Through 0.0 m` and the wiki's module agrees, so the number is not
where this mechanic lives — the EVO1 card is: *"Fire cross-shaped projectiles
that punch through enemies"*, printed once for the whole family.

**The Punch Through page never mentions the Ballistica**, in either of its two
lists, which is the only reason this was ever modelled as 0. Its definition of
the class is what the weapon is: *"Some weapons that shoot wide projectiles or
a stream of particles possess infinite body Punch Through … pierce an unlimited
amount of enemies, but not level geometry, objects, or barriers."* An X-shaped
projectile is a wide one, the Dread's and the whole Paris family's Incarnon
forms are on that list, and a community-curated gallery missing an entry is a
gap in the gallery rather than a fact about the weapon.

**Written as `infinite` now, not as a big number.** The word is the statement
and `space::INFINITE_BODY_PUNCH_THROUGH_M` is what the engine holds — finite
on purpose, because a budget that survives every body is spent as flight by
`dissipation_point` and an infinity there is a NaN epicentre. Thirty entries
that carried the number now carry the word.

### TWO PERKS, TWO STACK CLOCKS, and the tier makes them a choice

Both are tier-2 options on the same weapon and they decay differently:

| perk | stacks | clock |
| --- | --- | --- |
| Headcracker | +7.5% fire rate, 10x | INDEPENDENT — each stack carries its own, and they expire one by one |
| Prolific Perforation | +10% crit chance, 8x | CLASSIC — the whole pile goes at once when the window lapses |

Headcracker was already `per_stack_expiry`. Prolific Perforation was not
modelled at all: its clause sat as `out_of_scope` with the reason "one target",
which stopped being true when the arena grew a formation and punch through
started crossing bodies (`space::struck_along`). It is a real buff now —
`BuffTrigger::PunchThrough`, one stack per BOLT that left the body it hit, so a
four-bolt shot into a line earns four — with `BuffDecay::AllAtOnce` and the
crit chance in the bracket its own card names ("additive to other sources of
Critical Chance such as Pistol Gambit").

**AND THE ADMISSION WAS THE WRONG SHAPE ANYWAY.** "Cannot be triggered in this
fight" is a sentence the model should not need: a weapon with no punch through
and a fight with one body earn no stacks by the mechanic itself, and saying so
separately is a second implementation of the rule that can drift from the
first. What the entry now carries is the effect; the number it is worth against
a lone target is zero, and that falls out.

### AND THE PROJECTILE HAS WIDTH — an assumption, flagged as one

The X-shaped projectile is a horizontal line rather than a point, and it
**headshots everything it sweeps**: two rows of enemies 2 m apart, shot down
the middle, take a head hit each (owner). The engine had no width at all — a
shot was a ray that struck bodies within `BODY_RADIUS_M` of its centre line —
so `projectile_width_m` is new, defaults to 0, and 0 is a ray.

**3 m is a working figure and NOT a source.** Neither the wiki, the module nor
DE's export publishes a width for any weapon in the class, on this weapon or on
the Arca Plasmor and Catchmoon the same pages call "wide projectiles". A
measurement replaces it in this weapon's yaml and nowhere else.

The HEADSHOT half needed nothing: punch through already carries the aimed
pellet's hit location down the line ("the same round still flying in a straight
line: this plane holds it at one height"), and a swept body arrives by the same
path. What the width changed is only WHO is on the shot. Down the middle of
those two rows the cycle measures 91,754 DPS against the charged shot's 5,693,
which is the sweep and nothing else — no body is on the centre line at all.

---

## M68 — Primary Compression pays the EVOLVED base into the base-damage bucket, on the form that carries the AoE ✅ (owner, 2026-08-31)

**Burston Prime**, Primary Compression at **rank 1** (the card reads +60%
damage and +3.5% ammo efficiency per metre), Serration (+165%), Incarnon form
with a tier-2 evolution (+42 base damage). The HUD's own readout carries the
metres: a 2.0 m radius, a fifth of it kept, **1.6 m lost → +96%**.

| shot | reading | engine |
| --- | --- | --- |
| base form, aimed and not | 240 / 432, unchanged | no row — nothing to compress |
| Incarnon, from the hip | 146 / 437 | 145.75, x3.0 = 437.25 |
| Incarnon, aimed | 199 / 596 | 198.55, x3.0 = 595.65 |

The arithmetic is one pair of numbers and it settles four columns at once:

```
hip     55 x (1 + 1.65)        = 145.75
aimed   55 x (1 + 1.65 + 0.96) = 198.55
```

- **THE BASE IS THE EVOLVED 55**, 13 + the tier-2 perk's +42 — and not the 13
  that the CO term reads on the same attack (M48). One attack, two bases, and
  only CO gets the smaller one.
- **`Adds` is real.** The bonus joins Serration's bucket and is diluted by it.
  A `Multiplies` row on this build would read 145.75 x 1.96 = **286**, which is
  87 above what the game shows.
- **Effectiveness is 100% of the attack's own radius.** Any other radius, and
  any discount on the payment, moves the +96% the HUD prints.
- **THE RANK RAMP INTERPOLATES.** The wiki publishes ranks 0 and 5 only; a
  rank-1 card reading +60% / +3.5% is 0.5 + (1.0 - 0.5) x 1/5 and
  0.03 + (0.055 - 0.03) x 1/5, i.e. the linear reading of the two endpoints.

**THE ROW BELONGS TO A FORM, and the base form is the control.** Burston
Prime's base form is absent from the table and carries no AoE, and aiming does
not move its number — which is what the engine does by reading the FIRING
form's row (`ap`, not the build's) rather than the weapon's.

**What is NOT settled here.** The explosion's own aimed number was not read.
The engine pays the radial the same bonus as the direct hit, which follows
from a row that names an attack rather than a part, and no reading covers it.

And the base form's `240 / 432` is not reconciled: 88 x 2.65 is 233.2, and
432/240 = 1.8 is not that form's crit multiplier, so the pair is not one
condition's white and crit. The Incarnon's readings are pure Heat and match to
the digit, which is why they carry this entry; the base form's are IPS into a
target whose resistances the reading does not name. Only the half that is load
bearing — the number does not move on aim — is used.

## M69 — a heavy slam deals its ARSENAL number, not the radial one ✅ (owner, 2026-08-31)

**Magistar, no mods, the evolution ladder up to Critical Parallel, heavy slam,
target as close to the point of impact as it goes.**

| reading | value |
| --- | --- |
| highest white | **1094** |
| a critical | **3186** |
| critical multiplier | **3.0x** — the weapon's 2.0 plus Critical Parallel's +1x |

**THE ENTRY SAID 630, AND 630 IS IMPOSSIBLE.** Falloff only ever REDUCES —
linear from the point of impact to 70% at the edge — so the value at the
epicentre is at least the highest white number seen, and 1094 is 74% above the
figure the wiki's own attack row publishes.

**1050 IS THE NUMBER**, `heavySlamAttack` in the export. It is 4% under the
reading, which the entry carries as the gap it is rather than a number invented
to close it.

### …AND IT IS A DERIVATION, not a transcription the measurement leans on

| field | Magistar | as a multiple of the 210 base |
| --- | --- | --- |
| `heavyAttackDamage` | 1260 | **6x** — the hammer's heavy attack multiplier |
| `heavySlamAttack` | 1050 | **5x** |
| `heavySlamRadialDamage` | 630 | **3x** |
| `slamAttack` | 630 | 3x |
| `slamRadialDamage` | 420 | 2x |

**THE RADIAL FIGURES ARE THE CONSTANT** — 2x for a slam and 3x for a heavy slam,
with NO exception across the export's melee weapons, which is the wiki's own
sentence. **The epicentre figures are not**: `heavySlamAttack` is 4x on 86
weapons, 5x on 64, 3.5x on 10 and 1x on a handful, because it follows that
weapon's own HEAVY ATTACK rather than a class rule for slams —
`heavySlamAttack == heavyAttackDamage - base` holds for 118 of them. A hammer's
heavy is 6x, so its heavy slam is 5x, and the 1050 is what that arithmetic says
before any reading is taken.

### THE CRIT AGREES, AND IT IS THE SECOND READING OF THE SAME NUMBER

`3186 / 3.0 = 1062`, against a highest white of 1094 — two slams at slightly
different distances into the same falloff, which is the only thing that
separates them. A pair that agrees through a multiplier neither reading names
is worth more than either alone: it rules out the crit being a tier-2 hit,
which a 20% critical chance cannot reach, and it rules out the white being
anything but the epicentre value.

### …AND THE INCARNON FORM WAS NOT ACTIVE FOR IT

Critical Parallel is tier 4, so the ladder below it is taken — including the
Incarnon Form's `+100% Melee Damage`, which would have put the white at 2188
had it been running. It was not: the form is ENTERED by reaching 6x combo and
heavy attacking, and this reading is of the weapon before that. So the reading
does not test the +100%, and the engine's own decision to apply it for the whole
engagement (docs/MELEE.md §7) is neither confirmed nor denied here.

### What this does NOT settle

- **The light slam's own pair.** `slamAttack` 630 against `slamRadialDamage`
  420 is the same shape one tier down, and no reading covers it — so the
  weapon's own `slam:` keeps the figure it has.
- **Whether a distant body takes the 630.** One explosion at 1050 with falloff
  is what the reading supports and what the entry models; a second, weaker
  radial for everything past the impact point would need a crowd to see.
