# wfsim — Damage Mechanics Reference

This document records **how every number is actually computed**: elemental
combination, damage modifiers, critical hits, status/procs, armor and shields,
faction resistances, and time-based effects.

It is the authoritative specification the `engine/` crate implements. The engine
and this document must agree; when they disagree, a golden test decides which is
wrong (see [`CORE.md`](CORE.md) §2, §5).

## Conventions

Every mechanic below is recorded with a fixed structure:

- **Definition** — what the mechanic is, in one or two sentences.
- **Formula** — the exact math, with variables defined.
- **Source** — where it comes from: `wiki`, `datamine`, or `measured`
  (in-game Simulacrum). Per [`CORE.md`](CORE.md) §4 (principle 4), every rule in
  code must carry a comment pointing back to its source here.
- **Status** — `unverified` (transcribed, not yet checked against measurement),
  `verified` (a golden test confirms it), or `disputed` (sources disagree).

> ⚠️ Everything below starts as **`unverified`** and is a working transcription.
> Nothing here is "correct" until a golden test in `tests/golden/` confirms it
> against in-game measurement. Do not treat this file as ground truth yet.

Notation: `IPS` = Impact / Puncture / Slash (the physical types). `⊕` denotes
elemental combination. All percentages are expressed as fractions unless noted.

---

## 1. Damage types

**Definition.** Every hit is a vector over damage types, not a scalar.

- **Physical:** Impact, Puncture, Slash.
- **Primary elemental:** Cold, Electricity, Heat, Toxin.
- **Secondary (combined) elemental:** Blast (Heat⊕Cold), Corrosive (Electricity⊕Toxin),
  Gas (Heat⊕Toxin), Magnetic (Cold⊕Electricity), Radiation (Heat⊕Electricity),
  Viral (Cold⊕Toxin).
- **Special:** True, Void, Tau (context-specific; enumerate as encountered).

**Source:** wiki. **Status:** unverified.

---

## 2. Mod resolution (pipeline layer [1])

**Definition.** Collect every active bonus and sort each into the correct
*multiplicative bucket*. Bonuses of the same bucket add together; buckets
multiply against each other.

**Buckets (draft — to be confirmed):** base damage, per-element, multishot,
critical chance, critical damage, status chance, status duration, fire rate,
faction damage, plus conditional buckets (combo, arcanes, weapon-specific).

**Formula (shape).**
```
final_stat = base * (1 + Σ bonuses_in_bucket_A) * (1 + Σ bonuses_in_bucket_B) * …
```
The hard part is **which bonuses share a bucket** vs form an independent
multiplier. Getting this wrong changes results substantially. Examples where the
distinction matters (see [`GLOSSARY.md`](GLOSSARY.md)):
- **Crit chance:** flat crit chance (additive absolute) vs crit chance multiplier.
- **Fire rate:** fire-rate mod bonus (one additive bucket) vs an independent
  fire-rate multiplier (e.g. Dual Toxocyst Frenzy "+150%" is really ×2.5 applied
  on its own):
  `effective_fire_rate = base × (1 + Σ mod_bonuses) × Π multipliers`.

**Mod order matters.** Mods are an **ordered list**, not a set — elemental
combination (§3) depends on the order. An Effect can inject a modifier at a
defined position (Frenzy appends "+100% Toxin" at the **end** of the order).

**Ammo efficiency.** `shots_per_ammo = 1 / (1 - e)`; sources add (except
Energized Munitions, multiplicative); `e = 1.0` → infinite ammo.

**Source:** wiki + measured. **Status:** unverified. **High-risk** (CORE.md §3).

---

## 3. Elemental combination (pipeline layer [2])

**Definition.** Primary elements combine into secondary elements based on the
**order mods appear in the configuration**, not a fixed priority.

**Rules (draft).**
1. The weapon's **innate** elements combine first, before mod elements, under
   their own rules.
2. Mod-added primaries combine **pairwise in mod-slot order**: the first two
   compatible primaries merge into their secondary, then the next, etc.
3. A secondary element does not further combine.
4. Elements **injected by a buff** enter the order at their defined position —
   e.g. Frenzy's "+100% Toxin" is appended **last**, so it combines as the final
   mod. It is **additive with elemental mods** and combines with them: with a
   lone Heat mod equipped it yields Gas; if the build already produces a combined
   element containing Toxin (e.g. Corrosive), the injected Toxin is added to that
   element's damage instead. (Source: wiki, Dual Toxocyst Frenzy.)

