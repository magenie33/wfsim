//! WHAT A BUFF ASKS OF THE FIGHT before it can be earned, and the fight's
//! answer. The subject is `docs/BUFFS.md` §"…AND A FIGHT CAN REFUSE TO HAND
//! OUT WHAT EARNS IT"; this file is the vocabulary and the two tables.

use crate::arcanes_data::ArcTrigger;
use crate::loadout::BuffTrigger;

/// One thing the fight has to hand you before a buff can be earned.
///
/// THE VOCABULARY IS THE PLAYER'S, not the trigger enum's: `ReloadComplete` and
/// `ReloadFromEmpty` are two triggers and one thing a player does. A switch per
/// trigger would ask a reader which of two spellings their card uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuffEvent {
    /// Something died — the event a fight you walk into cold denies.
    Kill,
    /// A weak point was hit. Separate from the kill, because hitting heads and
    /// having been in contact are two different claims.
    Headshot,
    Hit,
    PunchThrough,
    /// A status landed, or the target already carried one.
    Status,
    Reload,
    /// A round left the barrel, whatever it did afterwards.
    Firing,
}

impl BuffEvent {
    /// The wire spelling — a scenario, a share link and a benchmark all carry
    /// it, so a rename loses a term of a fight rather than failing to compile.
    pub fn id(self) -> &'static str {
        match self {
            BuffEvent::Kill => "kill",
            BuffEvent::Headshot => "headshot",
            BuffEvent::Hit => "hit",
            BuffEvent::PunchThrough => "punch_through",
            BuffEvent::Status => "status",
            BuffEvent::Reload => "reload",
            BuffEvent::Firing => "firing",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "kill" => BuffEvent::Kill,
            "headshot" => BuffEvent::Headshot,
            "hit" => BuffEvent::Hit,
            "punch_through" => BuffEvent::PunchThrough,
            "status" => BuffEvent::Status,
            "reload" => BuffEvent::Reload,
            "firing" => BuffEvent::Firing,
            _ => return None,
        })
    }

    /// Every event, for the api to publish and the page to draw a switch per.
    pub const ALL: [BuffEvent; 7] = [
        BuffEvent::Kill,
        BuffEvent::Headshot,
        BuffEvent::Hit,
        BuffEvent::PunchThrough,
        BuffEvent::Status,
        BuffEvent::Reload,
        BuffEvent::Firing,
    ];
}

/// What a data-declared stacking buff asks for.
///
/// EXHAUSTIVE ON PURPOSE — no `_` arm, so a trigger added to [`BuffTrigger`]
/// cannot compile until somebody says what it asks for. A default would exempt
/// the next card from every switch on the page, silently.
pub fn of_trigger(t: BuffTrigger) -> &'static [BuffEvent] {
    use BuffEvent as E;
    match t {
        BuffTrigger::PlainHit | BuffTrigger::Hit => &[E::Hit],
        BuffTrigger::Headshot | BuffTrigger::ConsecutiveHeadshot => &[E::Headshot, E::Hit],
        BuffTrigger::PunchThrough => &[E::PunchThrough, E::Hit],
        // The condition is on the TARGET, which must already carry the status.
        BuffTrigger::HitEnemyWithStatus(_) => &[E::Status, E::Hit],
        BuffTrigger::StatusApplied => &[E::Status],
        BuffTrigger::ReloadComplete | BuffTrigger::ReloadFromEmpty => &[E::Reload],
        // A completed burst is rounds leaving the barrel AND landing.
        BuffTrigger::FullBurst => &[E::Firing, E::Hit],
        BuffTrigger::Kill => &[E::Kill],
        BuffTrigger::Firing => &[E::Firing],
    }
}

/// The same question for an arcane, whose triggers are their own vocabulary.
pub fn of_arc_trigger(t: ArcTrigger) -> &'static [BuffEvent] {
    use BuffEvent as E;
    match t {
        ArcTrigger::Kill | ArcTrigger::MeleeKill => &[E::Kill],
        // BOTH, and either switch denies it: a precision kill is a kill you
        // also had to aim.
        ArcTrigger::HeadshotKill => &[E::Kill, E::Headshot, E::Hit],
        ArcTrigger::WeakpointHit => &[E::Headshot, E::Hit],
        ArcTrigger::HeatStatus
        | ArcTrigger::ElectricityStatus
        | ArcTrigger::ToxinStatus
        | ArcTrigger::ColdStatus => &[E::Status],
        // NOTHING GRANTS IT — a Warframe stat does not become unavailable
        // because the fight is short, so no switch may touch it.
        ArcTrigger::Passive => &[],
    }
}

