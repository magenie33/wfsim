//! **WHAT A FIGHT CONSISTS OF, declared once for the whole product** — [`BUILD_AXES`](crate::builds::BUILD_AXES)'s sibling, for the
//! other half of a simulation.
//!
//! A BUILD is what you are carrying; a FIGHT is everything else, and until this
//! list existed the second half had no declaration at all. Three consequences,
//! and they are why this file exists rather than being tidy:
//!
//! 1. **THE PAGE RE-DERIVED THE RULES.** Some scenario fields are settled by
//!    the weapon rather than by the reader, and `parse_fight` settled them
//!    while `app.js` independently greyed the same boxes. Two implementations
//!    of one rule, able to drift in silence, because a settled field looks
//!    identical whoever settled it. The pattern `evo_forbids` and `auras: [id]`
//!    already refuse: THE ENGINE DECIDES AND `/api/meta` STATES THE CONSEQUENCE.
//!
//! 2. **`defaultScenario()` IS A HAND LIST.** A field the server gains and the
//!    page forgets does not RESET when a preset is switched — the Eximus bug of
//!    2026-08-07, where switching back to the official ruler left it fighting
//!    an Eximus because that yaml never says `eximus:`.
//!
//! 3. **A READER COULD NOT SEE THE WHOLE FIGHT.** What is hidden or settled for
//!    the current weapon was invisible, so "the same fight across two weapons"
//!    was a claim nobody could check.
//!
//! **THE DOCUMENT KEEPS EVERYTHING; THE UI SHOWS WHAT APPLIES.** The buff map's
//! own rule (AGENTS.md), generalised: the whole map travels because it is the
//! FIGHT's, and pruning it to what the current build can grant made the quick
//! calc a different fight the moment a candidate granted a buff the current
//! build lacked.
//!
//! # THE PRIMITIVE IS A CAPABILITY, NOT A "FORCED" FLAG
//!
//! The first version of this file had an `Applies::WhenWeapon(Rule)` and a
//! `forced_for` that matched on the axis id — which was a hand list wearing a
//! type, and whose declared `Rule` the function then ignored and re-derived.
//! Worse, "forced" is ONE VERB over several different relationships, and they
//! come apart the moment a second kind of weapon exists:
//!
//!   * the weapon has no such mechanism at all (a Sentinel has no reserve);
//!   * the mechanism is there and the answer is not yours (a Sentinel aims,
//!     always).
//!
//! So an axis declares the CAPABILITIES it needs, in order, each with the value
//! it resolves to when the weapon lacks one. Nothing is written down as forced:
//! it is DERIVED by asking the weapon. The reason is one sentence per
//! CAPABILITY rather than one per (axis, weapon) pair, so a new weapon class
//! costs no new prose.
//!
//! **THE AMMO BOX IS WHY.** It is settled either way and for OPPOSITE reasons —
//! `HasReserve` absent means nothing can run out (on), `CanResupply` absent
//! means pickups it cannot get (off). One flag read as the wrong one of those
//! two facts ticked-and-disabled the box on the whole roster, so the only
//! weapon whose ammo you could adjust was the one weapon the game gives no way
//! to adjust. Two capabilities cannot make that mistake; one flag
//! could not avoid it.
//!
//! **AND IT IS WHAT MELEE WILL NEED.** The same `aiming` axis wants OPPOSITE
//! values on two weapon kinds — a Sentinel is always aiming (`ChoosesAim`
//! absent, so true), a melee weapon never aims at all (`Aims` absent, so false)
//! — which one rule per axis cannot say and two capabilities say without
//! arguing. `Aims` is deliberately NOT declared yet: there is no melee weapon
//! in the roster, and a capability every weapon has is a capability nothing
//! tests. The shape is here so the day it lands is a one-line day.
//!
//! WHAT THIS FILE IS NOT: it does not parse, apply or validate. `parse_fight`
//! still reads the request and is still the authority on what a field MEANS.
//! This says which fields exist, and when the weapon takes the choice away.

/// A value a settled axis resolves to.
///
/// Two shapes, because those are the two an axis can be settled to today. An
/// `Id` or a `Structured` axis has never been settled by a weapon; the day one
/// is, it gets a variant and the match below stops compiling, which is the
/// right way to find out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisValue {
    Flag(bool),
    Number(f64),
}

