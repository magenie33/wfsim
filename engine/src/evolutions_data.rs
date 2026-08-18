//! Declarative Incarnon-evolution loader: `data/evolutions/*.yaml` -> the
//! evolution pool.
//!
//! Evolutions are DATA, not code (same pattern as [`crate::mods_data`] /
//! [`crate::arcanes_data`]): each yaml records the wiki-verified effects;
//! this module parses them into [`EvolutionDef`] and APPLIES a chosen set
//! onto a weapon's raw [`WeaponBase`] — evolutions alter BASE stats before
//! mods (flat base damage scales the vector pro-rata inside ModifiedBase;
//! Commodore's Fortune adds into the BASE crit chance that crit mods then
//! multiply). The engine previously hardcoded these numbers in the
//! `DtEvo2` enum; the enum remains as a selector, the values live here.

use std::sync::OnceLock;

use serde::Deserialize;
use serde_norway::Value;

use crate::loadout::WeaponBase;

#[derive(Debug, Deserialize)]
struct EvoFile {
    id: String,
    name: String,
    weapon: String,
    tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    #[serde(default)]
    icon: Option<String>,
    /// Verbatim in-game/wiki effect text (evolutions have no ranks, so no
    /// X templating).
    #[serde(default)]
    description: Option<String>,
    /// Wiki-flagged non-functional evolutions apply NOTHING.
    #[serde(default)]
    currently_broken: bool,
    /// Does THIS evolution's flat base damage stay out of the weapon's GunCO
    /// term? **DEFAULT YES** since 2026-08-16 — an omitted field means
    /// EXCLUDED, and `false` is the explicit opt-out nothing uses yet.
    ///
    /// IT USED TO DEFAULT NO, on the reading that the CO catalog "lists only
    /// discrepant attacks" so an unlisted perk feeds the term in full. The
    /// evidence went 15 to 0 against that:
    ///
    ///   · ELEVEN catalog rows print a DOUBLE value ("100 or 124") — the only
    ///     rows where anyone measured the weapon with its evolution installed.
    ///     All eleven came back excluded.
    ///   · ZERO rows anywhere measured the evolved case and found it included.
    ///     The "100%" rows print the UNEVOLVED base in their own damage column,
    ///     so they say the CO base equals the base of a weapon with no
    ///     evolution on it, which is true by construction and says nothing
    ///     about this question.
    ///   · FOUR perks across two weapons were measured by the owner, and all
    ///     four are excluded — the Dual Toxocyst's two (M49) and the Torid
    ///     Incarnon's two (M50). One of those four WAS on the catalog and three
    ///     were not, so the catalog's silence has now been tested three times
    ///     and meant "unmeasured" every time.
    ///
    /// The Torid's is the decisive shape: its two tier-2 perks give panels of
    /// 102 and 82, and every reading off BOTH solves to a CO base of ~51 — the
    /// unevolved value, constant across the pair. A term that fed on the
    /// evolution would have solved to 102 and 82 and would not have agreed
    /// with itself.
    ///
    /// AND THE ERROR IS ASYMMETRIC. The old default OVERSTATES, which for a
    /// calculator whose promise is matching in-game measurements is the worse
    /// direction: it ranks weapons on damage the game does not deal. 186
    /// weapon+perk pairs moved when this flipped, by 37% on average at two
    /// Galvanized stacks against two status types.
    ///
    /// THE FLAG IS STILL THE PERK's ON THE `Adding` SIDE, not the weapon's,
    /// because the catalog names perks and a perk reaches both forms of its
    /// transform group. `false` on a perk is how a measured exception gets
    /// recorded there.
    ///
    /// ON A `Multiplying` ENTRY THE CLASS ANSWERS FIRST and this flag is never
    /// read (M51, and see [`EvolutionDef::excludes_co_base`]). The two forms of
    /// one group can be different classes with OPPOSITE answers — the Torid is
    /// exactly that — so a perk's flag must not be able to reach the form it
    /// was not measured on.
    #[serde(default)]
    co_base_excludes_this_evolution: Option<bool>,
    /// …and on WHICH FORM it was measured, when the reading covers one of them.
    /// `base` or `incarnon`; omitted means the perk's whole transform group.
    ///
    /// A perk belongs to a GROUP and a reading comes off an ENTRY. Usually that
    /// gap does not matter, because the catalog rows name a weapon and both its
    /// forms behave alike. The Torid is where it does: both its forms are now
    /// measured on the same two perks and they answer OPPOSITELY — the Incarnon
    /// form is `Adding` and excludes (M50), the base form is `Multiplying` and
    /// feeds in full (M51). Recording the first without this scope would have
    /// asserted the second and been wrong.
    #[serde(default)]
    co_base_excludes_only_form: Option<String>,
    /// *"Does not affect Incarnon Form"* — the whole perk is the BASE form's.
    ///
    /// It is the EVOLUTION's flag and not an effect's because that is how the
    /// card reads it: on all eleven entries carrying the sentence it is the
    /// last clause and it qualifies everything before it, magazine and ammo
    /// and range together.
    #[serde(default)]
    base_form_only: bool,
    effects: Vec<Value>,
}

