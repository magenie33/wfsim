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

---

## The queue — what is waiting on a session

Entries believed and never checked in game. This is the inbox of this file:
an item leaves it ONE way, by becoming an M-number below — run, written up,
and applied.

Being listed here is **not** an admission that a number is wrong. A gap the
model KNOWS it has is stated where the model is — `unmodeled:` in the weapon's
own yaml, or `data/unmodelled/reasons.yaml` — and a rule believed off the wiki
carries **Status: unverified** beside itself in `docs/MECHANICS.md`. This list
is only the order to measure them in.

| entry | what one session would settle |
| --- | --- |
| **Onos** (`onos`, `onos_incarnon`) | The held beam's ramp: the yaml admits the damage climbs to ×2.5 over a hold and models none of it. Read the beam tick at trigger-down, mid-hold and at the top — is ×2.5 the end of a ramp or a step, what clock walks it, and does it survive letting go? Separately, the Incarnon form's charged arm-cannon shot is a second attack with its own explosion and has no entry at all. |
| **Opticor** (`opticor`, `opticor_quick`) | Its catalog row claims the explosion's CO term reads **1000** — the direct hit's whole base, 250% of the radial's own. M72 measured that exact claim on the Vandal and it held to the point, but a row is transcribed for the entry it NAMES, so this one needs its own four readings: unmodded, and three CO brackets, direct and radial each. |
| **Opticor Vandal** (`opticor_vandal`, `opticor_vandal_quick`) | **Read — M72.** The explosion's term is 400, 388 was 400 quantized, and the charge ramps ×1 → ×2 over the firable 60% of the bar. One reading is still wanted, and it is cheap: a release at a KNOWN point (bar 0.70) to show the ramp is straight — the line predicts 291 + 150 unmodded. It stays here until the change set M72 records is applied. |
| **Every melee class but the Tonfa** | Whether M73 generalises. A Tonfa's published "1.2 s charge" measured as 0.4 s of charge and 0.8 s of swing, and the wiki states that figure the same way for every class — so a Hammer's 1.2, a Sword's, a Nikana's are all suspect in the same way and none is read. The reading is M73's: time the input to the next possible input at 1.0x attack speed, then again with wind-up speed bought, and see which part moved. It decides how much a wind-up build is worth on every melee in the roster. |
| **Coda Sporothrix** (`coda_sporothrix`, and `sporothrix` behind it) | **The barb's 0.9 s eruption**, which both yamls admit they do not model — and the delay is an infobox field, not prose. The panel is not the question: every stat checks out against the rendered page, explosion included (48 = 25 Slash + 23 Viral, 2.0 m, 5% / 3.0×, 55%). Five readings, in this order. **Does the barb still erupt when the direct hit KILLS?** — that is what decides whether the delay is scheduling or lost damage, and with 11 rounds at 1.83/s there are two barbs in the air at once. **Does the eruption crit?** — the infobox gives it 5% / 3.0×, so a yellow number is a one-shot confirmation. **Does it take the Coda's valence element**, which is stated as raising "the listed base damage", and **does it take base-damage mods** — the page declines to say for either. **Two status rolls**: *"Initial hit and explosion apply status separately"*, and the eruption carries innate Viral. Also to re-check while there: the rendered infobox gives the AoE linear falloff 100% → 90% over 0–2 m where both yamls carry `falloff_reduction: 0.0`. It changes no answer at the epicentre, so it is a transcription to settle, not a bug to chase. |

---

---

## The index — every entry, and what it settled

An entry is one file under `docs/measurements/`, named `M<n>-<subject>.md`.
**The M-number is the citation**: `data/` and `engine/` cite `M66`, never a
path, so an entry may be renamed but never renumbered.