/// **A QUESTION ABOUT A WEAPON THAT THE WEAPON DATA ALREADY ANSWERS.**
///
/// A CLOSED SET, and each arm is a pure function of [`WeaponSpec`]. That is the
/// discipline that keeps this from becoming a second place to get a fact wrong
/// — the lesson `docs/CATALOGS.md` records in another domain. A capability that
/// needed its own data field would be a taxonomy inventing facts, not reading
/// them.
/// **WHY A WEAPON LACKS A CAPABILITY — and therefore whether a SCENARIO may
/// argue with it**.
///
/// The four capabilities are not the same kind of thing, and the difference
/// decides what a scenario file is allowed to CLAIM:
///
///   * a GAME FACT is the game's own rule. A Sentinel cannot put a shot on a
///     head; a scenario that said otherwise would produce a number nobody can
///     reproduce in game, which is the opposite of this product's promise.
///   * a HOUSE RULE is OURS. "Infinite ammo" is already a stand-in for ammo
///     PICKUPS the sim has no entities for — the finite reserve is "the
///     pessimistic half of a mechanic we only half have" (`parse_fight`). Which
///     half a fight is scored under is a RULER'S CHOICE, and the official
///     rulers already make it in prose: *"Ammo pickups are modelled. An entry
///     that cannot be resupplied runs on its own reserve."*
///
/// SO OVERRIDES SIT BEHIND LEGALITY, and that is the whole guard: a scenario
/// may say "in my fight, Arch-Guns have infinite ammo" and may not say "in my
/// fight, Sentinels land headshots". Exactly one of today's four is a house
/// rule, and it is the one the owner reached for first — which is a sign the
/// distinction was already there and merely unwritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Absence {
    /// The game's rule. A scenario may not override it.
    GameFact,
    /// Our own stand-in for something the sim does not model. A scenario may
    /// override it per weapon class.
    HouseRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The wielder decides whether to aim.
    ///
    /// A companion weapon is fired by the Sentinel and is ALWAYS aiming — the mechanism is there, the choice is not.
    ChoosesAim,
    /// A shot can land on a weak point.
    ///
    /// Distinct from [`Capability::ChoosesAim`] on purpose: a Sentinel aims and
    /// still never aims at the HEAD, so the two are separate answers about the
    /// same weapon. Collapsing them is how a headshot rate ends up settled by
    /// the wrong fact.
    AimsAtHead,
    /// There is a pool behind the magazine at all.
    ///
    /// False only for a companion weapon — "Ammo Max: ∞ / Ammo Type: None".
    HasReserve,
    /// The game gives some way to refill that pool mid-fight.
    ///
    /// False only for a ground Arch-Gun, which is removed when empty.
    CanResupply,
}

impl Capability {
    /// Is its absence the GAME's rule, or ours?
    pub fn absence(self) -> Absence {
        match self {
            // A Sentinel really does aim, really does never aim at a head, and
            // really has no reserve. None of the three is a simplification.
            Capability::ChoosesAim | Capability::AimsAtHead | Capability::HasReserve => {
                Absence::GameFact
            }
            // …but whether an unrefillable reserve should be SCORED as running
            // dry is our question, not the game's: the game removes the weapon
            // and the sim has no pickups either way. A scenario may pick.
            Capability::CanResupply => Absence::HouseRule,
        }
    }
}

impl Capability {
    /// Does this weapon have it?
    pub fn of(self, spec: &crate::weapons_data::WeaponSpec) -> bool {
        // ONE PLACE READS THE SLOT, so "what a companion weapon is" is written
        // once rather than in each arm.
        let companion = spec.slot == "sentinel";
        // …AND ONE PLACE READS MELEE, for the same reason. A swing is not
        // aimed and cannot be put on a head, and both of those are the GAME's
        // rules rather than ours — see `absence`.
        let melee = spec.slot == "melee";
        match self {
            Capability::ChoosesAim => !companion && !melee,
            Capability::AimsAtHead => !companion && !melee,
            // The same two facts `/api/meta` serves to the roster grid, read
            // from the same place: a pool behind the magazine at all, and
            // whether the game gives any way to refill it.
            Capability::HasReserve => spec.ammo_max.is_some_and(|a| a > 0.0),
            Capability::CanResupply => !spec.no_resupply,
        }
    }

