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
- **`disables: [stat]`**: a mod that LOCKS a stat (Pistol/Primary Acuity →
  multishot, the Cannonades → fire_rate) pins it at the WEAPON'S DEFAULT.
  "Equipping this mod will set weapon's `<stat>` to its default ignoring other
  bonuses, **even negative effects**" (wiki, both families) — so it is not a
  mod-bucket cleanup: the mod bucket, the conditional stacks, an evolution's
  permanent bonus, an arcane's live stacks and the weapon's own Frenzy passive
  all go. `resolve` handles what it can see and states the lock on
  `ResolvedPanel::locked`; the sim reads it back through `DummyParams::locks()`
  for the live sources it owns. See MEASUREMENTS M30.
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
authoritative rules):
1. **Hierarchy = mod layout order**, top-left slot first → bottom-right
   last. Adjacent-in-hierarchy uncombined primaries merge pairwise into
   secondaries.
2. **Innate weapon elements come LAST** in the hierarchy — not first.
   Exception: Kuva/Tenet weapons with two
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
   Implemented by pushing a mod's own elements in REVERSE of its card
   order; only a riven carries two, so it reaches nothing else. Verified
   against an in-game reading — MEASUREMENTS **M31**.
7. **Innate secondary elements** (Ogris Blast, Nukor Radiation, ...) are
   permanent and never combine; mod primaries combine independently
   alongside; a Kuva/Tenet progenitor element does NOT fold into an
   innate secondary. Likewise combined-element MODS (Magnetic Might
   family) add their secondary directly, outside the primary hierarchy —
   so their SLOT decides nothing. **That last half is contradicted by one
   in-game reading** (M31): moving a Magnetic Strafe past a Cold mod
   changed which elements paired, which nothing here allows. The rival
   model is that a combined element sits in the hierarchy and FLUSHES the
   pending primary above it. Unverified, not implemented; M31 states the
   three-mod experiment that decides it.
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

### Continuous ammo cost — the 0.5 rule does NOT reach a charge pool

Continuous weapons "consume 0.5 ammo per tick of damage", and the patch note
the wiki quotes says it plainly: "To help with ammo economy, Beam Weapons
consume 0.5 ammo per trace — unless they are Flamethrowers." Chained beams
consume nothing extra.

The engine does not model per-shot ammo COST at all: `ammo_cost` sits in every
`data/weapons/` entry and no Rust code reads it; the sim spends a flat 1.0
(minus ammo efficiency) per shot, beam ticks included.

**A charge pool is not ammo, and spends 1 per tick** (owner, 2026-08-01,
measured in game). The Torid page had already put the Incarnon outside the
ammo economy — "instead of drawing ammunition from its reserves, the Torid's
Incarnon Form uses a separate 'magazine'", and it "is not affected by Ammo
Efficiency" — and the 0.5 rule exists to help that economy, so it stops at the
boundary. 170 charges is therefore 170 ticks, which is what
`pseudo_reload.magazine` states.

That was worth checking rather than assuming: doubling the window to 340 over
a 60 s run took DPS 3634 → 3919 (**+7.8%**) and transforms 4 → 2, because a
longer window spends proportionally less time in the two transitions. The
number we ship is the measured one.

**The ammo-pool half is now IMPLEMENTED** (2026-08-01). It stopped being
unreachable the moment the Larkspur Prime and Verglas Prime joined the roster:
both are beams with real magazines, and the Larkspur page states its own
numbers — "0.5 per primary tick" against "Alt-fire consumes 10 ammo per shot".
`attack.ammo_cost` is read at last, and it multiplies the MAGAZINE spend, so it
changes reload cadence even where the reserve is infinite. See MEASUREMENTS
**M18(b)** for what it moved and what would confirm it.

---

### Slash on critical (Hunter Munitions / Internal Bleeding)

A CRITICAL hit rolls its own chance to apply a Slash status. The roll is
**independent**: wiki, "not affected by the weapon's Status Chance, or damage
type distribution, besides being indirectly affected by its Critical Chance".
It is per PELLET, and a weapon with no Slash anywhere in its vector still gets
one — that is the entire mod.

Modeled by pushing `Slash` onto that pellet's proc list, next to whatever the
status roll produced, rather than applying a bleed directly. That is what
makes the damage right by construction: a Slash proc is already
`0.35 x ModdedBase` per tick, armour ignored, scaled by **the proccing hit's**
crit and body-part multipliers, and lengthened by status-duration mods. So the
wiki's "Headshots, orange and red Critical Hits will greatly increase the
damage dealt" and Hunter Track's longer bleed both follow from the existing
bleed, with nothing restated.

It never fires on a non-crit, and a Slash-immune target is skipped before the
roll rather than after.

**Every DoT has a PARENT** — that is the design this follows, in the game and
in the engine. A bleed is not an independent damage source: it belongs to the
hit that caused it and reads that hit's multipliers off a provenance snapshot.
So "a critical hit's privilege" is the whole description of Hunter Munitions:
the TRIGGER changed from a status roll to a crit, and nothing else did.

That is verified, not asserted. `a_hunter_munitions_bleed_is_indistinguishable_from_any_other_slash`
runs the mod's bleed against a FORCED Slash proc under identical conditions —
plain, a 3x crit multiplier, a 3x body part, a tier-2 red crit, 1.9x status
duration, 1.5x status damage, and a Vigilante-promoted parent — and the two
agree in all seven. They agree to ~0.06% rather than exactly, and that gap is
not mechanical: the mod's roll consumes an RNG draw per pellet, so the builds
walk different random streams. It shrinks with runs (1.6% at 200, 0.06% at
6000), which is what sampling noise does and a real difference does not.

**Internal Bleeding / Hemorrhage is the same mechanic with a different
trigger** — an Impact status instead of a crit — and it has always fed the
same per-pellet proc list, so it has always had the same parent
(`an_internal_bleeding_bleed_is_indistinguishable_from_any_other_slash`).

The two STACKING rules differ, and the difference is not decorative:

- Hunter Munitions "can stack with Slash statuses applied using a weapon's
  innate status chance" — two bleeds on one hit — but "cannot produce multiple
  procs in a single instance of damage alongside FORCED Slash". So its push is
  guarded on `forced_procs`, not on the proc list.
- Internal Bleeding is stricter: it cannot double up with **any** other Slash
  source, "such as a weapon's innate Slash, Hunter Munitions, or the debuff
  from Seeking Talons". So its guard reads the proc list.
- Together: "drawn independently, and if both proc at the same time, only 1
  slash proc is applied." Hunter Munitions pushes first, so Internal
  Bleeding's guard sees it and skips — which IS the exclusion.

That exclusion reproduces a number the wiki publishes. On a shot that both
crits and applies an Impact status the Slash chance is 54.5% at fire rate
>= 2.5 and 79% below it — the union `1 - (1-0.30)(1-0.35)` and
`1 - (1-0.30)(1-0.70)`. The engine hits both from the two rolls
(`hunter_munitions_and_internal_bleeding_union_to_the_wikis_numbers`), which
is what shows the exclusion is modeled as an exclusion rather than as a second
bleed quietly going missing.

**Which attack parts roll it.** Hunter Munitions rolls on every SHOT instance
that crits — the direct hit and, off its own crit roll, the explosion. It does
NOT roll on lingering-FIELD ticks. That is a deliberate omission, not an
oversight: a field ticking several times a second would mint bleeds at a rate
nothing in the sources supports, and no source says whether it should. The
Vigilante promotion, which only raises a crit that already happened, does
reach all three.

**Status:** the mechanic is wiki-sourced; the resulting DPS is not measured.
On a 5-mod crit Torid vs a Thrax Centurion @9999 Steel Path it is worth
+20.5% DPS (21,092 -> 25,414), which is the shape expected of an
armour-bypassing bleed on a 64%-crit build — but it wants a measurement.

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

**A perk can READ `effective_cc`, not just feed it** — Prelude of Might, the
Furis and Braton Incarnon Genesis tier-4 option: "With Critical Chance below
40%: Increase Base Critical Damage Multiplier by +3x" (50% and +3.0–3.4x on the
Braton family). The wiki attaches a note to that same row in both families:

> Condition is affected by the critical chance increase effect of Puncture
> status.

So the threshold is tested against the crit chance **the hit has**, not the one
the arsenal prints — the whole of `effective_cc`, a target-side source included.
Puncture's Weakened is +5% flat crit chance received per stack to +25% at five
(§6), which means a build under the line walks over it on its own procs and gets
the perk back as they expire. Puncture is simply the source the wiki bothered to
name: it is the only one that raises your crit chance without your own panel
moving.

Consequences worth stating, because they are what the model has to reproduce:
- The check is **per shot**, and the value is the weapon's — not a pellet's. A
  weak-point-only crit chance (Pistol Acuity) or a SET one (Gotva Prime) is a
  property of where a projectile landed, not of the weapon being asked about.
- "Below" is **strict**: exactly at the threshold the perk is off.
- On a two-form weapon it is resolved **per form**, which is the Furis's whole
  interaction — its base form is 70% Puncture and its Incarnon form is pure Heat
  at 26% crit, so the form that generates the stacks turns the perk off in the
  form that was carrying it (26% + 3 stacks = 41%), while the base form's own 5%
  never crosses the line.

Implemented in two halves, deliberately: `loadout::resolve` GRANTS it against
the panel (`ResolvedPanel::crit_mult_below_cc` records what it granted) and the
sim takes it back on any hit whose `effective_cc` has reached the threshold.
The panel test remains sound as a short-circuit because every live source is a
bonus — a build already over the line on the panel alone can never come back
under it.

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
> Implemented: `DamageVector::quantized_against(modded_base)` (per-hit
> vector, BEFORE crits/type-modifiers/faction multipliers — those
> multiply quantized values) and `damage::quantize_base_crit_damage`
> (wired into the CD bucket when mod resolution lands). The page's
> flagged "conflicting info" is a mathematical pseudo-conflict: for pure
> multipliers, `Round(v/s)·s·k ≡ Round(kv/ks)·ks` — the two descriptions
> differ only when elemental mods change the vector's composition.

**THE SCALE'S DENOMINATOR IS `ModdedBase`, NOT THE VECTOR'S TOTAL** (measured,
2026-08-23, M57). The page states it twice as formulas — `Scale = ModdedBase/32`
and `x = TypeValue/ModdedBase` — where ModdedBase is `base × (1 + damage mods)`
with elemental portions EXCLUDED. Elements are in the NUMERATOR only, which is
what makes the note above true: a non-elemental bonus scales numerator and
denominator alike and cancels, an elemental one does not.

It was the vector's own total here for months, and the paragraph above is the
one that should have caught it — *"the two descriptions differ only when
elemental mods change the vector's composition"* names the exact case, and the
only test on the function is the 30/30/40 example, which carries no mods at all
and therefore has `ModdedBase == total`. A Braton Prime with Infected Clip and
Hellfire tells them apart: base 35, Gas 63, and the wrong denominator snaps four
components to 33 units instead of 32 — **101.06 against a measured 98**. Four
builds were measured and the right denominator reproduces all four to the digit.

One visible consequence: a MONO-TYPE vector is no longer automatically lossless.
It used to be exactly 32 units of itself; it is now however many units of
ModdedBase it happens to be, so a pure 63 Gas on a base of 35 is 57.6 units and
snaps to 58.

Related: per-shot **damage** quantization also exists and was changed from
1/16 to 1/32 steps in Update 40 (undocumented, per the wiki `Damage` patch
history). Exact mechanics not yet transcribed — same recorded-only status.

**A STATUS TICK'S ACCUMULATOR STARTS AT 1** (wiki `Damage/Calculation` §Damage
Over Time; measured, 2026-08-23, M58).

```
Unrounded Tick Damage = (Σ Sᵢ + 1) × C × M
```

`Sᵢ` is each stored damage seed (ModifiedBase with the applying hit's crit, body
part and faction in it), `C` is 0.5 for Heat / Electricity / Toxin / Gas and
0.35 for Slash, and `M` is the elemental, faction and status-damage bonuses.
`Dot::accumulator_unit` is the `1`.

- **Once per TICK GROUP.** Heat, Electricity and Gas consolidate, so one `1`
  however many stacks fold in; Slash and Toxin tick independently and each
  carries its own. The page says so outright and says why: it is "neither a
  final flat +1 damage bonus nor a bonus applied once per status stack".
- **Five statuses only.** Blast is not among them, and the capture confirms it
  independently — a detonation reads exactly `0.3 × base` at every tier.
- **One faction layer, not two.** The page's Toxin example is
  `(40 × 1.55 + 1) × 0.5 × 3.25 × 1.55`, with the `1` added between the seed's
  layer and `M`'s. So a Roar'd bleed is `f²` only in the limit where the seed
  dwarfs the 1. A FINAL multiplier (Eclipse) scales both and stays exact.
- **DoT ticks take no 1/32 quantization**, stated in the same section: "A proc
  is calculated from the attack's modded base damage rather than from the sum
  of its quantized damage-type values."

