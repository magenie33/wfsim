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

**Model assumption (2026-07-24, user; revised same day):** the gate is the
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

**Result (user in-game tests, 2026-07-24):**
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
calculated stat block for Commandeered Ash Prime @L1000 (user-supplied:
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

**Result (user, 2026-07-24):** **Yes — bidirectional.** The first proc's
brackets apply to every later contribution in both directions. Build
consequence: light the first Heat proc with the best-modded weapon; any
source can then feed the ramp at full value.

---

## M6 — Frozen exit semantics ✅ (informal, 2026-07-24)

**Question.** When Frozen (the 10th-Freeze-stack state) expires, are the
remaining 3 Cold stacks the surviving old ones (carried timers) or a
hard reset?

**Result (user, 2026-07-24):** **Hard reset**: Freeze is set to exactly
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

**Result (user, 2026-07-24):** **Hypothesis B — the trigger.** The 3
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
reload time (user-confirmed).

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

**Result (user, 2026-07-26):** transmute-in = the weapon's reload time
(2.35 s unmodded); **revert-out has its own base of 1.0 s** (measured
exactly 1.3 s under −30% reload speed). Both directions scale with
reload-speed bonuses. Full cycle downtime = 2.35 + 1.0 = **3.35 s**
unmodded; the pseudo-reload model updated accordingly. Minor residue:
the exact scaling formula direction (time × (1+penalty) vs
base/(1+bonus)) — one more mod value would pin it.

---

## M2 — Simulacrum "Steel Path" toggle: does it still boost armor?

**Question.** The toggle was introduced (U33.5) as "+250% Health, Armor,
and Shields", but U36 removed the armor bonus from Steel Path missions.
Does the Simulacrum toggle still touch armor?

**Sketch.** Armored target (e.g. Grineer Lancer) at a fixed level; compare
damage numbers of the same weapon with the toggle on/off; any change beyond
×(1/2.5) health scaling implies an armor change. (Only affects how we use
the Simulacrum as a lab — missions are authoritative for the engine.)

**Result:** _not yet run._
