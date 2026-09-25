// SPDX-License-Identifier: AGPL-3.0-or-later
//! WARFRAME ABILITY BUFFS — the fight's PLAYER, loaded from `data/abilities/`.
//!
//! A mod, an arcane and an evolution belong to the BUILD; Roar belongs to the
//! fight — which is why this is its own family and why the BOARD carries none
//! of these. [`resolve`] takes `strength` and a duration override as ARGUMENTS,
//! those being the two inputs that move when frames land.
//!
//! Four effect kinds, each difference measured or quoted:
//!
//! - [`AbilityEffect::FactionDamage`] (Roar) joins a Bane mod's bracket and so
//!   DOUBLE-DIPS on status for free.
//! - [`AbilityEffect::FinalDamage`] (Eclipse) is "an unique multiplier" and
//!   explicitly not double-dipped: "Unlike faction damage, which double dips
//!   for status effects, the one from Eclipse is applied once."
//! - [`AbilityEffect::AddElement`] (Nourish and the four augments) adds a
//!   percentage of ModifiedBase as its element and DOES NOT COMBINE — it lands
//!   on the finished vector, after the elemental hierarchy has run.
//! - [`AbilityEffect::ExtraHit`] (Xata's Whisper) fires a SECOND damage
//!   instance worth a percentage of the first (`fight::fire_extra_hit`).
//!
//! THE FIRST THREE ARE MULTIPLIERS AND THE FOURTH IS AN INSTANCE, which a
//! fifth effect kind has to land on one side of.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::rules::damage::DamageType;

/// What an ability buff does to a weapon's damage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AbilityEffect {
    /// Additive inside the FACTION bracket (Bane's), and matching every
    /// faction rather than one. Double-dips on status because that bracket
    /// does.
    FactionDamage(f64),
    /// Its own multiplicative bucket, applied ONCE — to the hit and to the
    /// status payload alike, never squared.
    FinalDamage(f64),
    /// A FLAT CRITICAL CHANCE, added AFTER the mods rather than into their
    /// bracket — Wrathful Advance's own sentence: *"The final critical chance
    /// applied to melee weapons is a flat value applied after mods (e.g. a
    /// melee weapon with 25% critical chance becomes 225%)"*.
    FlatCritChance(f64),
    /// A FLAT CRITICAL DAMAGE, added to the finished multiplier after the mods
    /// and before the tier: *"Total Critical Damage Multiplier = Base Critical
    /// Damage Multiplier × (1 + Relative Bonus) + Absolute Bonus"* (wiki
    /// `Critical_Hit`), the bucket Cold's received bonus is already in.
    FinalCritDamage(f64),
    /// **ATTACK SPEED / FIRE RATE, ADDITIVE WITH THE MODS.** Warcry states the
    /// bracket and works the example: *"Attack Speed bonus is additive to mods
    /// (e.g., Fury)"*, `Attack Speed Mods + Warcry Modifier × (1 + Strength
    /// Mods)` = `0.3 + 0.5 × (1 + 0.3)`. So the strength knob multiplies this
    /// ability's own share — which is what `scales_with: strength` already does
    /// — and the result joins the same sum a fire-rate mod does.
    FireRate(f64),
    /// `+x of ModifiedBase` as this element, NOT entering the elemental
    /// hierarchy. Additive with elemental mods in SIZE, separate from them in
    /// PLACEMENT.
    /// The element, its size, and whether its STATUS is guaranteed.
    ///
    /// The third field is Valence Formation's and nothing else's so far:
    /// *"applies that Element as a 200% bonus to your weapons WITH GUARANTEED
    /// STATUS for 20s"* (wiki `Valence_Formation`). It is not decoration — a
    /// hit that always procs can never be a hit that procs nothing, which is
    /// exactly the condition Overwhelming Attrition asks about ("On Hit that is
    /// neither Critical nor applies a Status Effect"). Reported from the game:
    /// the two cannot be used together and the augment silently wins.
    AddElement(DamageType, f64, bool),
    /// An EXTRA HIT: a whole second damage instance, entirely of this element,
    /// worth this fraction of the instance that triggered it. Not a multiplier
    /// on anything — `fight::fire_extra_hits`, MECHANICS §7 §"Extra Hit", and
    /// docs/EXTRA_HIT.md for the law its members share.
    ///
    /// THREE THINGS DIFFER BETWEEN MEMBERS and nothing else does, which is what
    /// makes this a category rather than four special cases:
    ///
    /// - the ELEMENT, fixed for most and CHOSEN for Resupply, whose gear wheel
    ///   offers ten. A chosen one arrives on the pick, so one definition serves
    ///   every choice;
    /// - whether its status is GUARANTEED or rolls the weapon's own chance.
    ///   Xata's rolls ("附加的虚空伤害具有基于武器本身触发几率的独立触发几率");
    ///   Toxic Lash is 100% Toxin, and Resupply grants "the selected Elemental
    ///   Damage and Status Effect";
    /// - a WEAPON CLASS that doubles it — Resupply is 20/30/40/50% on Sniper
    ///   Rifles against 10/15/20/25%. Applied in [`resolve`], the one function
    ///   handed both the ability and the weapon it is cast on, so nothing
    ///   downstream has to know what a sniper is.
    ExtraHit {
        element: DamageType,
        fraction: f64,
        forced_status: bool,
    },
    /// AMMO EFFICIENCY (Energized Munitions). Not a damage bracket at all — it
    /// divides what a shot costs the magazine, so what it buys is RELOADS not
    /// taken, which this sim already prices.
    ///
    /// It MULTIPLIES with the other sources rather than adding: "Stacks
    /// multiplicatively with other sources of Ammo Efficiency" (wiki). What
    /// multiplies is the COST, so two sources compose as `1 - (1-a)(1-b)` —
    /// see `fight::ammo_efficiency`, the one place that combines them.
    AmmoEfficiency(f64),
}