It is worth 0.5 damage before multipliers — 2.9% of a tick on a base-35 rifle
and 0.25% on a base of 400, which is why it took a small gun to see.

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

### The order inside one trigger pull

A volley leaves the muzzle at one instant and **does not settle at one
instant**. Measured (MEASUREMENTS M62):

1. A pellet resolves its own **explosion** before the next pellet's collision —
   `P1 direct, P1 blast, P2 direct, P2 blast`, not every collision and then
   every explosion.
2. **Every instance re-reads the target**, not every shot and not even every
   pellet: pellet 1's explosion already reads the stack pellet 1's collision
   left one instant earlier.
3. **An instance does not amplify itself.** Its own forced proc lands after it
   has been settled, so the first collision of a volley reads whatever was on
   the target before the trigger was pulled.

The measurement that pins all three is a Laetum forcing a Viral proc on both
halves of every pellet, into an unmitigated body: `200 / 1,200 / 450 / 1,500`,
which is the Viral ladder read at 0 / 1 / 2 / 3 stacks and is the only
assignment of those four numbers that has a stack count climbing.

It matters wherever a proc a volley applies changes what the rest of the volley
does — Viral and Disrupt amps, an armour strip, Condition Overload's type
count — which is most status builds. In the engine, `DebuffState::amps` is read
inside the stage loop for exactly this reason; `prune` stays once per pellet,
since the whole volley is at one instant and pruning again is a no-op.

### A gauge is not an adapter

A **gauge-switched form** is one you pay a meter to enter: fill it in the base
form, spend it in the other, come back. Three weapons' worth of vocabulary used
to say "Incarnon" for that, because every example was one — the data key was
`incarnon:`, the request token was `incarnon_cycle`, and the engine decided
"does this have a gauge" by asking whether the form was *named* `incarnon`.

The **Mausolon** is the counter-example (owner, 2026-08-15). Its alt-fire is
bought with kills — *"Getting 5 kills with the Mausolon's primary fire will
unlock an Alternate Fire that discharges a powerful laser that explodes on
impact"*, and once spent *"additional kills are needed to recharge the laser"*
(wiki) — and it is an ordinary Arch-Gun with no adapter, no Genesis and no
tier-1 unlock. The **Cortege** carries the identical sentence for its trio of
grenades. So the two questions were split:

- **`WeaponSpec::has_gauge`** — declared by the entry (`gauge_form:`). This is
  what makes a cycle, and it is the only thing that does.
- **`FormKind::is_adapter_form`** — still only the Incarnon form. This is what
  hides a form until its unlock is chosen and what keeps it out of a riven pool.

`ChargeOn` therefore has three members: `weakpoint_hits` (the Zariman pistols),
`direct_hits` (the Torid) and `kills`. The third is counted off a **mark**
rather than a per-shot delta, because a kill can land on a status tick between
two shots — and the mark advances in **both** forms, so a kill made with the
earned form never pays for the next one.

A kill-fed gauge is also the first mechanic here that can be *reached* and
still not *pay*: an unmodded Mausolon in a 30 s engagement earns its fifth kill
late enough that it transmutes and the fight ends before the 0.8 s charge
completes.

### Independent procs

Status effects that come from a specific weapon rather than from the damage-type
draw (`data/debuffs/independent_procs.yaml`, wiki `Status_Effect` §"Independent
from Damage"). They never compete for the roll and never renormalise anyone
else's — and the ones flagged `counts_for_condition_overload` add a status
**type** while they hold.

A weapon declares them by id (`independent_procs: [lifted]`); the engine owns
the duration, the same way it owns every other proc's. Implemented so far:

- **Lifted**, 1 s (owner, 2026-08-15) — the Mausolon alt-fire's explosion. Its
  crowd control (the target is suspended) is not modelled and cannot be: this
  arena has no movement. What is modelled is the count.

**Microwave** predates the list and still has its own flag
(`applies_microwave`); it is the same class of effect with an infinite duration.

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

**And the exceptions are individual quirks, not a law.** Two independent tells,
both the same shape — a weapon family split down the middle:

- **Lato Vandal has a row, Lato Prime does not**, same family, same Genesis.
- **Zylok Prime's Incarnon Mode has a row** (*"500 or 530 (with Evolution II)"*,
  CO base 500, so 100% or 94%) **while the base Zylok's does not** — the plain
  Zylok takes CO in full, the Prime does not.

Nothing mechanical separates two Latos or two Zyloks. That asymmetry is what a
per-entry slip in DE's code looks like — careless or deliberate, attached to one
entry rather than derived from a rule. No general mechanical law could produce
it, which is exactly why none of this may be modelled as one.

**The decisive tell: the table HAS a vocabulary for "these variants are alike",
and uses it.** A row names every entry it covers, so sameness is written down
rather than assumed:

- *"Braton / Mk1-Braton / Prime / Vandal"* — one row, four variants.
- *"Burston/Burston Prime"* — one row, two.
- *"Paris / Paris Prime / Mk1-Paris"* — one row, three, for the charged attack.

And the SAME Paris family splits two rows later: its **Incarnon** row reads
*"Paris / Paris Prime"* and the Mk1-Paris is not on it — the same weapons, the
same table, spanned in one row and separated in the next. So a row naming ONE
variant is naming one variant on purpose; it is not shorthand for the family.

That is what settles the Furis. Its row reads *"Furis"* alone where the
Burston's reads *"Burston/Burston Prime"*, so the MK1-Furis is absent by the
table's own grammar, not by an oversight — and the owner confirms it
(2026-08-06: DE treats the two as separate weapons internally, and the table has
already shown Prime-vs-base splits). The Furis excludes Evolution II's flat
damage from its CO base (100 of 128, the row's own 78%); the MK1-Furis does not,
and its +34 feeds CO in full. `furis_haven_foray` / `furis_stormburst` carry
`co_base_excludes_this_evolution`; the `mk1_furis_*` pair must not, and
`furis_co_split_tests` pins BOTH halves — the failure mode here is a tidy-up
that aligns them, which a test checking only one half would let through.

The Zylok's second row is worth reading for a different reason: *"Zylok / Zylok
Prime | Incarnon Form Radial Attack | AoE | 776 | 700 | 90% | Adding — Radial
hit only receives CO bonus on target directly hit by bullet. AoE does not scale
off multishot."* An AoE part receiving CO **at all** is an exception (the normal
rule is direct hits only), and it arrives with its own base fraction (90%) that
has nothing to do with an evolution. Three unrelated discrepancies in two rows
of one weapon.