/// One parsed evolution effect (the loader's vocabulary — kinds with no
/// single-target damage payload load as `Inert` so the evolution still
/// resolves and lists).
#[derive(Debug, Clone, PartialEq)]
enum EvoEffect {
    /// Adds to the BASE damage TOTAL, distributed pro-rata across the
    /// vector, BEFORE mods (inside ModifiedBase).
    FlatBaseDamage(f64),
    /// Adds into the BASE crit chance (crit mods multiply the new base).
    FlatBaseCritChance(f64),
    /// A flat addition to BASE multishot — the Braton family's Munitions Grit
    /// is +0.20, and its tier-mate's +60% multishot has nothing to act on
    /// without it. The yaml said so in a comment while loading inert
    /// ("with no multishot source the +60% applies to nothing"), which is the
    /// disclosure working and not a reason to leave it.
    FlatBaseMultishot(f64),
    /// Mounting Momentum: every SHELL loaded is +10% fire rate, and nothing
    /// takes the stacks away short of holstering.
    ///
    /// The first buff in the roster whose per-trigger gain is a weapon stat.
    /// It is what makes the perk a real choice: a magazine mod buys stacks and
    /// pays for them in reload time, and the two only trade off because the
    /// by-round reload is modelled (`WeaponSpec::reload_style`). Implementing
    /// one without the other would have handed the optimizer free fire rate.
    StackingFireRatePerShellReloaded { per_stack: f64, max_stacks: u32 },
    /// Adds into the BASE status chance — the same base-stat layer, so status
    /// mods multiply the new base (Torid's Survivor's Edge and Elemental
    /// Balance both say "Increase Base Status Chance"). NOT the post-mod flat
    /// layer that Elemental Excess occupies.
    FlatBaseStatusChance(f64),
    /// The same layer, but the two FORMS get different numbers. Boar's
    /// Elemental Balance reads "+12% per projectile" and "+96% for Incarnon
    /// Form" as two separate statements, not as a sum — a shotgun's pellet
    /// carries a twelfth of the status a beam tick does, so one number cannot
    /// serve both. Picked by `base.gauge_form.is_some()`, the same gate
    /// `FlatBaseMagazine` uses.
    FlatBaseStatusChanceByForm { base: f64, incarnon: f64 },
    /// Adds into the BASE crit MULTIPLIER (Boar's Critical Parallel: "+0.5x").
    /// Base-stat layer like the crit-chance one above, so crit-damage mods
    /// multiply the new base.
    FlatBaseCritMultiplier(f64),
    /// Flat BASE damage that an empty reload turns on and nothing turns off —
    /// Boar's Reified Bane, "On Reload From Empty: Increase Base Damage by
    /// +14". It is applied UNCONDITIONALLY, i.e. the run is modelled as
    /// holding it from t = 0 (user, 2026-08-03).
    ///
    /// Held is EXACT here, not an approximation, and the timing is why: the
    /// bonus lands the moment an empty reload BEGINS and does not wait for it
    /// to finish (measured in game — user, 2026-08-03; the wiki claims the
    /// opposite and loses, as it does to every measurement). So there is no
    /// gap: the magazine empties, the reload starts, the buff is already back,
    /// and it "lasts indefinitely until a manual reload is initiated while the
    /// magazine is not empty" — which the sim never does. Under the wiki's
    /// reading the buff would instead be DOWN for one reload every cycle, and
    /// holding it would overstate the build.
    ///
    /// **THIS IS THE EXCEPTION, AND THE NAME SAYS SO** (user, 2026-08-03).
    /// The DEFAULT for a reload-triggered effect is that it fires when the
    /// reload COMPLETES; a new one gets its own variant and that default,
    /// rather than reusing this. Two conditions have to hold together here
    /// and neither is the ordinary case:
    ///
    ///   1. the magazine must be EMPTY (a manual reload does not count — it
    ///      is what takes the bonus away);
    ///   2. it fires when the reload STARTS, not when it ends.
    ///
    /// Only Boar Prime's Reified Bane is known to work this way. Whether any
    /// other evolution ever joins it is open, so the variant stays narrow: a
    /// general "on reload" effect is not this one with a flag.
    ///
    /// It stays its own variant rather than being folded into `FlatBaseDamage`
    /// because it is a BUFF: `resolve` turns it into an `EvoBdBuff` so the bar
    /// can show it and a card can scale it back out — opening at ONE stack,
    /// which is the state a default test starts in.
    FlatBaseDamageOnEmptyReload(f64),
    /// A handling / mobility / multi-target stat with no single-target damage
    /// payload — recoil, accuracy, punch through, projectile speed, holstered
    /// reload. It COUNTS: the value lands in the panel's `indirect` bucket
    /// beside the mods' (user, 2026-08-03). Mods were given this treatment
    /// on 2026-08-01; evolutions were still dropping the number on the
    /// floor.
    Indirect(crate::loadout::IndirectStat, f64),
    /// Sets the ammo RESERVE outright (Mercenary Chamber: "Increase Base Ammo
    /// Capacity to 195") — a set, not an add, so it cannot ride the additive
    /// indirect bucket.
    AmmoMaxSet(f64),
    /// Adds whole rounds to the BASE magazine, before magazine mods (Torid's
    /// Extended Volley: +9 on a base of 5). Explicitly NOT the Incarnon form's
    /// charge-backed magazine — "Does not apply to Incarnon Form's Magazine" —
    /// which is why it lands on the base entry only.
    FlatBaseMagazine(f64),
    /// Renewed Horror: reloading from EMPTY arms a buff that multiplies the
    /// duration of the NEXT shot's lingering field. ✅ measured (M13): x2, so
    /// that field ticks 20 times instead of 10.
    FieldDurationOnEmptyReload(f64),
    /// A multishot bonus that pays only PAST a distance — Lone Enforcer's
    /// "+25% Multishot if no enemies are within 5m", as `(fraction, metres)`.
    ///
    /// It cannot be a [`GatedGrant`] like the rest: those are opened by the
    /// TENNO's state and `resolve` never sees the arena, while this asks where
    /// the two of them are standing. So it rides the panel and is settled in
    /// `DummyParams::from_panel`, which is the one place the build and the
    /// fight are both in scope — the same seam Primary Compression already
    /// uses (the panel brings the metres, the arena brings the answer).
    MultishotBeyondRange { value: f64, metres: f64 },
    /// Final Fusillade: a FLAT multishot add on the last round of the magazine,
    /// BASE FORM ONLY (user, 2026-07-30) — a charge-backed Incarnon magazine
    /// has no "last shot in magazine" to gate on, so `apply` drops it there.
    /// A flat multishot add on the magazine's last round, and WHICH BRACKET it
    /// lands in — the `bool` is the card's own word "Base".
    ///
    /// The two perks that grant this do not grant the same thing, and the wiki
    /// says so on the row rather than in a general rule:
    ///
    /// - Torid, Final Fusillade: *"+3 Multishot on last shot in magazine"* —
    ///   flat, on top of everything, `false`.
    /// - Burston, Forceful Finality: *"+5 **Base** Multishot on final magazine
    ///   burst"*, with a note attached to that row: *"Multishot bonus is added
    ///   before mods, and is thus multiplied by multishot bonuses"* — `true`.
    ///
    /// The note exists BECAUSE it is unusual, and the difference is not small:
    /// on a Burston Prime carrying Split Chamber and Vigilante Armaments the
    /// same +5 is 11 pellets rather than 5.
    MultishotOnLastRound { value: f64, base: bool },
    /// Flensing Spikes: armour removed per live Puncture status, as a
    /// fraction. A third strip source beside Corrosive and Heat, and the first
    /// that a WEAPON grants rather than a status carrying it.
    ArmorStripPerPunctureStatus(f64),
    /// Reaver's Rapture: +X base damage per COMPLETED BURST, reset when the
    /// magazine is refilled. No duration — it is held until something takes it.
    BaseDamagePerFullBurst { per_stack: f64, max_stacks: u32 },
    /// Plentiful Mayhem: multishot draws its extra rounds from ammo, and the
    /// projectiles it GENERATES deal +v damage as an independent multiplier.
    /// Affects both forms; the sim reads the per-form rule off `continuous`.
    MultishotConsumesAmmo(f64),
    /// A PERMANENT stacking multishot buff (Fevered Frenzy: on-ability-cast
    /// stacks with no timer, cleared only by death — so inside a sim run the
    /// stack count is a static CHOICE, full by default). `total` = the
    /// full-stack bonus (per_stack × max_stacks) that joins the weapon's
    /// buff multishot; `max_stacks` lets the per-buff config rescale it.
    AssumedMaxMultishot { total: f64, max_stacks: u32 },
    /// Unconditional CO rate (Carnage Reign): +v per status TYPE, additive
    /// with mod CO sources. `excludes_evolution_damage`: the GunCO base
    /// excludes evolution flat damage (wiki CO catalog, DT row).
    /// Condition Overload granted by an evolution, and the SPRINT SPEED the
    /// player needs for it. `min_sprint` is 0 when the card states no
    /// condition; the Latron family's Swift Punishment states 1.2.
    ConditionOverload { per_type: f64, min_sprint: f64 },
    /// Fire-rate bonus in the ORDINARY additive bucket — the same one the
    /// fire-rate mods feed, so it SUMS with them (Rapid Wrath).
    FireRateBonus { value: f64, min_sprint: f64 },
    /// "+X% Damage to enemies below half Health" — a bucket bonus with a
    /// condition on the TARGET rather than on the weapon or the player.
    /// "+X% Damage to enemies below half Health". `excludes_own_flat` is the
    /// Sicarus's note — *"does not take into account the Base Damage increase
    /// from this perk"* — and it is per CARD rather than a rule: the Kunai's
    /// page says the opposite about the same weapon's base increase ("CO-bonus
    /// DOES use base damage increase Evolution"), so applying the correction
    /// everywhere would dock a perk the wiki never docked.
    BaseDamageBelowHalfHealth { rate: f64, excludes_own_flat: bool },
    /// RESONANT RESTORE — "On Reload From Empty: Increase Base Magazine
    /// Capacity by +N. Stacks up to Nx".
    ///
    /// Not a `StackingGrant`, because what it grants is not a term in any
    /// bracket: it is the magazine CAPACITY, which every other line of the sim
    /// loop reads. "BASE" capacity, so the magazine mods multiply each stack.
    MagGrowthOnEmptyReload { per_stack: f64, max_stacks: u32 },
    /// EXACT PENANCE — "On Kill: 50% chance for Instant Reload".
    ///
    /// Distinct from `InstantReloadOnHeadshot` because of the card's own note:
    /// "Kills from status effects can also trigger the effect." That one asks
    /// for a weak-point direct hit; this one asks only that something died, so
    /// it is read off the kill counter.
    InstantReloadOnKill { chance: f64 },
    /// GALVANIC RELOAD — a magazine restore the TARGET's state gates.
    ///
    /// VERBATIM (Strun_Incarnon_Genesis): "On hitting a target affected by an
    /// Electricity status, 40% chance to restore 1 round in the magazine from
    /// ammo pool", with three notes, each of which decides something:
    ///   *The status effect may originate from any source.
    ///   *The bonus can only apply once per enemy hit.
    ///   *The bonus does not affect the Incarnon form.
    /// The second is why it is per SHOT and not per pellet — this is a shotgun
    /// family — and the third is the card's `base_form_only`.
    RoundRestoreOnStatusHit { status: crate::damage::DamageType, chance: f64, rounds: f64 },
    /// KING'S GAMBIT — one bullet, two brackets, and the wiki names both.
    ///
    /// VERBATIM (Sicarus_Incarnon_Genesis): "x0 Critical Chance on Bodyshots,
    /// +150% Critical Chance on Weakpoint Hits", under which:
    ///   *Bodyshot modifier is multiplicative with all sources of Critical
    ///    Chance, effectively making non-headshot critical hits impossible.
    ///   *Weakpoint modifier is additive with mods such as Pistol Gambit
    ///
    /// So `bodyshot_mult` MULTIPLIES a body pellet's chance and
    /// `weakpoint_bonus` joins `weakpoint_cc_rel`, the same relative bracket
    /// Pistol Acuity uses. Both are per-PELLET, which is what keeps them out of
    /// the panel's crit chance — and that is what makes the same page's other
    /// note true for free: Wiseman's Regard, which reads "current Critical
    /// Chance", is "**Not** affected by the King's Gambit Evolution II perk".
    CritChanceByBodyPart { bodyshot_mult: f64, weakpoint_bonus: f64 },
    /// ONE STAT FROM THE OTHER, capped. `from_crit` says which way round:
    /// Wiseman's Regard reads crit and pays status, High Ground the mirror.
    DerivedStat { from_crit: bool, rate: f64, cap: f64 },
    /// A grant the PLAYER's state switches on — "With Armor Over 450: +80%
    /// Multishot", "With Energy Max Over 700: +1x Base Critical Damage
    /// Multiplier", "With Sprint Speed 1.2 or Higher: +60% Projectile Speed".
    ///
    /// One variant for all of them: the gate and the bracket are both data, so
    /// the next perk that asks about the player is a yaml block.
    GatedByTenno {
        gate: crate::loadout::TennoGate,
        grant: crate::loadout::GatedGrant,
        value: f64,
    },
    /// Vicious Promise: crit chance and crit multiplier while the target has
    /// taken no damage. One variant for both halves, because the card grants
    /// them together and the condition is one sentence.
    CritOnUndamaged { crit_chance: f64, crit_multiplier: f64 },
    /// A RELOAD-SPEED bonus, into the same bucket the mods feed.
    ///
    /// The most common perk in the whole Incarnon set — Rapid Reinforcement is
    /// on 14 guns by docs/INCARNON.md's count, more than any other name — and it
    /// sat inert for all of them because this loader had no arm while the MODS
    /// loader did. One arm removes a slot from half the remaining program, which
    /// is what the intake kept demonstrating four rows at a time.
    ///
    /// UNCONDITIONAL ONLY. Ready Retaliation's "On Reload from Empty: +100%
    /// Reload Speed" is a different perk and stays inert: `condition:` is not
    /// read here, and granting a conditional bonus unconditionally is the one
    /// mistake worse than not granting it.
    ReloadSpeedBonus(f64),
    /// EXECUTIONER'S FORTUNE — a headshot has a chance to fill the magazine
    /// outright, no reload played.
    ///
    /// Two weapons word it two ways and `needs_kill` is the whole difference:
    /// the Furis pair pay on any headshot ("On Headshot: 10% chance for Instant
    /// Reload"), the Phenmor only on one that KILLS ("On Headshot kill: 20%
    /// chance to instant Reload"), which is far rarer against a single target
    /// that has to be worn down.
    InstantReloadOnHeadshot { chance: f64, needs_kill: bool },
    /// LINGERING JUDGEMENT — `hits` headshots inside `within` seconds open
    /// `value` extra headshot damage for `duration`.
    ///
    /// The bonus joins the ADDITIVE headshot bracket: "Headshot damage bonus
    /// stacks additively with Primary Deadhead's headshot damage bonus" (wiki,
    /// supplied by the owner 2026-08-10). That is the same bucket the arcane
    /// feeds, so the two sum before the bracket is spent rather than
    /// multiplying — which is what makes the perk worth much less on a Deadhead
    /// build than the card's +50% suggests.
    HeadshotDamageOnStreak { hits: u32, within: f64, value: f64, duration: f64 },
    /// SPITEFUL DEFILEMENT — `value` extra crit DAMAGE while the target carries
    /// fewer than `threshold` distinct status types.
    ///
    /// Two clauses decide where it lands and both are the wiki's, verbatim:
    /// "Bonus is added after mods as a flat value" puts it in the same
    /// after-mods bucket Cold's received crit-damage bonus uses, NOT in the
    /// weapon's base crit damage; and "Multiple instances of the same status
    /// effect are not counted separately, e.g. having 5 corrosive and 5
    /// radiation status effects on a target will not disable this buff" makes
    /// the counter DISTINCT TYPES — which is exactly Condition Overload's
    /// bucket, so the two read the same number and cannot disagree.
    ///
    /// It is therefore the anti-CO perk: the third status TYPE turns it off,
    /// and the third status type is where CO starts paying.
    CritDamageBelowStatusCount { threshold: u32, value: f64 },
    /// READY RETALIATION — reload speed, armed by STARTING a reload from empty
    /// and lasting a while after.
    ///
    /// THE TRIGGER IS THE RELOAD ACTION, NOT ITS COMPLETION (owner,
    /// 2026-08-10), so the reload that armed it is the first thing it speeds
    /// up. That one word is most of the perk's value: on a weapon that always
    /// reloads from empty — which is every weapon in this sim — it behaves
    /// like a permanent reload
    /// mod rather than like a bonus that has to be caught in time.
    ///
    /// The window still matters for what comes AFTER the reload: a transmute,
    /// or a second reload, inside the remaining seconds.
    ///
    /// It is an ordinary reload-speed bonus in every other respect — which is
    /// the correction that made it worth implementing. The Phenmor's page adds
    /// *"Affects untransformed Phenmor. Can affect transition into Incarnon form
    /// with a well-timed manual reload. Does not affect transition from Incarnon
    /// back to base form."* and the last clause is WRONG: nothing about the buff
    /// knows which direction an animation is going, so the revert takes it too.
    /// Ready Retaliation. NO DURATION: the buff is scoped to the reload
    /// action — it arrives when the reload starts and is gone when it ends
    /// (owner, 2026-08-11) — so there is no window to state and nothing that
    /// can lapse halfway through.
    ReloadSpeedOnEmptyReload { value: f64 },
    /// Prelude of Might: "With Critical Chance below 40%: Increase Base
    /// Critical Damage Multiplier by +3x", carrying the wiki's note on the same
    /// row — "Condition is affected by the critical chance increase effect of
    /// Puncture status".
    ///
    /// So the condition asks about the crit chance THE HIT HAS, which is
    /// neither of the two things a `condition:` can express: not the Tenno, not
    /// the target, but the panel the mods produced PLUS every live bonus on top
    /// of it — a target-side one included. That is why it is a variant and not
    /// a gate, and why it is settled in two places: `resolve` grants it against
    /// the panel (the optimistic half) and the sim takes it back per shot.
    CritMultiplierBelowCritChance { value: f64, below: f64 },
    /// Headcracker: "On Headshot: +5% Fire Rate for 2s. Stacks up to 10x",
    /// and — from the raw wikitext, which the rendered page's summary drops —
    /// "This effect has a 50% chance of activating."
    StackingFireRateOnHeadshot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
        chance: f64,
        /// HEADCRACKER IS FIFO, and it is data rather than a constant because
        /// this is the second perk to want it and neither shape is the rule.
        /// See [`crate::loadout::BuffDecay`].
        decay: crate::loadout::BuffDecay,
    },
    /// Stormburst: "On hitting an enemy affected by Electricity: +0.4
    /// Multishot for 2s. Stacks up to 3x."
    /// Blazing Barrel: *"On Firing: +X Multishot. Stacks up to Nx."*
    ///
    /// `base` is which bracket the card names, and it is the whole reason one
    /// perk name needs one variant rather than two: the Strun family reads
    /// "+0.05 BASE Multishot" and the Sybaris family "+5% Multishot", which are
    /// different numbers the moment a multishot mod is equipped.
    ///
    /// NO DURATION — neither wiki page states one, and both state the reset
    /// instead: the stacks stand until a reload (`ClearedBy::Reload`).
    /// A STACKING BUFF, stated entirely in data: what triggers it, what it
    /// grants, how much, how many, how long, and what takes it.
    ///
    /// Written after the fourth perk in a row that needed a new enum variant to
    /// say something the three before it had already said. The vocabulary the
    /// sim runs on — [`crate::loadout::BuffTrigger`], [`crate::loadout::BuffGrant`],
    /// [`crate::loadout::ClearedBy`], [`crate::loadout::BuffDecay`] — is
    /// expressive enough on its own; what was missing was a way for a yaml to
    /// NAME a combination of it. A perk whose trigger and grant both exist is
    /// now a yaml block and no Rust at all.
    ///
    /// The older single-purpose variants are kept where they carry reasoning a
    /// generic one cannot (Ready Retaliation's arming, Reaver's Rapture's burst
    /// arithmetic); this is for the plain ones.
    StackingGrant {
        trigger: crate::loadout::BuffTrigger,
        grant: crate::loadout::BuffGrant,
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
        chance: f64,
        decay: crate::loadout::BuffDecay,
        cleared_by: crate::loadout::ClearedBy,
    },
    StackingMultishotOnFiring {
        per_stack: f64,
        max_stacks: u32,
        base: bool,
    },
    StackingMultishotOnStatus {
        status: crate::damage::DamageType,
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// FLAT crit chance added AFTER mods (Elemental Excess: "Bonuses are
    /// added after mods as a flat value") — NOT the base-stat layer that
    /// Commodore's Fortune occupies.
    PostModCritChance(f64),
    /// FLAT status chance added after mods (Elemental Excess).
    PostModStatusChance(f64),
    /// Additive headshot-damage bonus (Caput Mortuum): joins the headshot
    /// bracket `(1 + Σ)` that multiplies the body-part multiplier.
    HeadshotDamage(f64),
    /// Devouring Attrition: on an instance that did NOT crit, `chance` to
    /// multiply it by `(1 + value)`. An INDEPENDENT multiplier ("multiplicative
    /// to base damage bonuses such as Hornet Strike") that applies to BOTH
    /// attack parts, the radial explosion included.
    ChanceDamageOnNoncrit { chance: f64, value: f64 },
    /// Incarnon gauge fill rate (Incarnon Efficiency): weakpoint hits build
    /// `1 + value` times the charge, so the hits needed to fill divide by it.
    IncarnonChargeRate(f64),
    /// Overwhelming Attrition: a hit that is NEITHER critical NOR applies a
    /// status grants a stack worth `+per_stack` damage for `duration`; on
    /// timeout ONE stack drops and the timer resets (the Galvanized decay,
    /// wiki-verbatim). The bonus is ADDITIVE to the base-damage bucket
    /// ("additive to base damage bonuses such as Hornet Strike") — unlike
    /// [`EvoEffect::ChanceDamageOnNoncrit`], which the same page calls
    /// multiplicative.
    StackingDamageOnPlainHit {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Lethal Rearmament: every HEADSHOT grants a stack of reload speed
    /// for `duration`, one stack lost per timeout (the Galvanized decay).
    /// Reload speed also scales the Incarnon transmute animations, so this
    /// shortens the whole cycle, not just reloads.
    StackingReloadSpeedOnHeadshot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// No damage payload here (holstered regen, recoil, timed utility
    /// buffs, the weapon unlock) — kept so the evolution loads and lists.
    /// THE TRANSFORMATION ITSELF — tier 1 of every Incarnon ladder, naming the
    /// form it unlocks. It changes no stat (the form's own entry carries those)
    /// and it is not a CHOICE: every one of these is `selection: fixed`,
    /// because installing the Genesis is what grants it.
    ///
    /// It was parsed as `Inert("unlocks_weapon")` and the target dropped on the
    /// floor, which left "which evolution unlocks the form" to be guessed from
    /// LADDER POSITION ("tier 1's first option"). Reading it is what lets the
    /// form and the evolution stop being two controls for one fact — asking to
    /// fire the Incarnon form implies the evolution that IS firing it (user,
    /// 2026-08-04).
    UnlocksForm(String),
    Inert(String),
    /// NOT A TODO — AN EDGE. The clause is understood and cannot pay out in
    /// this simulator, and one of `docs/UNMODELLED.md`'s classes says why.
    ///
    /// The mods have had this distinction since 2026-08-05 (`not_modeled` vs
    /// `out_of_scope`) for a reason the evolutions inherited without the fix:
    /// printing "not modelled yet" over both is what made the whole app look
    /// unfinished. A perk waiting on work someone can do and a perk waiting on
    /// a second body in the arena are different sentences to a player deciding
    /// what to equip.
    ///
    /// It stays OFF the ratchet in `unmodeled_effects` — nothing about this
    /// engine will ever close it — and ON the page, which is where the
    /// difference is for.
    OutOfScope { clause: String, reason: Scope },
    /// THE GAME DOES NOT DO IT. The card states a clause, the clause pays
    /// nothing when measured, and a hotfix restores it.
    ///
    /// A THIRD PROMISE, and the only one that is not a shortfall of ours
    /// (`live_bugs:` on an arcane has said the same thing since Primary
    /// Debilitate). [`EvoEffect::Inert`] is work someone can do and
    /// [`EvoEffect::OutOfScope`] is the edge of what a single-target damage
    /// simulator is; this one says the model is RIGHT and reality is broken.
    /// Reporting it as either of the others would tell a reader to wait for
    /// us, when what they should do is not pick the perk.
    ///
    /// The `clause` is the effect's own kind, so the line names WHICH half of
    /// a two-clause perk is dead — Carnage Reign's +60 base damage works and
    /// its "+33% per Status Type" does not (MEASUREMENTS M49).
    LiveBug { clause: String, note: String },
    /// A clause that QUALIFIES a neighbouring effect rather than being one —
    /// "Stacks up to 4x" on a card whose stacking bonus is the effect above it.
    ///
    /// It is not a gap and it must not be counted as one. All 51 of these sit
    /// in a perk that ALREADY declares a real gap (the conditional bonus they
    /// cap is itself inert), so counting them said "partly modelled" twice for
    /// one thing and put a third of the roster's inert total on a fragment of
    /// a sentence. `a_qualifier_never_stands_alone` is what keeps that true:
    /// the day one appears beside a working effect, it IS a gap — the cap goes
    /// unenforced — and the test fails so somebody looks.
    Qualifier(String),
}

/// A parsed Incarnon evolution.
#[derive(Debug, Clone)]
pub struct EvolutionDef {
    pub id: String,
    pub name: String,
    pub weapon: String,
    pub tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    pub icon: Option<String>,
    /// Verbatim effect text — what the cards display (like mods/arcanes).
    pub description: String,
    pub currently_broken: bool,
    /// What this perk DECLARES about feeding the weapon's GunCO term, or
    /// `None` for "nobody has said" — which is the common case and means the
    /// term IS fed. See [`Self::excludes_co_base`].
    pub co_base_excludes_this_evolution: Option<bool>,
    /// The FORM a declaration was measured on, when it covers only one — see
    /// the loader field of the same name.
    pub co_base_excludes_only_form: Option<crate::weapons_data::FormKind>,
    /// Everything this evolution grants applies to the BASE form only — see
    /// the loader field of the same name.
    pub base_form_only: bool,
    /// WHERE THE CARD AND THE GAME DISAGREE — one line per clause, each saying
    /// what the card prints and what the effect actually does.
    ///
    /// THE FOURTH KIND OF ADMISSION, and the second that is not a shortfall of
    /// ours. [`EvoEffect::Inert`] is work someone can do,
    /// [`EvoEffect::OutOfScope`] is the edge of what this simulator is, and
    /// [`EvoEffect::LiveBug`] says the model is right and the game is broken.
    /// This one says the model is right and the CARD is wrong: the effect works,
    /// it simply does not do what it says.
    ///
    /// It is NOT a live bug and must not be reported as one. A live bug tells a
    /// reader not to pick the perk; this tells them the perk is better or worse
    /// than its own text, which is the opposite advice. Swift Punishment prints
    /// "With Sprint Speed 1.2 or Higher" and its own wiki row says *"Despite
    /// the description, the effect only requires 1.1"* — a player reading the
    /// card would mod for a threshold the game does not ask for.
    ///
    /// Owner, 2026-08-18: anything that differs from what the game DISPLAYS is
    /// to be noted. It rides beside the effect rather than short-circuiting it,
    /// which is the whole difference from `live_bug` in the same position.
    misprints: Vec<String>,
    effects: Vec<EvoEffect>,
}

impl EvolutionDef {
    /// Σ flat base damage this evolution adds (0 when broken) — the panel
    /// attributes it as a non-mod source on the Base Damage row.
    pub fn flat_base_damage(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE crit chance (Commodore's Fortune; 0 when broken).
    pub fn flat_base_crit_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseCritChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE status chance (Survivor's Edge, Elemental Balance).
    pub fn flat_base_status_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseStatusChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE magazine rounds (Extended Volley).
    /// Σ flat BASE magazine this perk grants THIS player — the gated spelling of
    /// [`Self::flat_base_magazine`] below (Lone Gun's "+14 Base Magazine
    /// Capacity", which the card owes only when nothing else is carried).
    ///
    /// It cannot be folded where the ungated one is: `apply` never sees a
    /// Tenno, so the resolver answers the gate against `WeaponBase::gated`. The
    /// panel needs the same number attributed to the perk that grants it —
    /// a magazine that grew with no source listed is the panel telling half a
    /// story — and `base.gated` has no owner to name, so it asks here.
    pub fn gated_flat_magazine(&self, tenno: &crate::tenno_data::Tenno) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::GatedByTenno { gate, grant, value }
                    if *grant == crate::loadout::GatedGrant::FlatBaseMagazine
                        && gate.open(tenno) =>
                {
                    Some(*value)
                }
                _ => None,
            })
            .sum()
    }

    pub fn flat_base_magazine(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseMagazine(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ assumed-max multishot from permanent stacks (Fevered Frenzy).
    pub fn assumed_multishot(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::AssumedMaxMultishot { total, .. } => Some(*total),
                _ => None,
            })
            .sum()
    }

    /// The permanent stacked-multishot buff, if this evolution grants one:
    /// (full-stack bonus, max stacks). Drives the configurable buff card.
    pub fn ms_buff(&self) -> Option<(f64, u32)> {
        self.active_effects().find_map(|e| match e {
            EvoEffect::AssumedMaxMultishot { total, max_stacks } => Some((*total, *max_stacks)),
            _ => None,
        })
    }
}

/// One configurable buff an evolution grants — everything the Sim's and
/// the Optimizer's buff cards need, with no caller-side knowledge of which
/// effect produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvoBuffCard {
    /// The `apply_buff_config` key this card writes.
    pub id: &'static str,
    pub max_stacks: u32,
    /// PERMANENT stacks (no in-sim trigger, no decay): the count is a
    /// static choice for the run, so the card defaults locked.
    pub permanent: bool,
    /// WHERE THE CARD OPENS, and there are only two rules: a permanent buff
    /// starts full, an earned one starts at zero.
    ///
    /// A third briefly existed — "one reload's worth", for Mounting Momentum —
    /// and it was wrong twice over. Nothing a player sets should depend on the
    /// weapon's stats when the ceiling is the same 99 for every weapon (owner,
    /// 2026-08-08), and it contradicted the sim, which opens that buff at zero
    /// because an empty magazine takes the pile. A card that defaults to six
    /// while the fight starts at none is the plainest kind of lie a panel can
    /// tell.
    pub opens_at: CardOpens,
}

/// See [`EvoBuffCard::opens_at`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOpens {
    /// Earned in the run.
    Zero,
    /// Nothing decays it, so a lull does not cost it.
    Full,
}

impl EvolutionDef {
    /// EVERY configurable buff this evolution grants.
    ///
    /// The match below is EXHAUSTIVE on purpose: adding an `EvoEffect`
    /// variant fails to compile until someone states whether it is a buff
    /// the user can configure. That is the whole point — a buff that
    /// exists in the engine but not on the cards is invisible, and the
    /// only way to keep the two in step is to make forgetting impossible.
    /// `permanent` is the ONE thing this has to get right: a permanent buff
    /// has no trigger and no decay, so it survives a lull and starts full,
    /// while every timed buff starts EARNED at zero (docs/BUFFS.md).
    pub fn buff_cards(&self) -> Vec<EvoBuffCard> {
        self.active_effects()
            .filter_map(|e| match e {
                // No card: nothing to configure about a payout that cannot
                // happen. The disclosure line is what this perk gets.
                EvoEffect::OutOfScope { .. } => None,
                // PAYS NOTHING, which is the whole point — the sim reproduces
                // the game, and the game pays nothing here.
                EvoEffect::LiveBug { .. } => None,
                EvoEffect::AssumedMaxMultishot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "evo_multishot",
                    max_stacks: *max_stacks,
                    permanent: true,
                    opens_at: CardOpens::Full,
                }),
                EvoEffect::StackingDamageOnPlainHit { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_plain_hit_damage",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                // A BUFF, not a silent stat: the run holds it from t = 0, but
                // it is earned by an empty reload and the bar has to say so
                // (user, 2026-08-03). Permanent — nothing decays it — and one
                // stack, which is what "on/off" is in this vocabulary.
                EvoEffect::FlatBaseDamageOnEmptyReload(_) => Some(EvoBuffCard {
                    id: "evo_reload_damage",
                    max_stacks: 1,
                    permanent: true,
                    opens_at: CardOpens::Full,
                }),
                EvoEffect::StackingReloadSpeedOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_reload_speed",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::StackingGrant { trigger, grant, max_stacks, .. } => Some(EvoBuffCard {
                    id: stacking_card_id(*trigger, *grant),
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::StackingMultishotOnFiring { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_firing_multishot",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::StackingMultishotOnStatus { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_status_multishot",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::StackingFireRatePerShellReloaded { max_stacks, .. } => {
                    Some(EvoBuffCard {
                        id: "per_shell_fire_rate",
                        max_stacks: *max_stacks,
                        permanent: false,
                        opens_at: CardOpens::Zero,
                    })
                }
                EvoEffect::StackingFireRateOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_fire_rate",
                    max_stacks: *max_stacks,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                // READY RETALIATION IS NOT A CARD, and the distinction is the
                // one this list exists for. A card is a CONTROL: it configures
                // a buff the sim cannot earn on its own (Fevered Frenzy's
                // permanent stacks) or locks one it can. This buff is earned by
                // an event the sim already simulates — a reload from empty —
                // and there is nothing for a player to set, so a card here
                // would be a control that does nothing.
                //
                // It is still VISIBLE, which is what BUFFS.md actually requires:
                // `buff_roster` lists it and the replay draws its window, the
                // same way Pressurized Magazine's `on_reload_fr` is drawn
                // without an evolution card.
                // LINGERING JUDGEMENT earns its window from a headshot STREAK,
                // so the card starts at zero like every other earned buff.
                EvoEffect::HeadshotDamageOnStreak { .. } => Some(EvoBuffCard {
                    id: "evo_headshot_streak",
                    max_stacks: 1,
                    permanent: false,
                    opens_at: CardOpens::Zero,
                }),
                EvoEffect::ReloadSpeedOnEmptyReload { .. }
                // Nor is Executioner's Fortune: it is a roll on an event the
                // sim already has, and its whole effect is a magazine counter.
                | EvoEffect::InstantReloadOnHeadshot { .. }
                // Nor is Spiteful Defilement: its condition is the TARGET's,
                // read live, with nothing for a player to set.
                | EvoEffect::CritDamageBelowStatusCount { .. }
                // Static stat changes — nothing to configure at runtime.
                | EvoEffect::FlatBaseStatusChanceByForm { .. }
                | EvoEffect::FlatBaseCritMultiplier(_)

                | EvoEffect::Indirect(..)
                | EvoEffect::AmmoMaxSet(_)
                | EvoEffect::FlatBaseDamage(_)
                | EvoEffect::FlatBaseCritChance(_)
                | EvoEffect::FlatBaseMultishot(_)
                | EvoEffect::FlatBaseStatusChance(_)
                | EvoEffect::FlatBaseMagazine(_)
                | EvoEffect::FieldDurationOnEmptyReload(_)
                | EvoEffect::MultishotBeyondRange { .. }
                | EvoEffect::MultishotOnLastRound { .. }
                | EvoEffect::BaseDamagePerFullBurst { .. }
                | EvoEffect::ArmorStripPerPunctureStatus(_)
                | EvoEffect::MultishotConsumesAmmo(_)
                | EvoEffect::ConditionOverload { .. }
                | EvoEffect::FireRateBonus { .. }
                | EvoEffect::BaseDamageBelowHalfHealth { .. }
                | EvoEffect::GatedByTenno { .. }
                | EvoEffect::DerivedStat { .. }
                | EvoEffect::CritChanceByBodyPart { .. }
                | EvoEffect::RoundRestoreOnStatusHit { .. }
                | EvoEffect::InstantReloadOnKill { .. }
                | EvoEffect::MagGrowthOnEmptyReload { .. }
                | EvoEffect::CritOnUndamaged { .. }
                | EvoEffect::ReloadSpeedBonus(_)
                | EvoEffect::CritMultiplierBelowCritChance { .. }
                | EvoEffect::PostModCritChance(_)
                | EvoEffect::PostModStatusChance(_)
                | EvoEffect::HeadshotDamage(_)
                | EvoEffect::IncarnonChargeRate(_) => None,
                // Rolled per instance, not a buff with an uptime.
                EvoEffect::ChanceDamageOnNoncrit { .. } => None,
                // The transformation grants no CARD: what it unlocks is a
                // FORM, whose own weapon entry carries every stat it brings.
                EvoEffect::UnlocksForm(_)
                | EvoEffect::Inert(_)
                | EvoEffect::Qualifier(_) => None,
            })
            .collect()
    }
}