    /// What to tell a reader whose weapon does NOT have it.
    ///
    /// ONE SENTENCE PER (CAPABILITY, KIND OF WEAPON), and it took the WEAPON
    /// until melee landed. It was one sentence per capability, on the reasoning
    /// that a sentence about the weapon reads correctly wherever it is shown —
    /// which was true while a companion was the only thing that could not aim.
    /// A melee weapon cannot either, for a completely different reason, and
    /// "a sentinel weapon is fired by the companion" printed under a hammer is
    /// worse than no sentence at all.
    ///
    /// So it takes the spec. It still states a fact about the WEAPON rather
    /// than about the field, which is what makes it correct in the panel, in
    /// the ruler's own prose and in the disclosure banner alike.
    ///
    /// English is the source, as everywhere; the overlay translates it. A full
    /// sentence because it is the only thing the reader gets in exchange for
    /// losing the control, and a disabled box with a one-word tooltip reads as
    /// broken.
    pub fn why_absent(self, spec: &crate::weapons_data::WeaponSpec) -> &'static str {
        let melee = spec.slot == "melee";
        match self {
            Capability::ChoosesAim if melee => {
                "a melee swing is not aimed down sights, so nothing gated on aiming pays here"
            }
            Capability::AimsAtHead if melee => {
                "a melee weapon does not put a swing on a head in this arena, so this is 0 whatever is typed — and every on-headshot effect stays dead"
            }
            Capability::ChoosesAim => {
                "a sentinel weapon is fired by the companion and is always aiming — the state is real, the choice is not"
            }
            Capability::AimsAtHead => {
                "a sentinel weapon never aims at the head, so this is 0 whatever is typed — and every on-headshot effect stays dead"
            }
            Capability::HasReserve if melee => "a melee weapon has no ammo at all",
            Capability::HasReserve => "this weapon has no ammo reserve to run out of",
            Capability::CanResupply => {
                "this weapon cannot be resupplied — once its reserve is gone it is removed for five minutes, so the setting has nothing to stand in for"
            }
        }
    }
}

/// One capability an axis needs, and what the axis becomes without it.
///
/// The VALUE is here rather than on the capability because the same missing
/// capability can mean different things to different axes — and, when melee
/// lands, the same AXIS means opposite things under two different missing
/// capabilities.
#[derive(Debug, Clone, Copy)]
pub struct Requirement {
    pub cap: Capability,
    pub absent: AxisValue,
}

/// How a scenario axis is spelled on the wire and what shape it holds.
///
/// The KIND is here because the whole-fight panel renders from this list rather
/// than from a hand-written form: a reader asking to see the whole fight gets
/// every axis, including the ones this weapon hides, and a control cannot be
/// drawn without knowing whether it is a tick or a number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisKind {
    Flag,
    Number { min: f64, max: f64 },
    /// An id from a roster — an enemy, a Warframe.
    Id,
    /// Anything the panel shows as what it HOLDS rather than as a control: a
    /// formation, a buff map, an aura list. Edited in its own panel, because
    /// two controls over one document is how one of them silently undoes the
    /// other (the arena's own rule, 2026-08-16).
    Structured,
}

/// Which block of the panel an axis belongs to, so a reader asked to look at
/// "the whole fight" is not handed thirty flat rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Target,
    Engagement,
    Wielder,
    Squad,
}

/// One field of a fight.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioAxis {
    /// The stable id, and the wire field. Unlike a build axis these are the
    /// same string: a fight has one representation on the wire, and the page's
    /// `sim` object uses the wire spelling directly.
    pub id: &'static str,
    pub kind: AxisKind,
    pub group: Group,
    /// The capabilities this axis needs to be the reader's, IN ORDER. The first
    /// one the weapon lacks settles the axis — order is the rule, which is what
    /// lets the ammo box mean two opposite things without a special case.
    pub requires: &'static [Requirement],
}

const FREE: &[Requirement] = &[];

