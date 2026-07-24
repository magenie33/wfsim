# wfsim — In-game Measurement Protocols

Golden tests need real measurements. Each protocol here is written so a
single session produces an unambiguous answer. Add results inline (date +
numbers), then flip the corresponding `verification.status` / MECHANICS.md
entry.

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

**Model assumption (2026-07-24, user):** Toxin passes in **full** — it never
enters the shield pipeline, so the gate should not see it. Status:
**assumption / unverified** until this protocol is run.

**Target.** Corpus **Crewman** (no armor, Head 3.0x). Base @L1: 90 HP /
120 shields. Spawn at a level with comfortably large shields (e.g. 30+) so
multiple shots are needed per shield bar — exact values don't matter, only
the displayed damage numbers do.

**Weapon.** Hitscan, low crit, one Toxin mod, no other elements. E.g.
Lex (or Dual Toxocyst base) + Pathogen Rounds (+90% Toxin), optionally
Hornet Strike. Note the resulting panel: `P` physical + `T` toxin.
Avoid status-heavy setups or ignore the late (+1 s) DoT tick numbers.

**Steps.**
1. Shoot the **body** with shields still up. Expect per shot: blue `P`
   (shields) + white `T` (health, bypass). Record the white baseline `T`.
2. Whittle shields low, then fire the **breaking shot** at the body.
3. Read the white number of that same instant:
   - `≈ T` → Toxin is **ungated** (assumption confirmed).
   - `≈ 0.05 × T` → Toxin **is gated** (fix MECHANICS.md §8 + engine).
4. Repeat ≥5 times (discard crit-colored readings).

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

## M2 — Simulacrum "Steel Path" toggle: does it still boost armor?

**Question.** The toggle was introduced (U33.5) as "+250% Health, Armor,
and Shields", but U36 removed the armor bonus from Steel Path missions.
Does the Simulacrum toggle still touch armor?

**Sketch.** Armored target (e.g. Grineer Lancer) at a fixed level; compare
damage numbers of the same weapon with the toggle on/off; any change beyond
×(1/2.5) health scaling implies an armor change. (Only affects how we use
the Simulacrum as a lab — missions are authoritative for the engine.)

**Result:** _not yet run._
