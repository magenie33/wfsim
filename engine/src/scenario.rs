//! **WHAT A FIGHT CONSISTS OF, declared once for the whole product** (owner,
//! 2026-08-27) — [`BUILD_AXES`](crate::builds::BUILD_AXES)'s sibling, for the
//! other half of a simulation.
//!
//! A BUILD is what you are carrying; a FIGHT is everything else, and until this
//! list existed the second half had no declaration at all. The consequences
//! were three, and they are why this file exists rather than being tidy:
//!
//! 1. **THE PAGE RE-DERIVED THE RULES.** Three scenario fields are FORCED for
//!    some weapons — a sentinel is always aiming, its headshot rate is 0
//!    whatever is typed, and a weapon with no reserve cannot run out of ammo —
//!    and `parse_fight` forced them while `app.js` independently decided to
//!    grey the same boxes. Two implementations of one rule, which can drift in
//!    silence, because a forced field looks identical whoever forced it. It is
//!    the pattern `evo_forbids` and `auras: [id]` already refuse: THE ENGINE
//!    DECIDES AND `/api/meta` STATES THE CONSEQUENCE PER WEAPON.
//!
//! 2. **`defaultScenario()` IS A HAND LIST.** A field the server gains and the
//!    page forgets does not RESET when a preset is switched — which is exactly
//!    the Eximus bug of 2026-08-07, where switching back to the official ruler
//!    left it fighting an Eximus because that yaml never says `eximus:`.
//!
//! 3. **A READER COULD NOT SEE THE WHOLE FIGHT.** What is hidden or forced for
//!    the current weapon was invisible, so "the same fight" across two weapons
//!    was a claim nobody could check.
//!
//! **THE DOCUMENT KEEPS EVERYTHING; THE UI SHOWS WHAT APPLIES.** That is the
//! rule the buff map already proved (AGENTS.md): the whole map travels because
//! it is the FIGHT's, and pruning it to what the current build can grant made
//! the quick calc a different fight the moment a candidate granted a buff the
//! current build lacked. This generalises that to every axis — a field
//! irrelevant to the weapon in front of you is still part of the fight, and
//! dropping it is how two "identical" fights stop being identical.
//!
//! WHAT THIS FILE IS NOT: it does not parse, apply or validate anything.
//! `parse_fight` still reads the request and is still the authority on what a
//! field MEANS. This says which fields exist, what they default to, and when
//! the weapon takes the choice away — the three facts every surface was
//! guessing at separately.

/// How a scenario axis is spelled on the wire and what shape it holds.
///
/// The KIND is here because the escape hatch renders from this list rather than
/// from a hand-written form: a reader asking to see the whole fight gets every
/// axis, including the ones this weapon hides, and a control cannot be drawn
/// without knowing whether it is a tick or a number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AxisKind {
    /// A tick.
    Flag,
    /// A number, with the bounds a control should offer.
    Number { min: f64, max: f64 },
    /// An id from a roster — an enemy, a Warframe.
    Id,
    /// Anything the escape hatch shows as json rather than as a control: a
    /// formation, a buff map, a list of auras. Editable only in its own panel,
    /// which is where it belongs — this list exists to say the field EXISTS,
    /// not to replace the arena or the buff cards.
    Structured,
}

// NO `Eq`, because `Number` holds bounds and an f64 has none. `PartialEq` is
// what a comparison here ever wants.

/// WHEN A WEAPON TAKES THE CHOICE AWAY, and what to tell the reader.
///
/// THREE STATES, NOT TWO (owner, 2026-08-04, on the ammo box). It had one flag
/// and read it as the wrong one of two facts, which ticked-and-disabled the box
/// on every weapon but the single Arch-Gun — so the only weapon whose ammo you
/// could adjust was the one weapon the game gives no way to adjust. A rule that
/// can only say "forced" cannot tell "there is nothing to run out of" from
/// "there is, and you cannot refill it".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    /// The reader's, on every weapon.
    Always,
    /// Forced for weapons this predicate names — the value and the reason are
    /// computed per weapon by [`forced_for`].
    WhenWeapon(Rule),
}

/// The weapon properties a forcing rule may ask about.
///
/// A CLOSED SET, deliberately. Each arm is a question the WEAPON DATA already
/// answers, so a rule cannot come to depend on something a caller passes in —
/// and a new arm is a deliberate act rather than a predicate somebody inlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A companion weapon: fired by the Sentinel, not by the player.
    Sentinel,
    /// No ammo reserve at all — nothing to run out of.
    NoReserve,
    /// A reserve that cannot be refilled mid-fight (the Arch-Guns).
    NoResupply,
}

/// One field of a fight.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioAxis {
    /// The stable id, and the wire field. Unlike a build axis these are the
    /// same string: a fight has one representation on the wire, and the page's
    /// `sim` object uses the wire spelling directly.
    pub id: &'static str,
    pub kind: AxisKind,
    /// Which weapons take this choice away, if any.
    pub applies: Applies,
    /// Does the OPTIMIZER read it? A scope field (`finalists`, `final_runs`) is
    /// part of a SEARCH rather than of a fight, and the two are stored in
    /// different presets — this list carries only the fight's own.
    pub group: Group,
}