impl EvolutionDef {

    /// Σ unconditional CO rate per status type (Carnage Reign).
    pub fn co_per_type(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::ConditionOverload { per_type, .. } => Some(*per_type),
                _ => None,
            })
            .sum()
    }

    fn active_effects(&self) -> impl Iterator<Item = &EvoEffect> {
        // Broken evolutions contribute nothing (same rule as `apply`).
        self.effects
            .iter()
            .filter(move |_| !self.currently_broken)
    }

    /// WHAT THIS PERK DOES NOT DO YET — the effects that loaded as `Inert`,
    /// named.
    ///
    /// DERIVED, never declared. An `unmodeled: true` field beside the effects
    /// would be a second copy of the truth, free to disagree with the loader
    /// the moment somebody implements one and forgets the flag. This asks the
    /// loaded effects, so a perk stops confessing the instant it is modelled
    /// and starts the instant a new unknown kind is written.
    ///
    /// Empty means every effect is modelled. It is the honest thing for the
    /// UI to show and the honest thing to grep for (user, 2026-08-06: 如果有
    /// 的东西没做完，得说这个东西未完成 …… 不要隐瞒欺骗自己).
    pub fn unmodeled_effects(&self) -> Vec<&str> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::Inert(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The other admission: clauses that CANNOT pay out here, each with the
    /// class that says why.
    ///
    /// Separate from `unmodeled_effects` because the two are different promises
    /// — one is work someone can do, the other is the edge of what a
    /// single-target damage simulator is — and because the ratchet must only
    /// count the first. See [`EvoEffect::OutOfScope`].
    pub fn out_of_scope_effects(&self) -> Vec<String> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::OutOfScope { clause, reason } => {
                    Some(format!("{clause} — {}", reason.why()))
                }
                _ => None,
            })
            .collect()
    }

    /// DOES THIS PERK'S FLAT BASE DAMAGE STAY OUT OF THE GunCO TERM, on an
    /// entry of this form and this CO class?
    ///
    /// **A DECLARATION WINS, AND IT IS SCOPED TO WHAT WAS MEASURED.**
    /// `only_form` exists because a perk reaches BOTH entries of its transform
    /// group while a reading comes off ONE of them: the Torid's Incarnon form
    /// was measured (M50) and its base form was not, and they are not even the
    /// same CO class. A declaration that does not reach this form falls through
    /// to the default below rather than answering for it.
    ///
    /// **AN UNDECLARED PERK IS ANSWERED BY THE ENTRY'S CO CLASS**, and the two
    /// halves have very different amounts of evidence behind them (owner,
    /// 2026-08-16 — he drew the line here himself, twice):
    ///
    ///   · `Adding` — EXCLUDED. Fifteen to zero. Eleven catalog rows print a
    ///     DOUBLE damage value ("100 or 124 (with Evolution II)") and are the
    ///     only rows where anyone measured a weapon with its evolution
    ///     installed; all eleven exclude. Four owner measurements agree — the
    ///     Dual Toxocyst's two tier-2 perks (M49) and the Torid Incarnon's two
    ///     (M50). Against that, NOTHING anywhere has measured an evolved weapon
    ///     and found its evolution fed the term: every other catalog row prints
    ///     a single number that is the UNEVOLVED base, so it says the CO bonus
    ///     equals the base of a weapon with no evolution on it, which is true
    ///     by construction. Three of the four measurements are on perks the
    ///     catalog does not list, so its silence has been tested three times
    ///     and meant "unmeasured" every time.
    ///   · `Multiplying` — INCLUDED, and it is the CLASS that answers rather
    ///     than a default the perks happen to agree with. MEASURED on the
    ///     Torid's base form (M51), which is `Multiplying` where the form of
    ///     M50 is `Adding`: the same two tier-2 perks, +51 and +31, and the CO
    ///     multiplier came back 1.40 and 1.80 under BOTH — identical, where a
    ///     term reading the unevolved base would have printed 1.265 under the
    ///     +51 and 1.305 under the +31. The two answers are OPPOSITE, so the
    ///     "may well be the same on both sides" this comment used to carry was
    ///     wrong: which base the term reads is decided by the class, not
    ///     upstream of it.
    ///
    /// So THE CLASS ANSWERS FIRST ON A `Multiplying` ENTRY, above the
    /// declaration (owner, 2026-08-16). A perk reaches every form of its
    /// transform group and only one of them was ever the measured one, so a
    /// reading off an `Adding` form must not be able to reach across and dilute
    /// a `Multiplying` one — which the Torid's pair would do today without
    /// `only_form`, and which the NEXT such perk would do by forgetting it. The
    /// generalisation is deliberate and runs ahead of the catalog: the wiki
    /// lists a fraction for a minority of entries, and the owner's call is that
    /// this rule beats that table, to be revisited PER WEAPON if a measurement
    /// ever contradicts it (2026-08-16).
    ///
    /// THE RESERVED SLOT is the per-entry `co_base_fraction:` in the weapon
    /// yaml, which is 1.0 on all 26 `Multiplying` entries and is where a future
    /// measurement would land — one weapon's file, with nothing here to change.
    ///
    /// `Inert` gets the Adding answer and never reads it — it computes no CO
    /// term at all.
    pub fn excludes_co_base(
        &self,
        form: crate::weapons_data::FormKind,
        behavior: crate::loadout::CoBehavior,
    ) -> bool {
        if behavior == crate::loadout::CoBehavior::Independent {
            return false;
        }
        match self.co_base_excludes_this_evolution {
            Some(v) if self.co_base_excludes_only_form.is_none_or(|f| f == form) => v,
            _ => true,
        }
    }

    /// WHAT THE GAME ITSELF DOES NOT DO — the clauses measured to pay nothing,
    /// each with the note that says how we know.
    ///
    /// NOT counted by [`Self::fully_unmodeled`], deliberately: that question is
    /// about OUR shortfalls, and a reader deciding whether to wait for us is
    /// asking something different from a reader deciding whether to pick a
    /// perk DE has broken. Both reach the page; only one is work.
    /// WHERE THIS PERK'S CARD IS WRONG — see [`Self::misprints`] the field.
    ///
    /// NOT counted by [`Self::fully_unmodeled`] and not reported beside the
    /// live bugs: a perk whose card misstates a threshold is fully modelled and
    /// fully working, and filing it with the broken ones would tell a reader to
    /// avoid the one thing they should be told to trust.
    pub fn misprints(&self) -> &[String] {
        &self.misprints
    }

    pub fn live_bugs(&self) -> Vec<String> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::LiveBug { clause, note } => Some(format!("{clause} — {note}")),
                _ => None,
            })
            .collect()
    }

    /// Does this perk do NOTHING the sim can see? A perk whose every effect is
    /// inert is not a weaker choice, it is not a choice — and the tile you pick
    /// from should say so rather than look like its working tier-mates.
    pub fn fully_unmodeled(&self) -> bool {
        // BOTH admissions count here. The tile asks "is this a choice at all",
        // and a perk whose every clause is an EDGE is no more of one than a
        // perk whose every clause is a todo — the difference is why, not
        // whether.
        !self.effects.is_empty()
            && self.unmodeled_effects().len() + self.out_of_scope_effects().len()
                == self.effects.len()
    }

    /// One display line per effect — what the model computes (broken
    /// evolutions state the zero honestly at the call site, not here).
    pub fn describe(&self) -> Vec<String> {
        self.effects
            .iter()
            .map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => {
                    format!("+{v:.0} base damage (pro-rata, before mods)")
                }
                EvoEffect::FlatBaseCritChance(v) => {
                    format!("+{:.0}% BASE crit chance (crit mods multiply it)", v * 100.0)
                }
                EvoEffect::FlatBaseMultishot(v) => {
                    format!("+{v:.2} BASE multishot (multishot mods multiply it)")
                }
                EvoEffect::StackingFireRatePerShellReloaded { per_stack, max_stacks } => {
                    format!(
                        "+{:.0}% fire rate per shell reloaded, up to {max_stacks} stacks                          (+{:.0}% at the cap) — a full magazine is one reload's worth",
                        per_stack * 100.0,
                        per_stack * *max_stacks as f64 * 100.0
                    )
                }
                EvoEffect::FlatBaseStatusChanceByForm { base, incarnon } => format!(
                    "+{:.0}% BASE status chance ({:.0}% in Incarnon Form)",
                    base * 100.0,
                    incarnon * 100.0
                ),
                EvoEffect::FlatBaseCritMultiplier(v) => {
                    format!("+{v:.2}x BASE crit multiplier (crit damage mods multiply it)")
                }
                EvoEffect::Indirect(stat, v) => {
                    // Percent for the fractional stats, a bare number for the
                    // ones measured in their own unit (punch through: metres).
                    if matches!(stat, crate::loadout::IndirectStat::PunchThrough) {
                        format!("{:+.1} m {}", v, stat.label())
                    } else {
                        format!("{:+.0}% {}", v * 100.0, stat.label())
                    }
                }
                EvoEffect::AmmoMaxSet(v) => format!("ammo reserve set to {v:.0}"),
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => format!(
                    "+{v:.0} base damage from the moment an empty reload starts — held all run"
                ),
                EvoEffect::FlatBaseStatusChance(v) => format!(
                    "+{:.0}% BASE status chance (status mods multiply it)",
                    v * 100.0
                ),
                EvoEffect::FlatBaseMagazine(v) => {
                    format!("+{v:.0} base magazine (magazine mods multiply it)")
                }
                EvoEffect::MultishotBeyondRange { value, metres } => format!(
                    "+{:.0}% multishot with no enemy inside {metres:.0} m — worth nothing at point blank, which is where both boards are scored",
                    value * 100.0
                ),
                EvoEffect::FieldDurationOnEmptyReload(v) => format!(
                    "On reload from empty: x{v:.0} lingering-field duration on the next shot"
                ),
                EvoEffect::ArmorStripPerPunctureStatus(v) => format!(
                    "removes {:.0}% of the target's armour per Puncture status",
                    v * 100.0
                ),
                EvoEffect::BaseDamagePerFullBurst { per_stack, max_stacks } => format!(
                    "+{:.0}% base damage per full burst, x{max_stacks} (+{:.0}% at the cap), \
                     reset when the magazine is refilled",
                    per_stack * 100.0,
                    per_stack * f64::from(*max_stacks) * 100.0
                ),
                EvoEffect::MultishotOnLastRound { value, base } => format!(
                    "+{value:.0} {}multishot on the last round of the magazine (base form only)",
                    if *base { "base " } else { "" }
                ),
                EvoEffect::MultishotConsumesAmmo(v) => format!(
                    "+{:.0}% damage on multishot-generated projectiles; multishot consumes ammo",
                    v * 100.0
                ),
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => format!(
                    "+{:.0}% multishot ({max_stacks} on-ability-cast stacks, full by default)",
                    total * 100.0
                ),
                EvoEffect::ConditionOverload { per_type, min_sprint } => format!(
                    "+{:.0}% direct damage per status type on the target{}",
                    per_type * 100.0,
                    if *min_sprint > 0.0 {
                        format!(", at sprint speed {min_sprint} or higher")
                    } else {
                        String::new()
                    }
                ),
                EvoEffect::CritOnUndamaged { crit_chance, crit_multiplier } => format!(
                    "+{:.0}% BASE crit chance and +{crit_multiplier}x BASE crit damage while the                      target is undamaged (mods multiply both)",
                    crit_chance * 100.0
                ),
                EvoEffect::MagGrowthOnEmptyReload { per_stack, max_stacks } => format!(
                    "+{per_stack:.0} BASE magazine capacity on each reload from empty, up to {max_stacks} times (magazine mods multiply each stack)"
                ),
                EvoEffect::InstantReloadOnKill { chance } => format!(
                    "{:.0}% chance of an instant reload on any kill, including a status kill",
                    chance * 100.0
                ),
                EvoEffect::RoundRestoreOnStatusHit { status, chance, rounds } => format!(
                    "{:.0}% chance per shot to restore {rounds:.0} round from the ammo pool when the target carries a {status:?} status",
                    chance * 100.0
                ),
                EvoEffect::CritChanceByBodyPart { bodyshot_mult, weakpoint_bonus } => format!(
                    "x{bodyshot_mult:.0} crit chance on body shots, +{:.0}% BASE crit chance on weak points (additive with the crit mods)",
                    weakpoint_bonus * 100.0
                ),
                EvoEffect::DerivedStat { from_crit, rate, cap } => format!(
                    "+{:.0}% of current {} as base {}, up to +{:.0}%",
                    rate * 100.0,
                    if *from_crit { "crit chance" } else { "status chance" },
                    if *from_crit { "status chance" } else { "crit chance" },
                    cap * 100.0
                ),
                // ONE ARM PER BRACKET, and each says what the UNGATED spelling
                // of the same grant says — because the brackets do not share
                // units and the single line that used to be here multiplied
                // every one of them by 100 and then printed the Rust
                // identifier. Haven Foray's "+50 base damage with overshields"
                // read `+5000% FlatBaseDamage with overshields`, and Paladin
                // Virtue's +0.5x crit multiplier read `+50% BaseCritDamage` —
                // wrong on twenty cards, on the one line a player can check
                // (2026-08-13). The `>= 1.0` special case was the shape of the
                // bug: a unit chosen by the SIZE of the number rather than by
                // the bracket it lands in.
                EvoEffect::GatedByTenno { gate, grant, value } => {
                    use crate::loadout::GatedGrant as G;
                    let what = match grant {
                        G::FlatBaseDamage => {
                            format!("+{value:.0} base damage (pro-rata, before mods)")
                        }
                        G::FlatBaseMagazine => {
                            format!("+{value:.0} base magazine (magazine mods multiply it)")
                        }
                        G::BaseCritDamage => {
                            format!("+{value}x BASE crit damage (crit-damage mods multiply it)")
                        }
                        G::ConditionOverload => format!(
                            "+{:.0}% direct damage per status type on the target",
                            value * 100.0
                        ),
                        G::FireRate => format!("+{:.0}% fire rate", value * 100.0),
                        G::Multishot => format!("+{:.0}% multishot", value * 100.0),
                        // ACCURACY narrows the cone a pellet draws inside, so
                        // the card is stated as what it does rather than as the
                        // stat's name: at point blank it is worth nothing and
                        // at a range it is the difference between landing and
                        // not (`space`, `loadout::Spread`).
                        G::Accuracy => format!(
                            "+{:.0}% accuracy — a tighter cone, so more pellets land at a distance",
                            value * 100.0
                        ),
                        G::ProjectileSpeed => {
                            format!("+{:.0}% projectile speed", value * 100.0)
                        }
                    };
                    format!("{what} {}", gate.describe())
                }
                EvoEffect::BaseDamageBelowHalfHealth { rate: v, .. } => format!(
                    "+{:.0}% damage while the target is under half health",
                    v * 100.0
                ),
                EvoEffect::FireRateBonus { value, min_sprint } => format!(
                    "+{:.0}% fire rate{}",
                    value * 100.0,
                    if *min_sprint > 0.0 {
                        format!(" at sprint speed {min_sprint} or higher")
                    } else {
                        String::new()
                    }
                ),
                EvoEffect::ReloadSpeedBonus(v) => format!("+{:.0}% reload speed", v * 100.0),
                EvoEffect::HeadshotDamageOnStreak { hits, within, value, duration } => format!(
                    "+{:.0}% headshot damage for {duration:.0}s after {hits} headshots in {within:.0}s",
                    value * 100.0
                ),
                EvoEffect::CritDamageBelowStatusCount { threshold, value } => format!(
                    "+{:.0}% critical damage while the target has fewer than {threshold} status types",
                    value * 100.0
                ),
                EvoEffect::InstantReloadOnHeadshot { chance, needs_kill } => format!(
                    "{:.0}% chance to fill the magazine on a headshot{}",
                    chance * 100.0,
                    if *needs_kill { " kill" } else { "" }
                ),
                EvoEffect::ReloadSpeedOnEmptyReload { value } => format!(
                    "+{:.0}% reload speed on a reload from empty, for that reload",
                    value * 100.0
                ),
                EvoEffect::StackingGrant { trigger, grant, per_stack, max_stacks, duration, chance, .. } => format!(
                    "{per_stack} {grant:?} per stack x{max_stacks} on {trigger:?}{}{}",
                    if duration.is_finite() { format!(" for {duration:.1}s") } else { String::new() },
                    if *chance < 1.0 { format!(", {:.0}% of the time", chance * 100.0) } else { String::new() }
                ),
                EvoEffect::StackingMultishotOnFiring { per_stack, max_stacks, base } => format!(
                    "+{per_stack} {} multishot per stack x{max_stacks} on firing, until a reload",
                    if *base { "base" } else { "bucket" }
                ),
                EvoEffect::StackingMultishotOnStatus { status, per_stack, max_stacks, duration } => format!(
                    "+{per_stack} multishot per stack x{max_stacks} for {duration:.0}s while the                      target carries {status:?} (flat, like Final Fusillade's)"
                ),
                EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance, .. } => format!(
                    "+{:.0}% fire rate per stack x{max_stacks} for {duration:.0}s on headshot, \
                     {:.0}% chance each (additive with fire-rate mods)",
                    per_stack * 100.0,
                    chance * 100.0
                ),
                EvoEffect::CritMultiplierBelowCritChance { value, below } => format!(
                    "+{value:.1}x BASE crit multiplier while crit chance stays under {:.0}% \
                     (crit damage mods multiply it; the condition is checked per shot \
                     against the LIVE crit chance, so Puncture's Weakened can push a \
                     build over the line and take it away)",
                    below * 100.0
                ),
                EvoEffect::PostModCritChance(v) => format!(
                    "{}{:.0}% crit chance, flat AFTER mods",
                    if *v >= 0.0 { "+" } else { "" },
                    v * 100.0
                ),
                EvoEffect::PostModStatusChance(v) => format!(
                    "{}{:.0}% status chance, flat AFTER mods",
                    if *v >= 0.0 { "+" } else { "" },
                    v * 100.0
                ),
                EvoEffect::HeadshotDamage(v) => {
                    format!("+{:.0}% headshot damage (direct hits only)", v * 100.0)
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => format!(
                    "+{:.0}% reload speed per headshot ({max_stacks} stacks, {duration:.0}s) — shortens the transmutes too",
                    per_stack * 100.0
                ),
                EvoEffect::ChanceDamageOnNoncrit { chance, value } => format!(
                    "{:.0}% chance of +{:.0}% damage on a NON-crit instance (own multiplier, radial included)",
                    chance * 100.0,
                    value * 100.0
                ),
                EvoEffect::IncarnonChargeRate(v) => format!(
                    "weakpoint hits build +{:.0}% Incarnon charge",
                    v * 100.0
                ),
                EvoEffect::StackingDamageOnPlainHit {
                    per_stack,
                    max_stacks,
                    duration,
                } => format!(
                    "+{:.0}% damage per stack ({max_stacks} max, {duration:.0} s) on a hit that neither crits nor procs",
                    per_stack * 100.0
                ),
                EvoEffect::UnlocksForm(w) => {
                    format!("unlocks the {w} form — its stats are that form's own")
                }
                EvoEffect::Inert(what) => {
                    format!("{} (no single-target DPS effect)", what.replace('_', " "))
                }
                EvoEffect::OutOfScope { clause, reason } => {
                    format!("{clause} — {}", reason.why())
                }
                // NAMED AS DEAD ON THE CARD ITSELF. A reader comparing two
                // tier-2 options must see which half of this one pays nothing
                // — printing it like a working clause is the one thing this
                // line must never do.
                EvoEffect::LiveBug { clause, note } => {
                    format!("{clause} — DOES NOT WORK IN GAME: {note}")
                }
                // Said as what it is — a cap on the line above, not a line of
                // its own claiming the perk does less than it does.
                EvoEffect::Qualifier(what) => {
                    format!("{} (a cap on the bonus above)", what
                        .trim_start_matches("unmodelled_").replace('_', " "))
                }
            })
            .collect()
    }
}