/// One selectable buff, as the data declares it.
#[derive(Debug, Clone)]
pub struct AbilityDef {
    pub id: &'static str,
    /// ITS AUGMENT, where one changes what the FIGHT does rather than what the
    /// builder prints: `(mod id, seconds per melee kill, cap as a multiple of
    /// the window)`. Eternal War is the only one so far.
    pub augment: Option<(&'static str, f64, f64)>,
    /// **WHAT ONE CAST COSTS**, before Ability Efficiency — `None` where no
    /// source states it.
    ///
    /// Read from the BUILDER'S CARD (`data/warframe_abilities/<id>.yaml`) where
    /// the frame is one this app seats, so the number is written once. Seven of
    /// these buffs belong to frames the builder does not seat and no page this
    /// repo has read states their cost, so they are `None` and the fight
    /// REFUSES TO CAST THEM rather than inventing a price — they keep the
    /// assumed-up reading, and the page says which.
    pub energy_cost: Option<f64>,
    /// Whether casting it STOPS THE SHOOTING. True is the honest default: most
    /// casts root the frame, and a fight that assumed otherwise would invent
    /// shots a player never took.
    pub interrupts_fire: bool,
    /// Seconds one cast takes at 100% casting speed — [`CAST_SECONDS_UNMEASURED`]
    /// until a measurement replaces it.
    pub cast_seconds: f64,
    pub name: &'static str,
    /// The Warframe it comes from, or `Helminth` for a subsumed version.
    pub frame: &'static str,
    /// Buffs that REPLACE each other rather than adding: same family, only the
    /// strongest runs (wiki, Freeze Force: "Multiple Freeze Forces do not
    /// stack; the buff with the highest Ability Strength will take effect").
    pub family: &'static str,
    pub helminth: bool,
    /// THE SLOT THIS ABILITY CAN BUFF AT ALL, when its card names one.
    ///
    /// Wrathful Advance is *"melee final critical chance"* and pays a gun
    /// nothing — so a weapon it does not name never sees it, rather than the
    /// buff being declared unmodelled on the ability and quietly paid anyway.
    pub requires_slot: Option<&'static str>,
    /// At max rank and 100% Ability Strength.
    pub value: f64,
    /// At max rank and 100% Ability Duration, in seconds — `None` for a source
    /// with no window of its own (an arcane or a precept held on a condition).
    pub duration_seconds: Option<f64>,
    /// EVERY bracket this one cast grants. A list because a single ability can
    /// touch more than one — Redline sets fire rate AND reload speed off one
    /// gauge — and splitting those into two entries would let a player tick
    /// half of an ability.
    pub effects: Vec<AbilityEffect>,
    /// The elements this ability lets you CHOOSE between, empty when it fixes
    /// one. The page draws a picker from this; `resolve` reads the choice off
    /// the pick.
    pub elements: Vec<&'static str>,
    /// (class, multiplier) — a weapon class this is worth more on. See
    /// [`AbilityEffect::ExtraHit`].
    pub class_bonus: Option<(&'static str, f64)>,
    /// Does the page's Ability Strength knob move this one's numbers? False for
    /// the buffs whose wiki row carries no Strength icon — see the yaml field.
    pub scales_with_strength: bool,
    /// What this ability does that the sim does NOT compute, in the player's
    /// own words on the card. Same field name and same meaning as a mod's and
    /// an arcane's, so `/api/meta` publishes it under the same key and the page
    /// renders all three with one function — a gap that lives only in a yaml
    /// comment is a gap nobody can act on.
    pub unmodelled: Vec<&'static str>,
    /// …and the admission that is not a shortfall: this IS modelled, it matches
    /// the live game, and DE did not mean it to work this way. Xata's Whisper
    /// firing off a Blast detonation is the wiki's own Bugs entry.
    pub live_bugs: Vec<&'static str>,
    pub url: Option<&'static str>,
}

/// One buff as it RUNS in a fight: strength applied, duration decided.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAbility {
    pub id: &'static str,
    /// When it stops, in seconds from the start of the engagement.
    /// `f64::INFINITY` = the whole fight (the page's "whole fight" button).
    pub ends_at_seconds: f64,
    pub effects: Vec<AbilityEffect>,
    /// **SECONDS THIS WINDOW GROWS BY, PER MELEE KILL** — an augment's doing
    /// (Eternal War on Warcry), 0 when the frame casting it does not carry one.
    ///
    /// THE ONLY THING IN THIS FIGHT THAT MOVES AN ABILITY'S WINDOW. Every other
    /// window is fixed when the fight starts, so this is read at the ONE site
    /// that asks a live question — the shot loop's rate — and `resolve` refuses
    /// to build one on an ability whose effects are read anywhere else. When the
    /// Warframe becomes an actor that casts, the window becomes run state for
    /// every kind and this guard is what says so.
    pub extend_per_melee_kill_seconds: f64,
    /// …and the ceiling it grows to: *"up to a maximum of double the ability's
    /// duration after mods"* (W`Eternal_War`).
    pub extend_cap_seconds: f64,
    /// WHAT ONE CAST COSTS in this fight, efficiency already spent — `None`
    /// where no source states it, which is what stops it being cast at all.
    pub energy_cost: Option<f64>,
    /// …and what it costs in TIME, casting speed already spent.
    pub cast_seconds: f64,
    /// Whether that time comes out of the shooting.
    pub interrupts_fire: bool,
    /// How long one cast lasts — `ends_at_seconds` is the FIRST window's end,
    /// and a recast opens another of this length.
    pub window_seconds: f64,
}

/// **HOW LONG A CAST TAKES, UNTIL EACH ONE IS MEASURED.** One number for every
/// ability rather than eighteen invented ones: no page this repo has read
/// publishes a cast time, and a per-ability guess would read as sourced.
///
/// It is not idle: the builder already resolves CASTING SPEED, which divides
/// this (`time / (1 + bonus)`), so a build made for it genuinely casts faster.
/// Replace it one ability at a time with `cast: {seconds: …}` and a measurement.
pub const CAST_SECONDS_UNMEASURED: f64 = 1.0;

/// **CASTING THEM, AS AGAINST ASSUMING THEY ARE UP** — docs/BUFFS.md
/// §"Cast, or assumed up" for the whole of it, including what it does not
/// model and why an ability with no stated price is left alone.
///
/// WHO IS CAST IS THE ACTION PRIORITY LIST'S ANSWER AND NOBODY ELSE'S
/// (`data::apl`): casting is an ACTION, so a switch beside the list would be
/// two sources for one fact. Anything it does not name keeps the assumed-up
/// reading, which is what every board row was measured under.
///
/// A RECAST IS SEAMLESS, WHICH IS WHY A PLAN IS ENOUGH: recasting exactly at
/// expiry makes the windows contiguous, so "how many casts the energy buys" IS
/// "how long the buff is up" — nothing downstream learns that casting exists.
pub struct CastPlan {
    /// The moments a cast takes the trigger finger away, in order, each with
    /// the seconds it costs.
    pub interrupts: Vec<(f64, f64)>,
}

/// Plan the casting of the abilities `cast` names out of `energy`, shortening
/// each window to what the pool pays for. Returns what the shooting loses and
/// when. Anything `cast` does not name keeps the assumed-up reading.
pub fn plan_casts(
    list: &mut [ActiveAbility],
    cast: &[&str],
    energy: f64,
    fight_seconds: f64,
) -> CastPlan {
    // IN TIME ORDER, because one pool pays for all of them: two abilities due at
    // the same moment are paid in the order they were picked, and a pool that
    // cannot pay the second leaves it down from there.
    let planned = |a: &ActiveAbility| a.energy_cost.is_some() && cast.contains(&a.id);
    let mut due: Vec<(f64, usize)> =
        list.iter().enumerate().filter(|(_, a)| planned(a)).map(|(i, _)| (0.0, i)).collect();
    let mut left = energy;
    let mut ends: Vec<Option<f64>> = vec![None; list.len()];
    let mut interrupts = Vec::new();
    while let Some(k) = due
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| *t <= fight_seconds)
        .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.0.cmp(&b.0)))
        .map(|(k, _)| k)
    {
        let (t, i) = due.remove(k);
        let a = &list[i];
        let Some(cost) = a.energy_cost else { continue };
        if left < cost {
            continue;
        }
        left -= cost;
        let end = t + a.window_seconds;
        ends[i] = Some(end);
        if a.interrupts_fire {
            interrupts.push((t, a.cast_seconds));
        }
        if end.is_finite() {
            due.push((end, i));
        }
    }
    for (i, end) in ends.iter().enumerate() {
        // THE PLAN REPLACES THE WINDOW RATHER THAN CAPPING IT: `ends_at_seconds`
        // was ONE cast's worth, and recasting is exactly how a player keeps a
        // 20 s buff up for a 180 s fight. What limits it is the pool, above.
        if let Some(e) = end {
            list[i].ends_at_seconds = *e;
        } else if planned(&list[i]) {
            // Not one cast was affordable, so it was never up. Only for one the
            // LIST asked for: an ability nobody cast is assumed up, untouched.
            list[i].ends_at_seconds = f64::NEG_INFINITY;
        }
    }
    interrupts.sort_by(|a, b| a.0.total_cmp(&b.0));
    CastPlan { interrupts }
}