**So AoE parts carry their own CO eligibility, defaulting to NO.** Both the
explosion and the lingering field take a `takes_condition_overload` flag from
weapon data. The engine deliberately supports what the mods forbid, because the
game does it: the Torid's cloud declares it (its own catalog row), the
Burston/Burston Prime Incarnon radial declares it (its own catalog row, and the
roster's first explosion to take CO), the Zylok's Incarnon radial would, and
every unlisted AoE part gets nothing. Note the
Zylok's qualifier — CO reaches the radial only *on the target directly hit* —
which a single-target arena always satisfies; a multi-target model would have to
gate it per enemy.

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

**A DoT's BASE IS WHATEVER APPLIED IT.** Two rules, and which one you are under
depends on the applier. A WEAPON's own hit applies statuses off `ModifiedBase` —
"unmodded x (1 + BaseDamageBonuses)", which EXCLUDES the elemental portions
(wiki, Toxin_Damage). An ABILITY or a damage INSTANCE applies them off its own
damage number, elements included: Toxic Lash on a 200-damage weapon deals 78 and
its proc ticks for 39, half of 78; a syndicate blast's Gas cloud burns off the
blast's own damage, not the weapon's. Which of the two Primary Debilitate's
split falls under is OPEN and is the whole of MEASUREMENTS **M33** — it reads
`ModifiedBase` here.

**A RADIAL BELONGS TO THE FORM THAT DECLARES ONE.** An Incarnon cycle fires two
weapons in turn, and every per-shot property is read off the ACTIVE form — the
explosion included. A weapon whose Incarnon detonates does NOT detonate in its
base form, and the two lines that read the outer params instead gave it that
explosion on every base-form shot: +42% on a Burston Prime that never
transformed, all of it Heat the base form does not have. See MEASUREMENTS
**M32**.

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

**A weak-point bonus is conditional on WHERE THE BULLET LANDS, and no stack
policy can change that** — `AssumedMax` is about a buff's stack COUNT, not
about aim. Both halves of Acuity (Primary / Pistol) live in their own buckets
(`weakpoint_damage`, `weakpoint_cc_rel`), which the sim gates on `is_head` per
pellet, and Cascadia Accuracy's weak-point crit joins them there. The crit half
used to fold into the plain crit bucket under `AssumedMax`, splitting one mod
down the middle: its Weak Point Damage stayed conditional while its Weak Point
Crit Chance became unconditional. On the panel — always `AssumedMax` — that
read the Burston Prime Incarnon's 28% as **126% on every shot**, and handed the
same 126% to the RADIAL, which can never weak-point-hit at all. Both halves now
state themselves as their own rows next to the plain ones, on the direct part
only. Sim results never moved: the sim runs `Emergent`.

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

**Bounce** (wiki `Bounce` — the PROJECTILE half, and a different mechanic
from Ricochet above however often the community uses one word for both:
*"Bounce, sometimes unofficially referred to as 'ricochet' by the community,
is a property of Projectiles… Unlike true Ricochet, bouncing only happens with
projectiles, not with hitscan weapons."*).

**IT IS GEOMETRY, NOT TARGET-SEEKING**, and that is the whole difference:
*"Bouncing projectiles will **reflect off surfaces at the same angle of
incidence** (i.e. the angle at which a projectile moves away from a surface
equals the angle at which they enter). Bouncing projectiles will also lose
velocity after collisions."* A ricochet REDIRECTS to an enemy inside a stated
range; a bounce keeps flying in a direction the impact decided, and whatever
that line meets next is what it hits. Neither page gives the other's rule —
Ricochet publishes a RANGE per weapon and no direction, Bounce publishes a
DIRECTION and no range.

A rebound landing is a full hit, not a fraction of one: `Hit Mechanic`
§"Rebound and Bouncing Hits" — *"ReboundPtr counts as a MainPtr"*, and it
*"inherits the Extra Hit bonuses of its MainPtr"*.

The wiki's Bounce tables name the counts: the Latron family's Incarnon Form 6,
Miter 5, Panthera 5 (Prime 3), Mutalist Quanta / Quanta / Quanta Vandal
alt-fire 12, Trumna alt-fire 7, the Tetra family with Kinetic Ricochet 6,
Drakgoon 2/3 (6/7 with Fomorian Accelerant), Tenet Arca Plasmor 4, Azima
alt-fire 100, Angstrum Incarnon 1, Cyanex 1, Sporelacer 3, Mandonel 2,
Velocitus ∞ (Archwing only), and twelve throwing melees at 3.

**WHAT THIS ENGINE DOES, AND THE TWO ASSUMPTIONS IN IT.** Both mechanics run
through one `ricochet:` block and `chain::bounce_path`, which walks to the
NEAREST body not yet hit.

- For a true RICOCHET that is the mechanic: it seeks enemies, and `range_m`
  is the published bound.
- For a BOUNCE it is an **APPROXIMATION AND A GENEROUS ONE**. Nearest-first
  keeps the projectile inside the crowd it started in, so on a dense
  formation every bounce lands near the last one and a weapon that explodes
  per bounce stacks its spheres on one cluster. Measured on the group-clear
  ruler 2026-08-21: the Latron Prime's five bounces are worth **9.9x**
  (1295 kpm against 131 with none), and only 30 of 361 bodies took any
  damage — the signature of six 4 m spheres landing on top of each other.
  Reflection would spread them along a line instead. **The owner's reading is
  that a bounce should carry straight on in one direction** (2026-08-21),
  which is the wiki's rule; implementing it is a geometry decision that has
  not been taken.
- **`headshot_chance` IS AN OWNER ASSUMPTION, NOT A TRANSCRIPTION** (owner,
  2026-08-18, re-searched 2026-08-21). A rebound hit CAN headshot — it counts
  as a MainPtr — but where it lands is geometry this arena does not model, and
  **no source states a rate**. The whole roster uses **0.5**. The one published
  remark anywhere near it is the Neutralizer's *"Ricochets prioritize Weak
  Points"*, which is written as that weapon's NOTE and so is evidence that
  prioritising is the exception rather than the rule.

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
- Zone shapes: sphere / cylinder / cone — **not modelled**. The sim fights
  ONE target, which is either in the blast or is the blast's origin, so a
  zone has nothing to intersect. This becomes real with multi-target, and
  the geometry decisions it inherits are in [`UI.md`](UI.md) §Core decisions
  (top-down plane, actors as circles of radius 0.25 m).
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
- **No Condition Overload — unless the entry says otherwise.** CO is
  direct-damage only; radial/AoE components and non-directly-hit targets are
  excluded (§2). CO also ignores falloff as a final multiplier. Careful: CO is
  the *only* thing the radial loses here — weapon-wide damage buckets still
  reach it. The arcane base-damage stacks (Merciless & co) share a bracket with
  CO in the direct-hit formula, so the radial takes that ratio **without** the
  CO term.

  The exclusion is the RULE and not a law: an AoE part carries its own
  `takes_condition_overload`, defaulting to no, and the CO catalog names the
  entries that have it one at a time (§6). The roster's live example is the
  **Burston / Burston Prime Incarnon radial**, whose row reads *"55 | 13 | 24%
  | Adding — Radial hit only receives CO bonus on target directly hit by
  bullet. AoE does not scale off multishot."* Three separate facts: the
  explosion takes CO (on the directly-hit enemy, which a single-target arena
  always is), it computes on its own **unevolved** 13 base while the tier-2
  evolution's +42 raises its damage to 55 — 13/55 is the printed 24% — and it
  fires once per trigger pull rather than once per pellet. Because "direct hits
  only" is false on such a weapon, the panel builds that phrase from the entry
  instead of asserting it, and the explosion states its own CO row.
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

**THE EXPONENT.** "Affected twice" means the DoT goes as multishot **squared**,
and it is the one number about beams that is routinely misremembered (asked
again 2026-08-07). The usual guess is that a beam trades proc COUNT for proc
SIZE and comes out even — a fair reading of "multishot cannot add a beam", and
the reason it is worth writing the table out. It does not trade: the merge sums
BOTH halves, so nothing is given up.

| at multishot `M` | procs per tick | payload each | DoT total |
| --- | --- | --- | --- |
| gun | `M × SC` | 1× | `M` |
| beam, ROLLED status | `M × SC` | `M×` | **`M²`** |
| beam, FORCED proc | 1 | `M×` | `M` |

The forced row is the exception the wiki states in the same breath, and it is
what the "even trade" intuition actually describes: *"their damage output is
not affected twice by multishot, instead being equivalent to use on standard
weapons."* One proc a tick carrying `M×` = the gun's `M` procs carrying 1×.
Hunter Munitions on a beam is therefore linear; the weapon's own status chance
is not. DIRECT damage is linear either way — `M` instances of 1× on a gun, one
instance of `M×` on a beam — which is the half the intuition gets right.

What a beam pays for all this is the crit roll: one per tick, not `M`.

`a_beams_dot_scales_with_multishot_squared` asserts every row of that table
exactly (status chance pinned at 1.0 so the proc counts are not sample means).

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

#### A beam's GEOMETRY — sphere and chain (Torid Incarnon)

Shape, not a damage part. It lives in `attack.beam` and is deliberately **not**
a `radial:`, because in this engine that word means *a second damage instance*
and the wiki forbids that reading: *"The damage radius is not a separate damage
instance from the beam, so a target that is directly struck by the beam is still
only hit once."* Adding a radial here would double-count the one target the
arena has.

| field | Torid Incarnon | |
| --- | --- | --- |
| `range_m` | 37 | Punch Through *"has no effect on the behavior of the beam"* |
| `damage_radius_m` | 2.3 | the impact sphere; **Firestorm (Primed) enlarges it** |
| `radius_takes_multishot` | false | *"only targets directly hit by the beam benefit"* |
| `chain.hops` / `range_m` / `damage_per_hop` | 5 / 7 m / ×0.75 | a SEQUENCE of hops, each 75% of the one before — not five simultaneous targets |
| `chain.origin` | `radius_targets` | *"chain independently to 5 additional enemies starting from **each** target hit by the initial damage radius"* |
| `chain.takes_multishot` | false | chains from sphere-only targets inherit the sphere's rule |

**Why the sphere is worth so much more than its own damage.** Every enemy it
catches becomes a chain origin, so the instance count grows as `1 + 5·Y` in the
number of enemies inside it. That, not the sphere's damage, is what Firestorm
buys — and it is why the community describes Primed Firestorm as *"more enemies
hit and more beams spawned"* rather than a damage increase.

**Multishot is asymmetric here, and it is easy to get backwards.** The merged
beam multiplier reaches the **directly struck** target and nothing else: not the
sphere, and not chains that start from a sphere-only target. Chains starting
from the directly struck target are not excluded by the wiki's wording and so do
take it.

**`chain.nodes_have_radius` is `false`, on an argument rather than a citation.**
The sphere is **not an explosion** — it is the beam's hit-detection volume. An
explosion here is a separate damage instance with linear falloff (§Area of
Effect above), and every `radial:` in `data/` carries one; this sphere carries
none, because the wiki denies it the thing falloff attaches to: *"The damage
radius is not a separate damage instance from the beam."* That is also why a
directly struck target *"is still only hit once"* — one instance, and the sphere
only widens who receives it.

A sphere at a chain node could not belong to the beam, whose contact point is
elsewhere, so it would have to be the node's own damage instance — an explosion
needing a falloff nothing documents. Hence `false`, flipped 2026-08-06 from the
`true` the line carried since 2026-07-30 (user, on this argument). It also
explains the datamined asymmetry: no radius on the Incarnon attack, a falloff on
the Poison Cloud, because only one of the two is a damage instance.

**An argument is not a measurement.** MEASUREMENTS **M15** stays open; its Y=1
protocol is what settles this, and it also explains why counting damage numbers
in a clump cannot.

**Single-target impact: none.** Nothing in this block feeds a damage number
today; the arena has one enemy and the sphere cannot hit it twice. The panel
states the (mod-scaled) radius so an equipped Firestorm is not invisible, and
the rest is the multi-target model's input.

**Source:** wiki Torid Incarnon Genesis (verbatim throughout) + user
(2026-07-30) for `nodes_have_radius`. **Status:** geometry transcribed; the
node-sphere question is **unverified** (M15), and the 37 / 2.3 / 7 values
themselves have a SOURCE-SPLIT recorded in DATA_SOURCES.

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

### The two ends of a magazine, and why only one of them is "the magazine was full"

Two mod families read a magazine COUNTER rather than a stat, and they are not
mirror images.

**SYNTH CHARGE — the LAST round.** *"bonus damage to the final shot in the
Magazine"*, and the window is read BEFORE the round is paid for: this pull is
the last if at most one round is left to fire. On a burst weapon that is the
last BURST (Forceful Finality's *"final magazine burst"*, three rounds on a
Burston), which is why the gate is `mag_left ≤ burst.count`.

**THE CHAMBER FAMILY — the FIRST round, read AFTER.** Charged Chamber and Primed
Chamber both print *"+X% Damage on first shot in Magazine"*, and both pages
define it the same way: the bonus lands *"as long as the magazine counter is at
Max Magazine − 1 **after** a shot is fired"*, with the consequence spelled out —
*"when used alongside 100% ammo efficiency, make sure one shot is missing from
the magazine, since the buff doesn't apply on a completely full one"*.

On an ordinary weapon that IS the first shot out of a fresh magazine (full goes
to full − 1) and the distinction never surfaces. **AMMO EFFICIENCY is what makes
the wording load-bearing**: a free shot leaves the counter where it was, so a
full magazine pays nothing however often you fire it and one sitting at max − 1
pays every single shot. `mag_left == mag_max` at the top of the pull would have
inverted both cases — and that is the same bug DE fixed on the Vectis Incarnon
in ver 43.5 (*"Fixed the Vectis Incarnon Form benefitting from Primed Chamber on
every shot"*), which is also why the Incarnon form is NOT exempted here the way
Synth Charge's own card exempts it.

**ONE BRACKET, TWO CARDS.** Charged Chamber is *"multiplicative with other
damage mods"*, Primed Chamber *"is applied multiplicatively after all other
modifiers from mods and abilities"*, and they *"stack additively with each other
for up to 140% bonus damage"* — so the two sum and the sum is one factor beside
Double Tap's and Synth Charge's. They are deliberately NOT a mod family:
*"Despite its name … it is not the 'Primed version' of Charged Chamber, and thus
can be equipped alongside it."*

**AND IT REACHES STATUS DAMAGE** — *"The damage bonus applies to all Multishot
hits and to Status Damage"* — so the factor multiplies `mb_live` as well as the
instance. That one sentence is the whole difference from Synth Charge, whose
page never says either way and which therefore leaves the status base alone
(recorded as an open question on its card).

**A charge that eats the MAGAZINE — the Phantasma's alt fire.** Three of the
weapon's numbers are derived from one another rather than stated, and the field
that says so is `charge_ammo_per_second`:

    charge time = magazine / 11 s      the shot costs = magazine
    damage      = listed x magazine / 11

Wiki Notes, verbatim: *"Charging consumes ammo, up to a full magazine on full
charge"*, *"Damage dealt by the plasma bomb is directly proportional to the
amount of ammo consumed during the charge"*, and *"Charge rate consumes a set 11
ammo per second. Modding to increase magazine capacity will allow a longer total
charge, and thus more damage."* Confirmed in play (owner, 2026-08-09).

**This makes Magazine Capacity a DAMAGE stat, on the only weapon in the roster
where it is one.** A magazine mod lengthens the charge, raises its price and
raises the bomb — direct hit and explosion alike — in one ratio.

WHAT THE LISTED NUMBERS ARE is the one thing the wiki never says outright, and
three of its own figures answer it between them: the listed charge time is
1.00 s, the rate is 11 ammo/s, and the magazine is 11. One second at eleven a
second IS the magazine, so the listed time is a FULL charge and the listed
15 + 73 is a full charge of the unmodded magazine. At stock nothing moves; what
changes is that the shot now costs what it spends. A full charge dealing 15 + 73
*with a magazine mod on* would falsify it.

**Plentiful Mayhem — `Multishot consumes ammo … and increases Damage by +60%`.**
Four rules, all wiki, and the per-form split is the interesting one:

- *"Damage bonus from multishot consuming ammo is multiplicative to base damage
  bonuses like Serration"* — an INDEPENDENT final multiplier, not a member of
  the base-damage bucket. Same bracket as the beam ramp and Devouring
  Attrition; it multiplies the finished instance, and the status payloads are
  left out of it (unsourced, recorded as a choice). **Attrition no longer sits
  here**: measured, its roll travels into the statuses that instance applied
  (MEASUREMENTS M37), which is what a per-instance multiplier does — this bullet
  is the reading that has NOT been measured, kept only because nothing has
  tested it.
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

### THE SNIPER RIFLE — a combo counter and a scope

**Reference:** [`Sniper Rifle`](https://wiki.warframe.com/w/Sniper_Rifle)
(§Shot Combo Counter, §Zoom Buffs), cached at `vendor/wiki/sniper_rifle.wiki`.
Both mechanics belong to the WEAPON rather than the build, both are paid only
while scoped in, and neither is a stat on the panel — which is why they are
stated on the weapon's card.

**The Shot Combo Counter.** Every sniper rifle has a *Minimum Combo*, the number
of landing hits before the counter pays anything:

> "Each Sniper Rifle requires a minimum number of shots, referred to as
> *Minimum Combo*, before the Shot Combo Counter activates, starting with a
> damage bonus of 1.5x. Another 0.5x damage is added to the counter each time
> the Shot Combo Counter reaches a number of hits three times the amount needed
> for the previous damage bonus milestone."

    threshold(k) = min x 3^k        multiplier(k) = 1.5 + 0.5k        (k >= 0)

Below `min` the multiplier is 1.0. `SniperCombo::multiplier` WALKS that ladder
rather than evaluating `1.5 + 0.5*floor(log3(hits/min))`: `log3` of an exact
power of three is not exactly an integer in binary, and the floor lands one
short wherever the division rounds down — which is every tier boundary, the
only place the answer changes.

The Vectis Prime page's own table diverges from the page's own formula at tier
7 (it prints 3675 and 11025 where `5 x 3^k` gives 3645 and 10935). The formula
is implemented; the divergence is recorded in
`weapons_data::sniper_tests::the_combo_ladder_is_the_wikis` and is unreachable
in any fight this sim runs.

**What builds it.** One per LANDING hit — *"weapons with Multishot will count
each successful hit from the same shot as multiple shot instances"*, and per
enemy under Punch Through or ricochet. *"Area-of-effect and damage over time do
not affect the Shot Combo Counter"*, so only the direct part of a shot counts,
while the multiplier applies to the whole shot (the wiki calls it a bonus to
"total damage"; the Vectis Prime page's column header calls it a "Total Base
Damage Multiplier"). It is read BEFORE the hit is counted — the multiplier a
shot pays is the one that was under the reticle when it was fired.

**What takes it.** *"Reduced by 1 after a short period of time that no
successful hits have been made, or if the player misses a shot. All sniper
rifles have a 2 second combo duration, with the exception of the Lanka, which
has a 6 second combo duration."* Decay is by ONE and never a reset, which is
why a sniper that keeps firing never loses it. The MISS half cannot be modelled
here — this arena has no distance and every shot lands (docs/UNMODELLED.md) —
so the counter runs slightly generous, and each sniper says so on its card.

**Where the gate is.** *"Building combo and benefiting from its multiplier
requires being scoped in."* `loadout::resolve` empties `sniper_combo` when the
Tenno is not aiming, and it is the only place that decides — so the simulator,
the optimizer and the board's no-aim ruler all agree without any of them
knowing what a sniper is. It is also the one mechanic where that ruler changes
what a weapon HAS rather than what it hits.

**In an Incarnon cycle** the counter is the base form's. `DummyParams` for a
cycle is built from the INCARNON panel with the base form hung off
`cycle.base_form`, so the loop reads the SPEC from whichever form declares one
(the count survives the transform) and reads whether a hit counts and pays off
the ACTIVE form. The Vectis Incarnon forms declare no combo: nothing published
says whether it survives, and the page's only remark on zoom in that form is
that you must unzoom to enter it. That is on their cards.

**The scope.** Each zoom level carries a buff — crit chance, critical
multiplier or headshot damage depending on the weapon — and *"these zoom buffs,
which are intrinsic to the weapon and cannot be modified, generally stack
additively with similar buffs from mods"*. The Vectis Incarnon Genesis table
confirms the bracket from the other side, calling Sharpshooter's +25% headshot
damage "additive with Target Acquired, Vectis's Scope bonus, and similar
bonuses". Only the headshot kind is declared (`ScopeSpec`), because it is the
only kind the roster's snipers grant; the Lanka's and Komorex's are named by
the same section as exceptions and get their own field when either arrives.

The arena has no field of view and no distance, so nothing is traded for
magnification and the top zoom level is not a choice — the scope always sits
there while aiming, which is on the card.

**Roster:** Vectis (min 1, 4.5x / +50%), Vectis Prime (min 5, 6.0x / +60%).

**Status:** implemented and unit-tested against the wiki. NOT yet confirmed by
an in-game measurement — see docs/MEASUREMENTS.md.

### EXTRA HITS — a second instance, not a multiplier

**Reference:** [`Extra_Hit`](https://wiki.warframe.com/w/Extra_Hit). Read that
page before touching anything in this section; it is the only place the rule is
written down in general form, and every ability and arcane that grants one
inherits from it.

> **Extra Hit** is a unique buff that adds an additional hit to the target,
> dealing a percentage of the original damage value and independently rolling
> Status Effects. An Extra Hit may have different damage type distribution than
> the original hit; this is unlike Multishot, which always inherits the original
> hit's stats.

```
Extra Hit Damage = Weapon Hit Damage × Extra Hit Percentage
                   × (1 + Faction Damage Bonuses)

Weapon Hit Damage = Base Damage
                    × [ 1 + Elemental Bonuses
                        + Unmodded Impact Distribution   × Impact Bonuses
                        + Unmodded Puncture Distribution × Puncture Bonuses
                        + Unmodded Slash Distribution    × Slash Bonuses ]
                    × (1 + Damage Bonuses)
                    × (1 + Faction Damage Bonuses)
                    × Additional Multipliers
```

`Additional Multipliers` are the crit multiplier on a critical hit, the enemy
body-part multiplier, and "external weapon buffs that do not fit into other
bonus categories".

**Everything surprising about it falls out of `Weapon Hit Damage` already
containing a faction layer.** The bonus appears twice in the pair of formulas,
so an extra hit is worth `pct × (1 + faction)` of the hit rather than `pct` —
26% on the card is ×0.40 of the hit at a Primed Bane's +55%. The engine never
writes a 2: it multiplies the finished instance and applies
`faction_at_time` once more, and the layer count follows from what triggered it.

**And the body part twice, which the EN formula does not show and DE's own CN
card states outright** — 同理，弱点倍率也会被计算两次. A 3× headshot is 3× on the
hit and 3× again on the extra hit off it, so an extra-hit ability is worth
strictly more to a headshot build than any multiplier that merely scales the
hit. `dummy::fire_extra_hits` takes that as `part_again` from the caller,
because only the caller knows whether its instance struck a body part at all.

**The percentage's bracket is the BASE ATTACK's, not the triggering
instance's.** `Unmodded Impact Distribution` is the phrase that says so, and the
CN card works it out for the Heliocor: a slam whose own damage is 100% Impact
still scales its extra hit by `1 + 0.6 + 1.2×0.85 + 1.2×0.1`, the IPS shares of
the ORDINARY attack. `DummyParams::extra_hit_bracket` is that number, and each
call site passes the ratio between it and its own instance's bracket — exactly
1 on a direct hit, and the whole correction on an explosion or a detonation.

**What triggers one, and what does not.** A WEAPON damage instance does: the
direct hit, each multishot pellet separately, the explosion, "most non-standard
weapon hits ... including Acid Shells and Concealed Explosives". A status
payload does NOT — with one exception, filed by the wiki under Bugs:

> Only Xata's Whisper will be triggered by blast Detonations, no other extra hit
> will.

That detonation case is where three layers of faction stack up (the detonation
is already at `faction_at(f, DEPTH_PROC)`) and where the extra hit takes a full
elemental bracket the detonation itself is denied. **Measured** — MEASUREMENTS
**M40** decodes a supplied capture line by line.

Two more rules the engine follows from the same page:

- *"If a hit that would trigger an Extra Hit kills the enemy, the Extra Hit will
  not be triggered."* The call sits after the kill check, so this costs a line
  of placement rather than a condition.
- *"Damage over Time status effects created by an Extra Hit will use the Extra
  Hit Damage as Modded Base Damage"*, and they take faction a third time. No
  extra hit in the roster grants a DAMAGING element yet (Xata's is Void, whose
  proc is a Bullet Attractor), so this is a claim nothing collects on — it is
  written as `InstanceScale { mb_live: raw, .. }` and `DEPTH_DERIVED_PROC` so
  that the first one that does is right without an edit.

**Void's status is worth exactly one Condition Overload stack.** The Bullet
Attractor deals no damage and the arena has nobody to redirect fire from — but
Void is on Condition Overload's own list of counting procs, so it is tracked
like Radiation's Confusion: a presence with no payload. `DebuffState::attractor`.

**Sources of extra hits, from the wiki's table** (none but Xata's Whisper is
implemented; the ability layer is what would carry the rest): Toxic Lash
20–30% Toxin, Xata's Whisper 17–26% Void, Silken Stride 10–40% Toxin, Resupply
10–25% selectable, Uriel's Demonium Rune 30% Heat; Reconifex's Active Reload
25% Heat; Melee Duplicate 100%; Primary Debilitate's 0-damage status application
— which this engine already models, from the other end, as MEASUREMENTS M33/M37.

**Source:** wiki `Extra_Hit` + `Xata's Whisper` (EN and CN, which disagree in
detail and are reconciled in M40) + a supplied player capture. **Status:**
implemented and unit-tested; the Blast chain is measured, the lingering-field
trigger is an open question stated in M40.

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
form. The Incarnon form has **no row**, and that absence is a POSITIVE statement
rather than a gap: the table enumerates exceptions, so an attack it does not
name is ordinary — **Adding at +100%**, joining the base-damage bucket like
Hornet Strike (confirmed, user 2026-07-30). Inferring the class from a sibling
form would get it exactly backwards. Both halves of the Incarnon form's CO
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
settle it:
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

> **A fight has TWO actors.** This section is about the target because the
> target is the one that takes damage — but it is no longer the only one on the
> field. `engine::arena::Arena` is the engagement: a Tenno, a `TargetParams`
> with its hitboxes, and how long they are at it. The web api and the optimizer
> each build one from the same scenario and hand it to the same constructor,
> which is what makes a search's winner scored under the fight the replay runs.
>
> The **Tenno** (`data/tenno/`, `engine::tenno_data`) is shaped like a
> WARFRAME: health, shield, overguard, armor, energy, sprint — the wiki's own
> `Module:Warframes/data` field names, so a transcribed frame fills these in
> rather than needing a second vocabulary — plus a `state` block for what the
> player is DOING. `data/tenno/default.yaml` is the NEUTRAL player: aiming, no
> frame chosen, nothing running, energy full.
>
> **Player STATE gates mods.** One wrapper covers all of it:
> `condition: while_aiming | while_invisible | while_airborne` in a mod file
> resolves to `ModEffect::WhileTenno(TennoCondition, …)`, which
> `loadout::resolve_for` asks of the fight's Tenno. All of them live there,
> aiming included — one home for one kind of fact (user, 2026-08-02). A gated
> effect whose
> condition is false is absent from the static buckets AND from the emergent
> specs, so the buff never arms; the panel still lists the row, tagged with the
> condition, rather than folding a number in or hiding the mod.
>
> **Player STATS scale arcanes.** `kind: tenno_scaled` reads one Warframe stat:
> `per_unit × (stat − above)`, capped at the rank's value, optionally gated on
> how full the energy pool is. Two arcanes use it, and both were `unmodeled`
> until there was a player to ask:
>
> | arcane | reads | pays |
> |---|---|---|
> | Primary Bulwark | `armor` | +1% base damage per point past 1,000, cap +500% |
> | Primary Overcharge | `energy` | 35% of max energy as multishot at ≥90% energy, cap +350% |
>
> Both resolve to a passive one-stack buff on the bucket their family already
> feeds, so there is no new damage path to get wrong — checked by construction:
> Primary Overcharge at 257 energy and Split Chamber at rank 5 give the
> identical DPS, because +90% multishot is +90% multishot. **The BRACKET each
> joins is assumed, not measured** — see MEASUREMENTS M26.
>
> The neutral Tenno has no frame, so both contribute nothing until a scenario
> says what is behind the gun. That is the honest answer to "no frame chosen",
> not a zero invented to dodge the question.
>
> What still waits on a player who can be SHOT AT, all recorded as unmodelled:
> Secondary Fortifier's Overguard gain, Secondary Surge's remaining-energy
> scaling, the Warframe abilities in §6's GunCO omission list, and self-stagger.
> `health`/`shield` are placeholders at 1; no frame has 1 health, and nothing
> may treat the value as meaningful.
>
> **The third actor: the COMPANION.** A sentinel weapon does not belong to the
> Tenno — it belongs to a companion standing beside them, and that distinction
> is load-bearing for the Galvanized mods. Their trigger is the TENNO's: the
> on-kill roll comes from the Tenno's own weapons. The buff it grants then
> applies to the Tenno **and** the companion (user, 2026-07-31).
>
> So `StackPolicy::BaseOnly` is not "a companion is excluded". It is "this
> arena fires ONE weapon, so when that weapon is the companion's there is
> nothing on the field to generate the stacks, and only the unconditional base
> is honest". Measured: Galvanized Chamber on Verglas Prime resolves to
> multishot x1.8 (base +80% alone); the same mod on the Torid gives x3.3
> (+80% and five on-kill stacks of +30%).
>
> That answer becomes WRONG the day a Tenno weapon and a companion weapon are
> simulated side by side — the companion would then receive the Tenno's stacks
> and the base-only rule would be understating it. The companion has no entity
> of its own yet; when it gets one it belongs next to `data/tenno/`, and this
> policy is the first thing that has to change.

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

*Engine (2026-08-03): modeled — `engine::factions_data` loads the table,
`EnemySpec::target_params` resolves the one column (`FactionDamageOverride ??
Faction`) plus the Overguard column onto `TargetParams::type_mods`, and
`TargetState::apply` scales each component by the column the POOL it lands in
reads. A hit's per-type shape travels as `TypeShares` — the same value that
answers Toxin's shield bypass. The table's **fifteen columns are the whole
system** (user, 2026-08-03), so a faction it does not name — Stalker, Unknown,
the wildlife — resolves to neutral and takes every type as written. Cinematic
(bleed) is exempt everywhere, as the type's own definition says. Not modeled:
the Object pool (no object target exists yet).*

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
  break, Electricity burst = 3%/stack of **the broken pool's** max (max
  30%), "**When Shield or Overguard breaks**, deal Electricity Damage for
  3% of enemy's Max Shield or Overguard per stack with a forced Electricity
  Status Effect" — **or Overguard** is load-bearing, and the engine takes the
  pool that BROKE: the roster's only enemy is a Thrax Centurion, which carries
  15.5 M Overguard and no shield at all, so reading the line as shields-only
  deletes the burst entirely.
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

### Fire cadence on a CHARGE weapon — bows have their own formula

The shot interval is normally `1 / modded fire rate`. A charge weapon pays a
DRAW instead, and the wiki (`Fire Rate`) states two different formulas for
what that costs — VERBATIM, with the exception spelled out in the second:

```
Effective Fire Rate = 1 / (Modded Charge Time + Modded Reload Time)   ← BOWS
Effective Fire Rate = 1 / (Modded Charge Time + 1 / Modded Fire Rate) ← "charge
    weapons with the exception of bows, Epitaph, and Lanka"
```

So **a bow's cadence contains no fire-rate term at all**: draw + nock. What a
fire-rate bonus does instead is shorten the draw — *"Charge Time = Base Charge
Time / (1 + Mod Bonus)"* — and on a bow the bonus is doubled first, because
every fire-rate mod card prints "(x2 for Bows)". Cernos Prime unmodded:
`0.5 + 0.65 = 1.15 s` a shot (0.87/s), where the fire-rate stat alone would
have said 1.0/s; with Shred (+30% → +60%) the draw is `0.5 / 1.6 = 0.31 s` and
the cycle 0.96 s.

The fire-rate STAT is still the stat — it is what fire-rate gates read
(Hemorrhage's below-2.5 doubling), which is why the engine keeps both:
`base_fire_rate` and `charge_seconds` (`engine::loadout::WeaponBase`).

A tapped bow shot pays no draw, so the nock alone paces it (`charge_seconds:
0.0` → 1.54 shots/s on Cernos Prime). That is the bow formula taken at its
word rather than a measurement — **MEASUREMENTS M16**. The engine does NOT yet
implement the second formula: the roster has no non-bow charge weapon.

### Fire rate that MOVES while the trigger is held — the spool

Six weapons in the roster do not fire at one rate, and five of them go UP.
VERBATIM (wiki `Phenmor`, of the Incarnon form):

> Fire rate decreases from **100%** to **60%** over **51** shots as the trigger
> is held, reducing its effectiveness from prolonged periods of firing.
>
> Spool resets once the player stops firing, encouraging brief bursts of fire
> rather than sustained fire.

It is the **opposite of the beam ramp** and the two never meet: a continuous
weapon climbs to full damage and stays there, this one only ever falls, and it
falls in SHOTS rather than in seconds. The fall is linear — the page gives the
two ends and the count and nothing in between — so after `n` held shots the
cadence runs at `1 − 0.4 · min(n, 51)/51` of the live rate.

Three consequences, and the first is why this is not a footnote:

1. **51 shots is 3.8 s of a 408-round magazine.** A held Incarnon dump spends
   87% of its rounds at the floor, so reading the printed 13.33 rounds/s flat
   overstates the form's sustained output by **51%** — measured here at 9 275
   DPS against 6 140 on the same build once the spool was implemented.
2. **It scales with the live rate, not the listed one.** Rapid Wrath's +20% is
   worth +20% at the floor as well as at the ceiling; a fire-rate mod raises
   both ends and never buys its way out of the spool.
3. **The reset is derived, not declared.** The sim resets the count whenever a
   shot lands later than the moment the previous one made it due — which is
   what releasing the trigger IS, and what every reload, transform, dry
   magazine and dry-reserve stall already looks like. Clearing it branch by
   branch would have missed the plain reload path, which does not `continue`:
   it falls through and fires in the same iteration.

The stat itself is untouched, exactly as on a charge weapon: `fire_rate` stays
what the panel prints and what fire-rate gates read, and the spool multiplies
the CADENCE. Data: `attack.sustained_fire_rate: { floor, over_shots }`.

#### The five that climb

The same field pointed the other way. Each page states its spool TWICE — a
percentage per shot and a count of shots to optimal — and on all five the two
reconcile exactly, which is the strongest check available without a measurement:

| weapon | starts at | span | full from | the page's % per shot |
| --- | --- | --- | --- | --- |
| Gorgon | 20% | 7.5 | shot 9 | 10.667% |
| Gorgon Wraith | 20% | 5 | shot 6 | 16% |
| Prisma Gorgon | 20% | 6 | shot 7 | 13.33% |
| Soma | 25% | 5 | shot 6 | 15% |
| Soma Prime | 25% | 2.5 | shot 4 | 30% |

`over_shots` carries the span because it is the exact half — the Gorgon's
"10.667% per shot" IS 0.8/7.5 — and the test re-derives BOTH published figures
from it, so a mistyped span would have to be wrong in a way that keeps two
independent sentences true.

A climb costs TIME rather than rounds, and it is paid once per magazine: the
Gorgon's 90 rounds take 7.99 s instead of 7.20 s, +11%. None of their Incarnon
forms spool — those are Auto Charge, and their pages say so.

**One rule for both directions, including the reset.** The Gorgon family's pages
say *"Burst firing maintains spool-up"* while the Phenmor's says the spool
*"resets once the player stops firing"*, and it would be easy to read that as two
mechanics needing two flags. It is not worth one: this sim holds the trigger, so
the only pauses in it are the reloads the weapon forces, and no play pattern is
invented by treating those the same way everywhere (owner, 2026-08-10 — taken
to the limit, exempting a spool means firing one round at a time).

**What is not modelled is the play pattern that dodges it** — on the faller, a
player who taps rather than holds; on the risers, one whose pauses are short
enough to keep the spool through a reload. Both are on the weapon's card
(`unmodeled:`) and in docs/UNMODELLED.md, beside the reload-interruption ruling
they are the same shape as.

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

## 10. WHAT A STATUS-CHANCE CARD IS WORTH — the estimate, and the half everybody drops

Asked often enough to write down (owner, 2026-08-16, after a player asked why
the Burston Prime's board build takes **Galvanized Aptitude** over Serration or
Heavy Caliber). The naive comparison puts all three in the base-damage bucket
and stops there, which under-counts the status card by about half.

### The three cards, in the one bucket they share

| card | contribution to the base-damage bucket |
| --- | --- |
| Serration | flat **+1.65** |
| Heavy Caliber | flat **+1.65** (its accuracy penalty is free at contact) |
| Galvanized Aptitude | `0.4 × stacks × status TYPES` |

Serration and Heavy Caliber are the SAME CARD in a contact-range fight —
measured 144.616 against 144.612, a difference of 0.003%. So the argument only
ever has one opponent.

The crossover is arithmetic: `0.8 × N > 1.65` at **N > 2.06**. Measured on the
Burston Prime, same build but for the one slot:

| build | mean status TYPES | Aptitude | Serration | lead |
| --- | --- | --- | --- | --- |
| three elemental mods | 2.71 | 176.47 | 144.62 | **+22.0%** |
| three flat-stat mods | 1.59 | 36.74 | 36.86 | **−0.3%** |

Below two types the doubters are right. The board's build carries status
BECAUSE that is what makes the card worth its slot — and the archetype gap
(176 against 37) dwarfs the slot argument entirely.

### …AND THE BUCKET IS ONLY HALF OF IT

The half that gets dropped: **status chance is a rate, and the rate feeds
DIRECT-damage multipliers, not just DoT.** Viral multiplies HEALTH damage,
Magnetic multiplies OVERGUARD and SHIELD damage, Heat strips armour. All three
are `× (2 + 0.25 × (stacks − 1))`-shaped ladders on the TARGET, and how fast
they fill is exactly what a status-chance card buys.

Measured, the same two builds:

| | Aptitude | Serration |
| --- | --- | --- |
| status chance | 0.54 | 0.30 |
| procs / s | 33.5 | 18.9 |
| Viral stacks (mean) | 7.11 | 5.82 |
| Magnetic stacks (mean) | 2.41 | 1.65 |

### The estimate

Multiply four ratios. Each is a measurable intermediate, so the estimate can be
checked term by term rather than believed whole:

```
gain ≈  (1 + B + CO_new) / (1 + B + CO_old)     bucket, on the DIRECT share only
      × Viral(n_new)    / Viral(n_old)          health-damage ladder
      × Magnetic(n_new) / Magnetic(n_old)       overguard/shield ladder
      × Armour(strip_new) / Armour(strip_old)   Heat
```

`B` is whatever else is already in the base-damage bucket — on this build,
nothing: not one of its other seven mods is a base-damage mod, which is why the
estimate lands so cleanly.

Worked, for the Burston Prime board build:

```
CO       (1 + 0.4×1.762×2.71) / (1 + 1.65) = 1.096   ← ×0.979, the direct share
Viral    (2 + 0.25×6.11) / (2 + 0.25×4.82) = 1.101
Magnetic (2 + 0.25×1.41) / (2 + 0.25×0.65) = 1.088
                                    product = 1.199

measured 176.47 / 144.62            = 1.220     residual 1.8%
```

**+9.6% from the CO bucket and +9.4% from the ladders the status RATE fills.**
Half the card's value is invisible to an accounting that only looks at
"+40% per status type".

### …and it is not eaten in full, on three separate channels

Worth stating because each is a real ceiling nobody reaches:

- **STACK UPTIME.** The bonus is ON KILL and caps at 2. Measured: 85.5% of the
  fight at 2/2, 5.2% at 1, 9.3% at 0 — an effective 1.76 of 2, or 88%.
- **TYPE COUNT.** 2.71 live on average against the six the build can produce.
  The CO term is linear in this, so the shortfall is direct.
- **NOT ALL DAMAGE TAKES IT.** CO is a DIRECT-hit bonus. The Burston's Incarnon
  radial takes it at a base fraction of 24% (docs/CATALOGS.md) and status DoTs
  take none, so 2.1% of this build's output is outside the bracket. On a build
  whose damage is mostly DoT or AoE that share is the whole argument.

### Reproducing this

Every number above comes from `/api/simulate` with `replay: true` on the board's
own row: `damage_sources` for the shares, `dstacks` for the ladders, `stacks`
for the arcane's own uptime, and `score_mean` for the totals. Nothing here is
hand-derived except the two published ladder formulas.

## 10b. THE ORIGINAL BASE — what a GunCO term computes on

A weapon has an ORIGINAL BASE, and some things add to it while others only add
to what its panel prints. `WeaponBase::co_base` is the first, `base_vector` is
the second, and the GunCO term reads the first (owner, 2026-08-16).

```
gunco_bonus = rate x stacks x status_types x (co_base / panel)
```

**IT IS AN ABSOLUTE, NOT A FRACTION.** The engine held `co_base_fraction`, a
ratio recomputed as `original / evolved` wherever something raised the panel,
and the ratio was the wrong noun: it described the ARITHMETIC of one loadout
instead of the FACT underneath. Two things it could not say:

* **TWO SOURCES THAT DISAGREE.** A build carrying a flat-damage perk that feeds
  the term and one that does not has no single ratio. The catalog says the
  Despair is exactly that pair — Stalker's Vendetta excluded, Fatal Affliction
  not — and it only ever worked because they are tier-mates and you pick one.
* **A MECHANIC THAT DOES NOT EXIST YET.** Anything raising base damage now
  states whether it feeds this, and the GunCO code does not change. Under the
  ratio, a new source meant a new site recomputing `original / evolved`, which
  is the shape that kept producing the same bug.

`add_flat_base_damage(flat, into_co)` takes both amounts rather than a flag, so
the disagreeing case is expressible at the one site that folds base damage in.

**WHO FEEDS IT TODAY**

| source | panel | co_base |
|---|---|---|
| the weapon's own base | yes | yes |
| an Incarnon evolution, `Adding` entry | yes | **no** (M49, M50) |
| an Incarnon evolution, `Multiplying` entry | yes | **yes** (M51) |
| a perk the player's state gates | yes | yes — preserved, never measured |
| a base-damage MOD (Serration, Hornet Strike) | no, it is a multiplier | no |
| an explosion's own base | its own | its own, and it never grows |

The last row is behaviour preserved rather than chosen: the old code excluded a
radial's flat add from its CO base unconditionally while the direct hit's
followed the perk's flag, so the two halves of one weapon could disagree about
the same +42. They agree for every `Adding` entry now, and differ only on a
`Multiplying` entry with an EXPLOSION, which M51 did not reach: the second part
it measured is the Torid's lingering FIELD, which takes the weapon's own
fraction (`gunco_bucket` is handed `ap.co_base_fraction` for a field and the
radial's own for an explosion). What M51 does say about a second part is that
the +51 fed the cloud's term in full at its own evolved base of 91 — the same
answer as the direct hit's 151, on a different number, which is the reading's
sharpest half.

A weapon may also DECLARE a starting value below its own base (`co_base_fraction`
in the yaml, 0.5 on a bow's charged entry). That is the only place a fraction is
still written, because it is how the catalog prints it.

## 11. THE ARENA'S GEOMETRY — where a shot leaves, and what counts as a hit

The fight is two circles on a floor (`engine::space`). Everything below falls
out of three facts, and none of it is a special case.

**A BODY HAS A RADIUS: 0.2 m**, measured — walking into an enemy stops at 0.4 m
centre to centre, and two bodies of the same size touching at 0.4 m makes each
of them 0.2 (MEASUREMENTS M47). One number, and nothing derives from it.

**A SHOT LEAVES THE MUZZLE, not the centre** (owner, 2026-08-16). The muzzle is
a point on the shooter's own circumference facing what they are aiming at. The
facing is DERIVED rather than stored — you are looking at what you are shooting
at — so aiming at a second target turns the shooter and there is no third piece
of state to keep in sync.

**HITTING THE CIRCLE IS A HIT**, which makes the question ray-versus-circle and
nothing more. A pellet that leaves θ degrees off the aim line passes

```
miss = range · sin(θ)           hit ⟺ miss ≤ r
range = |player − target| − r                       (muzzle to the CENTRE)
```

from the target's centre. `range` is NOT how far the shot flies — the
perpendicular is dropped from the circle's centre, so that is the leg the
formula needs. It was called `travel` for a few hours and the name was worth an
inconsistency the owner caught immediately: a bullet vanishes at the SURFACE it
hits, so what it flies is the gap below, one radius shorter, and zero at
contact. It was `|player − target| · tan(θ)` until 2026-08-16,
which was wrong twice — measured from the centre rather than from the muzzle,
and `tan` rather than `sin`, so a wide cone's deviation ran off toward infinity
instead of being bounded by the distance it had to cover.

**CONTACT IS THEN UNMISSABLE, AT ANY CONE WIDTH**, and that is a property of the
geometry rather than a rule written into it: at contact the muzzle sits one
radius from the target's centre, so the closest approach is `r · sin(θ) ≤ r` for
every θ. Under the old formula a 60 degree cone dropped more than half its
pellets pressed against an enemy, which nothing in the game does — the Mandonel's
uncharged form and the Cryotra were both being simulated that way.
`space::no_weapon_in_the_roster_can_miss_at_contact` asserts it over the whole
roster, so a weapon added tomorrow with a wider cone is covered.

**A DISTANCE IS THE GAP, SURFACE TO SURFACE — zero at contact.** That is what
"how far apart are we" means once bodies have a size, it is what the arena shows
and what its quick sets set, and it is what point blank has always meant to a
player. The 0.4 m between the two centres is the model's business and nobody
should have to subtract it.

**THE GAP IS ALSO THE FLIGHT**, and that is why damage falloff reads it with
nothing to reconcile. A bullet vanishes when it reaches the target's surface
rather than carrying on to its centre, so muzzle-to-surface is at once what a
player calls the distance, what the projectile covers, and the key a published
window is quoted in — one quantity wearing three hats rather than three numbers
that have to be argued into agreement. Exactly, for a shot down the middle; a
grazing one lands further around the circle and covers up to one radius more,
which is under the resolution of any window DE prints.

It also gives CONTACT-CANNOT-MISS a one-step proof: a flight of zero leaves a
cone no distance to widen over. The ray-circle test reaches the same answer
from the other side, which is the check that the two halves agree.

**THE EXPLOSION IS THE THIRD DISTANCE** and is neither: it reads from its own
epicentre, wherever the pellet actually crossed. Each distance is chosen by
what is asking, which is why the model is not "one distance for everything".

**THE CONE ITSELF HAS TWO BRACKETS, and a mod picks one.** Accuracy DIVIDES:
the wiki's `Accuracy` page says *"Bonuses that increase accuracy decrease the
deviation (spread) of a shot"* and accuracy is `100 / spread`, so a +30% card
divides the angle by 1.3 rather than subtracting from it. ADDED SPREAD does not
go through that divisor. Split Flights is the roster's first — its rank ladder
is published as a spread table in DEGREES (+0.3 → +1.8) beside a card that words
it as accuracy, and its page settles the bracket outright: *"Added spread is not
affected by bonuses that increase accuracy, such as Twitch or Guided
Ordnance."* So `ModEffect::AddedSpread` lands after the division and a Twitch on
the same build narrows the weapon's own cone while leaving this alone. Filing it
in the accuracy bucket would have been wrong twice — scaling instead of adding,
and clawed back by exactly the mods the source says cannot touch it.

## 12. BEAM CHAINING — the mechanic a second target turns on

Nothing here is modelled yet: the arena holds one target, so every clause below
resolves to zero. It is written down because the Torid Incarnon carries all
THREE ways a shot reaches a second body at once, which makes it the weapon a
multi-target model should be built against (owner, 2026-08-16).

### The three paths to a second target

| separation | what reaches it | strength |
| --- | --- | --- |
| within the damage radius (2.3 m on the Torid, from the point of impact) | the beam's own instance — **not a second one**: *"a target that is directly struck by the beam is still only hit once"* | full |
| within the chain range (7 m) of a target that was hit | a chain hop | 75% of the hop before it |
| beyond both | nothing | — |

### Seeds and paths

**EVERY TARGET IN THE DAMAGE RADIUS IS A SEED.** Verbatim: *"The beam will
chain independently to 5 additional enemies starting from EACH target hit by
the initial damage radius. Each chain chooses targets independently, and an
enemy can be struck by multiple chains."*

**A PATH VISITS NOBODY TWICE** (owner, 2026-08-16). "Struck by multiple chains"
means the repeats come from DISTINCT paths — one per seed — rather than from a
path looping back on itself. That reading is what makes the arithmetic
well-defined, and it separates from the alternative at three targets:

| targets, all mutually in range | seeds + paths | "each seed links every other at 75%" |
| --- | --- | --- |
| 2 | 100% + 0.75 = **175%** | 175% — the same, so a two-enemy test cannot tell them apart |
| 3 | 100% + 0.75 + 0.75² = **231%** | 100% + 2 x 0.75 = 250% |

The 75% COMPOUNDS ALONG A PATH, so the difference grows with the crowd.

### Settled by the owner, 2026-08-17

* **THE NEXT HOP IS THE NEAREST VIABLE TARGET.**
* **NO LINE OF SIGHT** — a hop is a distance test and nothing else.
* **FIRESTORM DOES NOT WIDEN THE CHAIN RANGE.** It scales the damage radius and
  that is all. Two wiki pages disagreed and the weapon's own page is the one
  followed.
* **ONLY THE DIRECTLY STRUCK TARGET MAY BE HEADSHOT.** Everything the splash
  catches and everything a chain reaches lands on the body.
* **A HOP IS A BEAM WITH A SMALLER BASE DAMAGE** — nothing else about it
  changes. Its crit and status are rolled at full chance, and the procs it
  leaves scale with its own damage the way any hit's do, so no clause of the
  damage pipeline needs to know a chain exists.

`engine::chain` is the model. It answers the geometric half — which bodies take
an instance, at what share, whether multishot reaches it, whether it may
headshot — and hands each instance to the ordinary pipeline.

### What that adds up to

Take the owner's fixture: a **3 x 3 formation at 3 m**, aimed at the front
row's middle body, because the centre of a formation is behind it and cannot be
aimed at.

| | seeds | instances | total damage | headshot-eligible |
| --- | --- | --- | --- | --- |
| one enemy | 1 | 1 | 1.00 | 100% |
| no Firestorm | 1 | 6 | 3.29 | 30.4% |
| **Primed Firestorm** | **4** | **24** | **13.15** | **7.6%** |

**A PATH'S WHOLE OUTPUT IS A CONSTANT** once the formation is dense enough that
five hops never run out of bodies: `1 + f + … + f⁵` = **3.2881** for the Torid.
So the total is `seeds x 3.2881`, and neither the aim point nor the tie-breaking
moves it — ties redistribute and never add. `engine::chain` asserts that over
500 random tie-breaks.

**A RADIUS MOD BUYS SEEDS, and that is the whole of what it buys.** Primed
Firestorm's +44% takes the radius from 2.3 m to 3.31 m, which at 3 m spacing
reaches the three neighbours and not the two diagonals at 4.24 m: one seed
becomes four, and the shot deals four times as much. Its value is therefore a
STEP FUNCTION of the enemy spacing, with both edges pure geometry:

| spacing | seeds bare | seeds primed | worth |
| --- | --- | --- | --- |
| 1.5 m | 6 | 7 | 1.2x — the bare radius already covers the crowd |
| 2.0–2.3 m | 4 | 6 | 1.5x |
| 2.34 m | 1 | 6 | **6.0x** — the peak, at `3.31 / √2` |
| 2.5–3.31 m | 1 | 4 | 4.0x |
| 3.5 m and out | 1 | 1 | 1.0x — nothing is in reach either way |

**AND THE HEADSHOT CLAUSE REORDERS BUILDS.** One instance of twenty-four may
headshot and it carries 7.6% of the damage, so a build leaning on a head
multiplier keeps almost none of it in a crowd — while a status build collects
all 24 rolls at full chance. The same weapon, the same formation, and the two
builds rank in opposite orders from how they rank against one target. That
reordering is the reason a multi-target model is worth building rather than
approximating.

### A BLAST MEETS A BODY AT ITS NEAREST SURFACE

Three rulings, and they are one idea: a body is a CIRCLE, so a blast touches it
before it reaches its centre (owner, 2026-08-17). `engine::space` owns all three.

| | rule |
| --- | --- |
| **where it goes off** | on the body's surface FACING the shooter, one radius nearer than its centre — a round detonates where it touches (`detonation_point`) |
| **who is caught** | ANY part of a body touching the sphere is enough, so the reach is `radius + body radius` (`caught_by_blast`) |
| **how far it fell off** | to the body's NEAREST point, because a body across a gradient takes the best number on it and nothing in the game falls off the other way (`blast_reach`) |

The third is a rule rather than an average on purpose: an average would need a
shape integral to defend, and this needs only *"the round found the best part of
it"*, which is what a blast does.

**IT MOVES NO EXISTING NUMBER** (`one_fight`: every answer unchanged), because
at contact a pellet cannot miss, so the reach into a body is zero either way.
It bites at RANGE and in a FORMATION, which is where it was always going to.

### AN EXPLOSION HAS ONE EPICENTRE

The rulings above say where a blast goes off *on a body*. They did not say where
it goes off when the pellet **hit nothing**, and until 2026-08-19 the engine
quietly answered that twice.

- The **aimed** body read its falloff from how far the pellet passed it —
  correct, and the reason the miss distance is computed at all.
- Every **other** body read its distance from the aimed body's SURFACE, whatever
  the pellet did. So the crowd was blasted as though every shot landed perfectly.

Measured on the wire with an Akarius (7.2 m radius), a target at 10 m and a
bystander 2 m behind it: pointing **nine metres wide** dropped the aimed body
from 10105 to 3972 and left the bystander on **7120, against 7115 on target**. A
player reported it; no check we had could see it, because none existed for the
case — with ONE body the two epicentres coincide.

`space::detonation_of_miss` is the fix and it invents nothing: the model already
draws how far the pellet deviated and, when the weapon points off the body, which
way around the cone it went. Those two were being collapsed into a single
scalar. Kept apart they are a **point on the floor plus a height** — the cone's
cross-section is a disc, only its in-floor part moves the epicentre across the
arena, and the rest is how far over or under the shot went, which is a real
distance to everyone standing on the floor.

**IT CANNOT MOVE THE AIMED BODY'S NUMBER**, by construction rather than by care:
with the body at `O + a·û` and the pellet at `O + b·cos(2πφ)·û + b·sin(2πφ)·v̂`,
the distance between them is `√(a² + b² − 2ab·cos 2πφ)`, which is
`miss_distance_off_axis` exactly. That identity is asserted at `1e-9` wherever
the weapon points at the body — which is every fight the engine ran before aim
became a place you choose, and every board ruler, since none sets an aim point.
The whole test suite passed unchanged.

TWO SMALLER FAULTS FELL OUT WITH IT. The stage was skipped entirely when the
blast could not reach the **aimed** body, which threw the explosion away for the
bodies standing where it actually landed; it now asks whether it reaches anybody.
And that gate compared `aim_offset` where the damage compares
`blast_reach(aim_offset)`, so it fired one body radius early.

WHICH WAY the pellet went is now drawn whenever there is a crowd, not only when
the weapon points away — against one body only the magnitude decides anything,
which is why it was gated that way, and a crowd makes the side decide who is in
the blast. It comes off `rng::Draws::blast_dir`, a stream of its own, so adding
the draw shifts no other roll.

### What a radius mod is worth — and the reach is what decides it

`cargo run --release --bin formation_value -- [cols] [rows] [spacing]`. At 3 m:

| radius mod | radius | REACH | seeds | total | vs bare |
| --- | --- | --- | --- | --- | --- |
| nothing | 2.30 m | 2.50 m | 1 | 3.29 | 1.00x |
| Firestorm | 2.85 m | **3.05 m** | 4 | 13.15 | **4.00x** |
| Primed Firestorm | 3.31 m | 3.51 m | 4 | 13.15 | 4.00x |

**THE REACH IS THE NUMBER TO READ, not the radius**, and it corrected a headline
finding written a day earlier: with the radius alone, Firestorm's 2.85 m fell
short of a 3 m neighbour and this table said the mod was worth EXACTLY NOTHING
while its primed twin quadrupled the shot. With the reach it clears 3.00 m by
five centimetres, so both take four seeds — and **Primed Firestorm is worth
nothing OVER the ordinary one at exactly this spacing**. The step is real either
way; it just sits one body-radius further out than it looked.

That is the argument for the tool rather than for a rule of thumb: the answer
turns on centimetres, and it turns on the formation you are actually shooting
into.

### What the engine does with it now

The run loop consumes `chain::resolve`, so a formation takes the damage a chain
spreads into it — through the ORDINARY pipeline, one instance at a time. Each
body computes its own Condition Overload bucket (exact, not approximated: the
bucket is one multiplicative factor of `raw`, so `raw x share x bucket_here /
bucket_there` is the whole correction), its own half-health term off its own
health line, its own mitigation, its own procs and its own death.

**WHERE THE LINE BETWEEN A SOURCE AND A BODY IS** (owner, 2026-08-17): a counter
belongs to whoever it counts on. The pools, the procs, the DoTs and the armour a
hit strips are the BODY's. The buff bar, the Galvanized stacks, the arcane
runtime and the damage-instance number are the SHOOTER's, shared by every body
because one weapon is firing at all of them. That is also the shape a second
TENNO slots into — the loop's player-side locals become one source's, a list of
them, and nothing on the body side changes.

**MULTISHOT SPLITS THE WORK IN TWO PLACES**, which is the wiki's own rule made
structural. Chains launched from the body the beam STRUCK fire inside the pellet
loop, so they fire once per landing pellet — a merged beam's multishot IS its
pellet count. Chains launched from a body the RADIUS caught fire once for the
shot. `chain::Instance::multishot` is the flag that sorts them.

**AND NOBODY IS PROMOTED**, because nobody stays dead: `TargetState::apply`
respawns a body instantly where it stood, so a formation is N streams of targets
rather than N corpses — which is what a room-clear measurement wants and what
the single-target arena has always been. `formation::Formation::retarget` is the
aim policy for the day respawn becomes a setting; it is written, tested, and
called by nothing.

### Four ways a shot reaches a body that is not the aimed one

A weapon may carry more than one. The Torid carries three of them across its
cycle, which is why it is the weapon this was built against.

| mechanism | who it reaches | what it deals |
| --- | --- | --- |
| **the chain** | a path of up to `hops` bodies, one path per seed | `falloff^k` along the path |
| **the explosion** | every body the sphere touches | that body's own falloff, at its nearest point |
| **the lingering cloud** | every body standing in it, every tick | the same, for as long as the cloud lasts |
| **tendrils** | one body each, nearest the RETICLE, inside a cone and a reach | a WHOLE instance — they are extra beams, not a spread |
| **an echo** | every body near the one that was hit | a flat share of THAT hit — Secondary Irradiate's 80% to 180% |

The fourth is the odd one and the Ocucor is its only member. A tendril is not a
share of the shot: *"their base damage equals the primary beam's, and they roll
their own crits and status"*. It is worth EXACTLY NOTHING against one body —
*"tendrils homing in on the main beam's target are only cosmetic"* — so the
weapon's file modelled the COUNT and refused the damage, correctly, for as long
as the arena held one target. Four tendrils on four bodies are four more beams.

**A KILL IS WORTH SOMETHING BEYOND ITSELF**, and that is where a crowd
compounds. Two mechanics read the kill count and both now see the whole
formation: an on-kill MAGAZINE REFILL (Sentient Surge) postpones the reload,
and a reload is what clears the Ocucor's tendrils — so more bodies means more
kills, more refills, fewer reloads, longer-lived tendrils, and more damage
again. Nothing was added for it; it fell out of spread damage counting toward
`RunResult::kills`.

**EXCEPT A TENDRIL'S OWN KILL**, which spawns nothing — *"a kill by the primary
beam, or by a status effect from any source (including one a tendril applied),
spawns a tendril; a DIRECT kill by a tendril does NOT spawn another."* It is the
only kind of kill in this engine worth less than another, and `SpreadBy` is the
enum that carries which mechanism landed an instance so the difference can be
made. It was a `bool` for an hour and the hour was spent on a flipped argument.

### The chaining roster

Seven entries, and their constants are the whole of what differs between them.
The wiki's `Beam` page names which weapons chain; each number below is
transcribed from that weapon's own page.

| entry | hops | range | per hop | compounds |
| --- | --- | --- | --- | --- |
| Torid, Incarnon form | 5 | 7 m | 75% | yes |
| Atomos | 3 | 7 m | 75% | yes |
| Larkspur / Prime | 3 | **6 m** | 80% | yes |
| Boar / Prime, Incarnon form | 2 | 10 m | 80% | yes |
| **Kuva Nukor** | 2 | 9 m | 50% | **no** |

**THE KUVA NUKOR DOES NOT COMPOUND**, and it is one word on the page: *"each
doing 50% of the MAIN BEAM's damage"*, where every other reads "of the PREVIOUS
chain's". Both hops at 50%, not 50% and 25% — a factor of two on the second hop,
which is why `compounds` is a field rather than an assumption.

**THE LARKSPUR'S 6 m IS THE GROUND COLUMN.** *"In Atmospheric Mode, the beam
chains within 6 meters … In Archwing Mode, maximum chain distance extends to 30
meters."* This weapon is the reason AGENTS.md carries a rule about the
two-column Arch-Gun infobox: an export that took the Archwing column posted the
Larkspur Prime at half its damage. The arena is on the ground.

**NONE OF THE FIVE STATES A DAMAGE RADIUS**, so theirs is zero and only the body
the beam struck seeds a chain. The Atomos's page mentions *"a small radius of
damage around the beam"* and gives no number; a zero is the conservative reading
and a measurement is what would move it.

`cargo run --release --bin spread_audit` lists every entry that reaches a
formation and how: 62 of 224 today — 54 explosive, seven beams, one with
tendrils. The other 162 reach the body they hit and nothing else, which is not a
gap: most guns are guns.

All three read the same three blast rules, and all three hand ORDINARY damage
instances to the ordinary pipeline — each body computing its own Condition
Overload, its own half-health term, its own mitigation, its own procs and its
own death.

**SO A FULL INCARNON CYCLE IS SIMULABLE AGAINST A CROWD** (owner, 2026-08-17):
the base form spreads through an AREA and the transformed form through a CHAIN,
and both halves now have somewhere to go. A radius mod is worth something in
both, by two different mechanisms — it widens the cloud in one and seeds more
chains in the other — and worth EXACTLY NOTHING against a lone body in either,
which is the asymmetry both tests assert as equality.

### Ties, and why they are not modelled

**MEASURED (M52): the path is fixed, and its rule is not in the formation.** Two
paths read off a 5 x 4 Simulacrum formation confirm NEAREST — all ten hops went
to an orthogonal neighbour — and refute every tie-break expressible in relative
geometry: a fixed compass priority fits 8 of 10 over all 24 orderings, a turn
preference 8 of 10 over all 96, entity-index order 4 to 7. Arriving at one body
heading `+x` the path went straight and at the next it turned.

The owner's own clue explains it: a NON-HUMANOID model changes the path while
every relative position is identical, so what the order depends on is the
COLLIDER — the game's spatial query returning bodies in world-space broadphase
order, which is not a function of the formation.

So the model does not reproduce it. `chain::resolve` breaks ties by the lowest
body index: arbitrary, and STABLE, so **a formation that does not move always
chains the same way** — which is the property asked for in place of fidelity.
The unknowable part never reaches the answer, because the total is invariant to
tie-breaking; it decides which body dies first and nothing else.

### What is still not measured

Everything except the path rule above is the wiki plus the owner's rulings. The
two-enemy case is the clean first one for the DAMAGE side: both bodies take
175%, and the tie question cannot intrude because there is only one candidate.

### Settled elsewhere, and worth having here

* **Sinister Reach does NOT reach the chains** — *"Weapon Range Mods no longer
  affect the distance of chains for chainable Beam weapons. Main Beam distance
  is still affected"*. The Torid cannot equip it anyway.
* **Punch Through** *"will cause the beam to rapidly recalculate which enemies
  are chained to, spreading out on the damage, but not increasing it"* — and
  the Torid's page says punch-through has no effect on its beam at all.
* **Chains cost no ammo.**
* **MULTISHOT reaches only what the beam DIRECTLY struck** — not the radius,
  and not a chain that started from a target the radius caught rather than the
  beam.
* **The same code family** runs on other weapons with other constants: the
  Amprex chains 3 within 10 m at 0.5ⁿ, the Kuva Nukor 2 within 9 m at 50%.

### The reference implementation, and what it is worth

`https://malurth.github.io/AoE-simulator/` is a live top-down toy whose
parameter list is very close to the axis set a spatial model needs — entity
radius, chain radius, max chain length, beam length, multishot, enemy speed,
and a Primed Firestorm toggle. Its author disclaims the one rule that matters
most, so it is a reference for the SHAPE of the model and not a source for its
numbers.

## §13 Punch through

**It is not "how far it still flies after the first hit."** It is metres of
MATERIAL, and the wiki's own definition makes no distinction between kinds:

> "Geometry Punch Through is the total distance of material (object or enemy)
> that a weapon's projectile, bullet or beam can pass through before
> dissipating."

Each body crossed spends part of the budget; when it runs out the shot stops.
`space::struck_along` walks the aim ray and returns every body it reaches, in
the order it meets them.

### What a body costs — 0.5 m, and the table pins it

`space::BODY_MATERIAL_M = 0.5`, and it is **not** twice `BODY_RADIUS_M`. The
two are different quantities from different sources, and keeping them apart is
the decision:

- **`BODY_RADIUS_M = 0.2` is MEASURED** (M46, the owner's own reading): walking
  into an enemy stops at 0.4 m centre to centre. It governs spacing, the hit
  test and blast reach.
- **`BODY_MATERIAL_M = 0.5` is PUBLISHED**: the punch-through page's "Minimum
  Mod Ranks for Penetration" table.

Every one of that table's thirteen humanoid cells is reproduced by a single
threshold of 0.5 m, and the table brackets the value from both sides — the
largest rank that FAILS is 0.4 and the smallest that WORKS is 0.5:

| mod | largest ✗ | smallest ✓ |
|---|---|---|
| Shred / Seeking Fury / Merciless Gunfight | 0.4 | 0.6 |
| Primed Shred | 0.4 | 0.6 |
| Vigilante Offense | 0.25 | **0.5** |
| Power Throw | 0.3 | 0.7 |
| Metal Auger / Seeker / Seeking Force | 0.4 | 0.7 |

The page's other statement agrees from the other side: *"The torso hitbox of
three butchers combined adds up to over 1.2m of material"* — over 0.4 m each.
`space::tests::a_body_costs_what_the_wiki_table_says` asserts the whole table.

**Why not raise the radius to 0.25 instead.** It would overwrite an in-game
measurement with a table whose own note says *"Average data, result will differ
due to width variances"*, and move every distance-dependent number on the board
by 0.05 m for the privilege. The property that motivates the question survives
either way: crossing a body costs 0.5, so **0.5 m of punch through reaches the
second of two adjacent enemies**, which is exactly what the table says.

A FLAT COST, not a chord: the table publishes one number per enemy type and
warns the real thing varies with width, so charging a chord would be a geometry
this engine invented. QUADRUPEDS are out of scope — the table's own rows for
them disagree with each other (Power Throw's 0.7 penetrates where Vigilante
Offense's 0.75 does not), which is that caveat showing.

### A punched body is a DIRECT hit, and it starts its own chain

The body behind takes the shot itself, at full damage — the page names no
attenuation per body and the engine invents none. It carries multishot (every
pellet punches through) and it may HEADSHOT (owner, 2026-08-17: punch through
does not stop a shot being aimed). That makes it the opposite of a chain hop,
which does neither.

On a chaining weapon the two mechanics compose, and the wiki is explicit:

> "Each enemy hit by the main beam from Punch Through can generate a new set of
> 3 chains." — "Punch Through will cause the main beam to chain INDEPENDENTLY
> from each additional target hit, potentially doubling or tripling the total
> damage output when fired into a crowd." — "The chain from the target hit after
> the Punch Through can deal damage to the first target, and vice versa."

That last clause is the owner's own rule for two chains meeting (2026-08-17): a
body takes a second instance only when a SECOND independent link reaches it.
`chain::resolve` takes the struck bodies as its seed list and each seed keeps
its own `seen` set, so this falls out rather than being arranged.

### An AoE attack takes none of it — from its weapon or from a mod

A catalog rule, and the page states both halves:

> "With a very few exceptions, weapon projectiles with an area of effect (AoE)
> component will not Punch Through enemies or level geometry at all. Instead the
> projectile will explode on first contact." — "Projectile AoE weapons cannot
> have their Punch Through stat modified."

So a Shred on a grenade launcher is worth **literally nothing**. "An area of
effect component" is both shapes this engine models — a `radial` (one explosion
at impact) and a `lingering` cloud (an explosion that stays and ticks): the
Torid is the second kind and carries no `radial:` at all, so a rule naming only
radials would have let a grenade launcher take Primed Shred.

The page's *"very few exceptions"* announce themselves in the DATA: an exception
is an entry whose own infobox carries a punch-through figure, and the roster has
exactly one attack with both (the Vulcax's 2 m). It keeps its innate depth and
takes nothing from mods, which never invents a number the wiki did not print.

### …but the SHAPE is only the fallback. The entry decides

`punch_through_mods:` on an attack overrules it, and absent means ORDINARY —
which for punch through is *yes, mods apply*. The Torid's Incarnon form is why
the field exists rather than the shape alone deciding:

> "Punch Through mods have no effect on the behavior of the beam."

It is a BEAM with a 2.3 m damage radius, so it carries neither `radial:` nor
`lingering:` and the class rule above — which is about *projectiles* — does not
reach it. Shred would have gone on.

**The family does not decide it either**, which is the sharp part. The same wiki
sentence that classifies this weapon for Primary Compression names the group:
*"Does not work on Continuous Weapons or beam attacks with an AoE component. For
example, Ignis or Torid Incarnon Genesis"* — and the **Ignis is on the
punch-through page's EXCEPTION list**, with infinite body punch-through. Two
weapons the wiki puts in one group for one mechanic sit on opposite sides of
another. So the answer is transcribed per ENTRY, which is docs/CATALOGS.md's
rule generalised once more (owner, 2026-08-17: the Torid Incarnon was the
question that found it).

### The exception list is a real catalog, and it has an Arch-Gun section

**Infinite body punch-through** is written `999.0`. Its one qualifier —
*"innate punch through does not apply to surfaces"* — separates bodies from
geometry, and this arena has no geometry, so unlimited-through-bodies is the
whole of the mechanic here.

The page carries the list per CLASS, and the Arch-Gun section is the one worth
naming because nothing else on the page hints at it: **Cortege (Primary Fire),
Corvas (Atmospheric Mode), Corvas Prime, Grattler, Kuva Grattler, Mandonel,
Velocitus**. Four of those are AoE weapons, which is what the list is FOR — they
are the *"very few exceptions"* the class rule points at.

**Read the qualifiers as written.** The page restricts where it means to —
"Cortege (Primary Fire)", "Quellor (Alt-Fire)", "Corvas (Atmospheric Mode)",
"Mandonel (Charged Shot only: 2.4m)" in the finite list — so an UNQUALIFIED
entry names the weapon and reaches both its forms. "Corvas (Atmospheric Mode)"
is the two-column Arch-Gun trap again: this arena is on the ground.

**A weapon can be on BOTH lists**, and it is not a contradiction: the finite
figure is the GEOMETRY column and the infinite one is bodies. Corvas Prime is
1.4 m and unlimited; the Lanka is 5 m and unlimited. In this arena only the
second half exists.

**What the audit found** (2026-08-17, prompted by "can the Larkspur take it?"):
the Fluctus read `999.0` under a comment calling it *"INFINITE"* and it is on
NEITHER infinite list — the page gives it **275 m**, a published number rather
than a word, and it reaches everything here either way, which is exactly why the
wrong one survived. The Prisma Dual Decurions had copied its sibling's 1.2 m
where the page lists it separately at **1.4 m**. And twenty-two entries that
should have been unlimited were reading 0 to 5 m.

**The Larkspur is on neither list, anywhere on the page** — so it is ORDINARY
and takes punch-through mods, which for a chaining beam means extra bodies each
starting their own chain.

### The bug it found on the way in

`raw` is multiplied by the pellet's `part_factor`, and every spread mechanism
was fed `raw / bucket` — so a chain hop, a splash and an echo all inherited the
aimed pellet's HEADSHOT on their direct damage, while `spread_hit`'s own doc
comment said *"NEVER A HEADSHOT ... `part_factor` is 1.0 here"*. It was 1.0
where that comment looks (the PROC scale) and not in the damage, so the claim
was true of half the instance. The factor now comes back off at the one site
that has it, and punch-through is the single exception that keeps it. Single
target fights are untouched: with no formation, no spread mechanism runs.

## §14 The elements whose PROC is an area

**THREE, and the audit is exhaustive**: every one of the fifteen damage types was
checked against the `Status_Effect` page for a clause naming a radius, "nearby",
"surrounding" or "other enemies". Impact, Puncture, Slash, Cold, Heat, Toxin,
Corrosive, Magnetic, Radiation, Viral and Tau affect **only the enemy hit** —
stated per type, so this is a closed list rather than the ones that came to mind.

VOID is the fourth and is NOT damage: "creates a 2.5 meter radius field which
attracts projectiles for 3 seconds". It is an aim aid, this arena has no
projectile attraction, and modelling it would be inventing a benefit.

Gas and Electricity were an ordinary single-body DoT
until 2026-08-17 — right while the arena held one body, and half the mechanic
once it held 361. Verbatim, from each element's own page:

> **Gas** — "a gas cloud that deals a tick of damage each second to all enemies
> within a **3**-meter radius", "subsequent procs increase the radius by **0.3**
> meters up to **6** meters", "**6** second duration". "Up to **10** instances
> of the effect can stack on the same target, with each instance having its own
> timer."
>
> **Electricity** — the proc "chains between nearby enemies", hitting "all
> enemies in a **3**-meter radius", "every second for **6** seconds", and "only
> the original target will be stunned … others around it will only take damage".

**Only the DoT travels.** The stun, the arcane triggers (Conjunction Voltage,
Primary Blight) and the stack counts stay on the body that was hit; what reaches
the neighbours is the tick. The gas cloud's radius grows with the number of
clouds already on that body and is read BEFORE the new proc is counted, which is
what makes the first one 3 m rather than 3.3.

**An OUTBOX rather than a threaded queue.** `settle_procs` holds ONE body's
debuff state and spreading needs every body's, so the proc posts to
`DebuffState::area_out` — the per-body struct every proc path already has in
hand — and the drain, which is the one place that knows WHICH body is which,
hands the `Dot` to everyone the radius catches. It costs no parameter anywhere.
A `Dot` is self-contained (`{next_tick, ticks_left, value, dtype,
ignores_armor}`) and carries an absolute tick time, and nothing in this arena
moves, so draining a moment later is exact rather than approximate.

The copy is BODY-ONLY: `part_factor` comes back off, because an arc is not
aimed at anything. Same rule every instance that lands on a neighbour follows.

### What it cost, and what that cost was made of (2026-08-18)

An area proc hands a `Dot` to every body within its radius, and on a 19x19 grid
at 1.5 m spacing that is **29 bodies per proc**. The Phantasma Prime — six
beams, infinite body punch-through, twelve ticks a second — struck 19 bodies
deep with every one of its six beams, so a full status build produced **146,000
procs and 4.26 million DoT pushes per run**. Measured: **9,551 ms a run**,
against 88 with the spread switched off. A hundred runs was sixteen minutes.

Two things were wrong, and neither was the mechanic:

**The scan.** Each proc asked "who is within radius of this body?" by walking
every body — `O(bodies)` per proc, thousands of procs a second.
`space::Neighbours` answers it once per run: per body, its neighbours within
`AREA_MAX_M` (a full-stack gas cloud plus a body radius), nearest first, so a
lookup at any smaller radius is a prefix that stops at the first body out of
range. Same shape and same reason as `chain::Layout` — nothing in this arena
moves.

**The cap.** `dot_cap` was the unit's declared `stack_caps.general` and `None`
otherwise, so a generic enemy's DoT list was UNBOUNDED. It grew with every proc
and `process_ticks` walks it once per body per shot — linear cost, quadratic
outcome. It is TEN now where a unit declares nothing, which is the Gas page's
own wording ("up to 10 instances of the effect can stack on the same target")
and the rule the ten-stack families in `DEBUFF_ROSTER` already followed.

**9,551 ms -> ~150 ms, a 64x cut, with `one_fight` reporting every answer
unchanged.** A hundred runs of the group-clear ruler with the board's own #1
Phantasma Prime build went from ninety minutes in the browser to 85 seconds.

What is left is the mechanic itself: 146k procs times 29 neighbours is work
nobody can argue away, and it is what a six-beam infinite-punch-through shotgun
does to a nineteen-deep column. The remaining lever is PARALLELISM — the runs
are independent — and taking it means giving each run its own derived seed,
which changes every published number and is the owner's call rather than an
optimisation.

### The gap it uncovered: a formation body never ticked at all

`process_ticks` was called for the aimed body and for **nothing else**. So every
status a chain hop, a splash, a tendril or an echo applied to a neighbour was
recorded on its debuff state and **never paid out** — a ledger nobody read. Gas
and Electricity cannot work at all without it, which is how it surfaced.

Every body ticks now, once per shot, and the tick credits
`damage_by_body`. The player's buff state (`gal`, `arc`) is shared across them,
which is right: a kill is a kill whichever body it was.

### BLAST — the third, and the one whose halves differ most

> "Each blast stack will detonate, dealing **30%** weapon base damage to the
> target" after **1.5** seconds, each stack on its own timer. Stacks detonate
> **simultaneously** when "reaching **10** blast stacks" or "the target dying",
> and then "enemies within **5** meters are dealt **300%** of base damage per
> proc". "The initial target of the blast procs is not dealt this AoE damage,
> only the single target damage."

So a stack is worth **ten times as much to a neighbour as to the body carrying
it**, and SIMULTANEOUS is the trigger rather than the fuse: a stack that simply
burns down its own 1.5 s deals the single-target hit and nothing else. The
engine had the single-target half and its comment already said the radial was
"excluded — it never hits the host"; there was no host's neighbour to hit.

A detonation is a HIT and not a DoT, so it rides a second outbox
(`DebuffState::area_hit`) — it lands once and carries no `dtype` for the
neighbour to count as a status ("inherits no additional status effects"). A
one-tick DoT would have been both of those wrongly.

### A CLOUD IS A PLACE, AND IT OUTLIVES WHAT IT STUCK TO

Asked of the gas proc (owner, 2026-08-17) and true of both clouds:

> **Gas** — "If the host target dies, Gas will continue to tick damage on all
> enemies caught in the host's radius for its remaining duration."
>
> **The Torid's lingering cloud** — "Torid projectiles can also attach to
> corpses and will remain at their position even if they disintegrate, granting
> a fixed position mid-air and allowing a greater spread of toxin damage onto
> enemies."

`DebuffState::on_death` therefore keeps three things and drops everything else:
the outboxes (a cloud this body already produced belongs to its neighbours), the
GAS dots — only gas, because the page says it of gas and of nothing else — and
the blast stacks detonate on the way out. The weapon-made fields stopped being
cleared on a kill for the same reason.

**It moves scores.** The clouds were wiped on every respawn, which on a fight
with instant respawns is most of their uptime. `one_fight`'s three shapes do not
see it because their Thrax never dies.

`a_gas_or_electric_proc_reaches_the_bodies_standing_around_it` asserts a COUNT
rather than a total — the claim is that the proc reaches bodies the shot never
touched. Bodies at 2 m take it, bodies at 12 m do not, and TOXIN reaches nobody,
which is the control that says this is the element's mechanic and not the engine
spreading every DoT it has. The fixture is a weapon with no AoE of its own: the
Torid was the first one and its lingering cloud reached the neighbours by
itself, which that control caught.

## Open questions

- Exact multiplicative-bucket membership for every common mod (§2).
- Armor DR constant/curve and level-scaling formulas (§8).
- Innate-vs-mod elemental combination timing on multi-innate weapons (§3).
- Proc-weight formula when 3+ elements coexist (§6).
- Incarnon-form stat/mechanic overrides (see the Dual Toxocyst prototype).

---

### Conditional buffs with no live model — OPEN

`ModEffect::CondBuff` is the catch-all for a triggered buff whose TRIGGER the
sim does not model as an event. It is applied by `resolve_with` **under
AssumedMax only**, which has a consequence worth stating plainly:

- the **Stats panel** shows its number, correctly — that panel resolves under
  AssumedMax and says "conditional buff, assumed active";
- the **Simulator** runs Emergent, where the effect contributes **nothing**;
- and there is **no buff card** to switch it on, because a card needs
  something live to toggle. The Sim's buff list is built from effects that
  carry a real window (`OnReloadFireRate`, `OnReloadDamage`, the arcane
  buffs), and `CondBuff` carries none.

So a player equips one of these, sees the Stats panel change, runs the Sim,
and sees nothing move — with no control that explains the difference (owner,
2026-08-01, on Archgun Ace).

**Scope: three mods**, all of them a trigger nothing else needs yet.

| mod | trigger | grants |
|---|---|---|
| `archgun_ace` | on headshot kill | fire rate, reload speed |
| `catalyzer_link` | on ability cast (while aiming) | status chance |
| `embedded_catalyzer` | on ability cast (while aiming) | status chance |

An ability cast is not an event this arena has at all, so those two are honest
no-ops until a Warframe model exists. **Archgun Ace is the one that is simply
missing**: on-headshot-kill IS an event the sim raises (Deadhead rides it), so
its two grants want the same treatment `OnReloadFireRate` already has — a
`TimedBuff` on the panel, a window in the sim, and a card.

**Status:** known gap, not a wrong number — the Stats panel's reading is right
for what that panel is.