/// `stat:` names an [`IndirectStat`]. Deliberately EXPLICIT rather than a
/// fuzzy match: an unknown name falls through to `Inert(...)` and the pinned
/// inert test then fails, which is how a typo announces itself instead of
/// silently contributing nothing.
fn indirect_stat(name: &str) -> Option<crate::loadout::IndirectStat> {
    use crate::loadout::IndirectStat as I;
    Some(match name {
        "recoil" => I::Recoil,
        "accuracy" => I::Accuracy,
        "punch_through" => I::PunchThrough,
        "projectile_speed" => I::ProjectileSpeed,
        "holstered_reload_per_second" => I::HolsteredReload,
        "movement_speed_aiming" => I::MovementSpeed,
        "ammo_max" => I::AmmoMax,
        "zoom" => I::Zoom,
        "range" => I::Range,
        "beam_range" => I::BeamRange,
        "noise" => I::Noise,
        _ => return None,
    })
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn effect(v: &Value) -> Option<EvoEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    // A LIVE BUG SHORT-CIRCUITS THE KIND. Declared beside the effect it kills
    // rather than as a flag on the evolution, because a perk's two clauses can
    // disagree — Carnage Reign's +60 base damage works and its "+33% per
    // Status Type" does not (MEASUREMENTS M49). Intercepting here means every
    // effect kind gets it for free and no arm has to remember to check.
    // A MISPRINT DOES NOT SHORT-CIRCUIT, which is the whole difference between
    // it and `live_bug` one line below: the effect works and is parsed exactly
    // as it would be without the note. Collected by the caller, because it
    // belongs to the perk's card rather than to the effect's arithmetic.
    if let Some(note) = v.get("live_bug").and_then(Value::as_str) {
        return Some(EvoEffect::LiveBug {
            clause: kind.replace('_', " "),
            note: note.to_string(),
        });
    }
    Some(match kind {
        "flat_base_damage" => EvoEffect::FlatBaseDamage(f(v, "value").unwrap_or(0.0)),
        "flat_base_crit_chance" => EvoEffect::FlatBaseCritChance(f(v, "value").unwrap_or(0.0)),
        "flat_base_multishot" => EvoEffect::FlatBaseMultishot(f(v, "value").unwrap_or(0.0)),
        "stacking_fire_rate_per_shell_reloaded" => {
            EvoEffect::StackingFireRatePerShellReloaded {
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(0) as u32,
            }
        }
        "flat_base_status_chance" => {
            EvoEffect::FlatBaseStatusChance(f(v, "value").unwrap_or(0.0))
        }
        "flat_base_status_chance_by_form" => EvoEffect::FlatBaseStatusChanceByForm {
            base: f(v, "base").unwrap_or(0.0),
            incarnon: f(v, "incarnon").unwrap_or(0.0),
        },
        "flat_base_crit_multiplier" => {
            EvoEffect::FlatBaseCritMultiplier(f(v, "value").unwrap_or(0.0))
        }
        "flat_base_damage_on_empty_reload" => {
            EvoEffect::FlatBaseDamageOnEmptyReload(f(v, "value").unwrap_or(0.0))
        }
        // The handling family. `indirect` names its target in `stat:`; the
        // rest are named kinds that predate it and keep their spelling so the
        // yaml still reads like the card.
        "indirect" => match v.get("stat").and_then(Value::as_str).and_then(indirect_stat) {
            Some(st) => EvoEffect::Indirect(st, f(v, "value").unwrap_or(0.0)),
            None => EvoEffect::Inert(format!(
                "indirect ({})",
                v.get("stat").and_then(Value::as_str).unwrap_or("no stat")
            )),
        },
        "punch_through_bonus" => {
            EvoEffect::Indirect(crate::loadout::IndirectStat::PunchThrough, f(v, "value").unwrap_or(0.0))
        }
        "accuracy_bonus" => {
            EvoEffect::Indirect(crate::loadout::IndirectStat::Accuracy, f(v, "value").unwrap_or(0.0))
        }
        // NEGATIVE means less recoil, the same convention the MODS carry
        // (Primed Stabilizer ramps -0.15 -> -0.9). A positive value here would
        // read as more recoil, which no evolution grants.
        "recoil_reduction" => {
            EvoEffect::Indirect(crate::loadout::IndirectStat::Recoil, f(v, "value").unwrap_or(0.0))
        }
        "holstered_magazine_regen" => EvoEffect::Indirect(
            crate::loadout::IndirectStat::HolsteredReload,
            f(v, "value").unwrap_or(0.0),
        ),
        "multishot_beyond_range" => EvoEffect::MultishotBeyondRange {
            value: f(v, "value").unwrap_or(0.0),
            metres: f(v, "metres").unwrap_or(0.0),
        },
        "ammo_reserve_set" => EvoEffect::AmmoMaxSet(f(v, "value").unwrap_or(0.0)),
        "flat_base_magazine" => EvoEffect::FlatBaseMagazine(f(v, "value").unwrap_or(0.0)),
        "field_duration_on_empty_reload" => {
            EvoEffect::FieldDurationOnEmptyReload(f(v, "value").unwrap_or(1.0))
        }
        // `base:` IS REQUIRED, with no default, because the two spellings of
        // this perk are two different mechanics and a default would silently
        // pick one. A yaml that does not say loads as Inert and is reported as
        // unmodelled, which is the honest outcome for a card nobody has read.
        "armor_strip_per_puncture_status" => {
            EvoEffect::ArmorStripPerPunctureStatus(f(v, "value").unwrap_or(0.0))
        }
        "multishot_on_last_round" => match v.get("base").and_then(serde_norway::Value::as_bool) {
            Some(base) => EvoEffect::MultishotOnLastRound {
                value: f(v, "value").unwrap_or(0.0),
                base,
            },
            None => EvoEffect::Inert("multishot_on_last_round without `base:`".into()),
        },
        "multishot_consumes_ammo" => {
            EvoEffect::MultishotConsumesAmmo(f(v, "value").unwrap_or(0.0))
        }
        // REAVER'S RAPTURE, and the trigger is what picks this arm: a
        // `base_damage_bonus` payload on a `full_burst_hit` trigger. Both are
        // read rather than assumed — the same payload on another trigger is a
        // different perk and stays inert until someone models it.
        "stacking_buff"
            if v.get("trigger").and_then(Value::as_str) == Some("full_burst_hit")
                && v.get("per_stack")
                    .and_then(|p| p.get("base_damage_bonus"))
                    .is_some() =>
        {
            EvoEffect::BaseDamagePerFullBurst {
                per_stack: v
                    .get("per_stack")
                    .and_then(|p| p.get("base_damage_bonus"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            }
        }
        // THE GENERAL ARM. Everything the sim's own vocabulary can already
        // express, named by a yaml: trigger, grant, size, cap, clock, decay and
        // what takes the pile. It sits BELOW the two arms above, which carry
        // reasoning a generic one cannot.
        "stacking_buff"
            if v.get("trigger").and_then(Value::as_str).and_then(buff_trigger).is_some()
                && v.get("per_stack")
                    .and_then(Value::as_mapping)
                    .and_then(|m| m.keys().next().and_then(Value::as_str))
                    .and_then(buff_grant)
                    .is_some() =>
        {
            let trigger = buff_trigger(v.get("trigger").and_then(Value::as_str).unwrap()).unwrap();
            let per = v.get("per_stack").and_then(Value::as_mapping).unwrap();
            let (key, val) = per.iter().next().unwrap();
            let grant = buff_grant(key.as_str().unwrap()).unwrap();
            let duration = f(v, "duration_seconds")
                .or_else(|| f(v, "duration"))
                .unwrap_or(crate::loadout::NO_TIMEOUT);
            EvoEffect::StackingGrant {
                trigger,
                grant,
                per_stack: val.as_f64().unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
                duration,
                // Default 1.0 so a perk that does NOT roll reads as certain
                // rather than as never firing — and a hidden roll is the one
                // thing the rendered wiki card omits (Headcracker's 50%), so
                // this default is only ever right when someone checked.
                chance: f(v, "chance").unwrap_or(1.0),
                // The Galvanized family unless the card says otherwise: one
                // stack drops on timeout and the timer restarts.
                decay: match v.get("decay").and_then(Value::as_str) {
                    Some("per_stack_expiry") => crate::loadout::BuffDecay::PerStackExpiry,
                    _ => crate::loadout::BuffDecay::LoseOneAndReset,
                },
                cleared_by: match v.get("cleared_by").and_then(Value::as_str) {
                    Some("reload") => crate::loadout::ClearedBy::Reload,
                    Some("magazine_refilled") => crate::loadout::ClearedBy::MagazineRefilled,
                    Some("empty_magazine") => crate::loadout::ClearedBy::EmptyMagazine,
                    _ => crate::loadout::ClearedBy::Nothing,
                },
            }
        }
        "stacking_buff" => {
            // Only the multishot payload is modeled (Fevered Frenzy);
            // other stacking payloads load inert until needed.
            let per = v
                .get("per_stack")
                .and_then(|p| p.get("multishot_bonus"))
                .and_then(Value::as_f64);
            let max = v.get("max_stacks").and_then(Value::as_u64).unwrap_or(0);
            match per {
                Some(p) => EvoEffect::AssumedMaxMultishot {
                    total: p * max as f64,
                    max_stacks: max as u32,
                },
                // NAME the payload. "unmodeled payload" told the pinned inert
                // list nothing: two different unmodelled buffs read as the
                // same entry, and neither said what it granted.
                None => EvoEffect::Inert(format!(
                    "stacking_buff {}",
                    v.get("per_stack")
                        .and_then(Value::as_mapping)
                        .and_then(|m| m.keys().next().and_then(|k| k.as_str()).map(str::to_string))
                        .unwrap_or_else(|| "no payload".into())
                )),
            }
        }
        // THE CONDITION IS READ NOW, and it is a question about the PLAYER
        // rather than about this weapon: "With Sprint Speed 1.2 or Higher".
        // Unread, the perk paid out on every build including the ones that
        // cannot reach the threshold (2026-08-12).
        "condition_overload" => EvoEffect::ConditionOverload {
            per_type: f(v, "value").unwrap_or(0.0),
            min_sprint: sprint_condition(v),
        },
        "crit_on_undamaged" => EvoEffect::CritOnUndamaged {
            crit_chance: f(v, "crit_chance").unwrap_or(0.0),
            crit_multiplier: f(v, "crit_multiplier").unwrap_or(0.0),
        },
        // ONE KIND FOR EVERY "WITH <player stat>" PERK. The `grant:` names the
        // bracket, so a multishot gate and a crit-damage gate cannot be
        // confused for one another, and an unreadable `condition:` falls to
        // Inert rather than paying out unconditionally.
        "mag_growth_on_empty_reload" => EvoEffect::MagGrowthOnEmptyReload {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
        },
        "instant_reload_on_kill" => {
            EvoEffect::InstantReloadOnKill { chance: f(v, "chance").unwrap_or(0.0) }
        }
        "round_restore_on_status_hit" => {
            let Some(status) = v.get("status").and_then(Value::as_str).and_then(crate::damage::DamageType::from_name)
            else {
                return Some(EvoEffect::Inert(
                    "round_restore_on_status_hit with an unreadable `status:`".into(),
                ));
            };
            EvoEffect::RoundRestoreOnStatusHit {
                status,
                chance: f(v, "chance").unwrap_or(0.0),
                rounds: f(v, "rounds").unwrap_or(1.0),
            }
        }
        "crit_chance_by_body_part" => EvoEffect::CritChanceByBodyPart {
            // No defaults that pay out: a missing multiplier is 1 (ordinary),
            // a missing bonus is 0.
            bodyshot_mult: f(v, "bodyshot_mult").unwrap_or(1.0),
            weakpoint_bonus: f(v, "weakpoint_bonus").unwrap_or(0.0),
        },
        "status_chance_from_crit_chance" => EvoEffect::DerivedStat {
            from_crit: true,
            rate: f(v, "rate").unwrap_or(0.0),
            cap: f(v, "cap").unwrap_or(0.0),
        },
        "crit_chance_from_status_chance" => EvoEffect::DerivedStat {
            from_crit: false,
            rate: f(v, "rate").unwrap_or(0.0),
            cap: f(v, "cap").unwrap_or(0.0),
        },
        "gated_by_tenno" => {
            let Some(gate) = tenno_condition(v) else {
                return Some(EvoEffect::Inert("gated_by_tenno with an unreadable `condition:`".into()));
            };
            let grant = match v.get("grant").and_then(Value::as_str) {
                Some("condition_overload") => crate::loadout::GatedGrant::ConditionOverload,
                Some("fire_rate") => crate::loadout::GatedGrant::FireRate,
                Some("multishot") => crate::loadout::GatedGrant::Multishot,
                Some("base_crit_damage") => crate::loadout::GatedGrant::BaseCritDamage,
                Some("projectile_speed") => crate::loadout::GatedGrant::ProjectileSpeed,
                Some("accuracy_bonus") => crate::loadout::GatedGrant::Accuracy,
                Some("flat_base_damage") => crate::loadout::GatedGrant::FlatBaseDamage,
                Some("flat_base_magazine") => crate::loadout::GatedGrant::FlatBaseMagazine,
                other => {
                    return Some(EvoEffect::Inert(format!(
                        "gated_by_tenno grants {}, which is not a bracket this engine has",
                        other.unwrap_or("nothing")
                    )))
                }
            };
            EvoEffect::GatedByTenno { gate, grant, value: f(v, "value").unwrap_or(0.0) }
        }
        "base_damage_below_half_health" => {
            EvoEffect::BaseDamageBelowHalfHealth {
                rate: f(v, "value").unwrap_or(0.0),
                excludes_own_flat: v
                    .get("excludes_own_flat")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        }
        "fire_rate_bonus" => EvoEffect::FireRateBonus {
            value: f(v, "value").unwrap_or(0.0),
            // THE SAME `condition:` VOCABULARY the CO kind reads. One syntax
            // for "this perk asks about the player", so the second grant to
            // need it did not invent a second spelling.
            min_sprint: sprint_condition(v),
        },
        // The CONDITION is the only thing that varies between the roster's
        // three copies, and it is read rather than assumed: absent means any
        // headshot pays (the Furis pair), `headshot_kill` means only a killing
        // one does (the Phenmor).
        "headshot_damage_on_headshot_streak" => EvoEffect::HeadshotDamageOnStreak {
            hits: v.get("hits").and_then(Value::as_u64).unwrap_or(0) as u32,
            within: f(v, "within_seconds").unwrap_or(0.0),
            value: f(v, "value").unwrap_or(0.0),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        "crit_multiplier_below_status_count" => EvoEffect::CritDamageBelowStatusCount {
            threshold: v.get("threshold").and_then(Value::as_u64).unwrap_or(0) as u32,
            value: f(v, "value").unwrap_or(0.0),
        },
        "instant_reload_on_headshot" => EvoEffect::InstantReloadOnHeadshot {
            chance: f(v, "chance").unwrap_or(0.0),
            needs_kill: v.get("condition").and_then(Value::as_str) == Some("headshot_kill"),
        },
        // CONDITIONAL ONES STAY INERT. Ready Retaliation spells the same kind
        // with a `condition:`, which nothing here reads — so it falls through to
        // `Inert` and keeps saying so on its tile.
        "reload_speed_bonus" if v.get("condition").is_none() => {
            EvoEffect::ReloadSpeedBonus(f(v, "value").unwrap_or(0.0))
        }
        // …AND THE CONDITIONAL ONE, which needs a WINDOW to be a buff at all.
        // Only the Phenmor's page publishes one ("for 6 seconds"); the other
        // eleven Ready Retaliations state the bonus and no duration, and a
        // window nobody published is not one to invent — those files say so and
        // stay inert. So the duration is REQUIRED here rather than defaulted:
        // a missing one falls through to `Inert`, which is the honest answer
        // and the one whose tile says why.
        // NO `duration_seconds` REQUIRED any more, and that is the whole reason
        // eleven of the twelve weapons carrying this perk were inert. Only the
        // Phenmor's page publishes a window; the rest state the bonus and
        // nothing else — which read as missing data while the model was a
        // timer, and reads as "there is nothing to state" now that the window
        // IS the reload action.
        "reload_speed_bonus"
            if v.get("condition").and_then(Value::as_str) == Some("reload_from_empty") =>
        {
            EvoEffect::ReloadSpeedOnEmptyReload { value: f(v, "value").unwrap_or(0.0) }
        }
        // ON FIRING. `base:` is REQUIRED rather than defaulted, for the same
        // reason `multishot_on_last_round` requires it: the two brackets differ
        // only on builds that carry a multishot mod, so a wrong default is a
        // number that looks right on a bare weapon and is wrong on every real
        // build.
        // AN EDGE, DECLARED. The clause is transcribed as written and the
        // reason is one of UNMODELLED.md's classes — so the page can say "this
        // cannot pay out here, and here is why" instead of "not modelled yet".
        "out_of_scope" => {
            let Some(reason) = v.get("reason").and_then(Value::as_str).and_then(Scope::parse) else {
                return Some(EvoEffect::Inert("out_of_scope without a known `reason:`".into()));
            };
            EvoEffect::OutOfScope {
                clause: v
                    .get("clause")
                    .and_then(Value::as_str)
                    .unwrap_or("(no clause)")
                    .to_string(),
                reason,
            }
        }
        "stacking_multishot_on_firing" => {
            let Some(base) = v.get("base").and_then(Value::as_bool) else {
                return Some(EvoEffect::Inert("stacking_multishot_on_firing without `base:`".into()));
            };
            EvoEffect::StackingMultishotOnFiring {
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
                base,
            }
        }
        // THE STATUS IS IN THE KIND, and it is READ rather than matched one
        // spelling at a time. This arm was hardcoded to Electricity for the
        // Furis's Stormburst, so the Latron family's Riddled Target — the same
        // mechanic, triggered by PUNCTURE — sat inert beside machinery that
        // already did everything it needed (2026-08-12).
        k if k.starts_with("stacking_multishot_on_") && k.ends_with("_status") => {
            let name = &k["stacking_multishot_on_".len()..k.len() - "_status".len()];
            let Some(status) = crate::damage::DamageType::from_name(name) else {
                // A type this engine does not know is reported, not silently
                // dropped: the kind NAMES it, so a typo would otherwise read as
                // a perk DE never wrote.
                return Some(EvoEffect::Inert(format!("stacking multishot on {name} status")));
            };
            EvoEffect::StackingMultishotOnStatus {
                status,
                per_stack: f(v, "per_stack").unwrap_or(0.0),
                max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
                // Two spellings in the roster, both meaning seconds.
                duration: f(v, "duration").or_else(|| f(v, "duration_seconds")).unwrap_or(0.0),
            }
        }
        "on_headshot_fire_rate" => EvoEffect::StackingFireRateOnHeadshot {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
            // Default 1.0 so a perk that does NOT roll reads as certain rather
            // than as never firing.
            chance: f(v, "chance").unwrap_or(1.0),
            // The Galvanized family unless the card says otherwise — the same
            // default and the same word every other stacking buff here uses.
            decay: match v.get("decay").and_then(Value::as_str) {
                Some("per_stack_expiry") => crate::loadout::BuffDecay::PerStackExpiry,
                _ => crate::loadout::BuffDecay::LoseOneAndReset,
            },
        },
        "crit_multiplier_below_crit_chance" => EvoEffect::CritMultiplierBelowCritChance {
            value: f(v, "value").unwrap_or(0.0),
            below: f(v, "below_crit_chance").unwrap_or(0.0),
        },
        "flat_crit_chance_after_mods" => {
            EvoEffect::PostModCritChance(f(v, "value").unwrap_or(0.0))
        }
        "flat_status_chance_after_mods" => {
            EvoEffect::PostModStatusChance(f(v, "value").unwrap_or(0.0))
        }
        "headshot_damage" => EvoEffect::HeadshotDamage(f(v, "value").unwrap_or(0.0)),
        "chance_damage_on_noncrit" => EvoEffect::ChanceDamageOnNoncrit {
            chance: f(v, "chance").unwrap_or(0.0),
            value: f(v, "value").unwrap_or(0.0),
        },
        "incarnon_charge_rate" => EvoEffect::IncarnonChargeRate(f(v, "value").unwrap_or(0.0)),
        "stacking_damage_on_plain_hit" => EvoEffect::StackingDamageOnPlainHit {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
        },
        "on_headshot_reload_speed" => EvoEffect::StackingReloadSpeedOnHeadshot {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
        },
        "unlocks_weapon" => EvoEffect::UnlocksForm(
            v.get("weapon")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        // A QUALIFIER, not an effect: "Stacks up to 4x" caps the bonus above
        // it. See `EvoEffect::Qualifier`.
        other if other.starts_with("unmodelled_stacks_up_to") => {
            EvoEffect::Qualifier(other.to_string())
        }
        other => EvoEffect::Inert(other.to_string()),
    })
}

/// Apply a chosen evolution set onto a weapon's RAW base panel.
///
/// Order-independent: flat base damage sums first, then the vector scales
/// pro-rata ONCE; `co_base_fraction` = original / evolved total — the wiki
/// CO-catalog rule that every GunCO source computes on the pre-evolution
/// base ("CO-bonus does not use base damage increase Evolution").
/// `currently_broken` evolutions apply nothing.
pub fn apply(base: &mut WeaponBase, evos: &[&EvolutionDef]) {
    let original_total = base.base_vector.total();
    let mut flat = 0.0;
    // …AND HOW MUCH OF IT THE GunCO TERM'S BASE GROWS BY. Two sums rather than
    // one plus a flag, because a build can carry two flat-damage perks that
    // DISAGREE — the catalog says the Despair is exactly that, one tier-2
    // option excluded and the other not. The old code held a single ratio and
    // could only have been right about that pair by accident.
    let mut flat_into_co = 0.0;
    // "…but does not take into account the Base Damage increase from THIS
    // perk". Held as a pair of sums and resolved once `evolved` exists: the
    // rate as written, and the same rate weighted by the perk's own flat add,
    // so the correction is `Σr - Σ(r·own) / evolved` and no perk needs to know
    // what the others granted.
    let (mut half_hp_rate, mut half_hp_rate_own) = (0.0f64, 0.0f64);
    for e in evos
        .iter()
        .filter(|e| !e.currently_broken)
        // "Does not affect Incarnon Form" — the perk is EQUIPPED either way
        // (it is the same Genesis ladder), so it is skipped HERE, on the form
        // it does not reach, rather than refused at selection. On the base
        // form's panel it applies in full.
        .filter(|e| !(e.base_form_only && base.form == crate::weapons_data::FormKind::Incarnon))
    {
        for eff in &e.effects {
            match eff {
                // NOTHING TO APPLY. The form it unlocks is a separate weapon
                // entry with its own stats, so applying anything here would
                // count them twice.
                EvoEffect::UnlocksForm(_) => {}
                // NOTHING TO APPLY, because the game applies nothing. The
                // clause is kept so the card can say so ().
                EvoEffect::LiveBug { .. } => {}
                // …and nothing to apply for an EDGE either, by definition.
                EvoEffect::OutOfScope { .. } => {}
                // EACH PERK DECIDES ITS OWN CONTRIBUTION to the CO base,
                // which is the whole point of holding an absolute: two perks on
                // one build may disagree and there is no single ratio that
                // describes the pair.
                EvoEffect::FlatBaseDamage(v) => {
                    flat += v;
                    if !e.excludes_co_base(base.form, base.co_behavior) {
                        flat_into_co += v;
                    }
                }
                // Same bucket as the line above: it is base damage, and the
                // run is modelled holding it (see the variant's note).
                // Into the base like any other flat damage — the buff OPENS
                // FULL — and recorded so the buff card can take it back off.
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => {
                    flat += v;
                    // …AND SO DOES THIS ONE, by the same rule. It is a flat
                    // base add wearing a trigger, and nothing about the trigger
                    // changes which base the CO term reads.
                    if !e.excludes_co_base(base.form, base.co_behavior) {
                        flat_into_co += v;
                    }
                    base.reload_damage_buff += v;
                }
                // Into the SAME additive bucket a mod's indirect stat uses;
                // `resolve` seeds the panel from here.
                EvoEffect::Indirect(stat, v) => {
                    match base.indirect.iter_mut().find(|(s, _)| s == stat) {
                        Some(e) => e.1 += v,
                        None => base.indirect.push((*stat, *v)),
                    }
                }
                EvoEffect::AmmoMaxSet(v) => base.ammo_reserve = *v,
                // A base-stat evolution is a WEAPON stat change, so it lands
                // on EVERY attack part, not just the direct hit. That is the
                // same reading `resolve` already applies to Elemental Excess's
                // post-mod layer ("a WEAPON stat change, so the explosion takes
                // it too"), and the base layer is the more clearly weapon-wide
                // of the two.
                //
                // INFERENCE, not a citation: no source states whether Torid's
                // Commodore's Fortune / Survivor's Edge / Elemental Balance
                // reach its Toxin cloud. It matters — the cloud is most of that
                // weapon's damage — so it is called out here and in MECHANICS.
                // Nothing else in the roster is affected: only Dual Toxocyst
                // (no radial, no field) and the Torid have base-stat
                // evolutions at all.
                EvoEffect::FlatBaseCritChance(v) => {
                    base.base_crit_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_crit_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_crit_chance += v;
                    }
                }
                // BASE multishot, so the multishot MODS multiply it — the
                // same bracket a weapon's own innate multishot sits in. Not
                // pushed into the radial or the field: an explosion fires per
                // projectile already (`radius_takes_multishot`), so adding it
                // there would count the same pellets twice.
                EvoEffect::FlatBaseMultishot(v) => base.base_multishot += v,
                EvoEffect::StackingFireRatePerShellReloaded { per_stack, max_stacks } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "per_shell_fire_rate",
                        trigger: crate::loadout::BuffTrigger::ReloadComplete,
                        grant: crate::loadout::BuffGrant::FireRate,
                        // NOTHING TAKES THEM but holstering, and a holster is
                        // not something this arena does — so no clock, and the
                        // decay mode never runs.
                        decay: crate::loadout::BuffDecay::LoseOneAndReset,
                        duration: crate::loadout::NO_TIMEOUT,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        chance: 1.0,
                        initial_stacks: 0,
                        // 0 = ONE PER SHELL. `resolve` reads the modded
                        // magazine and turns it into a number, and `per_shell`
                        // keeps the RULE after the number replaces it — the
                        // Incarnon route loads a known count of shells and has
                        // to know which buffs are counting them.
                        stacks_per_trigger: 0,
                        per_shell: true,
                        cleared_by: crate::loadout::ClearedBy::EmptyMagazine,
                    });
                }
                EvoEffect::FlatBaseStatusChance(v) => {
                    base.base_status_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_status_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_status_chance += v;
                    }
                }
                EvoEffect::FlatBaseStatusChanceByForm { base: b, incarnon } => {
                    // The Incarnon entry is the one carrying the `incarnon:`
                    // block — the same gate `FlatBaseMagazine` uses to keep a
                    // magazine evolution off the charge pool.
                    let v = if base.gauge_form.is_some() { *incarnon } else { *b };
                    base.base_status_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_status_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_status_chance += v;
                    }
                }
                EvoEffect::FlatBaseCritMultiplier(v) => {
                    base.base_crit_damage += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_crit_damage += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_crit_damage += v;
                    }
                }
                // BASE FORM ONLY, and the gate is load-bearing: an Incarnon
                // form's `magazine_size` IS its charge pool (the pseudo-reload
                // rounds), so an ungated `+=` handed Extended Volley's +9 to
                // the 170-round gauge as well — "Does not apply to Incarnon
                // Form's Magazine" (wiki), and that magazine is outside the
                // ammo system entirely (user, 2026-07-30: it uses max charges).
                EvoEffect::FlatBaseMagazine(v) => {
                    if base.gauge_form.is_none() {
                        base.magazine_size += v;
                    }
                }
                EvoEffect::FieldDurationOnEmptyReload(v) => {
                    base.field_duration_on_empty_reload = *v;
                }
                EvoEffect::MultishotBeyondRange { value, metres } => {
                    base.multishot_beyond_range = Some((*value, *metres));
                }
                // BASE FORM ONLY: `incarnon.is_some()` marks the charge-backed
                // form, whose magazine is the gauge's round pool rather than a
                // reloaded magazine — nothing there is "the last round".
                // IT LIVES ON BOTH FORMS, and "cannot be stacked in Incarnon
                // form" falls out rather than being enforced: the Burston's
                // Incarnon is an AUTO weapon, so no burst ever completes there
                // and the trigger cannot fire. Enforcing it by form id would be
                // a rule that has to be right; deriving it from the trigger is
                // a rule that cannot be wrong.
                //
                // "Resets when activating incarnon" is the ordinary
                // `MagazineRefilled` clear — swapping either way reloads the
                // base magazine — so it needs nothing of its own either.
                EvoEffect::ArmorStripPerPunctureStatus(v) => {
                    base.armor_strip_per_puncture = *v;
                }
                EvoEffect::BaseDamagePerFullBurst { per_stack, max_stacks } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "full_burst_damage",
                        trigger: crate::loadout::BuffTrigger::FullBurst,
                        grant: crate::loadout::BuffGrant::BaseDamage,
                        decay: crate::loadout::BuffDecay::LoseOneAndReset,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        // NO CLOCK. "Resets on Reload" is not a duration, and a
                        // timeout would quietly drop stacks a player still has.
                        duration: crate::loadout::NO_TIMEOUT,
                        chance: 1.0,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::loadout::ClearedBy::MagazineRefilled,
                    });
                }
                EvoEffect::MultishotOnLastRound { value, base: is_base } => {
                    if base.gauge_form.is_none() {
                        if *is_base {
                            base.base_multishot_on_last_round = *value;
                        } else {
                            base.multishot_on_last_round = *value;
                        }
                    }
                }
                // "Affects both modes" — unlike Final Fusillade this one lands
                // on the charge-backed form too; what differs is the RULE, and
                // the sim picks that off `continuous`, not off the form id.
                EvoEffect::MultishotConsumesAmmo(v) => base.multishot_ammo_bonus = *v,
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => {
                    base.buff_multishot_bonus += total;
                    base.buff_ms_max_stacks = base.buff_ms_max_stacks.max(*max_stacks);
                }
                // CARRIED, NOT SPENT, when the perk states a speed: `apply`
                // works on the raw weapon and the player is not here — the
                // condition is answered in `resolve_for`, which has the Tenno.
                EvoEffect::ConditionOverload { per_type, min_sprint } => {
                    if *min_sprint > 0.0 {
                        base.gated.push((
                            crate::loadout::TennoGate::SprintAtLeast(*min_sprint),
                            crate::loadout::GatedGrant::ConditionOverload,
                            *per_type,
                        ));
                    } else {
                        base.innate_co_per_type += per_type;
                    }
                }
                // CARRIED, NOT SPENT, when the perk states a speed — the
                // player is not here. Answered in `resolve_for`, exactly like
                // the Condition Overload gate beside it.
                EvoEffect::CritOnUndamaged { crit_chance, crit_multiplier } => {
                    base.cc_on_undamaged += crit_chance;
                    base.cd_on_undamaged += crit_multiplier;
                }
                EvoEffect::GatedByTenno { gate, grant, value } => {
                    base.gated.push((*gate, *grant, *value));
                }
                EvoEffect::MagGrowthOnEmptyReload { per_stack, max_stacks } => {
                    base.mag_growth_on_empty_reload = Some((*per_stack, *max_stacks));
                }
                EvoEffect::InstantReloadOnKill { chance } => {
                    base.instant_reload_on_kill = Some(*chance);
                }
                EvoEffect::RoundRestoreOnStatusHit { status, chance, rounds } => {
                    base.round_restore_on_status = Some((*status, *chance, *rounds));
                }
                EvoEffect::CritChanceByBodyPart { bodyshot_mult, weakpoint_bonus } => {
                    // MULTIPLICATIVE, so it composes rather than replaces —
                    // two such perks on one weapon would multiply, which is
                    // what "multiplicative with all sources" means.
                    base.bodyshot_cc_mult *= *bodyshot_mult;
                    base.evo_weakpoint_cc_rel += *weakpoint_bonus;
                }
                EvoEffect::DerivedStat { from_crit, rate, cap } => {
                    if *from_crit {
                        base.base_status_from_crit = Some((*rate, *cap));
                    } else {
                        base.base_crit_from_status = Some((*rate, *cap));
                    }
                }
                EvoEffect::BaseDamageBelowHalfHealth { rate, excludes_own_flat } => {
                    half_hp_rate += rate;
                    if *excludes_own_flat {
                        half_hp_rate_own += rate * e.flat_base_damage();
                    }
                }
                EvoEffect::FireRateBonus { value, min_sprint } => {
                    if *min_sprint > 0.0 {
                        base.gated.push((
                            crate::loadout::TennoGate::SprintAtLeast(*min_sprint),
                            crate::loadout::GatedGrant::FireRate,
                            *value,
                        ));
                    } else {
                        base.evo_fire_rate_bonus += value;
                    }
                }
                EvoEffect::ReloadSpeedBonus(v) => base.evo_reload_bonus += v,
                EvoEffect::InstantReloadOnHeadshot { chance, needs_kill } => {
                    base.instant_reload_on_headshot =
                        Some(crate::loadout::InstantReload { chance: *chance, needs_kill: *needs_kill });
                }
                EvoEffect::HeadshotDamageOnStreak { hits, within, value, duration } => {
                    base.headshot_streak = Some(crate::loadout::HeadshotStreak {
                        hits: *hits,
                        within: *within,
                        value: *value,
                        duration: *duration,
                    });
                }
                EvoEffect::CritDamageBelowStatusCount { threshold, value } => {
                    base.cd_below_status_count = Some((*threshold, *value));
                }
                EvoEffect::ReloadSpeedOnEmptyReload { value } => {
                    base.rs_on_empty_reload = *value;
                }
                // Carried, not applied: `apply` works on the RAW base panel and
                // the condition needs the crit chance the mods produce, which
                // does not exist until `resolve` runs — and not even there in
                // full, since the live half only exists once a shot lands.
                EvoEffect::CritMultiplierBelowCritChance { value, below } => {
                    base.crit_mult_below_cc = Some((*value, *below));
                }
                EvoEffect::StackingGrant {
                    trigger, grant, per_stack, max_stacks, duration, chance, decay, cleared_by,
                } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: stacking_card_id(*trigger, *grant),
                        trigger: *trigger,
                        grant: *grant,
                        decay: *decay,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: *chance,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: *cleared_by,
                    });
                }
                EvoEffect::StackingMultishotOnFiring { per_stack, max_stacks, base: is_base } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_firing_multishot",
                        trigger: crate::loadout::BuffTrigger::Firing,
                        grant: if *is_base {
                            crate::loadout::BuffGrant::BaseMultishot
                        } else {
                            crate::loadout::BuffGrant::MultishotPercent
                        },
                        // NO CLOCK, so the decay never runs; the reload is what
                        // ends it. Both wiki pages say so in the same words —
                        // "There is no timer" (Sybaris), "resets entirely upon
                        // reloading" (Strun).
                        decay: crate::loadout::BuffDecay::PerStackExpiry,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: crate::loadout::NO_TIMEOUT,
                        chance: 1.0,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::loadout::ClearedBy::Reload,
                    });
                }
                EvoEffect::StackingMultishotOnStatus { status, per_stack, max_stacks, duration } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_status_multishot",
                        trigger: crate::loadout::BuffTrigger::HitEnemyWithStatus(*status),
                        grant: crate::loadout::BuffGrant::Multishot,
                        // FIFO, each stack on its own 2 s clock — owner
                        // observed in game (2026-08-07). Harsher than the
                        // Galvanized family: holding 3 needs 3 hits per
                        // window, not one.
                        decay: crate::loadout::BuffDecay::PerStackExpiry,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::loadout::ClearedBy::Nothing,
                    });
                }
                EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance, decay } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_headshot_fire_rate",
                        decay: *decay,
                        trigger: crate::loadout::BuffTrigger::Headshot,
                        grant: crate::loadout::BuffGrant::FireRate,
                        // A FRACTION here; `resolve` turns it into an absolute
                        // rate against the base, which is the bucket it joins.
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: *chance,
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::loadout::ClearedBy::Nothing,
                    });
                }
                EvoEffect::PostModCritChance(v) => base.post_mod_crit_chance += v,
                EvoEffect::PostModStatusChance(v) => base.post_mod_status_chance += v,
                EvoEffect::HeadshotDamage(v) => base.headshot_damage_bonus += v,
                EvoEffect::ChanceDamageOnNoncrit { chance, value } => {
                    base.noncrit_bonus = Some((*chance, *value));
                }
                EvoEffect::IncarnonChargeRate(v) => {
                    if let Some(i) = base.gauge_form.as_mut() {
                        i.charge_rate += v;
                    }
                }
                EvoEffect::StackingDamageOnPlainHit {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_plain_hit_damage",
                        // The Galvanized family, as each perk's own wiki text says.
                        decay: crate::loadout::BuffDecay::LoseOneAndReset,
                        trigger: crate::loadout::BuffTrigger::PlainHit,
                        grant: crate::loadout::BuffGrant::BaseDamage,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        // EARNED from zero, like every other TIMED buff: it
                        // has a duration, so a lull empties it and the fight
                        // has to fill it again (docs/BUFFS.md).
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::loadout::ClearedBy::Nothing,
                    });
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_headshot_reload_speed",
                        // The Galvanized family, as each perk's own wiki text says.
                        decay: crate::loadout::BuffDecay::LoseOneAndReset,
                        trigger: crate::loadout::BuffTrigger::Headshot,
                        grant: crate::loadout::BuffGrant::ReloadSpeed,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
                        stacks_per_trigger: 1,
                        per_shell: false,
                        cleared_by: crate::loadout::ClearedBy::Nothing,
                    });
                }
                EvoEffect::Inert(_) | EvoEffect::Qualifier(_) => {}
            }
        }
    }
    // …and the below-half-health rate, corrected against the base it will
    // actually multiply. With no flat damage anywhere the correction is nil and
    // the rate is the card's.
    if half_hp_rate != 0.0 {
        let evolved = original_total + flat;
        base.bd_below_half_health += if evolved > 0.0 {
            half_hp_rate - half_hp_rate_own / evolved
        } else {
            half_hp_rate
        };
    }
    if flat > 0.0 && original_total > 0.0 {
        // THE FOLD ITSELF LIVES ON `WeaponBase`, because a flat base-damage add
        // reaches this weapon by two routes — a plain perk here, and a perk the
        // player's state gates, which cannot be resolved until `resolve_for` has
        // the Tenno. Two implementations of "what +40 base damage does" is two
        // chances to be right about the vector and wrong about the explosion.
        // See `WeaponBase::add_flat_base_damage` for what it does and why.
        //
        // TWO SUMS GO IN: what the panel gains, and how much of that the CO
        // term's base gains with it. They are equal when every perk feeds and
        // zero apart when none does — and they are neither when a build carries
        // two that disagree, which is the case a single ratio could not state.
        base.add_flat_base_damage(flat, flat_into_co);
    }
}

