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
- **Special:** Void, Tau (context-specific; enumerate as encountered);
  **True** (wiki `Damage/True_Damage`) and **Cinematic**
  (`DT_CINEMATIC_DAMAGE`, wiki `Damage/Cinematic_Damage`) — functional
  twins: hidden, no faction modifiers, bypass **armor** DR only (never
  other DR sources), unaffected by physical/elemental bonuses, no procs,
  Sentients don't adapt. Distinct internal types with **disjoint
  sources**: True = Finisher attacks, many Warframe abilities, Basmu's
  pulse (never moddable onto weapons); Cinematic = Bleed ticks only.
  Keep them separate — finisher-damage effects touch neither Bleed nor
  Cinematic. (Community formerly called Cinematic "Finishing Damage".)

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

**Rules** (wiki `Status_Effect` §Status Chance / §Damage Distribution /
§Forced Procs / §Multishot).
- **Per-projectile roll**: the listed status chance `SC` is the probability
  that **each pellet individually** procs (multishot multiplies
  opportunities, not the per-hit chance). Each enemy touched by one attack
  rolls separately.
- **Type is drawn only after a roll succeeds**, weighted by damage share of
  the hit's (modded) vector: `P(type) = damage_type / total_damage`.
- **SC > 100%**: `floor(SC)` guaranteed rolls + `frac(SC)` chance of one
  more; **each roll's type is drawn independently** (the same type can
  repeat within one hit).