**Source:** wiki + measured. **Status:** unverified. **High-risk** — order
dependence and innate-vs-mod timing are top calibration targets (CORE.md §3).

---

## 4. Per-hit damage vector (pipeline layer [3])

**Definition.** After mods and elements, a single projectile has a concrete
damage vector `{Impact, Puncture, Slash, Cold, …, Corrosive, …}`.

**Formula.** Output of §2 applied over the base IPS distribution, plus the
elements produced by §3. Multishot pellets each carry this vector (see §7).

**Source:** derived. **Status:** unverified.

---

## 5. Critical hits (pipeline layer [4])

**Definition.** Critical chance can exceed 100%, producing **tiered** crits.
Terminology per [`GLOSSARY.md`](GLOSSARY.md): **crit tier** (0 white / 1 yellow /
2 orange / 3+ red), **big crit** = tier ≥ 2.

**Two crit-chance buckets — do not conflate.** The word "+crit chance" hides two
different operations:
- **Crit chance multiplier** (e.g. Point Strike +150%) scales the base.
- **Flat crit chance** (e.g. Secondary Enervate +10/stack) adds absolute
  percentage points, not scaled by base.

**Effective crit chance (draft).**
```
effective_cc = base_cc × (1 + Σ crit_chance_multipliers) + Σ flat_crit_chance
```

**Tiers.** For `effective_cc` (wiki `Critical_Hit` §Critical Tiers):
- Guaranteed tier `t = floor(effective_cc)`.
- Chance of tier `t+1` = `effective_cc - floor(effective_cc)`.
- A tier-`k` hit multiplies damage by `1 + k*(cd - 1)`, where `cd` is the
  critical damage multiplier ("Critical Tier Multiplier").
- Colors: tier 1 yellow, tier 2 orange (= "big crit"), tier 3+ red; each tier
  above 3 adds an exclamation mark (up to three), the color stays red.
- Scaling is **purely linear** in `k` and there is **no tier cap** — red crits
  carry **no** hidden bonus beyond the formula; the color is cosmetic.

**Quantization** (wiki `Critical_Hit` §Quantization). The **base** crit damage
multiplier is quantized to steps of `32/4095`:
```
quantized_base_cd = round(base_cd × 4095/32) × 32/4095
```
Order matters: **flat/absolute** CD bonuses (e.g. Incarnon "Critical Parallel"
+0.4) add into `base_cd` **before** quantization; **relative** (%) mods (e.g.
Vital Sense) multiply **after**. Required for shot-by-shot parity with in-game
numbers.

> **Decision (2026-07-24): recorded only, deliberately NOT implemented yet.**
> Implement when pipeline layer [1] (mod resolution) lands, before golden
> tests that compare per-shot numbers — without it those will never match
> exactly.

Related: per-shot **damage** quantization also exists and was changed from
1/16 to 1/32 steps in Update 40 (undocumented, per the wiki `Damage` patch
history). Exact mechanics not yet transcribed — same recorded-only status.

**Critical headshots** (wiki `Critical_Hit` §Critical Headshots). A critical hit
on a head/weak-point location gets an **additional 2.0x** bonus on top of the
location multiplier and the crit damage multiplier, folded into the tier
formula by doubling `cd`:
```
headshot_crit_tier_mult = hs_mult × (1 + k*(2*cd - 1))
```
Exceptions:
- Locations with a **1x** multiplier get **no** critical-headshot bonus (even if
  the multiplier is later raised by buffs).
- **Corpus humanoids** (helmeted) take only the plain 3.0x headshot damage — no
  critical-headshot bonus.