/// **EVERY FIELD A FIGHT HAS.**
///
/// Read against `parse_fight` and `tenno_from`, the two functions that actually
/// consume a request — derived from what they READ rather than from what the
/// page happens to send, so a field the page has never sent is still declared
/// and a field nothing reads is not.
pub const SCENARIO_AXES: &[ScenarioAxis] = &[
    // ---- the target ------------------------------------------------------
    ScenarioAxis { id: "enemy", kind: AxisKind::Id, group: Group::Target, requires: FREE },
    ScenarioAxis { id: "level", kind: AxisKind::Number { min: 1.0, max: 9999.0 }, group: Group::Target, requires: FREE },
    // THE SCENARIO'S OWN HOUSE RULES, per weapon class — the field that makes a
    // fight a complete document rather than one that describes the weapon in
    // front of you. It is FREE because it is about the
    // OTHER classes: a rule for Arch-Guns is legible, editable and travels on a
    // Burston's page, which is the whole point of it.
    ScenarioAxis { id: "class_rules", kind: AxisKind::Structured, group: Group::Engagement, requires: FREE },
    ScenarioAxis { id: "steel_path", kind: AxisKind::Flag, group: Group::Target, requires: FREE },
    // NULL is a real third state — "whatever this unit is by default" — so the
    // page stores it as `null | true | false`. Only an explicit choice is
    // stored, which is what keeps switching targets giving the elite unit
    // wherever one exists.
    ScenarioAxis { id: "eximus", kind: AxisKind::Flag, group: Group::Target, requires: FREE },
    ScenarioAxis { id: "custom_enemies", kind: AxisKind::Structured, group: Group::Target, requires: FREE },
    ScenarioAxis { id: "formation", kind: AxisKind::Structured, group: Group::Target, requires: FREE },
    ScenarioAxis { id: "player_at", kind: AxisKind::Structured, group: Group::Target, requires: FREE },
    ScenarioAxis { id: "target_at", kind: AxisKind::Structured, group: Group::Target, requires: FREE },
    ScenarioAxis { id: "aim_at", kind: AxisKind::Structured, group: Group::Target, requires: FREE },

    // ---- the engagement --------------------------------------------------
    ScenarioAxis { id: "duration", kind: AxisKind::Number { min: 1.0, max: 3600.0 }, group: Group::Engagement, requires: FREE },
    ScenarioAxis { id: "metric", kind: AxisKind::Id, group: Group::Engagement, requires: FREE },
    // THE ORDER IS THE RULE. No reserve at all wins over cannot-refill, because
    // a weapon with nothing to run out of is not a weapon whose pickups you are
    // denied — and the two settle this same box to OPPOSITE values.
    ScenarioAxis {
        id: "infinite_ammo",
        kind: AxisKind::Flag,
        group: Group::Engagement,
        requires: &[
            Requirement { cap: Capability::HasReserve, absent: AxisValue::Flag(true) },
            Requirement { cap: Capability::CanResupply, absent: AxisValue::Flag(false) },
        ],
    },

    // ---- the wielder -----------------------------------------------------
    ScenarioAxis {
        id: "aiming",
        kind: AxisKind::Flag,
        group: Group::Wielder,
        // WHEN MELEE LANDS this list gains `Aims` FIRST, absent = false: a
        // melee weapon does not aim at all, where a Sentinel aims and cannot
        // stop. Same axis, opposite values, two capabilities — which is the
        // case one rule per axis could not express.
        requires: &[Requirement { cap: Capability::ChoosesAim, absent: AxisValue::Flag(true) }],
    },
    ScenarioAxis {
        id: "headshot_pct",
        kind: AxisKind::Number { min: 0.0, max: 100.0 },
        group: Group::Wielder,
        requires: &[Requirement { cap: Capability::AimsAtHead, absent: AxisValue::Number(0.0) }],
    },
    ScenarioAxis { id: "invisible", kind: AxisKind::Flag, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "airborne", kind: AxisKind::Flag, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "overshields", kind: AxisKind::Flag, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "channeling", kind: AxisKind::Flag, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "solo_weapon", kind: AxisKind::Flag, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "frame", kind: AxisKind::Id, group: Group::Wielder, requires: FREE },
    // THE FOUR OVERRIDES. Absent means the floor, which is why none of them
    // carries a default here: a default would BE an override.
    ScenarioAxis { id: "wf_health", kind: AxisKind::Number { min: 0.0, max: 100_000.0 }, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "wf_armor", kind: AxisKind::Number { min: 0.0, max: 100_000.0 }, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "wf_energy", kind: AxisKind::Number { min: 0.0, max: 100_000.0 }, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "wf_sprint", kind: AxisKind::Number { min: 0.0, max: 10.0 }, group: Group::Wielder, requires: FREE },
    ScenarioAxis { id: "wf_energy_pct", kind: AxisKind::Number { min: 0.0, max: 1.0 }, group: Group::Wielder, requires: FREE },

    // ---- what the Warframe brings ---------------------------------------
    ScenarioAxis { id: "buffs", kind: AxisKind::Structured, group: Group::Squad, requires: FREE },
    ScenarioAxis { id: "auras", kind: AxisKind::Structured, group: Group::Squad, requires: FREE },
    ScenarioAxis { id: "shards", kind: AxisKind::Structured, group: Group::Squad, requires: FREE },
    ScenarioAxis { id: "abilities", kind: AxisKind::Structured, group: Group::Squad, requires: FREE },
    ScenarioAxis { id: "ability_strength", kind: AxisKind::Number { min: 0.0, max: 10.0 }, group: Group::Squad, requires: FREE },
    ScenarioAxis { id: "extra_stats", kind: AxisKind::Structured, group: Group::Squad, requires: FREE },
];