/// The yaml's word for a trigger. `None` = not one this engine runs, which is
/// what makes the general `stacking_buff` arm fall through to the inert one
/// instead of inventing a mechanic.
fn buff_trigger(s: &str) -> Option<crate::loadout::BuffTrigger> {
    use crate::loadout::BuffTrigger as T;
    Some(match s {
        "firing" => T::Firing,
        "headshot" => T::Headshot,
        "consecutive_headshot" => T::ConsecutiveHeadshot,
        "hit" => T::Hit,
        "plain_hit" => T::PlainHit,
        "reload_complete" => T::ReloadComplete,
        "reload_from_empty" => T::ReloadFromEmpty,
        "status_applied" => T::StatusApplied,
        "kill" => T::Kill,
        _ => return None,
    })
}

/// The yaml's word for a grant — the KEY of the `per_stack:` map, so the payload
/// names its own bracket and a perk cannot land in the wrong one by omission.
fn buff_grant(s: &str) -> Option<crate::loadout::BuffGrant> {
    use crate::loadout::BuffGrant as G;
    Some(match s {
        "base_damage_bonus" => G::BaseDamage,
        "base_damage" => G::FlatBaseDamage,
        "fire_rate_bonus" => G::FireRate,
        "reload_speed_bonus" => G::ReloadSpeed,
        "multishot" => G::Multishot,
        "base_multishot" => G::BaseMultishot,
        "multishot_percent" => G::MultishotPercent,
        "base_crit_damage" => G::BaseCritDamage,
        "headshot_damage_bonus" => G::HeadshotDamage,
        _ => return None,
    })
}