- Some parts take location damage but no crit bonus at all (e.g. MOA "fanny
  pack": 3.0x, no crit interaction).

Melee **combo** raises effective crit chance/damage; interaction with tiers is a
high-risk area.

**Source:** wiki + measured. **Status:** unverified (flat-vs-multiplier
distinction, big-crit definition, tier and critical-headshot formulas sourced
from wiki; all need measurement). **High-risk** (CORE.md §3).

---

## 6. Status / procs (pipeline layer [5])

**Definition.** Each hit may inflict status effects; probability and which
element procs are weighted, not uniform.

**Rules (draft).**
- **Status chance** per pellet is affected by multishot distribution (see §7).
- When multiple elements coexist, each element's **proc weight** is proportional
  to its share of the hit's elemental damage — **not** an even split.
- **DoT** effects (Heat, Toxin, Slash, Gas, …) deal damage over time with their
  own stack caps and durations; model each as a time series (feeds §9).

**Source:** wiki + measured. **Status:** unverified. **High-risk** — status
weighting and multishot interaction are top calibration targets (CORE.md §3).

---

## 7. Hit resolution (pipeline layer [6])

**Definition.** How shots actually land — the "hardcore" differentiator.

**Mechanics.** Multishot pellet count and its probabilistic split;
range/damage falloff; ballistics/projectile travel; hit chance; AoE radius and
falloff; headshot multiplier; punch-through. AoE self-damage/falloff and whether
headshots can crit are known edge cases.

**Body parts / location multipliers** (wiki `Enemy_Body_Parts`, `Headshot`).
Targets are made of **body parts**, each with its own damage multiplier. A
part carries three independent properties:
1. **Location multiplier** — humanoid head 3.0x (almost all Grineer / Corpus /
   Infested), body 1x. Outliers: Nox helmet 3x / exposed head 4x, Amalgam
   Machinist head 0.5x, MOA "fanny pack" 3x, Bursa riot shield 0x / front
   0.4x, boss weak points on a 0x body (Sargas Ruk vents 1x, Lephantis
   mouths 1x, Jordas engines 1x).
2. **Headshot trigger** (`is_head`) — **headshot is a trigger condition, not a
   damage stat**. "Effects that specify headshots only take effect when
   striking the target's head and do **not** apply against any other weak
   spot" (§Weak Spot Bonuses). So Frenzy/Covenant-style effects never fire on
   a MOA fanny pack or a boss weak point; Charger "mouth" is explicitly
   "1x, not a headshot".
3. **Critical-location eligibility** — whether a crit on this part gets the
   `2*cd` fold-in of §5. Ineligible even at >1x: MOA fanny pack, helmeted
   Corpus heads. 1x locations are never eligible.

Weapon-side exceptions: some weapons always hit at 1x (beams like Ignis,
launchers like Kuva Bramma); the **radial** part of AoE damage is always 1x
and **cannot trigger headshot conditions** (the direct projectile can).
Headshot-damage bonuses (e.g. sniper zoom) stack additively with each other.

The critical-headshot damage interaction is specified in §5.

**How many Hits a Shot produces (source: wiki).** A **Hit** (the on-hit-effect
trigger, per [`GLOSSARY.md`](GLOSSARY.md)) is not the same as a Multishot
instance or an enemy touched, and the count is weapon-archetype dependent:
- Hitscan hitting multiple enemies at once (Multishot / Punch Through) → 1 Hit.
- Projectile / non-chained beam hitting multiple enemies at once → multiple Hits.
- AoE explosion → 1 Hit.
- Shotgun-sidearm pellets (tied to Multishot) → not separate Hits.

This governs how many `Hit` events the timeline emits per Shot, which drives
on-hit effects (e.g. Secondary Enervate).

**Source:** wiki + measured. **Status:** unverified (hit-counting rules sourced
from wiki; falloff/ballistics/AoE math need measurement). **High-risk**
(CORE.md §3).

---

## 8. Target mitigation (pipeline layer [7])

**Definition.** How the target reduces incoming damage.

**Faction damage modifiers (post-U36 system).** As of **Update 36** ("Jade
Shadows", 2024-06, "Simplified Faction Resistances") the Damage-2.0
health/armor/shield **classes no longer exist**: every enemy has plain
Health / Armor / Shield, and damage-type vulnerabilities/resistances are
**faction-wide**, always active regardless of armor/shield presence:
```
vulnerable -> x1.5 incoming    resistant -> x0.5 incoming
```
Full table in `data/factions/damage_modifiers.yaml` (e.g. Grineer: +Impact
+Corrosive; Corpus: +Puncture +Magnetic; Zariman: +Void only). Special
layers: **Object** health takes no crits/status/modifiers; **Overguard** is
neutral except x1.5 Void, blocks status spillover, and grants CC immunity.
An enemy can carry a `FactionDamageOverride` (Thrax units count as Zariman).

**Armor → damage reduction.**
```
DR = armor / (armor + 300)
damage_to_health = incoming * (1 - DR)
```
Corroborated by wiki `Enemy_Level_Scaling` §Armor: armor is **hard-capped at
2,700 = 90% DR** (2700/3000 ✓); enemies that would *spawn* with < 200 armor
get 200 (initial value only — strips can still go below). Armor strip
(Corrosive −26%/stack to −80%, Heat −50%) modifies `armor` before this.

**Shields vs health.** Toxin (and its DoT) bypasses shields; Magnetic amps
damage to shields/Overguard; shield gating exists on some units. Details TBD.

**Level scaling** (wiki `Enemy_Level_Scaling`; community-derived, DE has not
confirmed — treat as unverified). Common structure, with `Δ = current level −
base level` and per-stat/per-faction coefficient & exponent:
```
current = base × [f1(Δ)·(1−S(Δ)) + f2(Δ)·S(Δ)]
f1/f2 = 1 + c·Δ^e   (low-level / high-level curves)
S = smoothstep between the transition bounds:
    S(Δ) = 3T² − 2T³,  T = (Δ − lo) / (hi − lo), clamped to [0,1]
```
- **Health** (transition 70–80): Grineer/Scaldra `f1: 0.015·Δ^2.12`,
  `f2: 10.7332·Δ^0.72`; Corpus `0.015·Δ^2.12` / `13.4165·Δ^0.55`; Infested
  `0.0225·Δ^2.12` / `16.0998·Δ^0.72`; Anarchs+Corrupted `0.015·Δ^2.1` /
  `10.7332·Δ^0.685`; Murmur/Sentient/**Unaffiliated** `0.015·Δ^2` /
  `10.7332·Δ^0.5`; Techrot `0.02·Δ^2.12` / `15.0998·Δ^0.7`.
- **Shields** (70–80): Corpus `0.02·Δ^1.76` / `2·Δ^0.76`; Corrupted+Anarchs
  `0.02·Δ^1.75` / `2·Δ^0.75`; Grineer/Sentient `0.02·Δ^1.75` / `1.6·Δ^0.75`;
  Techrot `0.02·Δ^1.76` / `3.5·Δ^0.76`.
- **Armor** (70–80, all factions): `0.005·Δ^1.75` / `0.4·Δ^0.75` (then the
  2,700 cap).
- **Overguard** (transition **45–50**, uses `x−1` not Δ): `0.0015·(x−1)^4` /
  `260·(x−1)^0.9`. All Eximus have base Overguard 12.
- **Damage** (dealt by enemies): default `1 + 0.015·Δ^1.55`; Grineer / Corpus
  / Techrot use a smoothstepped pair (`0.015·Δ^1.75` below Δ=1, `0.0075·Δ^1.55`
  above Δ=25) **and** a flat 2x on attacks (Infested: 3x).
- **Affinity**: `1 + 0.1425·level^0.5` (×3 base for Eximus) — uses **current
  level**, not Δ, and the result is floored.
- Eximus units additionally replace base health/shields with level-dependent
  boosted values (piecewise formulas in the wiki page §Health/§Shields tabs).

**Level cap.** Enemy levels cap at **9999**; only Void Fissure missions exceed
it. Implemented in `engine::scaling` with regression tests at the cap.

**Steel Path** (wiki `The_Steel_Path`). Enemy level **+100** (+50 in
Archwing/Railjack, +20 in Duviri with no stat bonus), health **×2.5**, shields
**×2.5**. **Armor is NOT increased** (removed in U36, which also fixed shields
accidentally double-applying to ×6.25). Caveat for golden tests: the
Simulacrum's "The Steel Path" toggle was described at introduction (U33.5) as
"+250% Health, Armor, and Shields" — whether its armor bonus also went away
with U36 must be **measured**.

**Simulacrum limits** (for golden-test planning): enemy level can only be set
up to `5 × Mastery Rank + 30` (+25 in some Simulacrum variants) — nowhere
near 9999, so level-cap behavior is only verifiable in endless missions.
The Simulacrum has **no** enemy-invincibility or instant-respawn toggle; the
engine's `TargetMode::{InfiniteHealth, InstantRespawn}` are simulator
conveniences (user decision 2026-07-24), and **on-death transformations are
not modeled** (a respawned Thrax is always the physical form — the spectral
form is skipped).

**Source:** wiki + measured. **Status:** unverified. **High-risk** (CORE.md §3).

---

## 9. Temporal integration (pipeline layer [8])

**Definition.** Advance along the time axis to produce a damage-vs-time series.

**Mechanics.** Fire cadence, magazine depletion and reload, combo build/decay,
DoT stacking, buff duration and refresh. Steady DPS, burst DPS, and TTK are
**statistics derived from this series**, not primary inputs (CORE.md §2).

**Source:** derived. **Status:** unverified.

---

## Open questions

- Exact multiplicative-bucket membership for every common mod (§2).
- Armor DR constant/curve and level-scaling formulas (§8).
- Innate-vs-mod elemental combination timing on multi-innate weapons (§3).
- Proc-weight formula when 3+ elements coexist (§6).
- Incarnon-form stat/mechanic overrides (see the Dual Toxocyst prototype).