/// **WHAT THIS WEAPON SETTLES THIS AXIS TO, AND WHY** — or `None` when the
/// choice is the reader's.
///
/// DERIVED, not written down: the first required capability the weapon lacks
/// decides. There is no per-axis code here and no list of ids to keep in step,
/// which is the whole difference from the version this replaced.
///
/// THE ONE PLACE THE RULE LIVES. `parse_fight` settles the value and the page
/// greys the control; both ask here, so the number that ran and the sentence on
/// screen cannot describe different rules.
pub fn settled_for(
    axis: &ScenarioAxis,
    weapon_id: &str,
) -> Option<(AxisValue, &'static str)> {
    settled_by(axis, weapon_id).map(|(v, why, _)| (v, why))
}

/// The axis with this id, for a caller holding a wire field.
pub fn axis(id: &str) -> Option<&'static ScenarioAxis> {
    SCENARIO_AXES.iter().find(|a| a.id == id)
}

/// **THE WEAPON CLASSES A SCENARIO RECORDS ITS RULES AGAINST**, in the order a panel lists them.
///
/// The SLOT, and nothing invented beside it: a house rule is about a family of
/// weapons that share a capability, and the slot is already the field that
/// decides every capability there is — a companion weapon aims where its
/// carrier looks because it is a companion weapon, an Arch-Gun is taken away
/// when empty because it is an Arch-Gun.
pub const WEAPON_CLASSES: &[&str] = &["primary", "secondary", "archgun", "sentinel", "melee"];

/// Which class's rules this weapon reads.
pub fn class_of(weapon_id: &str) -> Option<&'static str> {
    let spec = crate::weapons_data::spec(weapon_id)?;
    WEAPON_CLASSES.iter().copied().find(|c| *c == spec.slot)
}

