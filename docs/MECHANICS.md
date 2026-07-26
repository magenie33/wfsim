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

**Mod capacity & polarity** (wiki `Polarity`; `engine::mods`). Capacity =
weapon rank (max 30), doubled by an Orokin Catalyst (60). Slot drain:
**matching polarity −50% rounded UP** (11 → 6); **mismatched +25% rounded
half-UP** (11 → 14; measured 2026-07-26: 10 → 12.5 → 13); unpolarized = full drain. Aura/Stance slots
instead scale the capacity BONUS they grant (×2 matched, −20%
mismatched) — melee-relevant later. Forma adds/changes one slot polarity
at rank 30 (resets rank). An over-capacity loadout is an impossible
combination → hard error. The weapon data's `polarities` /
`exilus_polarity` fields (from the wiki data module) are the source of a
weapon's innate layout.

**The condition_overload (CO) bucket** (wiki
`Condition_Overload_(Mechanic)` — community-documented, bug-riddled but
stable for years). Bonuses of the form "+X% per Status Type on the
target": the melee CO mod, Galvanized Aptitude/Shot/Savvy, Secondary
Shiver, Cedo, and the Incarnon "Carnage Reign / Fatal Affliction"
family (DT: +33%).
- **Counting**: distinct status TYPES present (not stacks) — every
  damage-type proc counts (incl. Void/Tau) plus the hidden Lifted /
  Knockdown (mutually exclusive) / Microwave.
- **All CO sources stack additively with each other** in one bracket.
- **Direct damage only**: hitscan, projectiles, beams, direct hits of
  explosions, and **hitscan ricochets** benefit; radial/AoE components
  and non-directly-hit targets do not (instance classes again).
- **Two stacking behaviors vs other damage bonuses** (per attack,
  catalogued): "Multiplying" (rare, mostly projectiles — a true separate
  multiplier) vs "Adding" (common): `final = base × [(dmg bonuses ×
  other mults) + CO]` — the CO chunk additively joins the base-damage
  bracket and **ignores** final multipliers (falloff, Eclipse-type) and
  **Incarnon Evolution base-damage increases**.
- Worked entry for Dual Toxocyst Incarnon (official catalog): behavior
  Adding, CO base = 75 even with Carnage Reign's 135 panel — the perk's
  own +60 is excluded from its own CO term (effective 56% per type
  relative to the boosted panel).