impl ActiveAbility {
    /// Is it running at `t`? Half-open, so a 30s Roar is gone at exactly 30.
    pub fn live_at(&self, t: f64) -> bool {
        t < self.ends_at_seconds
    }
}

#[derive(Deserialize)]
struct EffectFile {
    kind: String,
    /// This effect's own number, when the ability's headline `value` is not it.
    /// Redline grants +75% fire rate and +50% reload speed off one cast.
    #[serde(default)]
    value: Option<f64>,
    #[serde(default)]
    element: Option<String>,
    /// `element: selectable` — the choices, in the order the game offers them.
    #[serde(default)]
    elements: Vec<String>,
    /// Does its status always land, or does it roll the weapon's chance?
    #[serde(default)]
    forced_status: bool,
    /// A weapon CLASS this is worth more on, and by how much (Resupply doubles
    /// on `sniper`). Two fields rather than a map: there is one such rule.
    #[serde(default)]
    class_bonus_for: Option<String>,
    #[serde(default)]
    class_bonus: Option<f64>,
}

#[derive(Deserialize)]
struct AugmentFile {
    id: String,
    seconds_per_melee_kill: f64,
    cap_multiple: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFile {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Deserialize)]
struct AbilityFile {
    id: String,
    name: String,
    frame: String,
    family: String,
    /// See [`AbilityDef::requires_slot`].
    #[serde(default)]
    requires_slot: Option<String>,
    #[serde(default)]
    helminth: bool,
    value: f64,
    #[serde(default)]
    duration_seconds: Option<f64>,
    /// ONE effect, the shape every ability had before Redline. Kept because it
    /// is what most of them are and a list of one reads worse than a value.
    #[serde(default)]
    effect: Option<EffectFile>,
    /// …or SEVERAL, for a cast that touches more than one bracket. Exactly one
    /// of the two is written; both or neither is a data error and says so.
    #[serde(default)]
    effects: Vec<EffectFile>,
    /// DOES ABILITY STRENGTH MOVE THIS NUMBER? `strength` (the default) for
    /// every buff whose card carries the Strength icon; `none` for the ones
    /// whose wiki row does not, and a buff that ignores the knob says so on its
    /// own card.
    ///
    /// **READ THE ICON, NEVER THE FAMILY**: a column heading carrying
    /// `{{Stat|<X>|icon=only}}` scales with X and a merely underlined one
    /// scales with nothing. Four of the five element-adding augments are
    /// identical and the fifth is not:
    ///
    /// ```text
    /// Shock Trooper      ! {{Stat|Ability Strength|icon=only}}Electricity Damage
    /// Fireball Frenzy    ! {{Stat|Ability Strength|icon=only}}Heat Damage
    /// Freeze Force       ! {{Stat|Ability Strength|icon=only}}Cold Damage
    /// Venom Dose         ! {{Stat|Ability Strength|icon=only}}Corrosive Damage
    /// Valence Formation  ! <u>Elemental Damage</u>
    /// ```
    ///
    /// Valence Formation does what the other four do and does NOT scale the
    /// way they do: its Duration column carries the Duration icon, so the row
    /// is marked to say Duration and not Strength.
    #[serde(default)]
    scales_with: Option<String>,
    /// `augment: {id, seconds_per_melee_kill, cap_multiple}` — see
    /// `AbilityDef::augment`.
    #[serde(default)]
    augment: Option<AugmentFile>,
    /// ONLY WHERE THERE IS NO BUILDER CARD to read it off — the loader refuses
    /// both, so the cost is stated once.
    #[serde(default)]
    energy_cost: Option<f64>,
    /// Whether the cast stops the shooting; true where it is not said.
    #[serde(default)]
    interrupts_fire: Option<bool>,
    /// Seconds at 100% casting speed, once one is measured.
    #[serde(default)]
    cast_seconds: Option<f64>,
    #[serde(default)]
    unmodelled: Vec<String>,
    #[serde(default)]
    live_bugs: Vec<String>,
    #[serde(default)]
    source: Option<SourceFile>,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every ability buff in the data, in id order.
pub fn all() -> &'static [AbilityDef] {
    static A: OnceLock<Vec<AbilityDef>> = OnceLock::new();
    A.get_or_init(|| {
        let mut out: Vec<AbilityDef> = Vec::new();
        for (path, text) in crate::data::files_under("abilities/") {
            let f: AbilityFile = serde_norway::from_str(text)
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            // ONE `effect:` OR A LIST OF `effects:`, never both and never
            // neither — a file that says both would have a silent winner, and
            // one that says neither is an ability that does nothing.
            let list: Vec<EffectFile> = match (f.effect, f.effects.is_empty()) {
                (Some(e), true) => vec![e],
                (None, false) => f.effects,
                (Some(_), false) => panic!("{path}: write `effect:` or `effects:`, not both"),
                (None, true) => panic!("{path}: needs an `effect:` or `effects:`"),
            };
            // THE PICKER'S CHOICES AND THE CLASS BONUS belong to the ABILITY,
            // not to one of its effects — only an extra hit carries either
            // today, and an ability has at most one. Taken off whichever effect
            // states them, before the list is consumed.
            let elements: Vec<String> =
                list.iter().find(|e| !e.elements.is_empty()).map_or(Vec::new(), |e| e.elements.clone());
            let class_bonus: Option<(String, f64)> = list.iter().find_map(|e| {
                match (e.class_bonus_for.clone(), e.class_bonus) {
                    (Some(c), Some(x)) => Some((c, x)),
                    _ => None,
                }
            });
            let effects: Vec<AbilityEffect> = list
                .into_iter()
                .map(|ef| {
                    // The two element-carrying kinds read the same field the
                    // same way, so it is parsed once — a second copy of these
                    // four lines is how one of them ends up accepting a typo
                    // the other rejects.
                    //
                    // A SELECTABLE ELEMENT IS PART OF THAT, and used not to be:
                    // it was handled in the `extra_hit` arm alone, so
                    // `add_element` rejected `selectable` outright — which is
                    // exactly the divergence the paragraph above forbids, one
                    // arm short of the rule. It cost the ONE ability that adds a
                    // COMBINED element: Lavos casts whichever mix he infused, so
                    // Valence Formation needs a picker without being an extra
                    // hit, and could not be stated at all.
                    //
                    // The choice DEFAULTS to the first entry and is replaced by
                    // the pick at resolve; a fixed element is itself.
                    let element = |kind: &str| {
                        let e = ef
                            .element
                            .as_deref()
                            .unwrap_or_else(|| panic!("{path}: {kind} needs an element"));
                        let e = if e == "selectable" {
                            ef.elements.first().map(String::as_str).unwrap_or_else(|| {
                                panic!("{path}: `element: selectable` needs `elements:`")
                            })
                        } else {
                            e
                        };
                        DamageType::from_name(e)
                            .unwrap_or_else(|| panic!("{path}: unknown element {e}"))
                    };
                    // EACH EFFECT MAY CARRY ITS OWN NUMBER, and falls back to
                    // the ability's headline `value`. Redline's fire rate and
                    // reload speed are 75% and 50% off one cast, so one value
                    // could not have served both.
                    let v = ef.value.unwrap_or(f.value);
                    match ef.kind.as_str() {
                        "faction_damage" => AbilityEffect::FactionDamage(v),
                        "final_damage" => AbilityEffect::FinalDamage(v),
                        "add_element" => AbilityEffect::AddElement(
                            element("add_element"), v, ef.forced_status),
                        "ammo_efficiency" => AbilityEffect::AmmoEfficiency(v),
                        "flat_crit_chance" => AbilityEffect::FlatCritChance(v),
                        "final_crit_damage" => AbilityEffect::FinalCritDamage(v),
                        "fire_rate" => AbilityEffect::FireRate(v),
                        "extra_hit" => AbilityEffect::ExtraHit {
                            element: element("extra_hit"),
                            fraction: v,
                            forced_status: ef.forced_status,
                        },
                        other => panic!("{path}: unknown ability effect kind {other}"),
                    }
                })
                .collect();
            let scales_with_strength = match f.scales_with.as_deref() {
                None | Some("strength") => true,
                Some("none") => false,
                Some(other) => panic!("{path}: unknown `scales_with: {other}`"),
            };
            // THE CARD IS THE AUTHORITY on what a cast costs, and the yaml may
            // state it only where the frame is not one this app seats.
            // BY THE BASE ID: a subsumed variant is the same ability, and its
            // card is filed under the name the frame casts it by.
            let base_id = f.id.trim_end_matches("_helminth");
            let card_cost = crate::data::warframes::ability(base_id).map(|a| a.energy_cost);
            let energy_cost = match (card_cost, f.energy_cost) {
                (Some(_), Some(_)) => {
                    panic!("{path}: the builder's card already states this energy cost")
                }
                (Some(c), None) => Some(c),
                (None, v) => v,
            };
            out.push(AbilityDef {
                id: leak(f.id),
                name: leak(f.name),
                frame: leak(f.frame),
                family: leak(f.family),
                helminth: f.helminth,
                requires_slot: f.requires_slot.map(leak),
                value: f.value,
                duration_seconds: f.duration_seconds,
                effects,
                scales_with_strength,
                elements: elements.iter().cloned().map(leak).collect(),
                class_bonus: class_bonus.map(|(c, x)| (leak(c), x)),
                unmodelled: f.unmodelled.into_iter().map(leak).collect(),
                augment: f.augment.map(|a| {
                    (leak(a.id), a.seconds_per_melee_kill, a.cap_multiple)
                }),
                energy_cost,
                interrupts_fire: f.interrupts_fire.unwrap_or(true),
                cast_seconds: f.cast_seconds.unwrap_or(CAST_SECONDS_UNMEASURED),
                live_bugs: f.live_bugs.into_iter().map(leak).collect(),
                url: f.source.and_then(|s| s.url).map(leak),
            });
        }
        out.sort_by_key(|a| a.id);
        out
    })
}

