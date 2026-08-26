//! **WHAT A FIGHT CONSISTS OF, declared once for the whole product** (owner,
//! 2026-08-27) — [`BUILD_AXES`](crate::builds::BUILD_AXES)'s sibling, for the
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
//! to adjust (2026-08-04). Two capabilities cannot make that mistake; one flag
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The wielder decides whether to aim.
    ///
    /// A companion weapon is fired by the Sentinel and is ALWAYS aiming (user,
    /// 2026-08-01, settling M18a) — the mechanism is there, the choice is not.
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
    /// Does this weapon have it?
    pub fn of(self, spec: &crate::weapons_data::WeaponSpec) -> bool {
        // ONE PLACE READS THE SLOT, so "what a companion weapon is" is written
        // once rather than in each arm.
        let companion = spec.slot == "sentinel";
        match self {
            Capability::ChoosesAim => !companion,
            Capability::AimsAtHead => !companion,
            // The same two facts `/api/meta` serves to the roster grid, read
            // from the same place: a pool behind the magazine at all, and
            // whether the game gives any way to refill it.
            Capability::HasReserve => spec.ammo_max.is_some_and(|a| a > 0.0),
            Capability::CanResupply => !spec.no_resupply,
        }
    }

    /// What to tell a reader whose weapon does NOT have it.
    ///
    /// ONE SENTENCE PER CAPABILITY, not one per (axis, weapon) pair — which is
    /// the whole reason a new weapon class costs no new prose. It states a fact
    /// about the WEAPON rather than about the field, so it reads correctly
    /// wherever it is shown.
    ///
    /// English is the source, as everywhere; the overlay translates it. A full
    /// sentence because it is the only thing the reader gets in exchange for
    /// losing the control, and a disabled box with a one-word tooltip reads as
    /// broken.
    pub fn why_absent(self) -> &'static str {
        match self {
            Capability::ChoosesAim => {
                "a sentinel weapon is fired by the companion and is always aiming — the state is real, the choice is not"
            }
            Capability::AimsAtHead => {
                "a sentinel weapon never aims at the head, so this is 0 whatever is typed — and every on-headshot effect stays dead"
            }
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
    let spec = crate::weapons_data::spec(weapon_id)?;
    axis.requires
        .iter()
        .find(|r| !r.cap.of(spec))
        .map(|r| (r.absent, r.cap.why_absent()))
}

/// The axis with this id, for a caller holding a wire field.
pub fn axis(id: &str) -> Option<&'static ScenarioAxis> {
    SCENARIO_AXES.iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // one flag could not tell apart (2026-08-04).
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
        assert_eq!(why, Capability::AimsAtHead.why_absent());

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
            let why = c.why_absent();
            assert!(why.len() > 30 && why.contains(' '), "{c:?}: {why:?}");
            let missing = crate::weapons_data::roster().filter(|s| !c.of(s)).count();
            assert!(missing > 0, "{c:?} is missing from no weapon — nothing tests it");
        }
    }
}