/// What an axis is, for this weapon, under this scenario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resolution {
    /// The scenario's own value for this axis applies, as typed.
    Free,
    /// A capability the weapon lacks settles it. `overridable` says whether a
    /// class rule COULD have argued — a reader looking at a settled row wants
    /// to know whether it is the game's answer or one they may change.
    Settled { value: AxisValue, why: &'static str, overridable: bool },
    /// The scenario's own class rule settles it, because what it argued with
    /// was a house rule rather than a game fact.
    ByScenario { value: AxisValue, cap_why: &'static str },
    /// The scenario tried to argue with the GAME. Refused — the capability's
    /// value runs — and said out loud, because a rule stated in a file and not
    /// applied by the engine is worse than one that was never written: to
    /// anyone auditing, it reads as if it were being applied.
    Refused { value: AxisValue, why: &'static str, wanted: AxisValue },
}

impl Resolution {
    /// What actually RUNS: the settled value where something settled it, and
    /// otherwise the reader's own — which the caller passes in, since only the
    /// caller has read the request.
    ///
    /// Generic on purpose. There is no per-axis code in this module and this is
    /// the reason it can stay that way: a consumer holding a wire field and its
    /// typed value gets the fight's answer without naming which axis it is.
    pub fn value(self, readers: AxisValue) -> AxisValue {
        match self {
            Resolution::Free => readers,
            Resolution::Settled { value, .. }
            | Resolution::ByScenario { value, .. }
            | Resolution::Refused { value, .. } => value,
        }
    }

    /// Did anything but the reader decide this?
    pub fn is_settled(self) -> bool {
        !matches!(self, Resolution::Free)
    }
}

impl AxisValue {
    pub fn as_flag(self) -> Option<bool> {
        match self {
            AxisValue::Flag(b) => Some(b),
            AxisValue::Number(_) => None,
        }
    }

    pub fn as_number(self) -> Option<f64> {
        match self {
            AxisValue::Number(n) => Some(n),
            AxisValue::Flag(_) => None,
        }
    }
}

/// **THE WHOLE RULE, IN ONE FUNCTION.**
///
/// `class_rule` is what this scenario says about this axis for THIS weapon's
/// class — the caller looks it up, so this module stays free of the wire format
/// and the function stays trivially testable.
///
/// OVERRIDES SIT BEHIND LEGALITY, which is the entire guard: a scenario may say
/// "in my fight, Arch-Guns have infinite ammo" and may not say "in my fight,
/// Sentinels land headshots". The first is arguing with OUR stand-in for ammo
/// pickups; the second would produce a number nobody can reproduce in game,
/// which is the opposite of what this product promises.
pub fn resolve(
    axis: &ScenarioAxis,
    weapon_id: &str,
    class_rule: Option<AxisValue>,
) -> Resolution {
    let Some((value, why, cap)) = settled_by(axis, weapon_id) else {
        // Nothing settles it, so a class rule has nothing to argue with and the
        // scenario's own value is already the answer. A rule here is not
        // refused, it is simply the same field said twice.
        return Resolution::Free;
    };
    let overridable = cap.absence() == Absence::HouseRule;
    match (class_rule, overridable) {
        (Some(wanted), true) => Resolution::ByScenario { value: wanted, cap_why: why },
        (Some(wanted), false) => Resolution::Refused { value, why, wanted },
        (None, _) => Resolution::Settled { value, why, overridable },
    }
}

/// The capability that settles this axis for this weapon, if any.
fn settled_by(
    axis: &ScenarioAxis,
    weapon_id: &str,
) -> Option<(AxisValue, &'static str, Capability)> {
    let spec = crate::weapons_data::spec(weapon_id)?;
    axis.requires
        .iter()
        .find(|r| !r.cap.of(spec))
        .map(|r| (r.absent, r.cap.why_absent(spec), r.cap))
}

/// Every (class, axis) pair a scenario is ALLOWED to carry a rule for.
///
/// DERIVED from the two tables rather than listed: a class may argue with an
/// axis exactly when some weapon in it lacks a capability that axis requires
/// AND that capability's absence is ours rather than the game's. So a
/// capability reclassified tomorrow moves this list by itself, and a class with
/// nothing to argue about offers nothing.
pub fn overridable_pairs() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for class in WEAPON_CLASSES {
        for a in SCENARIO_AXES {
            let argues = a.requires.iter().any(|r| {
                r.cap.absence() == Absence::HouseRule
                    && crate::weapons_data::all()
                        .iter()
                        .any(|w| w.slot == *class && !r.cap.of(w))
            });
            if argues {
                out.push((*class, a.id));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// The spec a capability's sentence is about. Every `why_absent` here names
    /// the weapon it is explaining, because the sentence is that weapon's.
    fn sp(id: &str) -> &'static crate::weapons_data::WeaponSpec {
        crate::weapons_data::spec(id).expect("roster weapon")
    }

    use super::*;

    /// **A SCENARIO MAY ARGUE WITH OUR STAND-IN AND NOT WITH THE GAME.**
    ///
    /// The guard the whole class-rule feature rests on, held from both sides on
    /// the SAME weapon so neither arm can pass by the axis being absent.
    ///
    /// An Arch-Gun's finite reserve is `infinite_ammo` OFF because it cannot be
    /// resupplied, and that is OUR pessimistic reading of a mechanic the sim
    /// only half has — so a scenario saying "Arch-Guns have infinite ammo here"
    /// is honoured. Its `aiming` is the reader's already, so the case is taken
    /// from a Sentinel, whose headshot rate is 0 because the game says a
    /// companion cannot put a shot on a head: that rule is refused, the game's
    /// value runs, and the refusal is REPORTED rather than swallowed.
    #[test]
    fn a_scenario_may_argue_with_our_stand_in_and_not_with_the_game() {
        let ammo = axis("infinite_ammo").unwrap();
        assert_eq!(
            resolve(ammo, "larkspur", None),
            Resolution::Settled {
                value: AxisValue::Flag(false),
                why: Capability::CanResupply.why_absent(sp("larkspur")),
                overridable: true,
            },
            "an Arch-Gun runs dry by default, and a reader may say otherwise",
        );
        assert_eq!(
            resolve(ammo, "larkspur", Some(AxisValue::Flag(true))),
            Resolution::ByScenario {
                value: AxisValue::Flag(true),
                cap_why: Capability::CanResupply.why_absent(sp("larkspur")),
            },
            "…and saying so is honoured",
        );

        let head = axis("headshot_pct").unwrap();
        assert_eq!(
            resolve(head, "artax", Some(AxisValue::Number(100.0))),
            Resolution::Refused {
                value: AxisValue::Number(0.0),
                why: Capability::AimsAtHead.why_absent(sp("artax")),
                wanted: AxisValue::Number(100.0),
            },
            "a companion cannot be granted headshots by a file",
        );
    }

    /// **WHAT A SCENARIO IS ALLOWED TO SAY IS DERIVED**, and today it is one
    /// sentence: Arch-Guns and their ammo.
    ///
    /// Pinned as an exact set rather than as "contains", because the failure
    /// this guards against is the list GROWING — a capability reclassified to
    /// `HouseRule` without anyone deciding it, which is precisely the change
    /// that quietly lets a scenario publish an unreproducible number. Widening
    /// it is a deliberate edit here, with the reason in the diff.
    #[test]
    fn a_scenario_may_say_exactly_one_thing_today() {
        assert_eq!(overridable_pairs(), vec![("archgun", "infinite_ammo")]);
    }

    /// **A RULE ABOUT AN AXIS NOTHING SETTLES IS NOT REFUSED — IT IS NOTHING.**
    ///
    /// A class rule can only ever argue with a capability. On a Laetum the ammo
    /// box is already the reader's, so a rule for `primary` has no opponent and
    /// the scenario's own field is the answer it would have given anyway.
    /// Reporting that as `Refused` would put a warning on screen for a reader
    /// who has done nothing wrong.
    #[test]
    fn a_rule_with_nothing_to_argue_with_is_not_a_refusal() {
        let ammo = axis("infinite_ammo").unwrap();
        assert_eq!(resolve(ammo, "laetum", None), Resolution::Free);
        assert_eq!(resolve(ammo, "laetum", Some(AxisValue::Flag(true))), Resolution::Free);
    }

    /// **EVERY WEAPON HAS A CLASS**, or a rule written for its family would
    /// never reach it and the panel could not say which column it reads.
    #[test]
    fn every_weapon_belongs_to_a_class() {
        for w in crate::weapons_data::all() {
            assert!(
                class_of(&w.id).is_some(),
                "{} is in slot {:?}, which no class covers",
                w.id,
                w.slot,
            );
        }
    }

    /// **NO ID IS DECLARED TWICE**, which would make one unreachable through
    /// `axis()` and leave a surface reading the wrong rule.
    #[test]
    fn every_axis_is_named_once() {
        let mut seen = std::collections::BTreeSet::new();
        for a in SCENARIO_AXES {
            assert!(seen.insert(a.id), "{} declared twice", a.id);
        }
        assert!(SCENARIO_AXES.len() >= 25, "{} axes", SCENARIO_AXES.len());
    }

    /// **THE THREE WEAPON CLASSES, ONE ASSERTION EACH** — asserted against real
    /// weapons rather than against the capability that produced them, because a
    /// test that re-stated the predicate would pass on any predicate.
    #[test]
    fn each_weapon_class_settles_what_it_always_did() {
        let aim = axis("aiming").unwrap();
        let head = axis("headshot_pct").unwrap();
        let ammo = axis("infinite_ammo").unwrap();
        let v = |a: &ScenarioAxis, w: &str| settled_for(a, w).map(|x| x.0);

        // A COMPANION WEAPON. Fired by the Sentinel: always aiming, never at a
        // head, and nothing to run out of.
        assert_eq!(v(aim, "artax"), Some(AxisValue::Flag(true)));
        assert_eq!(v(head, "artax"), Some(AxisValue::Number(0.0)));
        assert_eq!(v(ammo, "artax"), Some(AxisValue::Flag(true)));

        // AN ARCH-GUN. It HAS a reserve and cannot refill it, so the same box
        // is settled to the OPPOSITE value from the opposite fact — the case
        // one flag could not tell apart.
        assert_eq!(v(ammo, "larkspur"), Some(AxisValue::Flag(false)));
        // …and its aim is entirely the reader's, which is what makes the line
        // above about ammo and not about being unusual.
        assert!(v(aim, "larkspur").is_none() && v(head, "larkspur").is_none());

        // A PRIMARY. Nothing settled at all, on any of the three.
        for a in [aim, head, ammo] {
            assert!(settled_for(a, "laetum").is_none(), "{} is the reader's on a Laetum", a.id);
        }
        // An axis that requires nothing is never settled, whatever the weapon.
        assert!(settled_for(axis("level").unwrap(), "artax").is_none());
    }

    /// THE ORDER OF `requires` IS THE RULE: the FIRST missing capability
    /// decides, and the ones behind it are not consulted.
    ///
    /// NO WEAPON EXERCISES IT TODAY, and that is stated rather than papered
    /// over: the ammo box declares two capabilities, and nothing in the roster
    /// lacks both — a companion weapon has no reserve and can be resupplied in
    /// principle, an Arch-Gun has one and cannot. So the order is load-bearing
    /// code with no live case, which is exactly the kind that rots.
    ///
    /// It is therefore tested on a SYNTHETIC axis built from two capabilities a
    /// real weapon really does lack, rather than on a synthetic weapon: the
    /// mechanism is exercised with real facts, and the day a weapon lacks both
    /// ammo capabilities the assertion below tells us by going red.
    #[test]
    fn the_first_missing_capability_decides() {
        let artax = crate::weapons_data::spec("artax").unwrap();
        assert!(!Capability::AimsAtHead.of(artax) && !Capability::HasReserve.of(artax),
            "the fixture must lack both, or this proves nothing");

        let first_wins = ScenarioAxis {
            id: "synthetic", kind: AxisKind::Flag, group: Group::Engagement,
            requires: &[
                Requirement { cap: Capability::AimsAtHead, absent: AxisValue::Flag(true) },
                Requirement { cap: Capability::HasReserve, absent: AxisValue::Flag(false) },
            ],
        };
        let (v, why) = settled_for(&first_wins, "artax").unwrap();
        assert_eq!(v, AxisValue::Flag(true), "the SECOND requirement answered");
        assert_eq!(why, Capability::AimsAtHead.why_absent(sp("artax")));

        // …and reversing the list reverses the answer, which is what makes the
        // line above about ORDER rather than about which capability is missing.
        let second_wins = ScenarioAxis {
            requires: &[
                Requirement { cap: Capability::HasReserve, absent: AxisValue::Flag(false) },
                Requirement { cap: Capability::AimsAtHead, absent: AxisValue::Flag(true) },
            ],
            ..first_wins
        };
        assert_eq!(settled_for(&second_wins, "artax").unwrap().0, AxisValue::Flag(false));

        // THE LIVE CASE THIS GUARDS. If a weapon ever lacks both ammo
        // capabilities, the ammo box's order starts deciding a real answer and
        // somebody should look at it on purpose.
        let both = crate::weapons_data::roster()
            .filter(|s| !Capability::HasReserve.of(s) && !Capability::CanResupply.of(s))
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>();
        assert!(both.is_empty(),
            "these lack BOTH ammo capabilities, so the ammo box's order now decides              a real answer — check it is the one you want: {both:?}");
    }

    /// EVERY CAPABILITY EXPLAINS ITSELF IN A SENTENCE, and every one of them is
    /// actually MISSING somewhere in the roster — a capability nothing lacks is
    /// a rule nothing tests, and would sit here reading as covered.
    #[test]
    fn every_capability_is_exercised_and_explains_itself() {
        let caps = [
            Capability::ChoosesAim,
            Capability::AimsAtHead,
            Capability::HasReserve,
            Capability::CanResupply,
        ];
        for c in caps {
            // EVERY WEAPON THAT LACKS IT GETS A SENTENCE, not just the first
            // one found. The prose is per (capability, kind of weapon) as of
            // 2026-08-28, so asking one weapon would let a whole class print
            // another class's reason — which is exactly the fault that split
            // it (a companion's explanation under a hammer).
            let missing: Vec<_> = crate::weapons_data::roster().filter(|s| !c.of(s)).collect();
            assert!(!missing.is_empty(), "{c:?} is missing from no weapon — nothing tests it");
            for spec in &missing {
                let why = c.why_absent(spec);
                assert!(why.len() > 30 && why.contains(' '), "{c:?} on {}: {why:?}", spec.id);
            }
        }
    }
}