/// THE BUFF CARD'S ID, derived from what the buff IS rather than carried in the
/// yaml. It is a durable name — the roster, the saved config and the sampler all
/// key on it — so it is a finite reviewable table and not a formatted string.
fn stacking_card_id(
    trigger: crate::loadout::BuffTrigger,
    grant: crate::loadout::BuffGrant,
) -> &'static str {
    use crate::loadout::BuffGrant as G;
    use crate::loadout::BuffTrigger as T;
    match (trigger, grant) {
        (T::Firing, G::FireRate) => "on_firing_fire_rate",
        (T::Firing, G::BaseDamage) => "on_firing_damage",
        (T::StatusApplied, G::FireRate) => "on_status_fire_rate",
        (T::StatusApplied, G::BaseDamage) => "on_status_damage",
        (T::Headshot, G::FireRate) => "on_headshot_fire_rate",
        (T::Headshot, G::BaseDamage) => "on_headshot_damage",
        (T::ConsecutiveHeadshot, G::FlatBaseDamage) => "on_weakpoint_streak_damage",
        (T::ConsecutiveHeadshot, G::HeadshotDamage) => "on_weakpoint_streak_headshot_damage",
        (T::Hit, G::FlatBaseDamage) => "on_hit_damage",
        (T::PlainHit, G::BaseDamage) => "on_plain_hit_damage",
        (T::ReloadComplete, G::BaseDamage) => "on_reload_damage",
        (T::ReloadComplete, G::FireRate) => "on_reload_fire_rate",
        (T::Kill, G::FlatBaseDamage) => "on_kill_damage",
        (T::ReloadFromEmpty, G::FlatBaseDamage) => "on_empty_reload_damage",
        (T::ReloadFromEmpty, G::BaseCritDamage) => "on_empty_reload_crit_damage",
        // A pair nobody has written a card for yet. It is still a real buff and
        // still runs; it just shares one generic id, which is visible the first
        // time two of them appear on one weapon and is the point at which the
        // pair earns a name above.
        _ => "stacking_grant",
    }
}

/// `condition: "sprint_speed >= 1.2"`, as a number — 0 when the card states no
/// speed. Kept for the two kinds that spell their gate this way.
fn sprint_condition(v: &Value) -> f64 {
    match tenno_condition(v) {
        Some(crate::loadout::TennoGate::SprintAtLeast(x)) => x,
        _ => 0.0,
    }
}

/// `condition:` as a GATE — one spelling for every question a perk asks about
/// the player. Unknown wording returns `None`, which the caller turns into an
/// inert effect rather than a silently ungated grant: a condition nobody reads
/// is a perk that pays on every build including the ones that cannot have it.
fn tenno_condition(v: &Value) -> Option<crate::loadout::TennoGate> {
    use crate::loadout::TennoGate as G;
    let c = v.get("condition").and_then(Value::as_str)?;
    let num = |s: &str| s.trim().parse::<f64>().ok();
    if let Some(x) = c.strip_prefix("sprint_speed >= ").and_then(num) {
        return Some(G::SprintAtLeast(x));
    }
    if let Some(x) = c.strip_prefix("armor > ").and_then(num) {
        return Some(G::ArmorOver(x));
    }
    if let Some(x) = c.strip_prefix("energy_max > ").and_then(num) {
        return Some(G::EnergyMaxOver(x));
    }
    // The one gate with no number: the card asks whether you HAVE overshields,
    // not how many.
    if c == "overshields" {
        return Some(G::HasOvershields);
    }
    if c == "channeling" {
        return Some(G::ChannelingAbility);
    }
    // THE LOADOUT, not the frame and not what it is doing: "With No Primary
    // Equipped". Off by default, because the fight's Tenno walks in carrying
    // everything unless the scenario says otherwise.
    if c == "solo_weapon" {
        return Some(G::SoloWeapon);
    }
    None
}

/// WHY a clause can never pay out here — `docs/UNMODELLED.md`'s classes, as a
/// closed set, so a new gap either fits a reason already written down or is a
/// reason nobody has thought about yet (which that file says is itself worth
/// knowing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    OneTarget,
    NoDistance,
    NoMovement,
    NoHolster,
    InfiniteAmmo,
    NobodyShootsBack,
    WarframeAbilities,
}

impl Scope {
    fn parse(s: &str) -> Option<Scope> {
        Some(match s {
            "one_target" => Scope::OneTarget,
            "no_distance" => Scope::NoDistance,
            "no_movement" => Scope::NoMovement,
            "no_holster" => Scope::NoHolster,
            "infinite_ammo" => Scope::InfiniteAmmo,
            "nobody_shoots_back" => Scope::NobodyShootsBack,
            "warframe_abilities" => Scope::WarframeAbilities,
            _ => return None,
        })
    }

    /// The sentence a player reads. English is the source; the i18n overlay
    /// translates it like any other UI string.
    pub fn why(self) -> &'static str {
        match self {
            Scope::OneTarget => "the fight has one target, so this pays nothing",
            Scope::NoDistance => "every shot lands at point blank, so distance changes nothing",
            Scope::NoMovement => "the player does not move or aim by hand here",
            Scope::NoHolster => "this weapon is never holstered during the fight",
            Scope::InfiniteAmmo => "ammo reserves are unlimited, so nothing runs dry",
            Scope::NobodyShootsBack => "nothing damages the player in this fight",
            Scope::WarframeAbilities => "no Warframe ability is cast during the fight",
        }
    }
}

/// Every embedded yaml under data/evolutions (cached).
pub fn pool() -> &'static Vec<EvolutionDef> {
    static POOL: OnceLock<Vec<EvolutionDef>> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut out = Vec::new();
        for (path, text) in crate::data::files_under("evolutions/") {
            // The directory IS the table (data/README.md conventions):
            // everything under evolutions/ must parse as an evolution.
            let ef = serde_norway::from_str::<EvoFile>(text)
                .unwrap_or_else(|e| panic!("parse {path}: {e}"));
            // NAMING CONTRACT, enforced at load (user, 2026-07-29: full
            // weapon names, no abbreviations — long but unambiguous):
            //   id = "<weapon>_<evolution>"  and  filename = "<id>.yaml".
            // Scoping is NOT redundant with the `weapon:` field: evolution
            // NAMES repeat across weapons with different values (Marksman's
            // Hand is −50% recoil on Dual Toxocyst, −40% on Laetum), so the
            // id must carry the weapon. Deriving both the file name and the
            // prefix from it means the three can never drift apart.
            let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".yaml");
            assert!(
                ef.id == stem,
                "{path}: id '{}' must match the filename",
                ef.id
            );
            assert!(
                ef.id.strip_prefix(&ef.weapon).is_some_and(|r| r.starts_with('_')),
                "{path}: id '{}' must start with the weapon id '{}_'",
                ef.id,
                ef.weapon
            );
            let effects = ef.effects.iter().filter_map(effect).collect();
            // …AND THE CLAUSES WHOSE CARD IS WRONG. Read off the same effect
            // maps and kept beside them: `misprint:` names the disagreement and
            // changes nothing about how the effect is loaded.
            let misprints: Vec<String> = ef
                .effects
                .iter()
                .filter_map(|v| {
                    let note = v.get("misprint").and_then(Value::as_str)?;
                    let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
                    Some(format!("{} — {note}", kind.replace('_', " ")))
                })
                .collect();
            out.push(EvolutionDef {
                id: ef.id,
                name: ef.name,
                weapon: ef.weapon,
                tier: ef.tier,
                icon: ef.icon,
                description: ef.description.unwrap_or_default(),
                currently_broken: ef.currently_broken,
                co_base_excludes_this_evolution: ef.co_base_excludes_this_evolution,
                co_base_excludes_only_form: ef.co_base_excludes_only_form.as_deref().map(|s| {
                    match s {
                        "base" => crate::weapons_data::FormKind::Base,
                        "incarnon" => crate::weapons_data::FormKind::Incarnon,
                        other => panic!("co_base_excludes_only_form: unknown form {other:?}"),
                    }
                }),
                base_form_only: ef.base_form_only,
                misprints,
                effects,
            });
        }
        out
    })
}

/// Look up an evolution by id.
impl EvolutionDef {
    /// The form this evolution unlocks, if it is the transformation itself.
    ///
    /// THE TAG the form resolution reads. It replaces "tier 1's first option",
    /// which was a guess from ladder position that happened to hold for the
    /// four Incarnon weapons in the roster and says nothing about the fifth.
    pub fn unlocks_form(&self) -> Option<&str> {
        self.effects.iter().find_map(|e| match e {
            EvoEffect::UnlocksForm(w) => Some(w.as_str()),
            _ => None,
        })
    }
}

pub fn get(id: &str) -> Option<&'static EvolutionDef> {
    pool().iter().find(|e| e.id == id)
}

/// A weapon's choosable options at a tier (the web picker's rows).
pub fn options(weapon: &str, tier: u32) -> Vec<&'static EvolutionDef> {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon && e.tier == tier)
        .collect()
}