| # | what it settled | read |
| --- | --- | --- |
| [M1](measurements/M01-toxin-shield-bypass-gate.md) | Is Toxin's shield-bypass damage reduced by the enemy shield gate? | · |
| [M2](measurements/M02-simulacrum-steel-path-toggle.md) | Simulacrum "Steel Path" toggle: does it still boost armor? | · |
| [M3](measurements/M03-corpus-parazon-mercy-cap.md) | Corpus Parazon Mercy cap | ✅ 2026-07-24 (informal) |
| [M4](measurements/M04-anarch-health-curve.md) | Which health curve do Anarchs use? | ✅ 2026-07-24 |
| [M5](measurements/M05-heat-inherit-context-sync.md) | Heat Inherit context sync is bidirectional | ✅ 2026-07-24 (informal) |
| [M6](measurements/M06-frozen-exit-semantics.md) | Frozen exit semantics | ✅ 2026-07-24 (informal) |
| [M7](measurements/M07-freeze-post-thaw-stacks.md) | Who owns the 3 fresh post-thaw Freeze stacks? | ✅ 2026-07-24 |
| [M8](measurements/M08-magnetic-break-proc-attribution.md) | Magnetic break-proc attribution with mixed appliers | · |
| [M9](measurements/M09-incarnon-transition-timings.md) | Incarnon transition timings | ✅ 2026-07-26 |
| [M10](measurements/M10-incarnon-reload-buff-reach.md) | What does a reload-speed buff reach on an Incarnon weapon? | ✅ 2026-07-30 (informal) |
| [M11](measurements/M11-on-hit-per-trigger-or-instance.md) | Is an "on hit" perk judged per trigger pull or per damage instance? | ✅ 2026-07-30 (informal) |
| [M12](measurements/M12-torid-lingering-fields-stack.md) | Do overlapping lingering fields STACK on one target? (Torid) | ✅ 2026-07-30 |
| [M13](measurements/M13-torid-field-tick-clock.md) | The lingering field's tick clock, stacking, and Renewed Horror | ✅ 2026-07-30 (informal) |
| [M14](measurements/M14-ammo-remainder-on-buff-expiry.md) | What happens to the ammo remainder when an efficiency buff expires? | ✅ 2026-07-30 |
| [M15](measurements/M15-torid-incarnon-chain-nodes.md) | Does every chain NODE carry a damage sphere, or only the beam's contact point? (Torid Incarnon) | · |
| [M16](measurements/M16-cernos-prime-tap-rate.md) | How fast can a bow be TAPPED? (Cernos Prime, uncharged form) | · |
| [M17](measurements/M17-arch-gun-exilus-slot.md) | Do Arch-Guns have an exilus slot, and is Zodiac Shred eligible? | · |
| [M18](measurements/M18-sentinel-aiming-and-beam-ammo.md) | Sentinel aiming (answered), and the beam ammo rule (implemented) | · |
| [M19](measurements/M19-deadhead-stacking.md) | Do two Deadheads stack? (Primary + Secondary on one weapon) | · |
| [M20](measurements/M20-primary-frostbite-never-stacked.md) | Primary Frostbite could never earn a stack | · 2026-08-02 |
| [M21](measurements/M21-puncture-weakened-crit-explosions.md) | Puncture's Weakened was critting explosions | · 2026-08-02 |
| [M22](measurements/M22-primary-acuity-unconditional.md) | Primary Acuity was an unconditional +350%/+350% | · 2026-08-02 |
| [M23](measurements/M23-semi-rifle-cannonade-rules.md) | Semi-Rifle Cannonade stated its rules in prose and modelled none | · 2026-08-02 |
| [M24](measurements/M24-one-run-gain-cannot-rank.md) | a one-run gain screen cannot rank a status mod | · 2026-08-02 |
| [M25](measurements/M25-spectral-serration-invisibility-gate.md) | Spectral Serration paid +330% to builds that were not invisible | · 2026-08-02 |
| [M26](measurements/M26-arcanes-that-read-the-warframe.md) | the two arcanes that read a WARFRAME, and the one fact still missing | · 2026-08-02 |
| [M27](measurements/M27-buff-seed.md) | the buff seed decides nothing, or everything | · 2026-08-02 |
| [M28](measurements/M28-primary-frostbite-proc-source.md) | Primary Frostbite stacked off procs that applied no status | · 2026-08-02 |
| [M29](measurements/M29-reified-bane-reload-start.md) | Reified Bane starts at the reload, not at the end of it | · 2026-08-03 |
| [M30](measurements/M30-stat-lock-mod-bucket.md) | a stat LOCK stopped at the mod bucket | · 2026-08-04 |
| [M31](measurements/M31-riven-element-hierarchy.md) | a riven's two elements enter the hierarchy backwards, and a combined element may block the chain | · 2026-08-07 |
| [M32](measurements/M32-incarnon-explosion-base-form.md) | the Incarnon's explosion fired on every base-form shot | · 2026-08-07 |
| [M33](measurements/M33-primary-debilitate-split-base.md) | what base a Primary Debilitate split burns off | · 2026-08-08 — base decided, exponent open |
| [M34](measurements/M34-primary-debilitate-blast-threshold.md) | Primary Debilitate was dead on Blast, and only a run could tell | ✅ 2026-08-10 — threshold generalised |
| [M35](measurements/M35-riven-stat-pools-counted.md) | which riven stats a weapon can roll is not derivable, so it was counted | · 2026-08-08 |
| [M36](measurements/M36-felarx-incarnon-and-gun-co.md) | the Felarx's +2000% and Gun CO multiply | ✅ 2026-08-08 (owner) |
| [M37](measurements/M37-debilitate-dot-attrition-bug.md) | a Debilitate DoT eats Attrition TWICE, and it is a BUG | ✅ 2026-08-08 (owner) |
| [M38](measurements/M38-secondary-fortifier-rule.md) | Secondary Fortifier: the RULE is settled, the NUMBER is not | · 2026-08-09 |
| [M39](measurements/M39-secondary-fortifier-level-shaped.md) | Secondary Fortifier's value is LEVEL-SHAPED, and can be negative | ✅ 2026-08-09 |
| [M40](measurements/M40-xatas-whisper-decode.md) | Xata's Whisper decodes exactly, and two of its clauses are still open | · 2026-08-09 |
| [M41](measurements/M41-hitscan-incarnon-explosion-cadence.md) | a hitscan Incarnon's explosion fires ONCE PER TRIGGER PULL | ✅ 2026-08-11 (owner) |
| [M42](measurements/M42-scourge-field-lifetime.md) | the Scourge's field dies when the NEXT THROW STARTS, not when it lands | · 2026-08-14 (owner) |
| [M43](measurements/M43-throw-cycle-includes-reload.md) | a throw pays for its own reload, so the listed rate is HALF the cycle | ✅ 2026-08-14 (owner) |
| [M44](measurements/M44-sniper-combo-and-scope.md) | the sniper combo and the scope, IMPLEMENTED AND UNMEASURED | · 2026-08-14 |
| [M45](measurements/M45-mausolon-lifted-synergy.md) | the Mausolon's Lifted synergy, UNMODELLED AND UNMEASURED | · 2026-08-15 |
| [M46](measurements/M46-chill-ladder.md) | the chill ladder, walked one stack at a time | ✅ 2026-08-16 (owner) |
| [M47](measurements/M47-body-radius-on-the-floor.md) | a body is 0.2 m across the floor, measured by walking into one | ✅ 2026-08-16 (owner) |
| [M48](measurements/M48-burston-prime-co-direct-share.md) | the Burston Prime's CO reads 13 of its 55 on the DIRECT hit too | ✅ 2026-08-16 (owner) |
| [M49](measurements/M49-dual-toxocyst-co-and-carnage-reign.md) | the Dual Toxocyst computes CO on a flat 75, and Carnage Reign's +33% is GATED, not dead | ✅ 2026-08-26 (owner) — resolved |
| [M50](measurements/M50-torid-incarnon-co-base.md) | the Torid Incarnon's CO reads a flat 51, and the default flipped | ✅ 2026-08-16 (owner) |
| [M51](measurements/M51-multiplying-class-reads-evolved-base.md) | a `Multiplying` entry reads its FULL evolved base, and the two CO classes disagree | ✅ 2026-08-16 (owner) |
| [M52](measurements/M52-chain-path-is-fixed.md) | a chain's path is FIXED, and its rule is not in the formation | ✅ 2026-08-17 (owner) |
| [M53](measurements/M53-burston-incarnon-punch-through.md) | the Burston Incarnon PUNCHES THROUGH, and its blast lands behind you | ✅ 2026-08-20 (owner) |
| [M54](measurements/M54-blast-detonation-weak-point.md) | a BLAST detonation carries the weak point ×3 to everything its sphere reaches, and a TOXIN DoT carries nothing | ✅ 2026-08-22 (owner) |
| [M55](measurements/M55-soma-prime-gunco-base.md) | the Soma Prime's GunCO reads its own 12, and neither Incarnon perk raises it | ✅ 2026-08-22 (owner) |
| [M56](measurements/M56-blast-detonation-no-elemental.md) | a BLAST detonation takes NO elemental bonus, and Lavos can imbue Gas as its own element | ✅ 2026-08-23 (owner) |
| [M57](measurements/M57-quantization-divides-by-moddedbase.md) | quantization divides by ModdedBase, not by the vector's total | ✅ 2026-08-23 (owner) |
| [M58](measurements/M58-status-tick-accumulator.md) | a status tick's accumulator starts at 1, not at 0 | ✅ 2026-08-23 (owner) |
| [M59](measurements/M59-laetum-incarnon-irradiate-echo.md) | the Laetum's Incarnon form doubles Secondary Irradiate's echo | ✅ 2026-08-24 (owner) |
| [M60](measurements/M60-headshot-bonuses-add.md) | headshot bonuses ADD, and the crit-tier ladder holds | ✅ 2026-08-25 (owner) |
| [M61](measurements/M61-shield-break-spillover.md) | a shot that BREAKS a shield keeps killing through it | ⚠ 2026-08-27 (owner) — open |
| [M62](measurements/M62-volley-settles-per-pellet.md) | a volley settles pellet by pellet, and every instance re-reads the target | ✅ 2026-08-27 (owner) |
| [M63](measurements/M63-grimoire-orb-strikes.md) | the Grimoire's orb is six unaimed strikes at ×0.8, and one of them is not a shot | ✅ 2026-08-28 (owner) |
| [M64](measurements/M64-tome-meter-is-a-clock.md) | a Tome's meter is a clock you spend, and a kill leaves ammo on it | ✅ 2026-08-28 (owner) |
| [M65](measurements/M65-tome-mods-eight.md) | the eight Tome mods, and two readings that were wrong about all of them | ✅ 2026-08-28 (owner) |
| [M66](measurements/M66-ballistica-prime-charge-x2.md) | the charge multiplier is (1 + progress), and a full charge is the ×2 end of it | ✅ 2026-08-30 (owner) |
| [M67](measurements/M67-ballistica-prime-incarnon-pierce-and-perks.md) | the Ballistica's Incarnon form pierces bodies, and its two tier-2 perks keep different clocks | ✅ 2026-08-30 (owner) |
| [M68](measurements/M68-primary-compression-evolved-base.md) | Primary Compression pays the EVOLVED base into the base-damage bucket, on the form that carries the AoE | ✅ 2026-08-31 (owner) |
| [M69](measurements/M69-slam-radial-and-flat-add.md) | a slam's radial figure is what a body takes, and a flat base add lands once | ✅ 2026-08-31 (owner) |
| [M70](measurements/M70-gas-cloud-kill-attribution.md) | a Gas cloud's kill is the WEAPON's kill | ✅ 2026-09-01 (owner) |
| [M71](measurements/M71-riven-malus-only-sign.md) | a malus-only riven stat keeps DE's sign, and the listings confirm the size | · 2026-09-01 |
| [M72](measurements/M72-opticor-vandal-co-base-and-ramp.md) | the Opticor Vandal's explosion takes Condition Overload on a base of 400, and its charge ramps ×1 → ×2 across the firable part of the bar | ⚠ 2026-09-02 (owner) — recorded, not implemented |
| [M73](measurements/M73-heavy-attack-two-clocks.md) | a heavy attack is two clocks and each takes its own bucket; a Tennokai swing skips the first | · 2026-09-03 (owner) |
| [M74](measurements/M74-melee-influence-attribution.md) | Melee Influence pays the body it came from | · 2026-09-03 (owner) |
| [M75](measurements/M75-combo-point-per-hit-and-body.md) | a stance swing under 100% still earns a combo point, and every hit and body earns its own | · 2026-09-03 (owner) |
| [M76](measurements/M76-blast-one-hit-per-moment.md) | a Blast going off is one hit per MOMENT, not per stack | · 2026-09-03 (owner) |
| [M77](measurements/M77-electricity-stun-refresh.md) | Electricity's stun cannot be re-applied while it runs | · 2026-09-03 (owner) |
| [M78](measurements/M78-kuva-nukor-valence-in-co-base.md) | the valence bonus is inside the Condition Overload base, and the Kuva Nukor counts a status type nobody can see | · 2026-09-04 (owner) |
| [M79](measurements/M79-flat-base-add-and-eclipse.md) | a flat base-damage add rides BESIDE an attack's own multiplier, and Eclipse does not reach Condition Overload | · 2026-09-04 (owner) |

