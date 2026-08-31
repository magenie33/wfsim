//! WHICH TRIGGER A FIGHT SWITCHES OFF. The subject is `docs/BUFFS.md`
//! §"…AND A FIGHT CAN SWITCH OFF A TRIGGER" — the events still happen and
//! still score, the buff does not.
//!
//! THE VOCABULARY IS THE DATA'S OWN. A buff already declares what fires it, so
//! a switch per trigger needs no classification and cannot conflate two things
//! the data tells apart — a weak-point KILL is not a kill, and the coarse
//! vocabulary this replaced collapsed exactly those.

use crate::arcanes_data::ArcTrigger;
use crate::loadout::BuffTrigger;

/// The wire id of a data-declared trigger — what a scenario, a share link and a
/// benchmark carry.
///
/// EXHAUSTIVE ON PURPOSE — no `_` arm, so a trigger added to [`BuffTrigger`]
/// cannot compile until it is named here, where a default would leave the next
/// card with no switch and nothing to notice it by.
pub fn trigger_id(t: BuffTrigger) -> &'static str {
    match t {
        BuffTrigger::Kill => "kill",
        BuffTrigger::Hit => "hit",
        BuffTrigger::PlainHit => "plain_hit",
        BuffTrigger::Headshot => "headshot",
        BuffTrigger::ConsecutiveHeadshot => "consecutive_headshot",
        BuffTrigger::PunchThrough => "punch_through",
        BuffTrigger::StatusApplied => "status_applied",
        // The element is not part of the id: the condition is "the target
        // already carries this status", and a fight handing out none of them
        // hands out none of any type.
        BuffTrigger::HitEnemyWithStatus(_) => "hit_enemy_with_status",
        BuffTrigger::ReloadComplete => "reload_complete",
        BuffTrigger::ReloadFromEmpty => "reload_from_empty",
        BuffTrigger::FullBurst => "full_burst",
        BuffTrigger::Firing => "firing",
    }
}

/// The same for an arcane's own vocabulary. `None` for [`ArcTrigger::Passive`]:
/// nothing grants it, so no switch may take it away.
pub fn arc_trigger_id(t: ArcTrigger) -> Option<&'static str> {
    Some(match t {
        ArcTrigger::Kill => "kill",
        ArcTrigger::HeadshotKill => "headshot_kill",
        ArcTrigger::MeleeKill => "melee_kill",
        ArcTrigger::WeakpointHit => "weakpoint_hit",
        ArcTrigger::HeatStatus => "heat_status",
        ArcTrigger::ElectricityStatus => "electricity_status",
        ArcTrigger::ToxinStatus => "toxin_status",
        ArcTrigger::ColdStatus => "cold_status",
        ArcTrigger::Passive => return None,
    })
}

/// EVERY TRIGGER, IN THE ORDER THE PAGE DRAWS THEM, with the group each sits
/// under. The order is the wire's and the panel's, so it lives in one place.
///
/// A GROUP IS PRESENTATION AND THE SWITCHES ARE THE TRUTH: a ruler says `kill`,
/// or names `headshot_kill` on its own, and the group header is a way to tick
/// several at once rather than a value anything stores.
pub const ALL: &[(&str, &str)] = &[
    ("kill", "kill"),
    ("headshot_kill", "kill"),
    ("melee_kill", "kill"),
    ("hit", "hit"),
    ("plain_hit", "hit"),
    ("headshot", "hit"),
    ("weakpoint_hit", "hit"),
    ("consecutive_headshot", "hit"),
    ("punch_through", "hit"),
    ("status_applied", "status"),
    ("hit_enemy_with_status", "status"),
    ("heat_status", "status"),
    ("electricity_status", "status"),
    ("toxin_status", "status"),
    ("cold_status", "status"),
    ("reload_complete", "reload"),
    ("reload_from_empty", "reload"),
    ("firing", "firing"),
    ("full_burst", "firing"),
];