/// The buffs whose trigger is baked into their IDENTITY: a named field on
/// `DummyParams`, or a card id `evolutions_data::stacking_card_id` derives FROM
/// a trigger the card itself does not carry.
///
/// A TABLE BECAUSE THERE IS NOTHING TO DERIVE FROM — `on_kill_cd` is a kill
/// because of what it is. `None` is a failing test
/// (`every_buff_card_says_what_it_asks_of_the_fight` sweeps the whole roster),
/// never a buff nothing can deny.
pub fn of_builtin(id: &str) -> Option<&'static [BuffEvent]> {
    use BuffEvent as E;
    Some(match id {
        // Dual Toxocyst's passive: 3 s off a weak-point hit.
        "frenzy" => &[E::Headshot, E::Hit],
        // Condition Overload counts the statuses ON THE TARGET.
        "condition_overload" => &[E::Status],
        "on_kill_multishot" | "on_kill_cd" | "on_kill_damage" => &[E::Kill],
        "on_headshot_kill_cc" => &[E::Kill, E::Headshot, E::Hit],
        // A headshot streak and an Eximus weak point are weak points.
        "on_headshot_cc" | "evo_headshot_streak" | "on_eximus_weakpoint_bd" => {
            &[E::Headshot, E::Hit]
        }
        "on_reload_fr" | "on_reload_bd" | "evo_reload_damage" => &[E::Reload],
        // A landing hit, whatever it lands on: the shot combo counter,
        // Hata-Satya's pile, Secondary Enervate's.
        "sniper_combo" | "crit_per_hit" | "arcane:secondary_enervate" => &[E::Hit],
        // The Ocucor's tendrils cost a kill.
        "tendrils" => &[E::Kill],
        // Fevered Frenzy: permanent stacks, no in-sim trigger — the answer
        // `ArcTrigger::Passive` gets, for the same reason.
        "evo_multishot" => &[],

        // …AND THE STACKING CARDS. Written out rather than inverted from
        // `stacking_card_id` because that mapping is not injective — it has a
        // catch-all — and a table that guessed would be worse than one the
        // roster sweep checks.
        "on_firing_fire_rate" | "on_firing_damage" | "on_firing_multishot" => &[E::Firing],
        "on_status_fire_rate" | "on_status_damage" => &[E::Status],
        // Stormburst's: a hit on a target ALREADY carrying the status.
        "on_status_multishot" => &[E::Status, E::Hit],
        "on_headshot_fire_rate" | "on_headshot_damage" | "on_headshot_reload_speed"
        | "on_weakpoint_streak_damage" | "on_weakpoint_streak_headshot_damage" => {
            &[E::Headshot, E::Hit]
        }
        "on_hit_damage" | "on_plain_hit_damage" => &[E::Hit],
        "on_punch_through_crit_chance" => &[E::PunchThrough, E::Hit],
        "on_reload_damage" | "on_reload_fire_rate" | "per_shell_fire_rate"
        | "on_empty_reload_damage" | "on_empty_reload_crit_damage" => &[E::Reload],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE WIRE SPELLING ROUND-TRIPS — see [`BuffEvent::id`].
    #[test]
    fn every_event_round_trips_through_its_id() {
        for e in BuffEvent::ALL {
            assert_eq!(BuffEvent::from_id(e.id()), Some(e), "{}", e.id());
        }
        assert_eq!(BuffEvent::from_id("nonsense"), None);
    }

    /// A HEADSHOT IS A HIT, AND THE PAGE MUST NOT HAVE TO KNOW THAT: denying
    /// hits denies headshot buffs too, which only holds if every trigger
    /// wanting a weak point also names the plain hit.
    #[test]
    fn a_trigger_that_wants_a_weak_point_also_wants_the_hit() {
        for t in [
            BuffTrigger::Headshot,
            BuffTrigger::ConsecutiveHeadshot,
            BuffTrigger::PunchThrough,
        ] {
            let e = of_trigger(t);
            assert!(e.contains(&BuffEvent::Hit), "{t:?} names no hit: {e:?}");
        }
        assert!(of_arc_trigger(ArcTrigger::HeadshotKill).contains(&BuffEvent::Hit));
        assert!(of_arc_trigger(ArcTrigger::WeakpointHit).contains(&BuffEvent::Hit));
    }

    /// A PASSIVE IS DENIED BY NOTHING — it reads a Warframe stat a short fight
    /// does not take away.
    #[test]
    fn nothing_denies_a_passive() {
        assert!(of_arc_trigger(ArcTrigger::Passive).is_empty());
        assert_eq!(of_builtin("evo_multishot"), Some(&[][..]));
    }
}