/// How many evolution tiers this weapon's data declares — the tier count
/// is per weapon (Dual Toxocyst has 4, Laetum has 5), so callers must
/// never assume a fixed range.
pub fn tier_count(weapon: &str) -> u32 {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon)
        .map(|e| e.tier)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NOTHING IN THE ROSTER RESIZES AN INCARNON CHARGE POOL — no evolution,
    /// under any combination.
    ///
    /// A roster-wide invariant, stated by the owner (2026-08-10): there is no
    /// mechanism anywhere that restores charges in an Incarnon form or spends
    /// extra ones. The pool is filled by the GAUGE and emptied by firing, and
    /// that is the whole of it — which is why the magazine family of effects is
    /// gated on `incarnon.is_none()` in three separate places
    /// (`FlatBaseMagazine`, `MultishotOnLastRound`, and the ammo rules), and why
    /// Executioner's Fortune does not roll there at all.
    ///
    /// Three gates is three chances to forget the fourth, so this asserts the
    /// PROPERTY instead of the gates: every evolution a charge-backed form can
    /// carry, all at once, must leave its pool exactly the size the data
    /// declares. A future effect that reaches the gauge fails here rather than
    /// shipping as a quietly bigger magazine on seven weapons.
    #[test]
    fn no_evolution_resizes_an_incarnon_charge_pool() {
        let mut checked = 0;
        for spec in crate::weapons_data::all() {
            if spec.gauge_form.is_none() {
                continue;
            }
            // The whole ladder at once, one option per tier — the widest set a
            // build can hold, so anything that could reach the pool is in it.
            // The GROUP owns the evolutions, not the form — an Incarnon
            // entry's perks are filed under its base weapon's id.
            let group = spec.transform_group.as_deref().unwrap_or(&spec.id);
            let mut ids: Vec<&str> = Vec::new();
            for tier in 1..=tier_count(group) {
                for opt in options(group, tier) {
                    ids.push(opt.id.as_str());
                }
            }
            if ids.is_empty() {
                continue;
            }
            checked += 1;
            let bare = crate::loadout::WeaponBase::from_data(&spec.id, true, &[]);
            let loaded = crate::loadout::WeaponBase::from_data(&spec.id, true, &ids);
            assert_eq!(
                loaded.magazine_size, bare.magazine_size,
                "{}: an evolution resized the charge pool ({} -> {})",
                spec.id, bare.magazine_size, loaded.magazine_size
            );
        }
        assert!(checked >= 5, "only {checked} charge-backed forms checked");
    }

    #[test]
    fn loads_the_dt_evolution_pool() {
        let dt: Vec<_> = pool().iter().filter(|e| e.weapon == "dual_toxocyst").collect();
        assert!(dt.len() >= 9, "expected the 9 DT evolutions, got {}", dt.len());
        assert_eq!(options("dual_toxocyst", 2).len(), 2); // the EVO II choice
        // Broken evolutions carry the wiki flag.
        assert!(get("dual_toxocyst_ready_retaliation").unwrap().currently_broken);
        assert!(get("dual_toxocyst_neurotoxin").unwrap().currently_broken);
    }

    #[test]
    fn fevered_and_carnage_parse_their_wiki_values() {
        let fe = get("dual_toxocyst_fevered_frenzy").unwrap();
        assert!(fe.effects.contains(&EvoEffect::FlatBaseDamage(50.0)));
        assert!(fe
            .effects
            .contains(&EvoEffect::AssumedMaxMultishot { total: 1.0, max_stacks: 20 }));
        let ca = get("dual_toxocyst_carnage_reign").unwrap();
        assert!(ca.effects.contains(&EvoEffect::FlatBaseDamage(60.0)));
        // …AND ITS SECOND CLAUSE IS DEAD. "+33% Direct Damage per Status Type"
        // is on DE's own CO-source list and pays nothing in game, measured
        // twice over (MEASUREMENTS M49) — so it loads as a LIVE BUG rather
        // than as a CO source, which is what keeps the card able to SAY so
        // while the number stays at zero.
        assert!(
            !ca.effects
                .iter()
                .any(|e| matches!(e, EvoEffect::ConditionOverload { .. })),
            "the +33% pays nothing in game and must not load as a CO source"
        );
        assert_eq!(ca.live_bugs().len(), 1, "{:?}", ca.live_bugs());
        assert!(ca.live_bugs()[0].starts_with("condition overload — "), "{:?}", ca.live_bugs());
        // The perk is NOT fully unmodelled — its +60 works, and the tile must
        // not tell a player the whole option is dead.
        assert!(!ca.fully_unmodeled());
        let cf = get("dual_toxocyst_commodores_fortune").unwrap();
        assert!(cf.effects.contains(&EvoEffect::FlatBaseCritChance(0.20)));
    }

    #[test]
    fn broken_evolutions_apply_nothing() {
        use crate::loadout::WeaponBase;
        let with = WeaponBase::from_data("dual_toxocyst", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
        let mut probe = with.clone();
        apply(&mut probe, &[get("dual_toxocyst_ready_retaliation").unwrap()]);
        assert!((probe.base_vector.total() - with.base_vector.total()).abs() < 1e-9);
        assert_eq!(probe.base_crit_chance, with.base_crit_chance);
    }

    /// A broken evolution changes NOTHING — whatever it grants.
    ///
    /// The test above can only be as strong as the data it picks, and no
    /// SHIPPED broken evolution carries an effect `apply` would act on: both
    /// of them resolve to something `apply` ignores anyway, so a regression in
    /// the `currently_broken` filter would not have shown up there. This
    /// builds a synthetic one carrying ONE OF EVERY effect `apply` writes
    /// through, so the guard is on the filter itself rather than on today's
    /// data — including the two write paths added on 2026-08-03
    /// (`Indirect` and `AmmoMaxSet`), which reach fields the old test never
    /// looked at (user).
    #[test]
    fn a_broken_evolution_changes_nothing_whatever_it_grants() {
        use crate::loadout::{IndirectStat, WeaponBase};
        let everything = |broken: bool| EvolutionDef {
            misprints: Vec::new(),
            id: "synthetic".into(),
            name: "Synthetic".into(),
            weapon: "torid".into(),
            tier: 9,
            icon: None,
            description: String::new(),
            currently_broken: broken,
            co_base_excludes_this_evolution: None,
            co_base_excludes_only_form: None,
            base_form_only: false,
            effects: vec![
                EvoEffect::FlatBaseDamage(100.0),
                EvoEffect::FlatBaseDamageOnEmptyReload(50.0),
                EvoEffect::FlatBaseCritChance(0.5),
                EvoEffect::FlatBaseCritMultiplier(1.5),
                EvoEffect::FlatBaseStatusChance(0.5),
                EvoEffect::FlatBaseStatusChanceByForm { base: 0.4, incarnon: 0.9 },
                EvoEffect::FlatBaseMagazine(30.0),
                EvoEffect::Indirect(IndirectStat::Accuracy, 0.5),
                EvoEffect::AmmoMaxSet(999.0),
            ],
        };
        let base = WeaponBase::from_data("torid", false, &[]);

        let mut broken = base.clone();
        apply(&mut broken, &[&everything(true)]);
        assert!(
            (broken.base_vector.total() - base.base_vector.total()).abs() < 1e-9,
            "a broken evolution moved base damage"
        );
        assert_eq!(broken.base_crit_chance, base.base_crit_chance);
        assert_eq!(broken.base_crit_damage, base.base_crit_damage);
        assert_eq!(broken.base_status_chance, base.base_status_chance);
        assert_eq!(broken.magazine_size, base.magazine_size);
        assert_eq!(broken.ammo_reserve, base.ammo_reserve, "broken set the reserve");
        assert!(broken.indirect.is_empty(), "broken wrote an indirect stat: {:?}", broken.indirect);

        // ...and the SAME evolution unbroken must move every one of them, or
        // this test would pass on an `apply` that does nothing at all.
        let mut live = base.clone();
        apply(&mut live, &[&everything(false)]);
        assert!(live.base_vector.total() > base.base_vector.total());
        assert!(live.base_crit_chance > base.base_crit_chance);
        assert!(live.base_crit_damage > base.base_crit_damage);
        assert!(live.base_status_chance > base.base_status_chance);
        assert!(live.magazine_size > base.magazine_size);
        assert_eq!(live.ammo_reserve, 999.0);
        assert_eq!(live.indirect, vec![(IndirectStat::Accuracy, 0.5)]);
    }

    /// Final Fusillade is BASE FORM ONLY (user, 2026-07-30). Both forms load
    /// the SAME evolution id — the gate has to be the form, not the id, so this
    /// pins that the charge-backed form comes out with nothing.
    #[test]
    fn final_fusillades_last_round_multishot_skips_the_incarnon_form() {
use crate::loadout::WeaponBase;
        let evos = ["torid_final_fusillade"];
        let base = WeaponBase::from_data("torid", false, &evos);
        let inc = WeaponBase::from_data("torid_incarnon", false, &evos);
        assert!(
            (base.multishot_on_last_round - 3.0).abs() < 1e-9,
            "base form got {}",
            base.multishot_on_last_round
        );
        assert_eq!(
            inc.multishot_on_last_round, 0.0,
            "a charge-backed magazine has no last round to gate on"
        );
        // The flat base damage on the same evolution DOES reach both forms —
        // otherwise this test would pass on a build that dropped the whole
        // evolution rather than just its conditional half.
        let bare = WeaponBase::from_data("torid_incarnon", false, &[]);
        assert!(inc.base_vector.total() > bare.base_vector.total());
    }

    /// Extended Volley: "Does not apply to Incarnon Form's Magazine", and that
    /// form uses max charges rather than a magazine (user, 2026-07-30). The
    /// gate is load-bearing because an Incarnon form's `magazine_size` IS its
    /// charge pool — an ungated `+=` quietly made it 179 rounds.
    #[test]
    fn extended_volley_leaves_the_charge_pool_alone() {
use crate::loadout::WeaponBase;
        let evos = ["torid_extended_volley"];
        let base = WeaponBase::from_data("torid", false, &evos);
        let inc = WeaponBase::from_data("torid_incarnon", false, &evos);
        assert!((base.magazine_size - 14.0).abs() < 1e-9, "5 + 9 = {}", base.magazine_size);
        assert!(
            (inc.magazine_size - 170.0).abs() < 1e-9,
            "the charge pool must stay 170, got {}",
            inc.magazine_size
        );
    }
    /// EVERY evolution effect that loads INERT, pinned.
    ///
    /// An inert effect is a legitimate answer — "+50% Accuracy" decides
    /// nothing in an arena with no geometry — but it is indistinguishable at a
    /// glance from a MISSPELLED `kind:`, which also lands in `Inert(other)`
    /// and silently contributes nothing. That is the failure this exists for:
    /// the Boar's evolutions were written against a loader that had no
    /// crit-multiplier arm, and only reading the loader by hand caught it.
    ///
    /// So the set is written down. Adding an evolution whose effect does not
    /// load fails here until someone states which it is — a mechanic the arena
    /// cannot express, or a typo.
    #[test]
    fn the_inert_evolution_effects_are_the_ones_we_meant() {
        let mut found: Vec<String> = Vec::new();
        for def in pool() {
            for e in &def.effects {
                if let EvoEffect::Inert(what) = e {
                    found.push(format!("{} :: {what}", def.id));
                }
            }
        }
        found.sort();
        // Each line is a DECISION, and the reason is beside the effect in its
        // own yaml. Kept as a flat list so a diff here is readable.
        let expected: Vec<&str> = vec![
            // (The four `unlocks_weapon` tier-1 entries used to live here.
            // They still apply nothing — the form is a separate weapon with
            // its own stats — but they are no longer INERT: `UnlocksForm`
            // carries the form's id, and reading it is what lets a form
            // request imply the evolution that IS that form instead of
            // silently falling back to base (2026-08-04). Inert meant the
            // target was dropped at parse time and "which evolution unlocks
            // the form" had to be guessed from ladder position.)
            // (RELOAD CADENCE used to keep five Ready Retaliations here, on
            // the reasoning that nobody had published their WINDOW: only the
            // Phenmor's page states one ("for 6 seconds") and the rest say
            // "+100% Reload Speed" and stop, so a duration looked like a number
            // that would have to be borrowed from another weapon.
            //
            // There was nothing to borrow. The buff is scoped to the RELOAD
            // ACTION — it arrives when the reload starts and is gone when it
            // ends (owner, 2026-08-11) — so the silence was not missing data,
            // it was the absence of a thing to say. All twelve work now, the
            // Phenmor's 6 s is the buff icon's life rather than the bonus's,
            // and the loader no longer demands a window it should never have
            // wanted.)
            // ---- AMMO EFFICIENCY, and it is CONDITIONAL -----------------
            // Not an indirect stat: efficiency is real DPS the moment a
            // reserve runs dry. But one is gated on a movement state and one
            // on a headshot window, and applying either unconditionally would
            // overstate the build. They also land on the Laetum's Incarnon
            // magazine, which is charge-backed and takes no efficiency at all.
            // ---- ONE-STACK STACKING BUFFS -------------------------------
            // A "timed buff" is a stacking buff with ONE stack — same trigger,
            // same window — so it uses that vocabulary and lands here when its
            // PAYLOAD is one the engine does not model. The label names the
            // payload, so the two are told apart.
            //
            // Ripper Rounds: punch through, multi-target only. Neurotoxin:
            // "+70% Toxin for 3 s on headshot" — REAL DPS on a weapon played
            // at 100% headshots, and the one genuine gap in this list. It is
            // also `currently_broken` in game (DE's wiki, re-read 2026-08-03:
            // "Currently does not work"), and `apply` skips broken evolutions
            // wholesale, so the two cancel out today. Whoever models a
            // per-type buff payload should check DE fixed the perk first —
            // a mechanic that cannot be measured cannot be verified.
            "dual_toxocyst_neurotoxin :: stacking_buff toxin_damage_bonus",
            // ---- THE FURIS GENESIS ---------------------------------------
            // Five of its eight perks, and every one is written under a kind
            // this loader does NOT know — deliberately, because the kinds that
            // would have fit all pay out unconditionally:
            //
            //   `flat_base_damage` ignores `condition:`, so Haven Foray's
            //   overshield clause would have loaded a silent +30 on every
            //   build. `flat_base_crit_multiplier` ignores it too, so Prelude
            //   of Might would have granted +3x to everyone. And a
            //   `stacking_buff` carrying a multishot payload becomes
            //   AssumedMaxMultishot whatever trigger sits beside it, so
            //   Stormburst would have handed +1.2 multishot to builds with no
            //   Electricity in them.
            //
            // An unknown kind is the only spelling that means "nothing models
            // this yet" and stays true.
            //
            // THREE HAVE LEFT THIS LIST. Prelude of Might needed a
            // condition read off the RESOLVED panel, which nothing here did, so
            // it got `CritMultiplierBelowCritChance` and a late hook in
            // `resolve`. Headcracker needed a live stacking buff in the
            // additive fire-rate bucket; `resolve` converts its +5% into the
            // absolute rate that fraction is worth, so the sim never needed an
            // unmodded rate of its own. And STORMBURST needed a stacking buff
            // that could state a TARGET condition — which the static
            // `AssumedMaxMultishot` path cannot, but a LIVE buff can, because
            // it is bumped inside the fight where the target's debuffs are in
            // hand. That was the first buff added AFTER the StackingBuff
            // refactor, and it cost exactly what the design promised: one
            // trigger arm, one grant arm, no bookkeeping.
            //
            // What the remaining one needs: HAVEN FORAY needs a Tenno with
            // overshields, which `TennoCondition` has no room for.
            //
            // EXECUTIONER'S FORTUNE was here until 2026-08-10, on the reading
            // that it "needs a reload the sim can END rather than scale". That
            // was the wrong shape: its trigger is a HEADSHOT, and you cannot
            // shoot while reloading, so there is never a reload in flight for
            // it to end. It is a magazine that fills — the machinery Sentient
            // Surge already had — and the only thing missing was reading the
            // headshot and the kill at the one site that knows both.
            //
            // Every one of them now says so on its own tile — `unmodeled_effects`
            // is derived from these same variants, so this list and the UI
            // cannot disagree.
            // THE PHENMOR (2026-08-08), the first natural Incarnon after the
            // Laetum and the first weapon to bring FOUR inert perks at once.
            // Two are the family's and already argued above — an instant reload
            // the sim cannot end, and Ready Retaliation's reload-speed kind
            // that this loader has no arm for.
            //
            // The other two are new shapes, and both would be worth real damage
            // here rather than being handling stats:
            //
            // SPITEFUL DEFILEMENT is the ANTI-Condition-Overload perk — a crit
            // multiplier that pays while the TARGET carries fewer than three
            // statuses and stops the moment CO starts paying. The counter it
            // needs already exists (CO's bucket IS the status-type count); what
            // does not is a crit bracket that reads it.
            //
            // LINGERING JUDGEMENT is a buff armed by a headshot STREAK — two
            // inside two seconds, held for eight. The engine has per-headshot
            // triggers for fire rate and reload and a flat headshot-damage
            // bonus, but nothing that counts N hits inside a window. On the
            // official ruler, which puts every shot into a head, it would arm
            // on the second shot and never lapse: a flat +50% headshot damage
            // for the whole engagement, and the largest thing on this list.
            // THE BRATON FAMILY (2026-08-08) — one adapter, four weapons, so
            // every gap below is four rows of the same fact. THREE kinds:
            //
            // DARING REVERIE's larger half needs a CHANNELED ABILITY, a
            // Warframe state this arena has no concept of — it fires one weapon
            // and casts nothing. Worth naming because on three of the four
            // variants the conditional half is the BIGGER number, so a Braton's
            // figure here is not its ceiling.
            //
            // MUNITIONS GRIT's +20% multishot has no flat-multishot arm in this
            // loader. Its surcharge (`multishot_consumes_ammo`) IS modelled, and
            // the pair is circular: the surcharge only pays on projectiles
            // multishot generated, so the perk's own multishot is what makes its
            // own multiplier worth anything.
            //
            // GUNSMOKE PICK UP is out of reach twice — no ammo-restore kind, and
            // a PUNCH THROUGH trigger needs a second body behind the first.
            // THE LATRON FAMILY (2026-08-08) — three weapons, four kinds, and
            // two of them are near-misses rather than absences.
            //
            // RIDDLED TARGET wants the live stacking-multishot buff the engine
            // already has; that one's trigger is an ELECTRICITY status
            // (Stormburst's) and this one is PUNCTURE. The machinery exists and
            // the trigger arm does not, which is the whole gap — and it is a
            // large one here, since the base form is 60-80% Puncture, so four
            // stacks of +25% would be held up indefinitely off the weapon's own
            // main damage type.
            //
            // FLENSING SPIKES strips armour per PUNCTURE status. Armour
            // stripping exists for Corrosive and Heat, the two the game strips
            // with; a third rule has no arm. Against the official ruler's Thrax
            // at level 9999 it would be worth a great deal.
            //
            // MARKSMAN'S FOCUS is zoom, which is NOT merely cosmetic in
            // general — a zoom level carries its own damage or crit bonus on
            // many weapons — but carries none on this one. Marksman's Hand is
            // recoil and IS loaded, into the indirect bucket, like every other
            // handling stat here.
            // THE BOLTOR FAMILY (2026-08-08) — three weapons, three kinds.
            //
            // CRIMSON OVERTURE is an on-kill stacking buff on the BASE damage,
            // and it would be the first: the engine's on-kill stacks (Galvanized
            // Chamber's multishot, Bladed Rounds' crit damage) all multiply the
            // base rather than move it.
            //
            // HUNTER'S MANTRA's second half needs a CHANNELED ABILITY, and both
            // of its payloads are spatial anyway — punch-through needs a second
            // body and accuracy needs a miss to prevent.
            //
            // (RAPID REINFORCEMENT used to sit here, four rows at a time. It is
            // IMPLEMENTED now — `EvoEffect::ReloadSpeedBonus`, into the same
            // additive bucket the mods feed — because the intake kept adding it
            // and docs/INCARNON.md counts it on 14 guns. The CONDITIONAL member
            // of the family, Ready Retaliation, is still inert below: its
            // `condition:` is unread, and granting a conditional bonus
            // unconditionally is worse than not granting it.)
        ];
        // TWO POPULATIONS, AND THE PREFIX IS WHICH. The list above is the ARGUED
        // one: a hand-written perk whose effect the engine cannot express, where
        // an inert entry is a decision somebody made and wrote a reason for, and
        // where a NEW one appearing is a mistake until argued.
        //
        // The bulk Incarnon intake (2026-08-08) produces the other population.
        // Its rule engine turns a clause it does not recognise into a kind NAMED
        // `unmodelled_<the clause's own words>` — self-declaring by construction,
        // and there are hundreds of them, one per unrecognised clause per weapon.
        // Listing those individually would be a list nobody reads that grows by
        // 30 lines per adapter; the NAME is the declaration, and both the builder
        // and the optimizer print them as "not modelled yet".
        //
        // The invariant that still bites: the two populations may not mix. A
        // hand-written perk may not hide behind the prefix (its kind would have
        // to be renamed to do so, which is not something you do by accident), and
        // the argued list may not contain a prefixed kind.
        const BULK: &str = "unmodelled_";
        assert!(
            !expected.iter().any(|e| e.contains(&format!(":: {BULK}"))),
            "the argued list must not contain a bulk-intake kind — those declare              themselves by name"
        );
        let expected: Vec<String> = expected.into_iter().map(str::to_string).collect();
        let missing: Vec<&String> = expected.iter().filter(|e| !found.contains(e)).collect();
        let extra: Vec<&String> = found
            .iter()
            .filter(|f| !expected.contains(f))
            .filter(|f| !f.contains(&format!(":: {BULK}")))
            .collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "the inert set moved.
  NEW (implement it, or add it here with a reason, or let the intake name it   `unmodelled_*`): {extra:#?}
  GONE (drop it from the list): {missing:#?}"
        );
    }
    /// An evolution's HANDLING stats reach the resolved panel.
    ///
    /// They have no single-target damage payload, which is exactly why they
    /// used to be dropped — and dropping them meant the evolution equipped and
    /// its number vanished (user, 2026-08-03). This asserts the
    /// whole path: yaml -> loader -> `WeaponBase.indirect` -> `resolve`'s
    /// bucket, in the same place a mod's would land.
    #[test]
    fn an_evolutions_handling_stats_reach_the_panel() {
        use crate::loadout::{resolve, IndirectStat, StackPolicy};
        let of = |weapon: &str, evo: &str| -> Vec<(IndirectStat, f64)> {
            let base = crate::loadout::WeaponBase::from_data(weapon, true, &[evo]);
            resolve(&base, &[], StackPolicy::Emergent).indirect
        };
        let find = |v: &[(IndirectStat, f64)], want: IndirectStat| {
            v.iter().find(|(s, _)| *s == want).map(|(_, x)| *x)
        };

        // Practiced Grip: "+50% Accuracy".
        let grip = of("boar_prime", "boar_prime_practiced_grip");
        assert_eq!(find(&grip, IndirectStat::Accuracy), Some(0.50), "{grip:?}");

        // Fortress Salvo: "+4 Punch Through" (metres), alongside its +16 base
        // damage — a mixed evolution must deliver BOTH halves.
        let salvo = of("boar_prime", "boar_prime_fortress_salvo");
        assert_eq!(find(&salvo, IndirectStat::PunchThrough), Some(4.0), "{salvo:?}");

        // Marksman's Hand: "-50% Recoil". NEGATIVE, like the mods'.
        let hand = of("dual_toxocyst", "dual_toxocyst_marksmans_hand");
        assert_eq!(find(&hand, IndirectStat::Recoil), Some(-0.50), "{hand:?}");

        // Swift Deliverance: "+50% Projectile Speed", which was `unmodeled`.
        let swift = of("torid", "torid_swift_deliverance");
        assert_eq!(find(&swift, IndirectStat::ProjectileSpeed), Some(0.50), "{swift:?}");

        // Mercenary Chamber SETS the reserve rather than adding to a bucket.
        let base = crate::loadout::WeaponBase::from_data(
            "boar_prime", true, &["boar_prime_mercenary_chamber"],
        );
        assert_eq!(base.ammo_reserve, 195.0);
    }
}

/// The two Furis tier-4 perks, both added 2026-08-06, and both from the RAW
/// wikitext rather than the rendered page — which is the point of the pair.
/// Reading the effect column alone gave Headcracker no 50% roll and Prelude of
/// Might no "Base", and each omission makes the perk stronger than the game's.
#[cfg(test)]
mod furis_tier4_tests {
    use super::*;
    use crate::loadout::{resolve, StackPolicy, WeaponBase};

    fn cd_with(mods: &[&str], evos: &[&str]) -> f64 {
        let owned: Vec<String> = evos.iter().map(|s| (*s).to_string()).collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        let base = WeaponBase::from_data("furis_incarnon", true, &refs);
        let pool = crate::mods_data::pool_for_weapon("furis_incarnon");
        let picked: Vec<&crate::loadout::ModDef> = mods
            .iter()
            .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id}")))
            .collect();
        resolve(&base, &picked, StackPolicy::AssumedMax).crit_damage
    }

    /// "Increase BASE Critical Damage Multiplier by +3x" — so crit-damage mods
    /// multiply the raised base. Added AFTER the mods instead, a Primed Target
    /// Cracker build reads 10.14x where the game gives 13.44x, and the two only
    /// diverge once a crit-damage mod is on — which is why the word "Base",
    /// present in the wikitext and absent from the summary, decides it.
    #[test]
    fn prelude_of_might_raises_the_base_multiplier_not_the_final_one() {
        let evo = ["furis_evo1_incarnon_form", "furis_prelude_of_might"];
        let bare = ["furis_evo1_incarnon_form"];
        // 3.4 base, +3 = 6.4 with no crit-damage mod either way.
        assert!((cd_with(&[], &evo) - 6.4).abs() < 1e-9, "{}", cd_with(&[], &evo));
        // With +110%: (3.4 + 3.0) x 2.1 = 13.44, NOT 3.4 x 2.1 + 3.0 = 10.14.
        let modded = cd_with(&["primed_target_cracker"], &evo);
        assert!((modded - 13.44).abs() < 1e-6, "expected 13.44x, got {modded}");
        assert!((cd_with(&["primed_target_cracker"], &bare) - 7.14).abs() < 1e-6);
    }

    /// ...and it is CONDITIONAL: the perk pays only while the build's own crit
    /// chance stays under 40%, so taking it means not building crit chance.
    #[test]
    fn prelude_of_might_switches_off_above_the_threshold() {
        let evo = ["furis_evo1_incarnon_form", "furis_prelude_of_might"];
        // The form's own 26% is under the line; Primed Pistol Gambit clears it.
        assert!((cd_with(&[], &evo) - 6.4).abs() < 1e-9);
        let over = cd_with(&["primed_pistol_gambit"], &evo);
        assert!((over - 3.4).abs() < 1e-9, "over 40% crit it must pay nothing, got {over}");
    }

    /// Headcracker is a LIVE buff, so it is asserted on the loaded spec rather
    /// than on a panel: the 50% roll is the half that a summary drops.
    #[test]
    fn headcracker_carries_its_fifty_percent_roll() {
        let e = get("furis_headcracker").expect("furis_headcracker");
        let hit = e.effects.iter().find_map(|x| match x {
            EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance, .. } => {
                Some((*per_stack, *max_stacks, *duration, *chance))
            }
            _ => None,
        });
        assert_eq!(
            hit,
            Some((0.05, 10, 2.0, 0.50)),
            "raw wikitext: +5% for 2s, x10, \"This effect has a 50% chance of activating\""
        );
    }
}