/// The buffs whose trigger is baked into their IDENTITY: a named field on
/// `DummyParams`, or a card id `evolutions_data::stacking_card_id` derives FROM
/// a trigger the card itself does not carry.
///
/// CONDITION OVERLOAD IS NOT HERE, because its id is not one mechanic: it is
/// unconditional on a melee mod and earned on a KILL on a Galvanized one, so a
/// single hand-written answer is wrong for one of them whichever it names.
/// `StackSpec::earned_on` carries what that mod declared, and the page reads
/// the same field.
///
/// `Some(None)` is a buff nothing can deny — permanent stacks, no in-sim
/// trigger. `None` is an id this table has never heard of, which is a failing
/// test (`every_buff_card_says_what_triggers_it`) and never a silent pass.
pub fn of_builtin(id: &str) -> Option<Option<&'static str>> {
    Some(Some(match id {
        // Dual Toxocyst's passive: 3 s off a weak-point hit.
        "frenzy" => "headshot",
        // An evolution's CO-shaped multishot counts the statuses ON THE TARGET.
        "on_status_multishot" => "hit_enemy_with_status",
        "on_kill_multishot" | "on_kill_cd" | "on_kill_damage" | "tendrils" => "kill",
        "on_headshot_kill_cc" => "headshot_kill",
        // An Eximus weak point is a weak point.
        "on_headshot_cc" | "on_eximus_weakpoint_bd" | "on_headshot_fire_rate"
        | "on_headshot_damage" | "on_headshot_reload_speed" => "headshot",
        "evo_headshot_streak" | "on_weakpoint_streak_damage"
        | "on_weakpoint_streak_headshot_damage" => "consecutive_headshot",
        // A landing hit, whatever it lands on: the shot combo counter,
        // Hata-Satya's pile, Secondary Enervate's.
        "sniper_combo" | "crit_per_hit" | "arcane:secondary_enervate" | "on_hit_damage" => "hit",
        "on_plain_hit_damage" => "plain_hit",
        "on_punch_through_crit_chance" => "punch_through",
        "on_reload_fr" | "on_reload_bd" | "on_reload_damage" | "on_reload_fire_rate"
        | "per_shell_fire_rate" => "reload_complete",
        "evo_reload_damage" | "on_empty_reload_damage"
        | "on_empty_reload_crit_damage" => "reload_from_empty",
        "on_firing_fire_rate" | "on_firing_damage" | "on_firing_multishot" => "firing",
        "on_status_fire_rate" | "on_status_damage" => "status_applied",
        // Fevered Frenzy: permanent stacks, no in-sim trigger — the answer
        // `ArcTrigger::Passive` gets, for the same reason.
        "evo_multishot" => return Some(None),
        _ => return None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVERY TRIGGER THE ENUMS SPELL HAS A SWITCH, AND EVERY SWITCH IS SPELLED.
    /// A trigger with no entry in [`ALL`] is a buff no scenario can reach; an
    /// entry nothing produces is a control that moves no number.
    #[test]
    fn the_switch_list_and_the_triggers_agree() {
        let mut spelled: Vec<&str> = Vec::new();
        for t in [
            BuffTrigger::Kill, BuffTrigger::Hit, BuffTrigger::PlainHit,
            BuffTrigger::Headshot, BuffTrigger::ConsecutiveHeadshot,
            BuffTrigger::PunchThrough, BuffTrigger::StatusApplied,
            BuffTrigger::HitEnemyWithStatus(crate::damage::DamageType::Heat),
            BuffTrigger::ReloadComplete, BuffTrigger::ReloadFromEmpty,
            BuffTrigger::FullBurst, BuffTrigger::Firing,
        ] {
            spelled.push(trigger_id(t));
        }
        for t in [
            ArcTrigger::Kill, ArcTrigger::HeadshotKill, ArcTrigger::MeleeKill,
            ArcTrigger::WeakpointHit, ArcTrigger::HeatStatus,
            ArcTrigger::ElectricityStatus, ArcTrigger::ToxinStatus, ArcTrigger::ColdStatus,
        ] {
            spelled.push(arc_trigger_id(t).expect("only Passive has none"));
        }
        assert_eq!(arc_trigger_id(ArcTrigger::Passive), None);
        spelled.sort_unstable();
        spelled.dedup();
        let mut listed: Vec<&str> = ALL.iter().map(|(id, _)| *id).collect();
        listed.sort_unstable();
        assert_eq!(spelled, listed, "the switch list and the trigger enums disagree");
    }

    /// A GROUP IS A GROUP OF SOMETHING, and the groups are CONTIGUOUS — the
    /// page draws [`ALL`] in order and a group appearing twice would be two
    /// headers over one set.
    #[test]
    fn every_group_is_contiguous_and_holds_a_switch() {
        let mut groups: Vec<&str> = ALL.iter().map(|(_, g)| *g).collect();
        groups.dedup();
        assert_eq!(groups, ["kill", "hit", "status", "reload", "firing"]);
    }

    /// A BUILTIN NAMES A TRIGGER THE LIST KNOWS, so the card the page greys and
    /// the buff the run drops are the same claim.
    #[test]
    fn every_builtin_names_a_listed_trigger() {
        for id in ["frenzy", "on_status_multishot", "on_kill_multishot", "tendrils",
                   "on_headshot_kill_cc", "sniper_combo", "evo_reload_damage"] {
            let t = of_builtin(id).unwrap_or_else(|| panic!("{id} is not in the table"));
            let t = t.unwrap_or_else(|| panic!("{id} claims no trigger"));
            assert!(ALL.iter().any(|(x, _)| *x == t), "{id} names `{t}`, not a switch");
        }
        assert_eq!(of_builtin("evo_multishot"), Some(None));
        assert_eq!(of_builtin("nothing_of_the_sort"), None);
    }
}