## By weapon

Built from the `M<n>` citations in `data/weapons/`, so a weapon appears here
only where a reading of it is load-bearing in the roster. An entry that
settles a RULE rather than a weapon has no row and is found by number above.

| weapon | id | entries |
| --- | --- | --- |
| Amprex | `amprex` | [M15](measurements/M15-torid-incarnon-chain-nodes.md) |
| Atomos | `atomos` | [M15](measurements/M15-torid-incarnon-chain-nodes.md) |
| Ballistica Prime | `ballistica_prime` | [M66](measurements/M66-ballistica-prime-charge-x2.md) · [M67](measurements/M67-ballistica-prime-incarnon-pierce-and-perks.md) |
| Boar Prime | `boar_prime` | [M29](measurements/M29-reified-bane-reload-start.md) |
| Braton | `braton` | [M1](measurements/M01-toxin-shield-bypass-gate.md) |
| Braton Prime | `braton_prime` | [M41](measurements/M41-hitscan-incarnon-explosion-cadence.md) · [M56](measurements/M56-blast-detonation-no-elemental.md) · [M57](measurements/M57-quantization-divides-by-moddedbase.md) · [M58](measurements/M58-status-tick-accumulator.md) |
| Burston | `burston` | [M15](measurements/M15-torid-incarnon-chain-nodes.md) · [M41](measurements/M41-hitscan-incarnon-explosion-cadence.md) · [M53](measurements/M53-burston-incarnon-punch-through.md) |
| Burston Prime | `burston_prime` | [M48](measurements/M48-burston-prime-co-direct-share.md) · [M54](measurements/M54-blast-detonation-weak-point.md) · [M68](measurements/M68-primary-compression-evolved-base.md) |
| Cernos Prime | `cernos_prime` | [M16](measurements/M16-cernos-prime-tap-rate.md) · [M20](measurements/M20-primary-frostbite-never-stacked.md) · [M28](measurements/M28-primary-frostbite-proc-source.md) |
| Cryotra | `cryotra` | [M47](measurements/M47-body-radius-on-the-floor.md) |
| Dual Toxocyst | `dual_toxocyst` | [M9](measurements/M09-incarnon-transition-timings.md) · [M49](measurements/M49-dual-toxocyst-co-and-carnage-reign.md) |
| Felarx | `felarx` | [M36](measurements/M36-felarx-incarnon-and-gun-co.md) · [M37](measurements/M37-debilitate-dot-attrition-bug.md) |
| Furis | `furis` | [M35](measurements/M35-riven-stat-pools-counted.md) |
| Gotva Prime | `gotva_prime` | [M30](measurements/M30-stat-lock-mod-bucket.md) |
| Grimoire | `grimoire` | [M63](measurements/M63-grimoire-orb-strikes.md) · [M64](measurements/M64-tome-meter-is-a-clock.md) |
| Kuva Nukor | `kuva_nukor` | [M78](measurements/M78-kuva-nukor-valence-in-co-base.md) |
| Laetum | `laetum` | [M9](measurements/M09-incarnon-transition-timings.md) · [M10](measurements/M10-incarnon-reload-buff-reach.md) · [M11](measurements/M11-on-hit-per-trigger-or-instance.md) · [M15](measurements/M15-torid-incarnon-chain-nodes.md) · [M46](measurements/M46-chill-ladder.md) · [M59](measurements/M59-laetum-incarnon-irradiate-echo.md) · [M60](measurements/M60-headshot-bonuses-add.md) · [M61](measurements/M61-shield-break-spillover.md) · [M62](measurements/M62-volley-settles-per-pellet.md) |
| Larkspur Prime | `larkspur_prime` | [M19](measurements/M19-deadhead-stacking.md) |
| Magistar | `magistar` | [M69](measurements/M69-slam-radial-and-flat-add.md) · [M79](measurements/M79-flat-base-add-and-eclipse.md) |
| Mandonel | `mandonel` | [M47](measurements/M47-body-radius-on-the-floor.md) |
| Mausolon | `mausolon` | [M45](measurements/M45-mausolon-lifted-synergy.md) |
| Nukor | `nukor` | [M78](measurements/M78-kuva-nukor-valence-in-co-base.md) |
| Ocucor | `ocucor` | [M39](measurements/M39-secondary-fortifier-level-shaped.md) |
| Opticor | `opticor` | [M41](measurements/M41-hitscan-incarnon-explosion-cadence.md) |
| Opticor Vandal | `opticor_vandal` | [M72](measurements/M72-opticor-vandal-co-base-and-ramp.md) |
| Phantasma | `phantasma` | [M15](measurements/M15-torid-incarnon-chain-nodes.md) |
| Phenmor | `phenmor` | [M9](measurements/M09-incarnon-transition-timings.md) |
| Praedos | `praedos` | [M73](measurements/M73-heavy-attack-two-clocks.md) |
| Sancti Magistar | `sancti_magistar` | [M69](measurements/M69-slam-radial-and-flat-add.md) |
| Scourge | `scourge` | [M42](measurements/M42-scourge-field-lifetime.md) |
| Scourge Prime | `scourge_prime` | [M43](measurements/M43-throw-cycle-includes-reload.md) |
| Soma Prime | `soma_prime` | [M55](measurements/M55-soma-prime-gunco-base.md) |
| Torid | `torid` | [M12](measurements/M12-torid-lingering-fields-stack.md) · [M13](measurements/M13-torid-field-tick-clock.md) · [M15](measurements/M15-torid-incarnon-chain-nodes.md) · [M20](measurements/M20-primary-frostbite-never-stacked.md) · [M22](measurements/M22-primary-acuity-unconditional.md) · [M24](measurements/M24-one-run-gain-cannot-rank.md) · [M25](measurements/M25-spectral-serration-invisibility-gate.md) · [M27](measurements/M27-buff-seed.md) · [M50](measurements/M50-torid-incarnon-co-base.md) · [M51](measurements/M51-multiplying-class-reads-evolved-base.md) · [M52](measurements/M52-chain-path-is-fixed.md) |
| Verglas Prime | `verglas_prime` | [M22](measurements/M22-primary-acuity-unconditional.md) |
| Zylok | `zylok` | [M41](measurements/M41-hitscan-incarnon-explosion-cadence.md) |