pub fn get(id: &str) -> Option<&'static AbilityDef> {
    all().iter().find(|a| a.id == id)
}

/// One entry of a request: which buff, and how long the player says it lasts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbilityPick<'a> {
    pub id: &'a str,
    /// Seconds, or `None` for "the whole fight".
    pub duration_seconds: Option<f64>,
    /// Which element, where the ability offers a choice (Resupply's gear
    /// wheel). `None` everywhere else, and on a stored pick that predates the
    /// picker — the definition's own first choice stands in.
    pub element: Option<&'a str>,
}

/// Scale an ability's value by Ability Strength.
///
/// LINEAR, for all three kinds. Every page here states the bonus as a single
/// max-rank number "affected by Ability Strength" with a per-rank table that is
/// itself linear in rank, and none of them documents a cap — so 300% strength
/// is 3x the printed number and the calculator says so rather than guessing at
/// a ceiling DE has not written down.
pub fn at_strength(v: f64, strength: f64) -> f64 {
    v * strength
}

/// The buffs that are actually RUNNING, given what was picked and how strong
/// the frame is.
///
/// THE FAMILY RULE IS APPLIED HERE, once, so no consumer can forget it: within
/// a family only the strongest survives. Comparing by
/// resolved VALUE rather than by whether one is subsumed is what makes the
/// rule survive a buffed Helminth Roar beating an unbuffed Rhino's — which
/// is the case the wiki's "highest Ability Strength will take effect" is
/// about.
///
/// Unknown ids are dropped rather than erroring: a stored scenario outlives the
/// data, and a fight that refuses to run because a buff was renamed is worse
/// than one that runs without it. The page reports what it dropped.
/// **THE FRAME CASTING THEM**, as the four stats and the cards that change what
/// a cast is worth. One struct rather than six positional arguments: every one
/// of these arrives from the same resolve of the same Warframe build, and the
/// day a fifth stat matters it is added in one place.
#[derive(Debug, Clone, Copy)]
pub struct Caster<'a> {
    /// 1.0 = 100%.
    pub strength: f64,
    pub duration: f64,
    pub efficiency: f64,
    /// A share, not a multiplier: `time / (1 + bonus)`.
    pub casting_speed_bonus: f64,
    /// Augment mod ids the frame seats (`Resolved::augments`).
    pub augments: &'a [&'a str],
}

impl Default for Caster<'_> {
    /// THE FRAME NOBODY NAMED: every stat at 100% and no card, which is what
    /// the board is scored under.
    fn default() -> Self {
        Self { strength: 1.0, duration: 1.0, efficiency: 1.0, casting_speed_bonus: 0.0, augments: &[] }
    }
}