- **Forced procs** are guaranteed effects independent of both `SC` and the
  damage distribution, and occur **alongside** rolled procs ("not the same
  as 100% status chance"; DE-internal term, never shown in-game). They are
  **weapon-data attributes declared per attack part** (e.g. Astilla:
  direct hit forces Impact; the radial part does not). Special cases can
  even **bypass stack caps**: Evensong applies 7 Weakened procs on hit
  past the normal 5-stack limit.
- Per-hit proc set = `forced_list + N typed draws`,
  `N = floor(SC) + Bernoulli(frac(SC))`. Average procs per trigger pull
  = `Multishot × (forced_count + SC)` (official formula).
  Example (Astilla-like, forced Impact on direct, impact share `w`,
  `SC ≤ 1`): P(1 Stagger stack) = `1 − SC·w`, P(2 stacks) = `SC·w`,
  P(1 stack + another proc type) = `SC·(1−w)`; the explosion instance
  rolls independently with its own vector.
- **Status Vulnerability** (the proc of Void damage): +10% received status
  chance per stack (max +100% at 10) — a DebuffBar entry that feeds back
  into attackers' `SC`.
- **Negative status duration** (Riven, past −100%): all duration/DoT procs
  are nullified; instant procs still occur.
- **DoT** effects are DebuffBar entries with a `dot` sub-block whose stacks
  tick on the timeline (feeds §9). See the dedicated subsection below.

**Damage over Time** (wiki `Damage_over_Time`). The five status DoTs:
| type | proc | duration | delay | ticks at | refreshable |
|---|---|---|---|---|---|
| Slash | Bleed | 6 s | 1 s | 1..6 s (6 ticks) | no |
| Heat | Ignite | 6 s | 1 s | 1..6 s | **yes** (only one) |
| Toxin | Poison | 6 s | 1 s | 1..6 s | no |
| Electricity | Tesla Chain | 6 s | none | 0..5 s (last tick no damage) | no |
| Gas | Gas Cloud | 6 s | none | 0..5 s (last tick no damage) | no |
- `total_ticks = floor(tick_rate × (duration − delay)) + 1`; status DoTs
  tick at 1/s (ability DoTs often 2/s).
- **Snapshot scaling** — a tick inherits from its proccing hit: total
  damage buffs and base-damage mods, **faction bonuses applied a second
  time** (effective `(1+f)²`), status-damage bonuses, the hit's crit
  multiplier, body-part multiplier, stealth bonus, combo counter.
  NOT inherited: Sonar-style weakspot multipliers, physical-type mods.
- **Elemental DoT buffing**: elemental mods buff their own element's DoT
  (Hellfire → Ignite ticks); **combined-element DoTs (Gas, Blast) are NOT
  buffed by component mods** — only by literal matching-element damage;
  conversely Toxin mods DO buff a forced Toxin DoT even when combined
  into Corrosive on the panel.
- **Snapshot vs live — the boundary rule**: a DoT stack is a *replay of
  its proccing hit*. **Attacker-side state is snapshotted** at proc time
  (mods, buffs, crit tier, body part, combo, faction, status damage —
  frozen even if the buff later expires). **Defender-side state is
  evaluated live at each tick** (current armor — hence strips grow Heat
  ticks while Bleed ignores armor entirely; current pool the tick lands
  in; current damage-taken debuffs like Viral stacks; DR auras active at
  tick time). Implementation: the proc stores a frozen attacker
  contribution template in the DebuffBar; each tick runs that template
  through the defender's *current* mitigation pipeline. Open question:
  the wiki lists enemy debuffs (Molecular Prime) among snapshot-inherited
  factors — whether they are also (or instead) applied live per tick
  needs measurement.
- **Duration mods** (Continuous Misery, Lasting Sting, ... and negative:
  Rapid Resilience) affect **status-effect DoTs only**. Sickening Pulse
  duplicates active stacks with fresh timers.
- **DoT Detonation** (Expedite Suffering, Tragedy, Divine Retribution,
  Harmony heavy attack): ends stacks early, dealing all remaining ticks
  in one instance.
- Bleed specifics (wiki `Damage/Slash_Damage`): tick =
  `0.35 × [base × (1+base_dmg)(1+faction)] × (1+faction) × (1+status_dmg)
  × crit_mult × part_mult`, as **Cinematic** damage → ignores armor
  entirely (armor strips don't change ticks). Enemy-inflicted Bleeds use
  **10%**, not 35%. Some melee types force Bleed on Heavy Attacks.

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

**Damage instance classes.** Every damage instance carries a source class —
**direct** (projectile/hitscan contact), **aoe_radial** (the explosion), or
**ability** — because several rules key off it:
| rule | direct | aoe_radial | ability |
|---|---|---|---|
| body-part multiplier | yes | always 1x | n/a |
| triggers headshot conditions | yes | never | never |
| Weakened's crit-received bonus | yes | **no** | **no** |
| enemy shield gate | 5% (weakspot bypass) | some instances fully blocked | rider instances pass |

**Area of Effect** (wiki `Area_of_Effect`). One trigger pull on an AoE
weapon = a **direct** instance (the projectile, full body-part rules) plus
an **aoe_radial** instance (the explosion):
- The explosion **bypasses Line of Sight** — hits through cover and walls.
- **Linear damage falloff** from epicenter to sphere edge (per-weapon
  floor; exact falloff numbers live on `Damage_Falloff` — not yet
  transcribed).
- Zone shapes: sphere / cylinder / cone (engine: circle intersection on
  the 2D plane, `world::Circle::intersects`).
- **Each enemy caught rolls its own status** (and its own proc type).
- Explosions **self-stagger** the user (closer = harder knockback).
- Radius mods: Firestorm / Fulmination (+ primed variants) increase;
  Static Alacrity / Primary Compression decrease. Blast's Detonate
  mini-explosion radius is unaffected by them.

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

**Two independent faction systems — different keys** (wiki `Faction Damage
Bonus` + enemy-module schema):
- **System A — faction damage mods** (Bane/Cleanse/Expel/Smite, x1.30 /
  x1.55 Primed): keyed by the enemy's **`Faction`**. Total-damage
  multiplier, applied a **second time to DoT ticks** ("double dipping").
  Mods exist only for Grineer/Corpus/Infested/Orokin/Murmur (+ Sentient
  melee); strict matching — Grineer mods do NOT hit Corrupted or Narmer
  counterparts, Infested mods do NOT hit Techrot.
- **System B — the vulnerability column above** (x1.5/x0.5 per damage
  type): keyed by **`FactionDamageOverride ?? Faction`**. The override
  only redirects this column (schema: "faction resistance value").
- They **stack multiplicatively** when both apply (Lancer: Bane of
  Grineer x1.55 × Impact x1.5). They can also point at *different*
  factions on one enemy (a Corrupted unit with override "Corpus" takes
  Bane of Orokin but the Corpus column). **Thrax**: Faction "Unknown" →
  no faction mod ever applies; override "Zariman" → Void x1.5 column.
```
per-component = damage × bane_mult(faction match; ×2 dip on DoT ticks)
                        × column(override ?? faction, type) × pool math
```

**Independence of the type-modifier zone** (wiki `Damage_Type_Modifier`):
it is a clean per-component multiplier — no shared additive bucket, no
dilution in the bucket sense. Multiple sources of type modifiers stack
**multiplicatively** with each other; the zone is **independent** of
Damage Reduction and Damage Vulnerability systems; external buffs/debuffs
can push a modifier down to the floor of **−100% (0x)**. The only
"dilution" is compositional: ×1.5 applies to that type's *share* of the
vector (Impact at 20% of panel → ×1.5 on it = ×1.1 total).

**Toxin shield-bypass exceptions**: some enemies (e.g. **Treasurer**,
**Hounds**) cannot have their shields bypassed by Toxin at all.

**Armor → damage reduction (post-U36 formula — wiki `Damage/Calculation`
§Armored Enemies).**
```
DR = 0.9 × √(armor / 2700)
damage_to_health = incoming × (1 + type_modifier) × (1 − DR)
```
`armor` is the value **after** all strips/debuffs (Corrosive −26%/stack to
−80%, Heat −50%, Corrosive Projection, Terrify). The 2,700 cap is enforced
on the armor **value** by the stat system (data-side discipline: nothing in
the formula forbids a 10k-armor enemy — DE just never writes one, and the
scaling curve tops out at 2,700, where the formula evaluates to 90% DR).
Spawn minimum 200 (initial value only). ⚠️ The often-quoted
`armor/(armor+300)` is the **pre-U36** curve — both agree exactly at the
2,700 cap (90%), which hides the difference; at 300 armor the old curve
gives 50% DR, the new one **30%** (the U36 goal: make partial strip
worthwhile). Shields are never mitigated by armor.

**Shields vs health** (wiki `Shield`). Resolution is **per damage-type
component** of the hit vector:
- Every component except Toxin damages shields first. **Toxin (and its DoT)
  completely ignores shields** and hits health directly — but still passes
  armor DR (Toxin bypasses shields, not armor).
- **Enemy shield gate** — a **time window**, not a per-hit rule: when shields
  fully deplete, a **0.1 s** gate opens during which damage dealt to the
  enemy only applies **5%** to health (the breaking hit's spill *and* any
  further hits landing inside the window — fast fire rates and multishot
  pellets get eaten by it; a 1 shot/s weapon never notices). Exceptions:
  - hits on **weakspots bypass the gate entirely** (full damage in-window);
  - some AoE instances (e.g. slam attacks) get **no** 5% leak — the damage
    instance is **fully blocked** in-window;
  - **separate damage instances riding on an attack** (status-effect DoTs,
    Xata's Whisper) are not stopped by the gate.
  **Model decision (2026-07-24, revised same day):** the gate is understood
  as the enemy analogue of the player's shield-gate invulnerability — a
  0.1 s protection window on the *unit*. Inside it, **all direct hit damage
  is reduced to 5%, including Toxin's shield-bypassing damage** (the window
  protects the unit, not the shield pipeline). The only pass-throughs are
  the wiki-documented exceptions above (weakspot 100%, some AoE 0%, rider
  instances unaffected). Unverified — `MEASUREMENTS.md` **M1** decides it;
  an instant kill there would falsify this and revert to Toxin-ungated.

  **Evaluation semantics** (implementation contract): the gate is **unit
  state** — it opens the instant shields hit zero, no matter what (or
  where) the breaking hit was, and closes 0.1 s later. The **bypass is a
  per-hit property** of where each hit lands. Consequences:
  - any-break → weakspot hit in-window = **100%** (plus location mult);
  - weakspot-break (its own spill is full) → body hit in-window = **5%**;
  - within one window hits can alternate `body 5% / head 100% / body 5%`.
  Headshot play never feels the gate; body-aimed rapid fire eats it on
  every shield break.
- Magnetic status: +100% damage to shields/Overguard on the first stack,
  +25%/stack after (max +325%), and blocks natural shield regen; on
  break, Electricity burst = 3%/stack of max shields (max 30%).
- Shields recharge after a delay when not hit; status DoTs do not reset the
  delay timer.
- Faction vulnerability/resistance (×1.5/×0.5) applies **per component at
  all times** — the same multiplier whether the component lands on shields
  or health (post-U36).

**Damage Reduction framework** (wiki `Damage_Reduction`). All DR sources
stack **multiplicatively**; the full per-component chain on either side:
```
received = dealt × Π(1 − DRᵢ) × armor_factor × Π(1 + type_modifierᵢ)
```
- **Two armor formulas coexist**: players/Warframes use
  `net_armor/(net_armor+300)` (300 armor = 50% DR, still current);
  enemies post-U36 use `0.9·√(armor/2700)` (§ above). ⚠️ The DR page's
  enemy example still shows the old 300-curve with 100 armor (below the
  200 spawn floor) — stale pre-U36 content; `Damage/Calculation` wins
  for enemies.
- **Armor reduces health damage only** (never shields). Pure DR
  (ability-granted) reduces both. **DR of any kind does NOT apply to
  Overguard, Object health, or absorb effects** (Iron Skin, Snow Globe —
  though some absorbs scale their pool with armor).
- Type-modifier pool scoping: modifiers from mods/effects apply to both
  health and shields; modifiers innate to a pool apply to that pool only;
  modifiers on armor apply to health but not shields.
- Quick Thinking "energy as health": `DR = 1 − 100/net_efficiency`,
  efficiency sources additive; multiplicative with everything else.
- **Damage Attenuation** (bosses): DPS-adaptive reduction on enemy
  health, multiplicative with other types — recorded-only for now, a
  major future transcription target (per-boss formulas).

**Level scaling** (wiki `Enemy_Level_Scaling`; community-derived, DE has not
confirmed — treat as unverified). Common structure, with `Δ = current level −
base level` and per-stat/per-faction coefficient & exponent:
```
current = base × [f1(Δ)·(1−S(Δ)) + f2(Δ)·S(Δ)]
f1/f2 = 1 + c·Δ^e   (low-level / high-level curves)
S = smoothstep between the transition bounds:
    S(Δ) = 3T² − 2T³,  T = (Δ − lo) / (hi − lo), clamped to [0,1]
```
- Resolved wiki self-contradiction (M4, 2026-07-24): the Murmur tab's
  text also listed Anarchs, but the Commandeered Ash Prime @L1000 stat
  block matches the **Corrupted curves to the cent** — Anarchs = Corrupted
  (health `2.1/0.685`, shields `2.0/0.75`); the tab text is a typo.
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
- **Endless-mission spawn-level progression** (Survival/Defense ramp,
  Disruption `L(x) = 2.59·e^{0.139·round}`) is scenario-side spawn logic,
  recorded-only for now (wiki §Level Scaling During Endless Gameplay).

**Parazon Mercy windows** (wiki `Parazon` §Mercy — the authoritative,
complete rule set; the Impact page's list is a subset).
- Mercy prompts appear only on **mercy-eligible units** (a per-unit flag in
  enemy data; full list on the Parazon page: humanoid Eximus + Rogue
  Arcocanid/Culverin Eximus, Heavy Gunners, Bombards, Bailiffs, Nox,
  Scrambus/Comba, Nullifiers, Derivator, Amalgam Heqet/Machinist/MOA,
  Crawlers, all four Ancients incl. Protector, Mutalist MOAs, Deimos
  Carnis, and the 1999 heavies: Hollow Vein, Severed Warden, Anatomizer,
  Unseeing Herald, Fragments, Scaldra Dedicant/Eradicator, Techrot
  Obsolyte, Anarch Libritor).
- **Hard gates (before any window math)** — measured/wiki-confirmed
  2026-07-24: **shields must be fully depleted** (user test: 1 HP behind
  10k shields shows no prompt — the Corpus "shields removed" wording is a
  gate, not a bonus condition) and **Overguard must be gone** (wiki
  Overguard patch history: "you cannot Mercy Kill enemies with Overguard
  active").
- **Base window**: 40% of total health; **60%** on Corpus (shields are
  necessarily at zero past the gate); **80%** on Eximus.
- **Impact (Stagger) stacks**: +8% per proc, cap **80%** (**100%** on
  Corpus and Eximus; measured 2026-07-24 — the cap is reached, and the
  shields question is settled by the gate above).
- **Level decay**: above level **150** the window shrinks **1% per 5
  levels**, floor **10%**.
- The Mercy kill itself deals armor- and shield-bypassing damage equal to
  total hitpoints; the player (+companion) is invulnerable during the
  animation; the prompt expires after 10 s. Secret/Requiem Mercy
  (Larvlings, Thralls, Hounds, Liches, Sisters) is a separate flow.

**Level cap.** Enemy levels cap at **9999**; only Void Fissure missions exceed
it. Implemented in `engine::scaling` with regression tests at the cap.

**Eximus** (wiki `Eximus`, `Eximus/Compatibilities`). Eximus are empowered
variants of normal units: Overguard (base **12**, scaled by the overguard
curve), a **replaced base health** (the piecewise formulas above: factor 0.25
with shields/armor, 0.375 without, ×g(level), floored at 1.1× base), +1,000
base affinity (affinity multiplier ×3), and a type-specific aura/ability
(Arson, Arctic, Shock, ... — not yet modeled). **Eligibility is per-unit**:
the compatibility table covers Grineer / Corpus / Infested / Corrupted /
Sentient / Murmur units only, and even there not every unit × type combination
exists. **Zariman Thrax units have no Eximus variant** (their overguard is
innate). Engine rule: enemy data carries `can_be_eximus`; building an Eximus
target from a unit without one is an **error**, never a silent acceptance.

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