**The Incarnon base-stat layer.** Evolution effects phrased "Increase Base
X" modify the weapon's BASE stat before all mods: `flat_base_damage` adds
to the base total **distributed pro-rata across the damage vector**
(Dual Toxocyst 75 → 135 keeps the 10/80/10 split — equivalent to scaling
the vector ×1.8), and therefore flows into DoT ModdedBase;
`flat_base_crit_chance` adds percentage points to base CC before
crit-chance mods (same layer as Critical Parallel's flat CD).

**Mod order matters.** Mods are an **ordered list**, not a set — elemental
combination (§3) depends on the order. An Effect can inject a modifier at a
defined position (Frenzy appends "+100% Toxin" at the **end** of the order).

**Ammo efficiency.** `shots_per_ammo = 1 / (1 - e)`; sources add (except
Energized Munitions, multiplicative); `e = 1.0` → infinite ammo.

**Mod restrictions & conditional-buff activation** (`loadout::resolve`, 2026-07-27).
- **`requires: <trait>`** (ModDef): a mod whose required weapon trait
  (`WeaponBase.traits`, e.g. `semi_auto`/`beam`) is absent is INERT — all its
  effects/locks are skipped. Calc-layer, NOT an equip gate. Declared only when a
  general effect would otherwise be misapplied (Semi-Pistol Cannonade needs
  `semi_auto`); self-gating effects (beam range) declare nothing.
- **`disables: [stat]`**: a mod that LOCKS a stat from modding (Pistol Acuity →
  multishot, Semi-Pistol Cannonade → fire_rate) zeroes that stat's mod bucket
  (and any conditional stacks feeding it); the weapon's base value stays.
- **Conditional buffs** (`ModEffect::CondBuff`): triggered-buff mods whose
  trigger isn't event-modeled (on_ability_cast / on_reload / on_hit / …)
  contribute their assumed-max total (per_stack × max_stacks) ONLY under
  `StackPolicy::AssumedMax` (the panel/optimizer's optimistic 100%-uptime view);
  the emergent sim leaves them to the timeline. The `configured` policy (per-buff
  stacks/uptime — a future "test build") will sit between the two (BUFFS.md).

**Physical (IPS) damage mods vs elemental mods — DIFFERENT math** (wiki
`Damage/Calculation`; engine `loadout::resolve`). A physical mod (+X%
Impact/Puncture/Slash, e.g. Rupture) scales the BASE of THAT physical type and
is a SEPARATE multiplier, multiplicative with base damage, and NEVER enters the
elemental hierarchy:
```
type_final = base_of_type × (1 + Σ physical_mods_for_type) × (1 + Σ base_damage)
```
An elemental mod instead adds `modified_base × Σ` of that element and combines
via the hierarchy (§3). Two consequences: a physical mod does NOTHING to a type
the weapon lacks (base 0 → 0), and it never forms a combined element. Worked
(30 base Impact, +120% Impact, +90% Serration): `30 × 2.2 × 1.9 = 125.4`
(before per-type quantization, which rounds the `30 × 2.2` step). Modeled as
`ModEffect::Physical(type, v)` — data kind `physical_damage_bonus` (the loader
also routes an `elemental_damage_bonus` with an IPS element here).

**Utility / indirect mod buckets** (pistol-pool import 2026-07-26,
`data/mods/pistol/`). The declarative pool records these kinds; each carries
the REAL mechanic even where the engine does not consume it yet (the loader
ignores unknown `kind`s, so the mod still loads). Wiki-sourced calc:
- **Faction damage** (`faction_damage_bonus`; Expel/Bane/Cleanse/Smite —
  wiki `Faction_Damage_Bonus`): regular **+30% at max** (×1.05/rank →
  ×1.30), **Primed +55%** (×1.55), same across weapon classes. It is its
  own MULTIPLICATIVE bucket, but **additive with every other faction-bonus
  source** (Bane and Roar share one bracket): `… × (1 + Σ faction_bonuses)`.
  Worked (wiki): `100 × (1 + 1.65 Serration) × (1 + 0.30 Bane + 0.50 Roar)
  = 477`. Keyed by the enemy's **Faction**, strict match (Grineer mods do
  NOT hit Corrupted/Narmer Lancer), and **double-dips on DoT ticks** (the
  bonus is applied twice) — full treatment in §8 System A. Factions with
  mods: Grineer/Corpus/Infested/Orokin/Murmur (+ Sentient melee). *Engine:
  not modeled yet* — the 10 imported Expel mods carry it as data.
- **Status duration** (`status_duration_bonus`): scales status-effect DoT
  **duration only** (→ more ticks, ~linear DoT total) and **slows Heat's
  armor-strip ramp** (+100% dur → 1 s steps); no effect on instant procs
  (§6). *Engine: not modeled yet.*
- **Punch through** (`punch_through_bonus`, **meters** — wiki `Punch_Through`):
  pierces enemies/geometry up to the meter budget; each pierced target
  subtracts remaining potential; **every pierced target takes FULL damage**
  (no per-hit loss). Hitscan pierces instantly; single-target sim = no-op
  (§7). *Engine: not modeled yet.*
- **Magazine capacity** (`magazine_capacity_bonus`): +% of the BASE magazine
  (additive bucket, floored to a whole round); feeds reload cadence / sustain
  (§9), not per-hit damage. *Engine: not modeled yet.*
- **Zoom** (`zoom_bonus`): pistol zoom is pure FOV — **no damage** (unlike
  sniper zoom's additive headshot-damage bonus). Correctly a no-op.
- **Ammo mutation / conversion** (recorded `kind: unmodeled`): ammo-economy
  only, no damage.
- **Accuracy / recoil / on-equip handling**: aim inputs for the future
  shooter model (recoil is already an `Indirect` bucket, §mods_data); no
  theoretical-DPS effect (Magnum Force −55% accuracy downside, Reflex Draw
  on-equip, …).

**Source:** wiki + measured. **Status:** unverified. **High-risk** (CORE.md §3).

---

## 3. Elemental combination (pipeline layer [2])

**Definition.** Primary elements combine into secondary elements based on the
**order mods appear in the configuration**, not a fixed priority.

**The hierarchy algorithm** (wiki `Damage` §Modding / §Load Order — the
authoritative rules; supersedes the earlier draft):
1. **Hierarchy = mod layout order**, top-left slot first → bottom-right
   last. Adjacent-in-hierarchy uncombined primaries merge pairwise into
   secondaries.
2. **Innate weapon elements come LAST** in the hierarchy — NOT first (the
   old draft had this backwards). Exception: Kuva/Tenet weapons with two
   innate elements (weapon + progenitor): whichever comes first in
   **HCET order (Heat > Cold > Electricity > Toxin)** sits second-to-last,
   the other last.
3. **An innate element is pulled FORWARD** if any equipped mod shares its
   element: it adopts that mod's position (e.g. Stormbringer in slot 1
   moves Amprex's innate Electricity to first).
4. **Multiple mods of one element**: the FIRST placement establishes the
   element's position; later same-element mods just add damage there.
5. The innate element joins a combination established earlier in the
   hierarchy, or combines with the last uncombined mod element.
6. **Rivens with two elements**: the LAST-listed stat gets hierarchy
   priority (combines with mods higher up); the first-listed combines
   lower; with no other elemental mods the two combine with each other.
7. **Innate secondary elements** (Ogris Blast, Nukor Radiation, ...) are
   permanent and never combine; mod primaries combine independently
   alongside; a Kuva/Tenet progenitor element does NOT fold into an
   innate secondary. Likewise combined-element MODS (Magnetic Might
   family) add their secondary directly, outside the primary hierarchy.
8. Elements **injected by a buff** enter at their defined position —
   Frenzy's "+100% Toxin" appends at the END of the mod order, additive
   with Toxin mods, joining an existing Toxin-bearing combination if one
   formed (wiki, Dual Toxocyst).

Worked example (Load Order): Prova/Lecta (innate Electricity) + Cold(1)
Toxin(2) Heat(3) → Cold+Toxin = **Viral**, then Heat pairs with the
innate-last Electricity = **Radiation**.

**Source:** wiki (Damage §Modding). **Status:** unverified. **High-risk** —
order dependence remains a top calibration target (CORE.md §3).

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
Crit chance is **location-independent** — headshots do not raise the roll
(ability exceptions like Covenant's ×4-on-headshot aside); the head only
multiplies the outcome (location ×3, and cd doubled in the tier formula
on a crit), so crits on the head carry a larger premium at unchanged rate.

**The CD bucket has three layers** (insertion points differ — official
wording never distinguishes them):
```
cd_total = (base_cd + Σ weapon_flat_cd)           ← Critical Parallel-type
           × (1 + Σ relative_cd_mods)             ← Vital Sense-type
           + Σ receiver_flat_cd                   ← Cold stacks / Frozen, from
                                                     the target's DebuffBar, last
```
(Evidence: §Quantization ordering for the first two layers' relative
positions; the Cold page's Kunai example `1.6 × 2.1 + 0.5` for the
receiver layer.) The first layer is quantized —
`quantize(base_cd + Σ weapon_flat_cd)`, see §Quantization below. The tier
and headshot formulas below consume `cd_total`.

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

> **Decision (2026-07-24, final): quantization is ALIVE and implemented.**
> Reasoning: (a) `Damage/Calculation` §Quantization gives the engineering
> rationale — it is a **network serialization scheme** (one total integer
> + per-type 1/32 multiples), which does not obsolete with better tech;
> (b) the U40 change (1/16 → 1/32) is a *refinement* of the mechanism,
> not a removal; (c) the effect is material (the 30/30/40 example deals
> 103.125 off a 100 panel — +3.1%, far beyond our matching tolerance).
> Implemented: `DamageVector::quantized()` (per-hit vector, BEFORE
> crits/type-modifiers/faction multipliers — those multiply quantized
> values) and `damage::quantize_base_crit_damage` (wired into the CD
> bucket when mod resolution lands). The page's flagged "conflicting
> info" is a mathematical pseudo-conflict: for pure multipliers,
> `Round(v/s)·s·k ≡ Round(kv/ks)·ks` — the two descriptions differ only
> when elemental mods change the vector's composition.

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
- **Stack pools are SHARED per target** (high-confidence model,
  2026-07-24): the pool and its cap live on the target — 10 players'
  Magnetic procs compete for the same 10 slots, FIFO across all sources;
  effect magnitudes (armor strip %, shields amp) read the target's total
  count while payloads read each slot's own provenance. Evidence: the
  strip formulas use a single stack count; Hydroid's passive applies to
  Corrosive "from any source"; the UI shows one counter. Corollary: fast
  low-quality procs from teammates flush high-quality stacks out (FIFO
  is owner-blind). No explicit wiki sentence — verifiable in co-op.
- **Stack overflow is universally replace-oldest, FIFO by application
  time** (user rule, 2026-07-24): every capped stacking debuff (Stagger 5,
  Weakened 5, Corrosion 10, Confusion 10, Gas Cloud 10) replaces the
  stack with the EARLIEST application timestamp — remaining duration is
  irrelevant (a stack applied at t=1 with 10,000 s left is replaced
  before a t=2 stack with 1 s left). The Weakened page states this
  explicitly ("even if the oldest stack has a longer remaining
  duration"); generalized to all. Uncapped debuffs (Bleed, Poison, Tesla
  Chain) never overflow; Freeze's and Detonate's caps trigger state
  transitions instead (Frozen / detonation).
- **Status damage never procs status** (universal rule, user-confirmed
  2026-07-24 by contradiction: Heat ticks proccing Heat would self-stack
  forever). No damage instance originating from a status effect — DoT
  ticks, Detonate bursts, Tesla Chain hits, Gas Clouds — ever rolls a
  status proc. Proc rolls happen only on weapon/ability source instances.
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
- **Status immunity renormalizes the type draw** (wiki `Status_Effect`
  §Status Immunity Interactions): types the target is status-immune to
  are EXCLUDED from the draw and the remaining weights renormalize (the
  roll is never wasted). Independent of damage-type immunity.
  Implemented in `status::draw_proc_type`.
- **Independent procs** exist outside the damage-type system (Knockdown,
  Lifted, Ragdoll, Stun, Sleep, Silence, Slow, Disarmed, Big Stagger,
  Microwave — see `data/debuffs/independent_procs.yaml`); Knockdown /
  Lifted / Microwave count toward Condition Overload's status-type count.
  ⚠️ Generic Stagger (PT_STAGGERED) ≠ Impact's Stagger (PT_KNOCKBACK).
- **Negative-duration per-type detail** (outdated-flagged wiki table):
  no-delay DoTs still land their t=0 tick (Tesla Chain "occurs"), Heat's
  panic animation plays flameless, Blast expiry damage occurs with the
  explosion only on a killing trigger — consistent with duration→0
  semantics rather than blanket nullification.
- **Status-damage bucket, official semantics**: additive within the
  bucket (Emerald shard + Elementalist add), multiplicative against
  other buckets; **type-scoped members exist** (Ash passive = Slash
  status only, Emerald shard = Toxin only, Conductive Sphere =
  Electricity only) — the bucket carries an optional damage-type scope.
- **Continuous weapons** proc as if their multishot pellets were real
  (merged beam visuals notwithstanding) — multishot boosts status
  opportunities normally.
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
- **Two DoT models**: every DoT except Heat is `independent_stacks` (each
  proc = its own instance with its own clock). **Heat is a
  `singleton_accumulator`**: ONE DoT entity per target — each proc adds
  its contribution into the single tick value and refreshes the one
  shared clock ("Heat Inherit"); the entity's Heat%/faction modifier
  context is fixed by the **first** proc (status-damage mods excepted),
  enabling indefinite linear ramp while refreshed. **Measured 2026-07-24
  (user): the context sync is bidirectional** — a strong first proc
  elevates later unmodded contributions just as a weak one drags modded
  ones down.
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
- **Snapshot vs live — the boundary rule** (refined 2026-07-24): a DoT
  stack is a *replay of its proccing hit* — a tick is that hit's deferred
  damage. The dividing line is **not** attacker-vs-defender state but:
  **whatever fed the HIT's damage formula is snapshotted** (mods, buffs,
  crit tier — including receiver-side inputs the formula read, like the
  Cold cd bonus at hit time), while **whatever belongs to the tick-time
  MITIGATION pipeline is evaluated live** (current armor — hence strips
  grow Heat ticks while Bleed ignores armor entirely; current pool the
  tick lands in; damage-taken multipliers like Viral stacks; DR auras
  active at tick time). Corollary: Cold rides into tick snapshots, and
  Cold applied after the proc does not change existing ticks.
  **Officially confirmed for Viral** (the Viral page's worked example):
  DoT damage is computed by whether Viral is active *when the tick
  deals damage* — 35/s bleed doubles to 70/s while Viral is up and
  drops back when it expires; explicitly NOT double-dipped ("unlike
  faction damage multipliers"). Implementation: the proc stores a frozen attacker
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

**Multishot** (wiki `Multishot`). `total_projectiles = base_count ×
(1 + Σ multishot bonuses)`; the integer part is guaranteed, the fraction
is a chance of one more, rolled per trigger pull. Each projectile is an
independent damage instance (own crit roll, own status roll). No effect
on speargun throws or on continuous weapons' blast radii; the Arsenal
shows the summed damage (spread can waste pellets); accuracy interacts.

**Continuous (beam) weapons** (wiki `Continuous_Weapon` + `Multishot`
§Continuous Weapons) — three big deviations:
- **Damage ramp**: ticks start at ~20% damage and ramp to 100% over
  0.6 s of hitting a target; after 0.8 s off-target it decays back over
  2 s. Held trigger, hitscan ticks, typically 0.5 ammo/tick, no recoil,
  limited base range (Sinister Reach / Ruinous Extension).
- **Multishot MERGES on-target beams into ONE damage instance per tick**:
  merged damage AND status chance = the SUM of beams (SC 40% × roll 3 =
  120% → multiple procs per tick), but **crit chance stays single-beam**
  (one roll, unscaled). Consequences: damaging status DoTs benefit from
  multishot **twice** (proc chance × merged ModdedBase); **forced procs
  apply once per tick AFTER the merge** (Hunter Munitions on beams ≈ one
  proc per interval, not per pellet). Innate multi-beam weapons (Quanta)
  keep one instance per base beam, each rolling its multishot bonus
  independently.
- **Beam chaining** (Amprex/Kuva Nukor family): chains hit secondary
  targets at decreasing damage; Firestorm-type mods extend the LINK
  range (Beam Length bonuses do not); punch-through redistributes chains
  without adding damage; chain beams consume no ammo.

**Ricochet** (wiki `Ricochet` — hitscan only; projectiles use the separate
Bounce mechanic). A hit on an ENEMY instantly redirects to other enemies
within the weapon's ricochet range. Triggers **per hit, not per shot**:
each multishot pellet ricochets once, and each punch-through victim
spawns its own ricochets. Terrain hits never ricochet; **corpse hits DO**
(exception: Neutralizer). Per-weapon ranges (DT Incarnon 5 m, Lato
Incarnon 10-12 m, ...); Neutralizer's range scales with Ability Range and
prioritizes weak points. DT-specific: ricochets can headshot and trigger
Frenzy; ragdolled enemies cannot be ricochet targets.

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
Spawn minimum 200 (initial value only).

**Per-type damage floor (armor only)** (wiki `Armor`): damage reduced by
armor has a **minimum of 1 per damage type** in the hit's vector (a
3-type Braton always deals ≥3 vs any armor). Non-armor DR sources have
no such floor.

**Armor stripping — three semantic classes** (wiki `Armor` §Removing):
1. **Percentage of TOTAL armor** (Warframe abilities, Corrosive
   Projection 18%/aura additive to 72%, Sharpened Claws, Vicious Bond):
   accumulates additively against the total — a 50% ability fully strips
   in two casts.
2. **Percentage of CURRENT armor** (Heat 50% ramped, Corrosive 20%+6%/
   stack): multiplicative factors, diminishing returns.
3. **Flat off BASE armor** (Shattering Impact −6, Amalgam Argonak −6):
   permanently subtracts from the *base* value **before level scaling**
   (so each point removes `level_multiplier` points of total armor); can
   reach full strip.
```
net_armor = (base − Σ flat_base_strips, ≥0) × level_multiplier
            × (1 − Σ total_pct_strips)         [class 1, additive within]
            × Π (1 − current_pct_stripᵢ)       [class 2: heat, corrosive]
e.g.      = armor × (1 − 0.5_heat) × [1 − (0.20 + 0.06·corr_stacks)]
                  × (1 − 0.18 · corrosive_projections)
```
Heat's strip ramps 15/30/40/50% in 0.5 s steps (2 s to max; re-procs don't
hasten it) and ramps back down 1.5 s-stepwise over 6 s after expiry;
**status-duration mods slow the ramp** (+100% duration → 1 s steps).
⚠️ The often-quoted
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