pub fn resolve(
    picks: &[AbilityPick<'_>],
    by: &Caster<'_>,
    weapon_class: &str,
    weapon_slot: &str,
) -> Vec<ActiveAbility> {
    let (strength, duration, efficiency, casting_speed_bonus, augments) =
        (by.strength, by.duration, by.efficiency, by.casting_speed_bonus, by.augments);
    let mut best: Vec<(&'static str, f64, ActiveAbility)> = Vec::new();
    for p in picks {
        let Some(def) = get(p.id) else { continue };
        // AN ABILITY THAT NAMES A SLOT PAYS NO OTHER. Wrathful Advance buys
        // "melee final critical chance" and a gun sees none of it.
        if def.requires_slot.is_some_and(|s| s != weapon_slot) {
            continue;
        }
        // THE STRENGTH KNOB, applied once per effect and skipped entirely by an
        // ability whose card carries no Strength icon. `value` (the headline)
        // still decides the family contest below, so a buff that ignores
        // strength cannot be beaten by a weaker sibling that scales.
        let scale = |v: f64| {
            if def.scales_with_strength {
                at_strength(v, strength)
            } else {
                v
            }
        };
        let value = scale(def.value);
        // THE PICK'S element wins wherever the ability offers a choice, and
        // "wherever" is the whole point: this was written into the `extra_hit`
        // arm alone, so an `add_element` with a gear wheel would have drawn the
        // picker, taken the pick, and paid the FIRST entry whatever was chosen. An ability offers at most one choice, so one reading of
        // it serves every effect it has.
        let picked = |stated: DamageType| {
            p.element
                .and_then(DamageType::from_name)
                .filter(|_| !def.elements.is_empty())
                .unwrap_or(stated)
        };
        let effects: Vec<AbilityEffect> = def
            .effects
            .iter()
            .map(|e| match *e {
                AbilityEffect::FactionDamage(v) => AbilityEffect::FactionDamage(scale(v)),
                AbilityEffect::FinalDamage(v) => AbilityEffect::FinalDamage(scale(v)),
                AbilityEffect::AddElement(t, v, f) => {
                    AbilityEffect::AddElement(picked(t), scale(v), f)
                }
                AbilityEffect::AmmoEfficiency(v) => AbilityEffect::AmmoEfficiency(scale(v)),
                AbilityEffect::FlatCritChance(v) => AbilityEffect::FlatCritChance(scale(v)),
                AbilityEffect::FinalCritDamage(v) => AbilityEffect::FinalCritDamage(scale(v)),
                AbilityEffect::FireRate(v) => AbilityEffect::FireRate(scale(v)),
                AbilityEffect::ExtraHit { element, fraction, forced_status } => {
                    AbilityEffect::ExtraHit {
                        element: picked(element),
                        fraction: scale(fraction)
                            * def
                                .class_bonus
                                .map_or(1.0, |(c, x)| if c == weapon_class { x } else { 1.0 }),
                        forced_status,
                    }
                }
            })
            .collect();
        // THE AUGMENT IS THE FRAME'S, not the pick's: it pays only when the
        // Warframe casting this is carrying it.
        let grows = def
            .augment
            .filter(|(id, _, _)| augments.contains(id))
            .map_or((0.0, 0.0), |(_, per_kill, cap_multiple)| {
                let ends = p.duration_seconds.unwrap_or(f64::INFINITY);
                (per_kill * duration, ends * cap_multiple)
            });
        assert!(
            grows.0 <= 0.0
                || effects.iter().all(|e| matches!(e, AbilityEffect::FireRate(_))),
            "{}: a window that grows is read only where the fight asks a live              question, which today is the rate — see ActiveAbility",
            def.id
        );
        let live = ActiveAbility {
            id: def.id,
            ends_at_seconds: p.duration_seconds.unwrap_or(f64::INFINITY),
            effects,
            extend_per_melee_kill_seconds: grows.0,
            extend_cap_seconds: grows.1,
            // EFFICIENCY AND CASTING SPEED ARE SPENT HERE, the one place handed
            // both the ability and the frame casting it — the same reason the
            // strength knob is spent here (W`Ability_Efficiency`:
            // `cost x max(2 - efficiency, 25%)`; W`Ability_Duration`'s sibling
            // rule for speed: `time / (1 + bonus)`).
            energy_cost: def.energy_cost.map(|c| c * (2.0 - efficiency).max(0.25)),
            cast_seconds: def.cast_seconds / (1.0 + casting_speed_bonus),
            interrupts_fire: def.interrupts_fire,
            window_seconds: p.duration_seconds.unwrap_or(f64::INFINITY),
        };
        match best.iter_mut().find(|(f, _, _)| *f == def.family) {
            Some(slot) if slot.1 >= value => {}
            Some(slot) => *slot = (def.family, value, live),
            None => best.push((def.family, value, live)),
        }
    }
    best.into_iter().map(|(_, _, a)| a).collect()
}

/// The FACTION-bracket total running at `t` (Roar's contribution to the bucket
/// a Bane mod is in).
pub fn faction_bonus_at(list: &[ActiveAbility], t: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::FactionDamage(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// The FIRE-RATE share running at `t` (Warcry's attack speed). Summed, and the
/// caller adds it to the mods' own sum rather than multiplying by it.
pub fn fire_rate_at(list: &[ActiveAbility], t: f64, extra_seconds: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t - if a.extend_per_melee_kill_seconds > 0.0 { extra_seconds } else { 0.0 }))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::FireRate(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// The FINAL multiplier running at `t` (Eclipse). Multiplicative between
/// distinct sources, which is what "an unique multiplier" says — and there is
/// only ever one of these today, since Eclipse's two variants share a family.
pub fn final_mult_at(list: &[ActiveAbility], t: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::FinalDamage(v) => Some(1.0 + v),
            _ => None,
        })
        .product()
}

/// FLAT CRITICAL CHANCE running at `t` — added after the mods, never into
/// their bracket. Two sources would add, the way two of anything here do.
pub fn flat_crit_at(list: &[ActiveAbility], t: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::FlatCritChance(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// FLAT CRITICAL DAMAGE running at `t` — added to the finished multiplier,
/// never scaled by the weapon's base. Two sources add.
pub fn final_crit_damage_at(list: &[ActiveAbility], t: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::FinalCritDamage(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// Which of the ADD-ELEMENT buffs are running at `t`, as (element, fraction of
/// ModifiedBase). Two buffs granting the SAME element add, because they are
/// additive with elemental mods and therefore with each other.
pub fn added_elements_at(list: &[ActiveAbility], t: f64) -> Vec<(DamageType, f64)> {
    let mut out: Vec<(DamageType, f64)> = Vec::new();
    for a in list.iter().filter(|a| a.live_at(t)) {
        for e in &a.effects {
            if let AbilityEffect::AddElement(ty, v, _) = *e {
                match out.iter_mut().find(|(t2, _)| *t2 == ty) {
                    Some(slot) => slot.1 += v,
                    None => out.push((ty, v)),
                }
            }
        }
    }
    out
}

/// The EXTRA HITS running at `t`, as (element, fraction of the triggering
/// instance).
///
/// THE ELEMENTS WHOSE STATUS IS GUARANTEED, at time `t`.
///
/// Valence Formation imbues an element *"with guaranteed Status"* (wiki), so
/// every hit while it is up applies that element's proc whatever the weapon's
/// status chance. It rides the same `forced` list a weapon's own "guaranteed
/// Impact proc" rides, which is what makes the rest of the status path — the
/// immunity renormalisation, the DoT bookkeeping — need to know nothing about
/// it.
///
/// A SET, not a sum: two abilities forcing the same element force it once.
pub fn forced_status_elements_at(list: &[ActiveAbility], t: f64) -> Vec<DamageType> {
    let mut out: Vec<DamageType> = Vec::new();
    for a in list.iter().filter(|a| a.live_at(t)) {
        for e in &a.effects {
            if let AbilityEffect::AddElement(ty, _, true) = *e {
                if !out.contains(&ty) {
                    out.push(ty);
                }
            }
        }
    }
    out
}

/// A LIST, and they do not merge the way [`added_elements_at`] merges two
/// grants of one element: each source is its own second damage instance with
/// its own status roll, so Toxic Lash and Xata's Whisper on one weapon are two
/// extra hits and not one bigger one (wiki `Extra_Hit`, which counts them
/// per source). Only one per FAMILY can be in this list — [`resolve`] settled
/// that before anything got here.
pub fn extra_hits_at(list: &[ActiveAbility], t: f64) -> Vec<ExtraHitLive> {
    list.iter()
        .filter(|a| a.live_at(t))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::ExtraHit { element, fraction, forced_status } => {
                Some(ExtraHitLive { element, fraction, forced_status })
            }
            _ => None,
        })
        .collect()
}

/// AMMO EFFICIENCY running at `t`, composed MULTIPLICATIVELY between abilities
/// — "Stacks multiplicatively with other sources of Ammo Efficiency" (wiki,
/// Energized Munitions). What multiplies is the ammo COST, so two 75% sources
/// are `1 - 0.25 x 0.25` = 93.75% and never 150%.
///
/// The other sources (the buff bar, the arcanes) add among themselves and this
/// composes with their total — `fight::ammo_efficiency` is where that happens,
/// because it is the one function that has all of them.
pub fn ammo_efficiency_at(list: &[ActiveAbility], t: f64) -> f64 {
    let cost: f64 = list
        .iter()
        .filter(|a| a.live_at(t))
        .flat_map(|a| a.effects.iter())
        .filter_map(|e| match *e {
            AbilityEffect::AmmoEfficiency(v) => Some(1.0 - v),
            _ => None,
        })
        .product();
    1.0 - cost
}

/// One extra hit as the sim needs it: what element, what share of the instance
/// that triggered it, and whether its status is a roll or a certainty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtraHitLive {
    pub element: DamageType,
    pub fraction: f64,
    /// Toxic Lash is "100% (Toxin status chance)" and Resupply grants "the
    /// selected Elemental Damage and Status Effect"; Xata's rolls the weapon's
    /// own. The difference is per member, so it travels with the member.
    pub forced_status: bool,
}

/// Every distinct moment the ACTIVE SET changes, `0.0` first and each expiry
/// after it, deduplicated and sorted.
///
/// The sim precomputes a damage vector per interval rather than per shot: the
/// set is piecewise constant and changes at most once per buff, so this turns
/// a per-shot rebuild into at most seven of them.
pub fn checkpoints(list: &[ActiveAbility]) -> Vec<f64> {
    let mut out = vec![0.0];
    for a in list {
        if a.ends_at_seconds.is_finite() && a.ends_at_seconds > 0.0 && !out.contains(&a.ends_at_seconds) {
            out.push(a.ends_at_seconds);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in checkpoints"));
    out
}

#[cfg(test)]
mod cast_tests {
    use super::*;

    fn ab(id: &'static str, window: f64, cost: f64, interrupts: bool) -> ActiveAbility {
        ActiveAbility {
            id,
            ends_at_seconds: f64::INFINITY,
            effects: vec![],
            extend_per_melee_kill_seconds: 0.0,
            extend_cap_seconds: 0.0,
            energy_cost: Some(cost),
            cast_seconds: 0.5,
            interrupts_fire: interrupts,
            window_seconds: window,
        }
    }

    /// **A POOL IS A BUDGET OF CASTS, AND THE WINDOW IS WHAT IT BUYS.** A 20 s
    /// Warcry at 75 energy out of a 300 pool is four casts, so it is up for 80 s
    /// of a longer fight and down after — which is the whole point of casting
    /// rather than assuming.
    #[test]
    fn a_pool_buys_a_number_of_casts_and_the_window_is_what_it_bought() {
        let mut list = vec![ab("warcry", 20.0, 75.0, true)];
        let plan = plan_casts(&mut list, &["warcry"], 300.0, 180.0);
        assert_eq!(list[0].ends_at_seconds, 80.0);
        // …AND EACH CAST TOOK THE TRIGGER FINGER: four of them, half a second each.
        assert_eq!(plan.interrupts.len(), 4);
        assert_eq!(plan.interrupts.iter().map(|(_, s)| s).sum::<f64>(), 2.0);
        assert_eq!(plan.interrupts[1].0, 20.0, "the recast is at the lapse");
    }

    /// ONE POOL PAYS FOR ALL OF THEM, so two abilities share it and the second
    /// goes down first when it runs out.
    #[test]
    fn one_pool_pays_for_every_ability() {
        let mut list = vec![ab("a", 10.0, 50.0, false), ab("b", 10.0, 50.0, false)];
        plan_casts(&mut list, &["a", "b"], 150.0, 100.0);
        // 150 buys three casts between them, and the ties go to the pick order.
        let total: f64 = list.iter().map(|a| a.ends_at_seconds).sum();
        assert_eq!(total, 30.0, "{:?}", list.iter().map(|a| a.ends_at_seconds).collect::<Vec<_>>());
    }

    /// A CAST NOBODY CAN AFFORD NEVER HAPPENS, and the buff was never up.
    #[test]
    fn an_ability_no_pool_can_pay_for_is_never_up() {
        let mut list = vec![ab("warcry", 20.0, 75.0, true)];
        let plan = plan_casts(&mut list, &["warcry"], 10.0, 180.0);
        assert!(list[0].ends_at_seconds.is_infinite() && list[0].ends_at_seconds < 0.0);
        assert!(plan.interrupts.is_empty());
    }

    /// **THE LIST DECIDES WHO IS CAST.** One named ability is paid for and
    /// shortened to what the pool bought; the one beside it, priced and
    /// affordable, is still ASSUMED UP — because nothing in the action list
    /// says the player ever casts it.
    #[test]
    fn an_ability_the_list_never_names_is_assumed_up() {
        let mut list = vec![ab("warcry", 20.0, 75.0, true), ab("roar", 20.0, 75.0, true)];
        list[1].ends_at_seconds = f64::INFINITY;
        let plan = plan_casts(&mut list, &["warcry"], 150.0, 180.0);
        assert_eq!(list[0].ends_at_seconds, 40.0, "two casts is what 150 bought");
        assert!(list[1].ends_at_seconds.is_infinite() && list[1].ends_at_seconds > 0.0);
        // …and the pool paid for the named one alone, so the shooting lost two
        // casts rather than four.
        assert_eq!(plan.interrupts.len(), 2);
    }

    /// AN ABILITY WITH NO PRICE IS LEFT ALONE — the gap is ours, not the build's.
    #[test]
    fn an_ability_with_no_stated_cost_keeps_its_window() {
        let mut list = vec![ab("x", 10.0, 0.0, true)];
        list[0].energy_cost = None;
        list[0].ends_at_seconds = 30.0;
        let plan = plan_casts(&mut list, &["x"], 0.0, 180.0);
        assert_eq!(list[0].ends_at_seconds, 30.0);
        assert!(plan.interrupts.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ability_loads_and_says_where_it_came_from() {
        let a = all();
        assert!(a.len() >= 12, "{} abilities", a.len());
        for d in a {
            assert!(!d.name.is_empty());
            assert!(d.value > 0.0, "{}", d.id);
            // A window is stated wherever there is one, and a castable ability
            // always has one: a price with no window buys nothing.
            assert!(d.duration_seconds.is_none_or(|s| s > 0.0), "{}", d.id);
            assert!(d.energy_cost.is_none() || d.duration_seconds.is_some(), "{}", d.id);
            // A NUMBER WITHOUT A SOURCE IS A GUESS. Every one of these is a
            // wiki figure and the file has to name the page it came off.
            assert!(
                d.url.is_some_and(|u| u.starts_with("https://wiki.warframe.com/")),
                "{} has no wiki source",
                d.id
            );
        }
    }

    /// THE ONE RULE THE PAGE CANNOT BE TRUSTED WITH. Picking Roar and Roar
    /// (Helminth) is an ordinary thing to do — one is your frame, one is a
    /// squadmate's — and adding them would be worth +80% instead of +50%.
    #[test]
    fn two_buffs_of_a_family_do_not_stack_and_the_stronger_wins() {
        let picks = [
            AbilityPick { id: "roar_helminth", duration_seconds: None, element: None },
            AbilityPick { id: "roar", duration_seconds: None, element: None },
        ];
        let live = resolve(&picks, &Caster { strength: 1.0, ..Default::default() }, "", "melee");
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, "roar");
        assert_eq!(faction_bonus_at(&live, 0.0), 0.5);

        // …and STRENGTH decides it, not which one is subsumed: the wiki's rule
        // is "the buff with the highest Ability Strength will take effect", so
        // a 200%-strength Helminth Roar (0.60) beats a 100% Rhino's (0.50).
        // Same call, different strengths, is the closest this can get to two
        // players — and it is what the field means.
        let solo = resolve(&[AbilityPick { id: "roar_helminth", duration_seconds: None, element: None }], &Caster { strength: 2.0, ..Default::default() }, "", "melee");
        assert!(faction_bonus_at(&solo, 0.0) > 0.5);
    }

    /// Different families DO add — Roar and Eclipse are different buckets, and
    /// two different elements are two different elements.
    #[test]
    fn different_families_all_run_at_once() {
        let picks = [
            AbilityPick { id: "roar", duration_seconds: None, element: None },
            AbilityPick { id: "eclipse", duration_seconds: None, element: None },
            AbilityPick { id: "shock_trooper", duration_seconds: None, element: None },
            AbilityPick { id: "freeze_force", duration_seconds: None, element: None },
            AbilityPick { id: "xatas_whisper", duration_seconds: None, element: None },
        ];
        let live = resolve(&picks, &Caster { strength: 1.0, ..Default::default() }, "", "melee");
        assert_eq!(live.len(), 5);
        assert_eq!(faction_bonus_at(&live, 0.0), 0.5);
        assert!((final_mult_at(&live, 0.0) - 3.0).abs() < 1e-9);
        let els = added_elements_at(&live, 0.0);
        assert_eq!(els.len(), 2);
        assert!(els.iter().all(|(_, v)| (*v - 1.0).abs() < 1e-9));
        // …and the extra hit is in NEITHER of those two lists, which is the
        // whole point of it being a fourth kind: it is not an element added to
        // the vector and not a multiplier on it.
        let xh = extra_hits_at(&live, 0.0);
        assert_eq!(xh.len(), 1);
        assert_eq!(xh[0].element, DamageType::Void);
        assert!((xh[0].fraction - 0.26).abs() < 1e-9);
    }

    /// THE SUBSUMED COPY IS THE WHOLE ABILITY, and that is why there is only
    /// one card for it.
    ///
    /// Every other Helminth variant here exists because the subsumed version is
    /// WEAKER — Roar loses 20 points, Eclipse 170. The wiki lists no reduced
    /// ladder for this one, so the two would have been the same 26% under two
    /// names, and a family whose members are identical is one buff listed twice. Asserted from the other side: the pairs that DO
    /// differ still differ, so deleting the duplicate cannot be mistaken for
    /// a licence to collapse the rest.
    #[test]
    fn the_subsumed_whisper_needed_no_card_of_its_own() {
        assert!(get("xatas_whisper").is_some());
        assert!(
            get("xatas_whisper_helminth").is_none(),
            "the subsumed copy carries the same numbers — it is one buff"
        );
        for (full, cut) in [
            ("roar", "roar_helminth"),
            ("eclipse", "eclipse_helminth"),
            ("nourish", "nourish_helminth"),
        ] {
            assert!(get(cut).unwrap().value < get(full).unwrap().value, "{cut}");
        }
    }

    /// AN EXTRA HIT ADMITS WHAT IT DOES NOT DO — the Bullet Attractor it
    /// applies and the Blast interaction that is a bug, both on its own card,
    /// because a card renders its own text and nothing else speaks for it.
    #[test]
    fn the_whisper_states_its_gaps_and_its_bug() {
        let d = get("xatas_whisper").expect("xatas_whisper");
        assert!(
            d.unmodelled.iter().any(|u| u.contains("Bullet Attractor")),
            "it says nothing about the Void proc"
        );
        assert!(
            d.live_bugs.iter().any(|b| b.contains("Blast")),
            "it does not admit the Blast interaction is a bug"
        );
        // NEGATIVE CONTROL: an ability with nothing to admit admits nothing. A
        // check that only asserts presence passes just as well on a data set
        // that shouts "not modelled" at everything.
        let roar = get("roar").expect("roar");
        assert!(roar.unmodelled.is_empty() && roar.live_bugs.is_empty());
    }

    /// A DURATION IS A DURATION. The buff bar is not consulted; these start at
    /// the first shot and stop, which is what the page's control means.
    /// WRATHFUL ADVANCE IS A FLAT CRITICAL CHANCE, AND A GUN NEVER SEES IT.
    ///
    /// *"The final critical chance applied to melee weapons is a flat value
    /// applied AFTER mods (e.g. a melee weapon with 25% critical chance becomes
    /// 225% when a rank 3 Wrathful Advance is active)"* — so +200% is +2.00 on
    /// the finished number, and the subsumed version is half of it: *"melee
    /// critical chance bonus reduced to 25% / 50% / 75% / 100%"*.
    ///
    /// THE SLOT IS THE GATE. The card says MELEE, so a rifle resolves the pick
    /// to nothing at all rather than the buff being declared and paid anyway.
    #[test]
    fn wrathful_advance_is_flat_melee_crit_and_a_gun_resolves_none_of_it() {
        let at = |id: &'static str, slot: &str| {
            let picks = [AbilityPick { id, duration_seconds: None, element: None }];
            flat_crit_at(&resolve(&picks, &Caster { strength: 1.0, ..Default::default() }, "hammer", slot), 0.0)
        };
        assert_eq!(at("wrathful_advance", "melee"), 2.0, "Kullervo's own is +200%");
        assert_eq!(at("wrathful_advance_helminth", "melee"), 1.0, "subsumed is half");
        assert_eq!(at("wrathful_advance", "primary"), 0.0, "a gun is not what the card names");

        // …AND ABILITY STRENGTH MOVES IT, the way it moves every other value
        // whose card carries the Strength icon.
        let picks = [AbilityPick { id: "wrathful_advance", duration_seconds: None, element: None }];
        assert_eq!(flat_crit_at(&resolve(&picks, &Caster { strength: 1.5, ..Default::default() }, "hammer", "melee"), 0.0), 3.0);
        // …and it ends when the buff does. An unset duration is the page's
        // WHOLE FIGHT, so the ten seconds the card states have to be asked for.
        let ten = [AbilityPick {
            id: "wrathful_advance",
            duration_seconds: Some(10.0),
            element: None,
        }];
        assert_eq!(flat_crit_at(&resolve(&ten, &Caster { strength: 1.0, ..Default::default() }, "hammer", "melee"), 9.0), 2.0);
        assert_eq!(flat_crit_at(&resolve(&ten, &Caster { strength: 1.0, ..Default::default() }, "hammer", "melee"), 11.0), 0.0);
    }

    #[test]
    fn a_duration_ends_the_buff_and_whole_fight_never_does() {
        let live = resolve(
            &[
                AbilityPick { id: "roar", duration_seconds: Some(30.0), element: None },
                AbilityPick { id: "eclipse", duration_seconds: None, element: None },
            ],
            &Caster { strength: 1.0, ..Default::default() },
            "",
            "melee",
        );
        assert_eq!(faction_bonus_at(&live, 29.9), 0.5);
        assert_eq!(faction_bonus_at(&live, 30.0), 0.0);
        assert_eq!(faction_bonus_at(&live, 1e9), 0.0);
        assert!((final_mult_at(&live, 1e9) - 3.0).abs() < 1e-9);
        assert_eq!(checkpoints(&live), vec![0.0, 30.0]);
    }

    /// The four augments pay a MEASURED element each, and Venom Dose's is the
    /// one a rule engine would get wrong: it is named for a toxin and pays
    /// Corrosive.
    #[test]
    fn the_augments_pay_the_elements_the_wiki_says() {
        for (id, want) in [
            ("shock_trooper", DamageType::Electricity),
            ("fireball_frenzy", DamageType::Heat),
            ("freeze_force", DamageType::Cold),
            ("venom_dose", DamageType::Corrosive),
            ("nourish", DamageType::Viral),
        ] {
            let d = get(id).unwrap_or_else(|| panic!("{id} missing"));
            assert!(
                d.effects.iter().any(|e| matches!(*e, AbilityEffect::AddElement(t, _, _) if t == want)),
                "{id}: {:?}",
                d.effects
            );
        }
    }

    /// THE ONE THAT ADDS A **COMBINED** ELEMENT, which is what makes it worth
    /// having and what made it impossible to state.
    ///
    /// Every other member of this family pays one fixed primary, so a picker
    /// and an `add_element` had never met: `selectable` was implemented on the
    /// `extra_hit` arm alone, in both the parser and the resolver. Two failures
    /// in a row, the second silent — the file would have loaded, the page would
    /// have drawn the wheel, and every pick would have paid Heat.
    ///
    /// WHY A COMBINED ELEMENT IS THE POINT: a DoT tick reads `1 + Σ THAT
    /// ELEMENT's own bonuses` and only literal same-element sources count, so
    /// 90% Toxin + 90% Heat makes the HIT Gas and contributes nothing to the
    /// Gas burn. Nothing in a mod list can add Gas literally; this can — the
    /// wiki's own sentence being *"added parallel to the weapon's Elemental
    /// Damage, meaning it will NOT combine with elements on the weapon"*
    /// (measured on a Braton Prime).
    #[test]
    fn valence_formation_imbues_the_combined_element_it_was_given() {
        let def = get("valence_formation").expect("valence_formation");
        assert_eq!(def.elements.len(), 10, "four primaries and six combinations");
        let pick = |e: Option<&'static str>| {
            let live =
                resolve(&[AbilityPick { id: "valence_formation", duration_seconds: None, element: e }], &Caster::default(), "rifle", "melee");
            added_elements_at(&live, 0.0)
        };
        // A COMBINATION SURVIVES THE PICK. Gas is the measured case and is not
        // reachable any other way.
        assert_eq!(pick(Some("gas")), vec![(DamageType::Gas, 2.0)]);
        // …and so does a primary, which the family's other four also do.
        assert_eq!(pick(Some("cold")), vec![(DamageType::Cold, 2.0)]);
        // No pick is the first choice, the same stand-in the gear wheel uses.
        assert_eq!(pick(None), vec![(DamageType::Heat, 2.0)]);

        // ABILITY STRENGTH DOES NOT MOVE IT, and the mod's
        // own stats table marks its two columns differently to say so: the
        // Duration column carries the Ability Duration stat icon and the
        // Elemental Damage column is underlined with no icon at all. A column
        // with a stat icon scales with that stat; this one has none.
        //
        // Asserted at a strength the knob would obviously move — 2.5x is +150%,
        // which would read as +500% elemental damage on a card and in the sim.
        let strong = resolve(
            &[AbilityPick { id: "valence_formation", duration_seconds: None, element: Some("gas") }],
            &Caster { strength: 2.5, ..Default::default() },
            "rifle",
            "melee",
        );
        assert_eq!(added_elements_at(&strong, 0.0), vec![(DamageType::Gas, 2.0)]);
        // …AND THE CONTROL, because an assertion that a number did not move
        // passes just as well on a build where the knob is wired to nothing:
        // Roar's row DOES carry the Strength icon, and 50% x 2.5 is 125%.
        let roar = resolve(&[AbilityPick { id: "roar", duration_seconds: None, element: None }], &Caster { strength: 2.5, ..Default::default() }, "rifle", "melee");
        assert!((faction_bonus_at(&roar, 0.0) - 1.25).abs() < 1e-9, "{}", faction_bonus_at(&roar, 0.0));
    }
}
    /// A FAMILY WITH TWO IDENTICAL MEMBERS IS ONE BUFF LISTED TWICE.
    ///
    /// The Helminth variants exist because the subsumed version is WEAKER —
    /// Roar 50% → 30%, Eclipse 200% → 30%, Nourish 75% → 45%. Where the wiki
    /// lists no reduced ladder the ability is unchanged, and a second card
    /// carrying the same number is a choice nobody can make wrongly and nobody
    /// can make rightly: the family rule already runs whichever is stronger, so
    /// ticking both is ticking one.
    #[test]
    fn no_two_abilities_of_a_family_are_the_same_buff() {
        for a in all() {
            for b in all() {
                if a.id >= b.id || a.family != b.family {
                    continue;
                }
                assert!(
                    (a.value - b.value).abs() > 1e-9 || a.duration_seconds != b.duration_seconds,
                    "{} and {} are the same buff — keep one",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// THE CATEGORY'S THREE PER-MEMBER FACTS, asserted on the members that
    /// have them — because each one is a field, and a field nobody reads is a
    /// field that quietly stops working.
    #[test]
    fn the_extra_hit_members_differ_only_in_what_the_data_says() {
        // 1. A CHOSEN element. Resupply's gear wheel offers ten; the pick
        //    decides, and the definition's first choice stands in for a pick
        //    that predates the picker.
        let def = get("resupply").expect("resupply");
        assert_eq!(def.elements.len(), 10, "the gear wheel");
        let dflt = resolve(&[AbilityPick { id: "resupply", duration_seconds: None, element: None }], &Caster::default(), "rifle", "melee");
        assert_eq!(extra_hits_at(&dflt, 0.0)[0].element, DamageType::Heat, "the first choice");
        let cold = resolve(
            &[AbilityPick { id: "resupply", duration_seconds: None, element: Some("cold") }],
            &Caster { strength: 1.0, ..Default::default() },
            "rifle",
            "melee",
        );
        assert_eq!(extra_hits_at(&cold, 0.0)[0].element, DamageType::Cold);
        // …and a chosen element is ignored where the ability fixes one.
        let fixed = resolve(
            &[AbilityPick { id: "xatas_whisper", duration_seconds: None, element: Some("cold") }],
            &Caster { strength: 1.0, ..Default::default() },
            "rifle",
            "melee",
        );
        assert_eq!(extra_hits_at(&fixed, 0.0)[0].element, DamageType::Void);

        // 2. A WEAPON CLASS that doubles it: 25% on a rifle, 50% on a sniper.
        let rifle = extra_hits_at(&dflt, 0.0)[0].fraction;
        let sniper = extra_hits_at(
            &resolve(&[AbilityPick { id: "resupply", duration_seconds: None, element: None }], &Caster::default(), "sniper", "melee"),
            0.0,
        )[0]
        .fraction;
        assert!((rifle - 0.25).abs() < 1e-9, "{rifle}");
        assert!((sniper - 0.50).abs() < 1e-9, "{sniper}");

        // 3. A FORCED status, or the weapon's own roll. Toxic Lash is "100%
        //    (Toxin status chance)" and Resupply grants "the selected Elemental
        //    Damage and Status Effect"; Xata's rolls.
        for (id, forced) in [("toxic_lash", true), ("resupply", true), ("xatas_whisper", false)] {
            let live = resolve(&[AbilityPick { id, duration_seconds: None, element: None }], &Caster::default(), "rifle", "melee");
            assert_eq!(extra_hits_at(&live, 0.0)[0].forced_status, forced, "{id}");
        }

        // …and Toxic Lash's own number, for guns: 30% Toxin, 45 s.
        let tl = get("toxic_lash").expect("toxic_lash");
        assert!((tl.value - 0.30).abs() < 1e-9);
        assert_eq!(tl.duration_seconds, Some(45.0));
        assert!(tl.effects.iter().any(|e| matches!(*e,
            AbilityEffect::ExtraHit { element: DamageType::Toxin, .. })));
    }