/// THE FURIS FAMILY SPLITS ON CONDITION OVERLOAD, and the split is the point.
///
/// One Incarnon Genesis upgrades either weapon, so the tempting move is to give
/// them the same CO treatment. The catalog says otherwise by saying nothing:
/// its row names "Furis" and carries that weapon's numbers, there is no
/// MK1-Furis row, and absence from that table is a positive statement that the
/// attack behaves normally (owner confirmed 2026-08-06 — the MK1 does not have
/// the restriction). Lato Vandal has a row and Lato Prime does not, same family
/// and same Genesis, which is what a per-entry slip in DE's code looks like.
///
/// Pinned in BOTH directions so a later tidy-up cannot quietly align them.
#[cfg(test)]
mod furis_co_split_tests {
    use super::*;

    fn excludes(id: &str) -> bool {
        get(id)
            .unwrap_or_else(|| panic!("{id}"))
            .excludes_co_base(
                crate::weapons_data::FormKind::Incarnon,
                crate::loadout::CoBehavior::AdditiveWithBaseDamage,
            )
    }

    #[test]
    fn the_furis_tier2_pair_excludes_its_own_base_from_condition_overload() {
        // "100 or 128 (with Evolution II) | 100 | 100% or 78%" — the CO term
        // keeps multiplying the unevolved 100. On the TIER, because the row
        // names "Evolution II" with no perk number and both options grant +28.
        assert!(excludes("furis_haven_foray"));
        assert!(excludes("furis_stormburst"));
    }

    /// …AND SO DOES THE MK1's, by the DEFAULT rather than by a row.
    ///
    /// It read `!excludes(...)` and was the tidiest illustration of the old
    /// default: same Genesis, same two perk NAMES, and the catalog has a Furis
    /// row and no MK1 Furis row — so the pair differed on nothing but whether
    /// somebody had written them down. That is a description of a survey's
    /// coverage, not of a game mechanic, and an Adding entry no longer reads it
    /// as one (2026-08-16). MULTIPLYING entries still do, and deliberately: see
    /// `excludes_co_base`.
    #[test]
    fn the_mk1_tier2_pair_excludes_it_too() {
        assert!(excludes("mk1_furis_haven_foray"));
        assert!(excludes("mk1_furis_stormburst"));
    }

    /// AN EXPLICIT `false` IS STILL HONOURED, so a measured exception has
    /// somewhere to go. Nothing in `data/` uses it — asserted here so that
    /// stays a fact about the roster rather than an assumption, and so the
    /// opt-out is known to work on the day something needs it.
    #[test]
    fn the_opt_out_exists_and_nothing_uses_it() {
        let opted_out: Vec<&str> = pool()
            .iter()
            .filter(|d| d.co_base_excludes_this_evolution == Some(false))
            .map(|d| d.id.as_str())
            .collect();
        // …and a FORM-SCOPED declaration is the other half of the machinery:
        // the Torid's pair, measured on the Incarnon form and silent about the
        // base one (M50).
        let scoped: Vec<&str> = pool()
            .iter()
            .filter(|d| d.co_base_excludes_only_form.is_some())
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(scoped, ["torid_final_fusillade", "torid_plentiful_mayhem"], "{scoped:?}");
        assert!(
            opted_out.is_empty(),
            "these declare `co_base_excludes_this_evolution: false` — each needs a              measurement in docs/MEASUREMENTS.md: {opted_out:?}"
        );
    }

    /// A QUALIFIER NEVER STANDS ALONE, which is the whole reason it is not
    /// counted as a gap.
    ///
    /// "Stacks up to 4x" is a cap on the bonus above it, and in every perk
    /// that carries one, that bonus is ITSELF inert — so counting the cap said
    /// "partly modelled" twice for one thing, and put a third of the roster's
    /// inert total on a fragment of a sentence.
    ///
    /// The day one appears beside a WORKING effect the argument stops holding:
    /// the bonus applies and its cap does not, which is a real gap and has to
    /// be counted again. That is what this fails on.
    #[test]
    fn a_qualifier_never_stands_alone() {
        let mut alone = Vec::new();
        let mut seen = 0;
        for def in pool() {
            let quals = def
                .effects
                .iter()
                .filter(|e| matches!(e, EvoEffect::Qualifier(_)))
                .count();
            if quals == 0 {
                continue;
            }
            seen += quals;
            // EITHER ADMISSION counts as something to qualify. The cap on
            // "On Punch Through Hit: +10% Critical Chance for 3s. Stacks up to
            // 8x" is not orphaned because the clause it caps became an EDGE
            // rather than a todo — the perk still does nothing and still says
            // so, which is all this test is protecting (2026-08-12).
            let gaps = def.unmodeled_effects().len() + def.out_of_scope_effects().len();
            if gaps == 0 {
                alone.push(def.id.clone());
            }
        }
        // A FLOOR, not a count. This number FALLS as cards get modelled — a
        // modelled stacking card carries its own `max_stacks:` and needs no
        // orphaned "Stacks up to Nx" beside it, which is what took it from 21
        // to 16 when the five Resonant Restores landed (2026-08-12). The
        // assertion only ever protected against the loader dropping the shape
        // entirely, so the floor is set well below the live count and lowered
        // when it is genuinely passed rather than raised to meet it.
        assert!(seen > 10, "only {seen} qualifiers — did the loader stop reading them?");
        assert!(
            alone.is_empty(),
            "a qualifier with nothing to qualify — the cap is real and uncounted: {alone:?}"
        );
    }

    /// THE RATCHET. What the app does not model may go DOWN and not up.
    ///
    /// The disclosure is derived, so the count is honest without anyone
    /// maintaining it — and honest is not the same as improving. A tag that
    /// nobody is obliged to remove becomes a way of feeling finished
    /// (owner, 2026-08-08, asking whether this transparency is good for the
    /// work as well as for the reader).
    ///
    /// Lower this number when a kind gets implemented; that is the only edit
    /// this line should ever see.
    /// *"Does not affect Incarnon Form"* — obeyed, on the two perks where it
    /// is worth a number.
    ///
    /// Eleven evolutions carry the sentence and nine of them qualify something
    /// this sim does not model anyway (ammo capacity, range, an AoE hold), so
    /// the qualifier was transcribed as a NAMED INERT effect and the perk it
    /// qualified went on applying to both forms. On the two that raise a
    /// MAGAZINE that was a real over-valuation: a Zylok Incarnon was fired with
    /// 20 rounds where the card gives it 12.
    ///
    /// Asserted on BOTH forms of BOTH weapons, because a gate that skips
    /// everything passes the half of this that only checks the Incarnon.
    #[test]
    fn a_base_form_only_evolution_reaches_the_base_form_and_stops_there() {
        for (base_id, form_id, perk, added) in [
            ("zylok", "zylok_incarnon", "zylok_extended_volley", 12.0),
            ("onos", "onos_incarnon", "onos_extended_volley", 10.0),
        ] {
            let b_off = crate::loadout::WeaponBase::from_data(base_id, true, &[]);
            let b_on = crate::loadout::WeaponBase::from_data(base_id, true, &[perk]);
            assert_eq!(b_off.form, crate::weapons_data::FormKind::Base);
            assert!(
                (b_on.magazine_size - b_off.magazine_size - added).abs() < 1e-9,
                "{base_id}: the base form takes the whole perk, {} -> {} (+{added} expected)",
                b_off.magazine_size, b_on.magazine_size
            );

            let f_off = crate::loadout::WeaponBase::from_data(form_id, true, &[]);
            let f_on = crate::loadout::WeaponBase::from_data(form_id, true, &[perk]);
            assert_eq!(f_off.form, crate::weapons_data::FormKind::Incarnon);
            assert!(
                (f_on.magazine_size - f_off.magazine_size).abs() < 1e-9,
                "{form_id}: the Incarnon form takes NONE of it, {} -> {}",
                f_off.magazine_size, f_on.magazine_size
            );
        }
    }

    #[test]
    fn the_number_of_unmodelled_evolution_effects_only_goes_down() {
        const CEILING: usize = 5;
        let n: usize = pool().iter().map(|d| d.unmodeled_effects().len()).sum();
        assert!(
            n <= CEILING,
            "{n} inert evolution effects, ceiling {CEILING} — a new gap needs \
             either an implementation or a deliberate raise of this line"
        );
        // …and it is not allowed to drift far BELOW without the ceiling
        // following it down, or the ratchet stops ratcheting.
        assert!(
            n + 25 >= CEILING,
            "{n} inert effects against a ceiling of {CEILING}: lower the ceiling"
        );
    }
}
#[cfg(test)]
mod headcracker_decay_tests {
    /// HEADCRACKER'S TEN STACKS EACH CARRY THEIR OWN CLOCK.
    ///
    /// Owner observed it in game (2026-08-13), which is the same rule
    /// Stormburst carries and the same way it was found. The loader HARDCODED
    /// the Galvanized rule here, on the reading that the card says nothing
    /// else — and the two are not close: under Galvanized decay one headshot
    /// inside the window holds
    /// all ten, under FIFO it holds exactly one.
    ///
    /// Asserted on the DECAY the buff carries rather than on a fight, because
    /// that is the fact that was wrong; `dummy`'s `LiveStacks` already has both
    /// families and is tested on each.
    #[test]
    fn every_headcracker_decays_stack_by_stack() {
        let mut seen = 0;
        for e in crate::evolutions_data::pool() {
            if !e.id.ends_with("_headcracker") {
                continue;
            }
            seen += 1;
            let base = crate::loadout::WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()]);
            let b = base
                .stacking_buffs
                .iter()
                .find(|b| b.id == "on_headshot_fire_rate")
                .unwrap_or_else(|| panic!("{}: no fire-rate buff", e.id));
            assert_eq!(
                b.decay,
                crate::loadout::BuffDecay::PerStackExpiry,
                "{}: each of its {} stacks runs its own clock",
                e.id,
                b.max_stacks
            );
            assert_eq!(b.max_stacks, 10, "{}", e.id);
            assert!((b.chance - 0.5).abs() < 1e-9, "{}: the 50% roll is in the notes cell", e.id);
        }
        assert_eq!(seen, 5, "the Headcracker roster moved");
    }
}

#[cfg(test)]
mod after_mods_layer_tests {
    use super::*;

    /// WHICH PERKS LAND AFTER THE MODS, pinned — because the wording that
    /// distinguishes them is one word long and it has been lost twice.
    ///
    /// A card that reads "Increase BASE Critical Chance by +10%" is a base-stat
    /// bonus and the crit mods multiply it. A card that reads "+10% Critical
    /// Chance", with "Bonuses are added after mods as a flat value" under it, is
    /// not — it lands on the final number and is worth several times less on a
    /// modded build. Same stat, same size, same perk NAME sometimes, different
    /// bracket.
    ///
    /// It has been got wrong in both directions of authorship: the bulk intake
    /// normalised the Felarx's two tier-4 cards into the "Increase Base…"
    /// phrasing that DE does not use for them, and a HAND-WRITTEN file argued
    /// the Phenmor's Survivor's Edge onto the base layer because the Boar's
    /// perk of the same name lives there. So this is a list, and a new member
    /// has to be argued for.
    #[test]
    fn the_after_mods_perks_are_the_ones_we_meant() {
        let mut found: Vec<String> = Vec::new();
        for def in pool() {
            for e in &def.effects {
                let what = match e {
                    EvoEffect::PostModCritChance(v) => format!("crit {v:+}"),
                    EvoEffect::PostModStatusChance(v) => format!("status {v:+}"),
                    _ => continue,
                };
                found.push(format!("{} :: {what}", def.id));
            }
        }
        found.sort();
        let expected = [
            // Its status half is DIVIDED by the base multishot of 4 — verbatim
            // on the page, and right for a per-pellet model: 10% / 4.
            "felarx_brutal_edge :: crit +0.1",
            "felarx_brutal_edge :: status +0.025",
            "felarx_racking_wrath :: crit -0.1",
            "felarx_racking_wrath :: status +0.05",
            // The Laetum's, already on the right layer when it was written by
            // hand — the perk that gave the engine these two kinds.
            "laetum_elemental_excess :: crit -0.1",
            "laetum_elemental_excess :: status +0.2",
            // One projectile, so no division: the listed number is per pellet.
            "onos_elemental_excess :: crit -0.1",
            "onos_elemental_excess :: status +0.2",
            "phenmor_elemental_excess :: crit -0.1",
            "phenmor_elemental_excess :: status +0.2",
            "phenmor_survivors_edge :: crit +0.1",
            "phenmor_survivors_edge :: status +0.1",
        ];
        assert_eq!(found, expected, "the after-mods list moved");
    }

    /// A POST-MOD BONUS IS WORTH LESS THAN THE SAME NUMBER OF BASE POINTS, and
    /// the gap is the whole reason the layer matters.
    #[test]
    fn the_two_layers_are_not_worth_the_same() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let after = ["felarx_brutal_edge".to_string()];
        let a: Vec<&str> = after.iter().map(|s| s.as_str()).collect();
        // A crit mod, so the base layer has something to be multiplied by.
        let pool = crate::mods_data::class_pool("shotgun");
        let crit = pool.iter().find(|m| m.id == "blunderbuss").or_else(
            || pool.iter().find(|m| m.id == "primed_ravage")).expect("a crit mod");
        let plain = resolve(&WeaponBase::from_data("felarx", true, &[]), &[crit],
                            StackPolicy::AssumedMax);
        let post = resolve(&WeaponBase::from_data("felarx", true, &a), &[crit],
                           StackPolicy::AssumedMax);
        // +10 points, flat, whatever the mod did.
        assert!(
            ((post.crit_chance - plain.crit_chance) - 0.10).abs() < 1e-9,
            "flat after mods: {} -> {}", plain.crit_chance, post.crit_chance
        );
    }
    /// A LIVE BASE-CRIT-DAMAGE BUFF REACHES THE DIRECT HIT AND NOTHING ELSE.
    ///
    /// `cd_total` is the direct hit's crit multiplier; a radial explosion and a
    /// lingering field each compute their OWN from their own base stats, and
    /// neither reads this grant. That is not a decision — nobody has measured
    /// whether Mauler's Magazine reaches an explosion, and no weapon carrying
    /// the grant has one, so there is nothing to be right or wrong about yet.
    ///
    /// This is the tripwire for the day that changes. A silently-absent factor
    /// on an AoE part is worth a large fraction of the weapon's damage and would
    /// read as "this build is weaker than it should be" rather than as a bug.
    #[test]
    fn no_weapon_with_a_base_crit_damage_buff_has_an_aoe_part() {
        let mut offenders: Vec<String> = Vec::new();
        for e in pool() {
            let grants_cd = e.effects.iter().any(|f| {
                matches!(f, EvoEffect::StackingGrant { grant, .. }
                    if *grant == crate::loadout::BuffGrant::BaseCritDamage)
            });
            if !grants_cd {
                continue;
            }
            let base = crate::loadout::WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()]);
            if base.radial.is_some() {
                offenders.push(format!("{} ({}): radial", e.id, e.weapon));
            }
        }
        assert!(
            offenders.is_empty(),
            "a base-crit-damage buff now rides a weapon with an AoE part, whose crit \
             multiplier is computed separately and does not read it — decide and \
             measure before shipping it:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// A `# from:` COMMENT IS A CUT, AND A CUT AT THE WRONG PLACE PAYS TWICE.
    ///
    /// The intake transcribes a card by splitting its sentence and filing each
    /// piece as an effect, leaving `# from: "<fragment>"` above each one. When
    /// the split lands INSIDE a clause, the head becomes an unconditional grant
    /// the card never had and the tail becomes an inert remainder — and the
    /// perk pays for both, because nothing downstream can tell that the two
    /// were one sentence. Three faults of exactly this shape, all found on
    /// 2026-08-12 and all invisible to every other test:
    ///
    ///   - the Dera's High Ground: "Increase Base Critical Chance by +25% of
    ///     current Status Chance" cut at the plus sign into a flat +25% base
    ///     crit chance plus an inert "of current Status Chance";
    ///   - the Kunai's Deathtrap Trigger: "…by +1.4x for 4s" cut into a
    ///     PERMANENT +1.4x beside the on-equip window it belongs to;
    ///   - Vicious Promise, on all three Paris: both bullets cut before "on
    ///     undamaged enemies", so the perk paid unconditionally AND on an
    ///     undamaged target.
    ///
    /// So a fragment must END where its clause ends: at a sentence stop, a
    /// comma, a semicolon, or the end of the description. Anything else is a
    /// sentence taken apart in the middle, whatever the pieces then load as.
    #[test]
    fn a_transcribed_fragment_ends_where_its_clause_ends() {
        let mut bad: Vec<String> = Vec::new();
        for (path, text) in crate::data::files_under("evolutions/") {
            let Some(desc) = text.lines().find_map(|l| {
                l.strip_prefix("description:")
                    .map(str::trim)
                    .and_then(|d| d.strip_prefix('"'))
                    .and_then(|d| d.strip_suffix('"'))
            }) else {
                continue;
            };
            for line in text.lines() {
                let Some(frag) = line
                    .trim()
                    .strip_prefix("# from:")
                    .map(str::trim)
                    .and_then(|f| f.strip_prefix('"'))
                    .and_then(|f| f.strip_suffix('"'))
                else {
                    continue;
                };
                let Some(at) = desc.find(frag) else {
                    bad.push(format!("{path}: `{frag}` is not in the description"));
                    continue;
                };
                let tail = desc[at + frag.len()..].trim_start_matches(' ');
                if !tail.is_empty() && !tail.starts_with(['.', ',', ';']) {
                    bad.push(format!("{path}: `{frag}` is cut mid-clause, before `{}`", &tail[..tail.len().min(48)]));
                }
            }
        }
        assert!(bad.is_empty(), "{} fragment(s) cut mid-clause:\n  {}", bad.len(), bad.join("\n  "));
    }

}