/// Which block of the panel an axis belongs to. The escape hatch groups by it,
/// so a reader asked to look at "the whole fight" is not handed 30 flat rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Who is being shot, and where they stand.
    Target,
    /// How the engagement is run — its length, its metric, its ammo rule.
    Engagement,
    /// Who is holding the gun and what they are doing.
    Wielder,
    /// What the Warframe brings that is not the build's: auras, shards,
    /// ability buffs.
    Squad,
}

/// **EVERY FIELD A FIGHT HAS.**
///
/// Read against `parse_fight` and `tenno_from`, which are the two functions
/// that actually consume a request — this list is derived from what they read
/// rather than from what the page happens to send, so a field the page has
/// never sent is still declared and a field nothing reads is not.
pub const SCENARIO_AXES: &[ScenarioAxis] = &[
    // ---- the target ------------------------------------------------------
    ScenarioAxis { id: "enemy", kind: AxisKind::Id, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "level", kind: AxisKind::Number { min: 1.0, max: 9999.0 }, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "steel_path", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Target },
    // NULL is a real third state — "whatever this unit is by default" — which
    // is why it is a Flag the page stores as `null | true | false` rather than
    // a plain bool. Only an explicit choice is stored, so switching targets
    // keeps giving the elite unit wherever one exists.
    ScenarioAxis { id: "eximus", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "custom_enemies", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "formation", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "player_at", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "target_at", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Target },
    ScenarioAxis { id: "aim_at", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Target },

    // ---- the engagement --------------------------------------------------
    ScenarioAxis { id: "duration", kind: AxisKind::Number { min: 1.0, max: 3600.0 }, applies: Applies::Always, group: Group::Engagement },
    ScenarioAxis { id: "metric", kind: AxisKind::Id, applies: Applies::Always, group: Group::Engagement },
    // FORCED EITHER WAY AND FOR OPPOSITE REASONS — see `Applies`. Declared
    // against the stronger rule first: a weapon with no reserve is forced ON,
    // and `forced_for` checks that arm before the resupply one.
    ScenarioAxis { id: "infinite_ammo", kind: AxisKind::Flag, applies: Applies::WhenWeapon(Rule::NoReserve), group: Group::Engagement },

    // ---- the wielder -----------------------------------------------------
    ScenarioAxis { id: "aiming", kind: AxisKind::Flag, applies: Applies::WhenWeapon(Rule::Sentinel), group: Group::Wielder },
    ScenarioAxis { id: "headshot_pct", kind: AxisKind::Number { min: 0.0, max: 100.0 }, applies: Applies::WhenWeapon(Rule::Sentinel), group: Group::Wielder },
    ScenarioAxis { id: "invisible", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "airborne", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "overshields", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "channeling", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "solo_weapon", kind: AxisKind::Flag, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "frame", kind: AxisKind::Id, applies: Applies::Always, group: Group::Wielder },
    // THE FOUR OVERRIDES. Absent means the floor, which is why they are
    // declared with no default here: a default would BE an override.
    ScenarioAxis { id: "wf_health", kind: AxisKind::Number { min: 0.0, max: 100_000.0 }, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "wf_armor", kind: AxisKind::Number { min: 0.0, max: 100_000.0 }, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "wf_energy", kind: AxisKind::Number { min: 0.0, max: 100_000.0 }, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "wf_sprint", kind: AxisKind::Number { min: 0.0, max: 10.0 }, applies: Applies::Always, group: Group::Wielder },
    ScenarioAxis { id: "wf_energy_pct", kind: AxisKind::Number { min: 0.0, max: 1.0 }, applies: Applies::Always, group: Group::Wielder },

    // ---- what the Warframe brings ---------------------------------------
    ScenarioAxis { id: "buffs", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Squad },
    ScenarioAxis { id: "auras", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Squad },
    ScenarioAxis { id: "shards", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Squad },
    ScenarioAxis { id: "abilities", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Squad },
    ScenarioAxis { id: "ability_strength", kind: AxisKind::Number { min: 0.0, max: 10.0 }, applies: Applies::Always, group: Group::Squad },
    ScenarioAxis { id: "extra_stats", kind: AxisKind::Structured, applies: Applies::Always, group: Group::Squad },
];

