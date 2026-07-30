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

**Where a pellet lands** is rolled **per pellet**, not per trigger pull: the
sim's `headshot_pct` is a per-pellet aim weight, because aiming at the head
does not put every pellet of a spread on it (decision 2026-07-29). It follows
that the Incarnon gauge charges per headshot *pellet* (multishot fills it
faster), on-headshot buffs trigger from any one pellet of a pull, and the
reported headshot rate is pellets/pellets. Mean headshot rate is identical
under either model; the per-pellet roll gives lower variance.

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
  time** (rule adopted 2026-07-24): every capped stacking debuff (Stagger 5,
  Weakened 5, Corrosion 10, Confusion 10, Gas Cloud 10) replaces the
  stack with the EARLIEST application timestamp — remaining duration is
  irrelevant (a stack applied at t=1 with 10,000 s left is replaced
  before a t=2 stack with 1 s left). The Weakened page states this
  explicitly ("even if the oldest stack has a longer remaining
  duration"); generalized to all. Uncapped debuffs (Bleed, Poison, Tesla
  Chain) never overflow; Freeze's and Detonate's caps trigger state
  transitions instead (Frozen / detonation).
- **Status damage never procs status** (universal rule, confirmed
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
- **GunCO family = ONE machinery** (wiki `Condition_Overload_(Mechanic)`;
  2026-07-27): every source contributes `rate × target-counter` into
  ONE shared bracket — computed on the ORIGINAL base (evolution flat
  damage excluded, the `co_base_fraction`), combined per the weapon's
  CoBehavior class (additive-with-base-damage / independent / inert),
  direct hits only, and all sources ADDITIVE with each other. Sources
  differ only in their counter: Condition Overload (Galvanized Shot,
  Carnage Reign innate) counts distinct status TYPES on the target;
  Secondary Shiver counts Cold STACKS (Frozen counts as the full 10).
  `engine::dummy` folds them through one `gunco_sources` list.
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
  enabling indefinite linear ramp while refreshed. **Measured 2026-07-24:
  the context sync is bidirectional** — a strong first proc
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

### Condition Overload — the GunCO family, in full

Source: wiki `Condition_Overload_(Mechanic)` (transcribed 2026-07-29). This is
the authority for the bracket the engine implements as `CoBehavior` +
`gunco_sources`; the summary bullet above is the short form.

**The counter.** Distinct status **TYPES** on the target, each counted once no
matter how many stacks: the three physical procs, the four primary elements,
every combined element (Blast, Corrosive, Gas, Magnetic, Radiation, Viral,
Void, Tau) and the "hidden" independent procs that carry a counter
(Lifted, Knockdown, Microwave). Whether another player's statuses feed *your*
counter is **not stated** by the wiki.

**The two stacking classes** (the wiki's five ranked behaviours collapse to
these, plus a null case):

| class | formula | who |
|---|---|---|
| **Multiplying** | `base × most damage bonuses × CO × other multipliers` | the rarer case, mostly projectile weapons |
| **Adding** | `base × [(most damage bonuses × other multipliers) + CO]` | the common case — hitscan and many projectiles |
| **Inert** | no bonus at all | radial/AoE components |

"Adding" is *additive with every +% damage source* — Serration-likes, Vex
Armor, Arcane Fury and other CO-like bonuses all share the bracket.

**What the bonus reaches.** Hitscan, direct projectile hits, hitscan ricochet,
homing / bouncing / punch-through / wave projectiles, embedded clouds on the
directly-embedded target, beams (including AoE / chain / multi-beam), and the
Blast / Electricity / Gas **proc** damage generated on the initial target
(which then carries to that proc's radius).

**What it never reaches** — this is the rule §7's radial part obeys: projectile
explosion radii (Ogris), hitscan explosion radii on non-directly-hit targets,
embedded-cloud radii on non-directly-hit targets, and pure radial weapons
(Balefire Charger, Stug, Sonicor, Azima turret). Weapon-specific exclusions
exist too (Proboscis Cernos tendrils, Vadarya Prime lightning).

**Multipliers the additive recalculation OMITS** (they are outside the
bracket): Extinguished Dragon Key, range-based damage falloff, Longbow
Sharpshot / Primary Compression, Warframe ability buffs (Furious Javelin,
Equinox Duality …), and a bow's charge multiplier — a charged shot's CO is
computed off the UNCHARGED base.

**Evolution exclusion — a per-PERK anomaly, not a law.** The line *"CO-bonus
does not use base damage increase Evolution"* reads like a general rule, and the
engine used to treat it as one: any weapon carrying a flat-damage evolution had
its CO term scaled by `co_base_fraction = original_base / evolved_base`. **That
is wrong**, and reading the catalog's columns is what settles it:

| column | Dual Toxocyst, Incarnon Mode |
| --- | --- |
| Attack Unmodded Damage | 75 **or 135 (with Evolution II Perk 1)** |
| **Actual CO Damage Bonus at +100%** | **75** |
| CO Damage Bonus Relative To Base Damage | 100% **or 56%** |
| Math/Behavior Type | Adding |
| Notes | CO-bonus does not use base damage increase Evolution |

A +100% CO adds **75**, never 135 — so the CO term is computed on the *unevolved*
base and the "56%" is just 75/135 restated. Crucially the row names **Perk 1**
(Carnage Reign), and the table's own preamble says it is *"listing only
discrepant attacks. Anything not listed should be assumed to be Additive with
+100% bonus"*. So:

- **Perk 2 (Fevered Frenzy) feeds CO in full**, even though it also raises base
  damage (+50). It is not in the table, therefore not discrepant.
- **Every Torid perk feeds CO in full** — its two rows sit at 100%, and they stay
  there with Final Fusillade or Plentiful Mayhem equipped.

The flag therefore lives on the **evolution** (`co_base_excludes_this_evolution`
on `carnage_reign.yaml`), which is the granularity the catalog names. Keying it
off the weapon would have docked Perk 2; keying it off the Adding behaviour
class would have docked the Torid's Incarnon form as well. `co_base_fraction` is
1.0 everywhere except Dual Toxocyst + Carnage Reign.

**The catalog is AUTHORITATIVE, and absence is a positive statement.** An attack
missing from the table is not an attack nobody checked — it is an attack that
behaves *normally*: Additive, +100% bonus, exactly what the mods say on the tin.
The table's job is to enumerate the exceptions, so "not listed" carries as much
information as a row does.

**And the exceptions are individual quirks, not a law.** The tell is that
**Lato Vandal has a row while Lato Prime does not**, though they are the same
weapon family with the same Incarnon Genesis. Nothing mechanical distinguishes
two Latos; that asymmetry is what a per-entry slip in DE's code looks like —
careless or deliberate, but attached to one entry rather than derived from a
rule. A general mechanical law could not produce it.

That is the evidence FOR modelling this per entry rather than per weapon or per
behaviour class, and it is what makes the Dual Toxocyst reading exact rather
than cautious: **Evolution II Perk 1 ⇒ GunCO computes on the unevolved base
(56%); Perk 2 ⇒ GunCO computes on the full base (100%)**. Perk 2 is absent from
the table, and absence means normal.

**Sources and rates.** Melee Condition Overload +80%/status; Galvanized
Aptitude +40%/status ×2 stacks (rifle); **Galvanized Shot +40%/status ×3
stacks** (pistol); Galvanized Savvy +40%/status ×2 (shotgun); Secondary Shiver
+45% per Freeze stack (its counter is stacks, not types); innates such as Cedo
+60%/status and assorted Incarnon perks at +30–100%; the Shattering Frost
decree (+80% vs Frozen, up to +240%).

**Source:** wiki. **Status:** unverified (the class per weapon and the exact
bracket arithmetic need Simulacrum measurement).

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

**Aiming is a SCENARIO input, not an assumption.** A pile of mods only pay out
`while aiming` — Galvanized Crosshairs and Galvanized Scope, Hydraulic
Crosshairs, Argon Scope, Sharpened Bullets, Bladed Rounds, Pressurized
Magazine, Embedded Catalyzer and Catalyzer Link. The sim used to satisfy that
condition silently, which flatters every build carrying one: a Dual Toxocyst
with Galvanized Crosshairs measures **52.33% crit rate / 203,591 DPS** aiming
against **36.92% / 150,041 DPS** hip-firing — a quarter of the DPS handed over
for free. It is now `aiming` on the Sim and Optimizer scenario (default ON, the
old behaviour), gating `ModEffect::WhileAiming` in `loadout::resolve_with`. The
optimizer reads the same flag: scoring a build with aim assumed and replaying
it without would rank a buff the replay never grants.

Aiming does more in-game than gate buffs (zoom, spread, some weapons' fire
behaviour, movement speed) — none of that is modeled yet, and the flag makes no
claim about it.

**Multishot** (wiki `Multishot`). `total_projectiles = base_count ×
(1 + Σ multishot bonuses)`; the integer part is guaranteed, the fraction
is a chance of one more, rolled per trigger pull. Each projectile is an
independent damage instance (own crit roll, own status roll). No effect
on speargun throws or on continuous weapons' blast radii; the Arsenal
shows the summed damage (spread can waste pellets); accuracy interacts.

**The rolling unit is the DAMAGE INSTANCE, and the instance is per attack
part PER ENEMY.** Two wiki statements pin this, and together they settle
every multi-target case:

> "Each damage instance has its own chance to apply Critical Hits or
> Status Effects." — `Multishot`

> "If a single attack hits multiple enemies, each enemy gets their own
> status roll to determine if they will receive a status effect from the
> attack and which status effect they will receive." — `Status_Effect`

So a shot that reaches three enemies is not one roll shared three ways —
it is three instances, each rolling for itself. This holds for every way
an attack reaches more than one target:

| reaching multiple enemies via | instances | each rolls |
| --- | --- | --- |
| **Multishot** | one per projectile | own crit, own status |
| **Punch through** | one per pierced enemy | own crit, own status |
| **Radial (AoE)** | one per enemy in radius | own crit, own status |
| **Ricochet** | one per redirected hit | own crit, own status |
| **Direct + radial on the SAME enemy** | two (§7) | own crit, own status |

Punch through has weapon-page corroboration: Sagek Prime's on-hit effect
"is triggered separately for each bullet when using Multishot, as well as
each enemy hit with Punch Through" — per victim, not per trigger pull.

Crit granularity is stated per attack/pellet on `Critical_Hit` ("Each
attack, or each pellet in the case of most shotguns and weapons with
Multishot, rolls its own chance to critically hit") and is **not**
spelled out per enemy anywhere. The per-enemy reading follows from the
damage-instance rule above rather than from a sentence naming punch
through — flagged as such: **status per enemy is verbatim; crit per enemy
is inferred.**

The one documented EXCEPTION is beam merging, below: on-target beams
collapse into ONE instance whose status chance SUMS but whose crit
chance stays single-beam. That exception is what makes the general rule
legible — merging is called out precisely because instances are normally
independent.

**Single-target consequence.** Our arena has one enemy, so punch through,
AoE spread and ricochet all currently resolve to zero extra instances and
the rule costs nothing to honour. It is recorded now because the
multi-target model must not "optimise" it into one shared roll.

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

**Radius mods are `ModEffect::BlastRadius`, and the name is a trap.** All four
read *"Improves the Blast Radius of weapons with Radial Attacks. +X% Blast
Range"* — **Blast RANGE, not Blast damage**. A generated skeleton that reads the
word "Blast" as an element hands every AoE weapon +44% Blast DAMAGE and drags a
whole element into its damage vector, which is exactly the bug these four
carried. They scale every part that HAS a radius — the radial explosion and the
lingering field alike (*"Firestorm mods will now affect Torid gas clouds"*, and
the Torid Incarnon page says its 2.3 m beam radius takes it too). The falloff
FLOOR is untouched: *"Only mods that increase the explosion radius change how far
the falloff reaches; they do not change the floor."* No single-target damage
consequence — a stuck grenade puts the target at the epicentre either way — but
it is what the panel states, and Primary Compression reads the MODDED radius
(below).

**Projectile SPEED is a range stat, and a different one.** VERBATIM (wiki
`Range`): *"Some weapons that shoot projectiles may have a projectile lifetime
associated and not an explicit maximum range stat. For these weapons, the only
way to increase their maximum range is to use Projectile Speed bonuses instead
of range ones."* So **max range = projectile speed × projectile lifetime**,
which the page works out: *"Arca Plasmor shoots a projectile that travels at
60m/s with a 0.5s duration, meaning that its maximum range is 30m. Applying a
Fatal Acceleration, will increase the projectile's speed to 84m/s, meaning its
new maximum range will be 42m."* A projectile-speed bonus (Terminal Velocity,
Torid's Swift Deliverance evolution) is therefore a real capability change, not
cosmetics.

It does **not** touch AoE radius — the two are independent stats, which Static
Alacrity proves by carrying both at once (*"+50% Projectile Speed"* AND *"-50%
Blast Radius"*, scaling independently across ranks). What it does also scale is
travel-distance falloff: *"Mods including Rivens that have positive or negative
Projectile speeds will affect a weapon's entire Damage Falloff range
accordingly, making them more or less effective at longer ranges."*

**Unmodeled, deliberately.** The arena is single-target at a fixed engagement,
so it models neither travel time nor range, and no roster weapon publishes a
projectile lifetime — the Torid's wiki page and WFCD both stop at `shot_speed:
40` — so the resulting range is not even derivable from data. Recorded here so
the omission is a stated scope boundary rather than an oversight.

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

### Radial (AoE) attack parts

A shot can carry **more than one damage instance**: the *direct* hit and a
*radial* explosion are **separate attack parts** with their own damage vector,
crit chance/multiplier and status chance (the weapon data declares each part —
e.g. Laetum Incarnon: direct 100 Impact, radial 300 Radiation). The
directly-hit enemy takes **both**.

**Falloff from the epicenter is linear**, between a `start` and an `end`
distance, down to a floor set by `reduction` — the fraction of damage
*removed* at maximum distance:

```
radial_mult(d) = 1 − reduction × clamp((d − start) / (end − start), 0, 1)
```

`end` is the blast radius. Laetum: `start 0, end 2 m, reduction 0.2` → 300 at
the centre, 240 (80%) at 2 m and beyond. Cross-check (wiki AoE table):
Acceltra, 50% max reduction, base 44 → minimum 22. ✔

Rules the radial part follows, each differing from the direct part:

- **No body-part multipliers, and no headshot CONDITIONS.** The rule is stated
  per weapon on every AoE page — *"Explosion has a headshot multiplier of 1x
  and cannot trigger headshot conditions"* (Aeolak, Torid, Corinth, Bubonico,
  Tenet Envoy, Propa/Phahd Scaffold, …) — and generally in the AoE-rework
  patch note: *"Radial damage no longer gains extra headshot damage or
  triggers headshot conditions."* Note the *no longer*: before that pass a
  radial DID headshot, so this is a deliberate design rule, not a side effect.
  A radial instance therefore never headshots, never feeds headshot-gated
  buffs, and **never charges an Incarnon gauge that counts weakpoint hits** —
  where a blast lands on the model is irrelevant, it has no hit location.
  Cross-check from the other direction: the two AoE Incarnon weapons don't
  use weakpoint charging at all. *"Angstrum Incarnon Genesis and Torid
  Incarnon Genesis are instead charged through direct hits"* (Incarnon), and
  the Torid page spells the exclusion out: *"Direct shots charge Incarnon
  Transmutation"*, *"Torid's poison cloud does not build charges."* That is
  DE routing around a gauge an explosion can never fill.
- **It can crit** — the part carries its own crit chance/multiplier in the
  weapon data (Laetum radial: 22% / 2.2×). And crit BUFFS reach it, scaled
  against **that part's** base. The rule is the same one mods already follow —
  a relative bonus joins the crit bucket, and a bucket multiplies whichever
  base it is applied to (`r.base_crit_damage × (1 + Σ)`) — so a buff in that
  bucket cannot be direct-only. The distinction that matters is *relative vs
  absolute*, not direct vs radial:
  - **relative** (Galvanized Crosshairs/Scope, Primary Blight/Frostbite's
    stacks, Sharpened Bullets, Overcharge, Outburst) → each part multiplies
    its **own** unmodded base. Resolving one of these against the direct
    part's base and storing the absolute result is a trap: it silently
    excludes the explosion, *and* it makes the same mod behave differently
    under `AssumedMax` (where the bonus arrives inside the resolved part stat
    through the bucket) than under `Emergent`. Two policies disagreeing about
    one mod is the tell.
  - **absolute** (Cold's flat crit damage *received*, a flat crit-chance
    grant like Enervate's, the Weakened debuff) → lands identically on every
    part; nothing to rescale.

  The only crit thing a radial genuinely loses is the body-part layer: the
  crit-headshot `2×cd` fold-in needs a hit location, and an explosion has
  none.
- **Status rolls independently**, per enemy: "If one AoE hits multiple enemies,
  each enemy gets their own status roll." Forced procs are declared per part
  (Astilla: the direct hit forces Impact, the radial does not — §6).
  Independently of the DIRECT hit, too, on the very enemy that took both —
  wiki (Laetum): *"Initial hit and explosion apply status separately."* The
  explosion draws from its **own** damage vector, so a Laetum shot can proc
  Impact/Slash off the direct hit and Radiation off the blast in the same
  instant. Those radial procs then feed Condition Overload on subsequent
  direct hits, even though the radial itself gets no CO bonus.
- **No Condition Overload.** CO is direct-damage only; radial/AoE components
  and non-directly-hit targets are excluded (§2). CO also ignores falloff as a
  final multiplier. Careful: CO is the *only* thing the radial loses here —
  weapon-wide damage buckets still reach it. The arcane base-damage stacks
  (Merciless & co) share a bracket with CO in the direct-hit formula, so the
  radial takes that ratio **without** the CO term.
- **Self-stagger, never self-damage** (post-U29): "The explosion inflicts
  self-stagger to the user."
- Only mods that increase the **explosion radius** change how far the falloff
  reaches; they do not change the floor.

**Single-target consequence.** In our arena the projectile detonates on the
target, so `d = 0` and the radial lands at full value; falloff only matters
once multiple targets exist. The data still records `start/end/reduction` so
the multi-target model has it.

**Radius is worth DAMAGE, and only one thing buys it** — Primary Compression,
which shrinks the blast while aiming and pays for the lost metres:

```
radius_lost  = radius_MODDED × (1 − 0.2)          # continuous, not per whole metre
damage_bonus = damage_per_metre(rank) × radius_lost   # ×1.0/m at max rank
ammo_eff     = eff_per_metre(rank)    × radius_lost   # ×0.055/m at max rank
```

Verbatim on the continuity: *"Despite the description stating 'per meter
lost,' the bonuses smoothly scale between whole number radius values … a loss
of 6.5 meters of radius gives +650% Damage and +35.75% Ammo Efficiency."* The
formula reproduces the wiki's whole per-weapon table at max rank (0.8 × radius
× 100%): Acceltra 4.0 m → +320%, Kuva Bramma 8.3 m → +664%, Miter 0.2 m →
+16%, Vectis 0.1 m → +8%.

It reads the **modded** radius — the table's Primed Firestorm column is
exactly 1.44× its base column on every row — which is why the engine cannot
run it yet: `ResolvedRadial.radius_m` is carried through UNMODDED and there is
no blast-radius bucket. Two further inputs are **per weapon attack**, from
that table rather than from the arcane (the same shape as `CoBehavior`):
whether the bonus **Multiplies, Adds or Both** (projectile weapons multiply,
but Ambassador/Battacor/Ferrox/Opticor/Trumna and the Braton/Burston
Incarnons add), and whether it works at all — *"Does not work on Continuous
Weapons or beam attacks with an AoE component"*, plus a long tail of 0% rows.

Torid is the cautionary pair: its normal **Toxin Cloud** is 100% effective and
multiplies off a 3.0 m reference radius (+240% at max rank) while *"cloud
radius is not reduced"* — it pays nothing and collects anyway — and its
**Incarnon form AoE is a flat 0%, "Doesn't Work."** So on one weapon the same
arcane is a top-tier multiplier in the base form and literally inert in the
transformed one.

**How the sim runs it** (`engine::dummy`): each landed projectile walks a
short list of ATTACK STAGES — the direct hit, then the radial when the weapon
declares one. A stage is one damage instance: it rolls its own crit tier, its
own status draw, and reports into its own damage source. The direct stage
alone carries the body-part multiplier, the forced procs and the CO bucket. A
weapon with no radial has a one-stage list, which is why adding the stage loop
left every non-AoE golden bit-identical.

**Per-instance is the granularity for "on hit" perks too** — ✅ **measured**
(MEASUREMENTS M11). Overwhelming Attrition ("On Hit that is neither Critical
nor applies a Status Effect") is judged per damage instance, not per trigger
pull: fired into a crowd, ONE Laetum shot takes the buff from empty to its
3-stack cap, which is only possible if each instance the shot produced armed
it separately.

The single-target case is measured too, and directly: one shot at a **lone**
target grants exactly **2** stacks — the direct hit and the explosion each arm
it. Two (not the cap of 3) is the tell: the count tracks instances. So two
attack parts on the SAME enemy are two instances, which corroborates the
verbatim separate-status rule above from the perk side.

### Continuous (beam) weapons

Trigger "Held". Two rules differ from a gun, both from wiki Continuous_Weapon
and Multishot, and both are implemented (`WeaponBase::continuous`, set from the
data module's trigger).

**`fire_rate` is TICKS per second**, not shots. Torid Incarnon: 8.

**Multishot MERGES.** VERBATIM (Multishot, Continuous Weapons): *"additional
beams that hit the same target instead merge into a singular damage tick. This
combined tick has damage **and** Status Chance equal to the **sum** of the
individual beams, but the Critical Chance is still equal to that of a single
beam."* The multiplier is the ROLLED integer, which the page works out
explicitly (*"When multishot rolls a value of 2, the status chance of that
damage instance would be 2 x 40% = 80%"*). Three consequences the page names,
and all three fall out of merging rather than needing their own code:

- Damaging status effects are *"affected **twice** by multishot"* — the summed
  status chance produces more procs AND the merged instance's ModifiedBase makes
  each payload bigger.
- *"Forced status effects … are applied after the damage instances are merged"*,
  so one forced proc per tick instead of one per beam — *"the number of forced
  procs being lower than expected"*.
- Crit chance is unchanged, so a beam gains nothing from multishot on the crit
  roll — only one roll happens.

**Damage RAMP.** *"Initial damage starts at a lower percentage, and ramps up to
100% of its damage over 0.6 seconds of hitting a target. 0.8 seconds after the
weapon stops hitting a target, the damage decays back to its initial point over
2 seconds. For most weapons, this lower percentage is 20%."* Held fire advances
the ramp one tick-period at a time; a gap longer than the 0.8 s grace decays it.
The per-weapon exceptions the page lists (Convectrix 60/80%, Phage 70%, Embolist
30%) would be weapon data; nothing in the roster needs one yet.

Applied as a FINAL multiplier on the instance and NOT on ModifiedBase — a
transient scaling of output, not a weapon-stat change, so the status payloads
are left out of it. **Unsourced either way**, and unlike the merge it is a
sub-2% question on sustained fire (at 8 ticks/s the ramp costs ~2.4 ticks out of
a 170-round magazine).

**Source:** wiki Continuous_Weapon + Multishot. **Status:** merge fully sourced;
the ramp's interaction with status payloads is a modeling choice.

### Multishot perks that are not a flat bonus

Two Torid evolutions grant multishot without being a number the resolver can
fold into `panel.multishot`. Both are conditional on something only the sim
knows, so both stay separate fields the shot loop evaluates.

**Final Fusillade — `+3 Multishot on last shot in magazine`.** A FLAT add (the
evolution grants multishot outright, not a percentage of base), gated on the
pull being the magazine's last round. On the base form's 5-round magazine that
is one pull in five firing four grenades instead of one — roughly +60% average
multishot, not a rounding error.

**BASE FORM ONLY** (user, 2026-07-30): it does not fire in Incarnon Form, whose
magazine is the charge pool rather than a reloaded magazine, so there is no
"last shot in magazine" to gate on. Both forms load the *same* evolution id, so
the engine gates on the form's own charge-backed marker (`incarnon.is_some()`)
rather than on a weapon id — `engine::evolutions_data::apply` drops it there.

**Plentiful Mayhem — `Multishot consumes ammo … and increases Damage by +60%`.**
Four rules, all wiki, and the per-form split is the interesting one:

- *"Damage bonus from multishot consuming ammo is multiplicative to base damage
  bonuses like Serration"* — an INDEPENDENT final multiplier, not a member of
  the base-damage bucket. Same treatment as the beam ramp and Devouring
  Attrition: it multiplies the finished instance, so the status payloads are
  left out of it (unsourced, recorded as a choice).
- *"…only applies to projectiles **generated by** multishot"* — the weapon's own
  projectile never takes it. With no multishot source there is no generated
  projectile and the perk is worth **exactly nothing**.
- *"Affects both modes. In the case of Incarnon Form, it pools directly from its
  magazine"* — the extra projectiles cost a round each: from reserve Capacity on
  a magazine-fed form, from the charge pool on a charge-backed one, which
  SHORTENS the Incarnon window. The magazine round itself is never part of this;
  it comes from the magazine and always takes ammo efficiency, perk or no. The
  surcharge bills the RAW rolled multishot, not the 60%-scaled figure (user,
  2026-07-30) — the bonus is paid in damage, not billed again in ammo. **Ammo
  efficiency does not reach the surcharge at all** (✅ measured, user
  2026-07-30): the magazine round keeps its discount, every generated
  projectile pays full price, and even a 100% efficiency source does not make
  multishot free. The two ammo paths are genuinely separate systems.
- **The extras can STARVE** (user, 2026-07-30). Projectiles are produced in
  order, each paying its round as it goes, and one that cannot pay **is not
  fired at all** — the same rule as running dry normally. With 3 charges left
  and a 4-multishot pull: the round is spent, two extras fire, the third does
  not exist. So the perk degrades as the pool empties instead of holding a flat
  value, and a starved pull is cheaper AND weaker rather than being clamped
  while every projectile still flies. On a beam the merge means starvation shows
  up as a smaller merge multiplier, not as fewer instances. Pinned by
  `plentiful_mayhem_drops_the_pellets_the_reserve_cannot_pay_for`.
- *"In the Incarnon form, instead of increasing the damage of additional
  projectiles created by multishot, all multishot bonuses are increased by 60%"*
  — because a merged beam has no separable generated projectile to scale.

**The two branches agree in expectation**, which is the tell that this is one
perk stated twice rather than two perks. With multishot `M` and base multishot
1, in units of one un-bonused projectile:

| form | mechanism | expected damage |
| --- | --- | --- |
| base | 1 original + (M−1) generated, only the generated ×1.6 | `1 + 1.6(M−1)` |
| Incarnon | multishot bonus (M−1) scaled ×1.6; merged beam's damage ∝ multishot | `1 + 1.6(M−1)` |

Identical. Note the identity **needs base multishot = 1.0** — both Torid forms
are, but a weapon with innate multishot > 1 would split `1 + 1.6(M−1)` from
`base_ms × (1 + 1.6·bonus)`, and this code would need the distinction.

The +60% follows a generated grenade into the **cloud** it leaves (user,
2026-07-30). That is where the perk's value is on this weapon: the cloud is most
of the Torid's damage, so a version that stopped at the impact would make
Plentiful Mayhem near-worthless.

**Source:** wiki Torid Incarnon Genesis + user (2026-07-30) for the two rules
the page does not state (the base-form gate, and the cloud inheriting the
bonus). **Status:** implemented and unit-tested; the ammo-surcharge basis and
the status-payload exclusion are recorded modeling choices.

### Lingering damage FIELDS (zones)

A third kind of attack part: an area that **persists and ticks**, rather than
landing once. The Torid's grenade is the reference — it disperses a cloud that
keeps damaging whatever stands in it.

Torid's Poison Cloud, from the weapon data module: **40 Toxin, 1 tick per
second, 10 s, 3 m radius**, its own **15% / 2.0×** crit and **25%** status,
falloff `start 0, end 3, reduction 1.0` — note `reduction 1.0` means the
damage falls to **zero** at the rim, unlike the Laetum radial's 0.2.

The field belongs to the weapon's BASE state. Read the data module's attack
list as the two STATES of a transform weapon, not as parallel attacks: Torid's
`Grenade Impact` + `Poison Cloud` are the two parts of one base-state shot
(direct + field, the same shape as the Laetum's direct + radial), while
`Incarnon Form` is the other state entirely — a beam, which the wiki gives a
2.3 m radius and a 5-target chain the module's summary does not enumerate.

A zone follows the radial's rules (no body-part multiplier — "headshot
multiplier of 1x and cannot trigger headshot conditions"; its own crit and
status rolls — "initial hit and explosion apply status separately"), plus:

- **The direct hit and the field both apply.** Grenades *stick* to whatever
  they hit, so a directly-hit enemy takes the impact AND cannot leave the
  cloud: the wiki calls this out as guaranteeing "the maximum possible
  damage". In a single-target arena that is the normal case — the target
  takes every tick.
- **Ticks are weapon damage, not a status DoT.** They roll crit per tick and
  scale with the weapon's mods; they are not a Toxin *proc* and do not share
  the status DoT's coefficients.
- **No self-damage, no self-stagger** for this one (self-damage was removed in
  Update 27.4.3; the page states the explosion "does not inflict
  self-stagger").

**Condition Overload: the field DOES take it — on the attached target only.**
This reverses an earlier inference here (that the CO catalog's "no radial/AoE"
exclusion covered zones). The catalog has a category for exactly this shape,
and Torid is its named example on the excluded side:

> applies: "**Embedded Cloud** — *(e.g., Pox)* only on directly-embedded
> target."
> does not: "**Embedded Cloud Radius** — *(e.g., Torid primary fire)* on every
> non-directly hit target."

The Torid page says the same thing from the weapon side: *"Galvanized Aptitude
is multiplicative to base damage sources on direct hits and resulting clouds of
regular form. Clouds receive the multiplicative bonus **only on the attached
target**."* And the catalog row gives the class outright:

| weapon | attack | type | base | CO base | % | behavior |
| --- | --- | --- | --- | --- | --- | --- |
| Torid | Main-fire | Projectile | 100 | 100 | 100% | **Multiplying** |
| Torid | Toxin AoE Cloud | AoE | 40 | 40 | 100% | **Multiplying** |
| Pox | DoT Cloud | AoE | 20 | 50 | 250% | Adding |

**The class is per FORM, and the Torid proves it.** Those two rows are the BASE
form. The Incarnon form has **no row**, and the engine used to read that as "not
covered" and infer `Multiplying` from its siblings. Backwards: the table
enumerates exceptions, so absence is the positive statement that the attack is
ordinary — **Adding at +100%**, joining the base-damage bucket like Hornet
Strike (confirmed, user 2026-07-30). Both halves of the Incarnon form's CO
behaviour follow from that one absence: the class *and* the 100% base.

So one weapon runs both classes depending on which form is out — Multiplying
listed for the base form, Adding by omission for the Incarnon — which is why
`co_behavior` is per-form weapon data rather than a weapon-wide property. Note
the 100% in the two listed rows likewise holds *with evolutions equipped*: the
Torid takes no exclusion, that being Dual Toxocyst Perk 1's alone (§6).

So a grenade that STICKS to an enemy makes that enemy the directly-embedded
target, and every tick it takes carries CO. In a single-target arena that is
always the case. Pox's row adds the timing rule: *"Damage recalculates on every
tick"* — the CO bonus is read LIVE per tick from the target's current status
count, not snapshotted when the cloud is created.

**Ticks are full damage instances, and mods reach them.** Three patch notes
settle what used to be guesswork:
- *"Fixed Torid gas clouds not receiving damage buffs from mods."* — the tick
  takes the weapon's damage buckets.
- *"Changed Critical Chance logic by allowing it to occur on Radial Explosions
  … This fixes an issue with the Torid's gas cloud not allowing for
  criticals."* — a tick rolls its own crit.
- Firestorm-family radius mods *do* enlarge the cloud (per-weapon: the Torid
  page says so explicitly).

And status is per tick, not per cloud: *"Toxin clouds can proc Hunter Munitions
on each tick of damage."* A forced-proc mod firing once per tick is only
possible if each tick is its own instance — the same damage-instance rule §7
opens with.

**The first tick lands WITH the impact** — ✅ measured (MEASUREMENTS M13): one
grenade shows the direct-hit number and the cloud's first number together, then
nine more over the remaining nine seconds. So **ticks = duration × tick rate**,
ten for a 10 s cloud at 1/s. The wiki's *"Clouds do not instantly do damage, so
enemies that are quick may run through the cloud without taking any damage"*
describes the grenade ARMING; reading it as a delayed first tick cost a full
tenth of the field's damage.

**How the sim runs it** (`engine::dummy`): every landed direct pellet spawns a
`FieldState` — per PELLET, since each multishot projectile is its own grenade
and its own cloud — whose first tick is due immediately. Ticks are settled
by `process_field_ticks`, INTERLEAVED with the status settlement so that each
tick sees the statuses its predecessors applied (that is what makes the live CO
read meaningful) and its own procs still burn afterwards. Each tick runs
`settle_procs`, the same status machinery a pellet and a radial stage use — one
function, not three copies. A field carries the resolved part of the form that
SPAWNED it: a cloud outlives a transmute, and only one form of a transform group
has a field at all.

**A base-stat EVOLUTION reaches every attack part.** Commodore's Fortune,
Survivor's Edge and Elemental Balance all say "Increase Base Critical/Status
Chance", and that base-stat layer is a WEAPON stat change — the same reading
already applied to Elemental Excess's post-mod layer above, and the base layer is
the more clearly weapon-wide of the two. So the Torid's cloud takes them, which
matters: the cloud is most of that weapon's damage, and Survivor's Edge would be
nearly worthless on it otherwise. **INFERENCE, not a citation** — no source
states it either way. Nothing else in the roster is affected (only Dual
Toxocyst, which has neither a radial nor a field, and the Torid carry base-stat
evolutions at all).

Known approximation, recorded rather than hidden: the MOD-side buff state a tick
reads (Galvanized Scope's crit buff, Overwhelming Attrition's stacks) is
snapshotted at the most recent shot, not re-read at the tick. At 1.5 shots/s
that is under a second of staleness on buffs measured in seconds. Condition
Overload and the arcane runtime — the ones that matter — ARE read live.

**Overlapping fields STACK** — ✅ measured (MEASUREMENTS M13): several grenades
on ONE target run as N concurrent tick streams, not one refreshed field. That is
what makes the cloud the weapon's main damage — a 5-round magazine at 1.5
shots/s can have all five attached at once, worth up to ~5× sustained
single-target DPS over the refresh reading.

It stays a two-branch DATA field, `lingering.stacking` (`stack` | `refresh`),
selected per weapon and with **both branches unit-tested**. The Torid stacks;
the branch is not a global rule, and a future weapon may well refresh.

**Duration can be bought conditionally.** The Torid's Renewed Horror evolution
(*"On Reload from Empty: Lingering damage field duration doubles on first
shot"*) is modeled as `field_duration_on_empty_reload`, a multiplier the sim
applies to the field spawned by the first shot after an empty reload — measured
at ×2, i.e. 20 ticks instead of 10 (M13). One cloud in five on a 5-round
magazine, so under `stack` semantics a flat +20% of the cloud's total ticks.
Reverting out of an Incarnon form also refills the base magazine but is NOT a
reload from empty, so it does not arm the buff — a modeling choice, recorded in
M13.

**Source:** wiki Torid + Condition Overload (Mechanic) catalog + patch history
+ the weapon data module + MEASUREMENTS M13. **Status:** implemented; CO
eligibility and mod scaling are sourced, per-tick crit/status sourced, and the
tick clock, stacking and the Renewed Horror multiplier are **measured** (M13).

**Source:** wiki (Area of Effect, Damage Falloff) + the weapon data modules.
**Status:** unverified (needs Simulacrum measurement of direct+radial totals).

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
  2026-07-24: **shields must be fully depleted** (in-game test: 1 HP behind
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
conveniences (decision 2026-07-24), and **on-death transformations are
not modeled** (a respawned Thrax is always the physical form — the spectral
form is skipped).

**Source:** wiki + measured. **Status:** unverified. **High-risk** (CORE.md §3).

---

## 9. Temporal integration (pipeline layer [8])

**Definition.** Advance along the time axis to produce a damage-vs-time series.

**Mechanics.** Fire cadence, magazine depletion and reload, combo build/decay,
DoT stacking, buff duration and refresh. Steady DPS, burst DPS, and TTK are
**statistics derived from this series**, not primary inputs (CORE.md §2).

### Ammo Efficiency — a FRACTIONAL ammo cost

Not a chance to save a round. VERBATIM (wiki `Ammo`): *"Ammo Efficiency
determines the number of shots that occur before consuming ammo… if a weapon has
75% ammo efficiency and each shot originally costs one ammo, every four shots
will use one ammo"*, with

```
shots per ammo consumed = 1 / (1 − efficiency)
```

The implementation is a **divided cost with the remainder kept**, which the
Energized Munitions page states outright: *"The way this works is by dividing
the ammo cost so each shot consumes a quarter of the original, and keeps track
of the fractions as well."* So the per-shot cost is `ammo_cost × (1 −
efficiency)` and the magazine carries a fractional value — which is exactly what
`engine::dummy` does (`magazine -= 1.0 - efficiency`, evaluated live per shot so
a decaying buff is read at the moment of firing).

**A partial round still fires.** The single-round-magazine case proves the gate
is "anything left", not "a whole round left": *"For weapons with a single-round
magazine like the Exergis, the round gets consumed after the 4th shot"* — at
0.25 cost the magazine reads 0.75 / 0.50 / 0.25 / 0, and all four shots happen.
The sim's gate is the same (`magazine < 1e-9` triggers the reload, so any
positive remainder fires).

**Stacking is ADDITIVE, with one named exception.** *"Sources of ammo efficiency
stack additively with each other except for Energized Munitions, which stacks
multiplicatively."* The engine sums its sources (Frenzy, Akimbo Slip Shot,
Primary Crux) in `engine::dummy::ammo_efficiency` — correct for everything
modelled, since Energized Munitions is a Warframe ability and out of scope. **If
it is ever added it must multiply, not join the sum.**

**100% is a real ceiling** (user, 2026-07-30): a shot can cost nothing, never
less. Stacking past the cap buys nothing, and in particular efficiency never
starts *refunding* — the magazine cannot climb while the weapon fires. So the
per-shot cost is `max(0, ammo_cost × (1 − min(1, Σ efficiency)))`, and "free" is
the floor rather than a waypoint.

**Charge-backed magazines are exempt**, per weapon data
(`unaffected_by_ammo_efficiency`): *"Incarnon Form is not affected by Ammo
Efficiency (such as Energized Munitions)"*. So Primary Crux's efficiency grant
goes inert in an Incarnon form while its status-chance grant keeps working.

**A reload draws WHOLE rounds** — ✅ measured (M14). Reserve is spent in whole
rounds only, so a reload adds `floor(capacity − current)` and a magazine sitting
on a fraction keeps it: on a 5-round magazine 1.5 → **4.5**, 3.25 → **4.25**,
and 4.25 → **refused**, because the draw would be zero (in game that reads as an
already-full magazine, the HUD having ceilinged it to 5). One function,
`engine::dummy::reload_draw`, and it is the **global** rule — the auto-reload an
Incarnon transform performs runs on it too, not a separate fill-to-full.

**An overdraw's DEBT survives the reload** — ✅ measured (M14). When the
efficiency source lapses mid-magazine, the next shot costs a full round out of
whatever fraction is left and the counter goes NEGATIVE. Measured on a 5-round
magazine: 3 buffed shots leave 4.25, five full-cost shots take that to −0.75,
and the reload returns **4.25, not 5.00**.

This is the *same* rule as the one above rather than a second one, which is why
the engine needs no special case for it: a shot cannot overdraw by a whole
round, so after running dry the counter sits in (−1, 0] and
`floor(capacity − current)` comes out to a full magazine. The reload therefore
**adds** to the counter instead of assigning to it — assigning would forgive the
debt and hand back a free fraction of a round. All of it is a no-op without an
efficiency source, since a 1.0 cost lands the magazine exactly on 0.

**Reloading is gated on "can I fire", not on "is the magazine empty"**
(`engine::dummy::can_fire`). Two rules meet there and each rules out the naive
test in one direction: the magazine gate is *anything left* rather than *enough
to pay* (M14 — a 0.25 remainder fires a full-cost shot), and a shot that costs
**nothing** needs no round at all (user, 2026-07-30). The Dual Toxocyst hits the
second exactly: its last round headshots, the magazine lands on 0, and that same
kill arms Frenzy's +100% efficiency — so the next shot is free and fires instead
of forcing a reload. A charge-backed magazine can never take that branch, since
it is exempt from efficiency entirely.

**The in-game HUD shows the CEILING** of that fractional counter, which is how
M14 was readable at all: from 4.25 a single 0.25 shot moves the readout 5 → 4,
where a clean 5.00 magazine would have stayed at 5. Worth knowing before
comparing any sim ammo number against a screenshot — and if the UI ever displays
a live magazine, it must ceil, not round or truncate.

**Source:** wiki `Ammo` + `Ammo Efficiency` + `Energized Munitions` +
MEASUREMENTS M14. **Status:** formula, fraction-keeping, the partial-round gate
and the stacking rule are sourced; the overdraw debt, its survival across a
reload, and the ceiling display are **measured** (M14).

---

## Open questions

- Exact multiplicative-bucket membership for every common mod (§2).
- Armor DR constant/curve and level-scaling formulas (§8).
- Innate-vs-mod elemental combination timing on multi-innate weapons (§3).
- Proc-weight formula when 3+ elements coexist (§6).
- Incarnon-form stat/mechanic overrides (see the Dual Toxocyst prototype).