/// What this weapon forces this axis to, and why — or `None` when the choice is
/// the reader's.
///
/// **THE ONE PLACE THE RULE LIVES.** `parse_fight` forces the value and the
/// page greys the control; both ask here, so the number that ran and the
/// sentence on screen cannot describe different rules.
///
/// The REASON is English and is the source, as everywhere else: the overlay
/// translates it. It is a full sentence because it is the whole point — a
/// disabled control that does not say why is a control that looks broken.
pub fn forced_for(axis: &ScenarioAxis, weapon_id: &str) -> Option<(f64, &'static str)> {
    let Applies::WhenWeapon(rule) = axis.applies else { return None };
    let spec = crate::weapons_data::spec(weapon_id)?;
    let sentinel = spec.slot == "sentinel";
    // THE SAME TWO FACTS `/api/meta` serves, read from the same place. A pool
    // behind the magazine at all, and whether the game gives any way to refill
    // it — one read as the other is the 2026-08-04 bug.
    let has_reserve = spec.ammo_max.is_some_and(|a| a > 0.0);
    match (axis.id, rule) {
        // A SENTINEL WEAPON IS ALWAYS AIMING (user, 2026-08-01, settling M18a).
        ("aiming", Rule::Sentinel) if sentinel => Some((
            1.0,
            "a sentinel weapon is always aiming — it just never aims at the head, so on-headshot effects never fire",
        )),
        // …AND ITS HEADSHOT RATE IS 0 AND NOT YOURS (owner, 2026-08-19). The
        // value was already forced from both ends; what was missing was the
        // control saying so, since a column that is shown and not applied looks
        // exactly like one that works.
        ("headshot_pct", Rule::Sentinel) if sentinel => Some((
            0.0,
            "a sentinel weapon is fired by the companion and never aims at the head, so this is 0 whatever is typed — and every on-headshot effect stays dead",
        )),
        // THE AMMO BOX, both of its forced states. Order matters: a weapon with
        // no reserve at all is forced ON, and only a weapon that HAS one it
        // cannot refill is forced OFF.
        ("infinite_ammo", _) if !has_reserve => {
            Some((1.0, "this weapon has no ammo reserve to run out of"))
        }
        ("infinite_ammo", _) if spec.no_resupply => Some((
            0.0,
            "this weapon cannot be resupplied — once its reserve is gone it is removed for five minutes, so the setting has nothing to stand in for",
        )),
        _ => None,
    }
}

/// The axis with this id, for a caller holding a wire field.
pub fn axis(id: &str) -> Option<&'static ScenarioAxis> {
    SCENARIO_AXES.iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **NO ID IS DECLARED TWICE**, which would make one of them unreachable
    /// through `axis()` and leave a surface reading the wrong rule.
    #[test]
    fn every_axis_is_named_once() {
        let mut seen = std::collections::BTreeSet::new();
        for a in SCENARIO_AXES {
            assert!(seen.insert(a.id), "{} declared twice", a.id);
        }
        assert!(SCENARIO_AXES.len() >= 25, "{} axes", SCENARIO_AXES.len());
    }

    /// **THE THREE FORCED FIELDS FORCE WHAT THEY ALWAYS DID**, asserted against
    /// real weapons rather than against the rule that produced them — a test
    /// that re-stated the predicate would pass on any predicate.
    ///
    /// The ammo box's THREE states are the sharp case, and the one that was a
    /// live bug: one flag read as the wrong one of two facts left the only
    /// adjustable weapon being the one weapon the game gives no way to adjust
    /// (2026-08-04).
    #[test]
    fn the_forced_fields_force_what_they_always_did() {
        let aim = axis("aiming").unwrap();
        let head = axis("headshot_pct").unwrap();
        let ammo = axis("infinite_ammo").unwrap();

        // A SENTINEL WEAPON: aiming on, headshots off, ammo forced ON because
        // it has no reserve at all.
        assert_eq!(forced_for(aim, "artax").map(|x| x.0), Some(1.0));
        assert_eq!(forced_for(head, "artax").map(|x| x.0), Some(0.0));
        assert_eq!(forced_for(ammo, "artax").map(|x| x.0), Some(1.0));

        // AN ARCH-GUN: it HAS a reserve and cannot refill it, so the box is
        // forced OFF — the opposite value, from the opposite fact.
        assert_eq!(forced_for(ammo, "larkspur").map(|x| x.0), Some(0.0),
            "an Arch-Gun's reserve is real and unrefillable");

        // AN ORDINARY RIFLE: nothing forced at all, on any of the three.
        for a in [aim, head, ammo] {
            assert!(forced_for(a, "laetum").is_none(), "{} is the reader's on a Laetum", a.id);
        }
        // …and an axis with no rule is never forced, whatever the weapon.
        assert!(forced_for(axis("level").unwrap(), "artax").is_none());
    }

    /// EVERY REASON IS A SENTENCE. A disabled control whose tooltip is a word
    /// reads as broken; this is the one thing the reader gets in exchange for
    /// losing the control.
    #[test]
    fn a_forced_field_explains_itself() {
        for a in SCENARIO_AXES {
            for w in ["artax", "larkspur", "laetum"] {
                if let Some((_, why)) = forced_for(a, w) {
                    assert!(why.len() > 30, "{}/{}: {why:?}", a.id, w);
                    assert!(why.contains(' '), "{}/{}: {why:?}", a.id, w);
                }
            }
        }
    }
}
