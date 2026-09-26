// SPDX-License-Identifier: AGPL-3.0-or-later
//! **WHAT THE FRAME AND ITS OPERATOR DO, ON ONE TIMELINE** — docs/BUFFS.md
//! §"Cast, or assumed up" and §"Snapshots".
//!
//! Every planned rule of the action list (`Apl::planned`) is laid out here
//! before the fight: casts, the summoning of an Exalted weapon, and the
//! Operator's actions. ONE PLAYER DOES ONE THING AT A TIME, so an action due
//! while another is under way starts when that one ends.
//!
//! **A WARFRAME BUFF IS A SNAPSHOT.** A cast reads Ability Strength at the
//! instant it is cast and keeps that number for its whole window; a buff that
//! lapses later does not reach back into it. So each cast is its own entry,
//! resolved at its own strength, and an Exalted weapon's damage is the strength
//! its summoning cast read.

use crate::data::abilities::{self, AbilityPick, ActiveAbility, Caster, CAST_SECONDS_UNMEASURED};
use crate::data::apl::{Action, Apl};
use crate::data::warframes::{focus_school, FrameEffect, FrameStat, NodeTrigger};

/// **HOW LONG `operator_sling` TAKES, UNTIL IT IS MEASURED**: Transference out,
/// a Chained Sling, Transference back. One number for the whole sequence, and
/// none of it is modded — casting speed does not touch Transference.
pub const OPERATOR_SLING_SECONDS_UNMEASURED: f64 = 3.0;

/// A stretch of time Ability Strength is raised, and what raised it.
#[derive(Debug, Clone, PartialEq)]
pub struct StrengthWindow {
    /// The Focus node's id.
    pub from: String,
    pub starts_at_seconds: f64,
    pub ends_at_seconds: f64,
    /// A fraction, added to the frame's strength (1.0 = 100%).
    pub bonus: f64,
}

impl StrengthWindow {
    fn live_at(&self, t: f64) -> bool {
        self.starts_at_seconds <= t && t < self.ends_at_seconds
    }
}

/// When the Exalted weapon came out, and the strength its cast snapshotted.
/// `at_seconds` is `f64::NEG_INFINITY` for one assumed out before the fight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summon {
    pub at_seconds: f64,
    pub strength: f64,
}

/// The frame's side of one fight.
#[derive(Debug, Clone, PartialEq)]
pub struct FramePlan {
    /// Every ability window: one entry per CAST for an ability the list names,
    /// the assumed entry untouched for one it does not.
    pub abilities: Vec<ActiveAbility>,
    /// When the player is busy and not attacking, in time order: `(at, seconds)`.
    pub interrupts: Vec<(f64, f64)>,
    pub strength: Vec<StrengthWindow>,
    pub summon: Summon,
}

/// Who is doing it: the frame's stats, what it has picked, and the Operator.
pub struct Frame<'a> {
    /// `strength` is the frame's own, before any window below.
    pub caster: Caster<'a>,
    pub picks: &'a [AbilityPick<'a>],
    /// The picks resolved at `caster` — the family contest already settled.
    pub assumed: &'a [ActiveAbility],
    /// The linked Operator's active Focus school.
    pub school: &'a str,
    /// The ability that summons the weapon being fought with, if it is Exalted.
    pub summoned_by: Option<&'a str>,
    pub weapon_class: &'a str,
    pub weapon_slot: &'a str,
}

/// The frame's Ability Strength at `t`: its own plus every source live then.
/// A source counts ONCE however many of its windows overlap — earning a buff
/// again refreshes it, never stacks it.
pub fn strength_at(base: f64, windows: &[StrengthWindow], t: f64) -> f64 {
    let mut by_source: Vec<(&str, f64)> = Vec::new();
    for w in windows.iter().filter(|w| w.live_at(t)) {
        match by_source.iter_mut().find(|(f, _)| *f == w.from) {
            Some(slot) => slot.1 = slot.1.max(w.bonus),
            None => by_source.push((&w.from, w.bonus)),
        }
    }
    base + by_source.iter().map(|(_, b)| b).sum::<f64>()
}

/// The school's nodes an action earns, as `(id, seconds, strength bonus)`.
fn earned_by(school: &str, trigger: NodeTrigger) -> Vec<(String, f64, f64)> {
    focus_school(school).map_or_else(Vec::new, |s| {
        s.nodes
            .iter()
            .filter_map(|n| {
                let (tr, secs) = n.trigger?;
                (tr == trigger).then(|| {
                    let bonus = n
                        .effects
                        .iter()
                        .filter_map(|e| match e {
                            FrameEffect::Bonus(FrameStat::AbilityStrength, v) => Some(*v),
                            _ => None,
                        })
                        .sum();
                    (n.id.clone(), secs, bonus)
                })
            })
            .collect()
    })
}

/// **LAY OUT THE FRAME'S SIDE OF THE FIGHT.** Energy is not spent: the pool is
/// unlimited until regeneration is modelled, so a named ability is recast
/// whenever its window lapses (or `lead` seconds before; `if=once` never), and
/// an ability whose price no source states is still never cast — it keeps the
/// assumed reading.
pub fn plan(apl: &Apl, frame: &Frame<'_>, fight_seconds: f64) -> FramePlan {
    let actions = apl.planned();
    let base = frame.caster.strength;
    let named = |a: &ActiveAbility| {
        actions.iter().any(|(act, _)| matches!(act, Action::Cast { ability } if ability == a.id))
            && a.energy_cost.is_some()
    };
    let mut abilities: Vec<ActiveAbility> = frame.assumed.iter().filter(|a| !named(a)).cloned().collect();
    let mut interrupts = Vec::new();
    let mut strength: Vec<StrengthWindow> = Vec::new();
    let mut summon = Summon { at_seconds: f64::NEG_INFINITY, strength: base };
    // `(due, index into actions)`, all due at the buzzer.
    let mut due: Vec<(f64, usize)> = (0..actions.len()).map(|k| (0.0, k)).collect();
    let mut busy = 0.0f64;
    while let Some(pos) = due
        .iter()
        .enumerate()
        .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.1 .1.cmp(&b.1 .1)))
        .map(|(i, _)| i)
    {
        let (at, k) = due.remove(pos);
        let start = at.max(busy);
        if start >= fight_seconds {
            continue;
        }
        let (action, lead) = actions[k];
        match action {
            Action::OperatorSling => {
                let secs = OPERATOR_SLING_SECONDS_UNMEASURED;
                interrupts.push((start, secs));
                busy = start + secs;
                let earned = earned_by(frame.school, NodeTrigger::OperatorSling);
                // A SLING THAT EARNS NOTHING IS DONE ONCE: there is no window
                // whose lapse would call for another.
                let next = earned.iter().map(|(_, s, _)| busy + s).fold(f64::INFINITY, f64::min);
                for (from, s, bonus) in earned {
                    strength.push(StrengthWindow { from, starts_at_seconds: busy, ends_at_seconds: busy + s, bonus });
                }
                if next.is_finite() && lead.is_finite() {
                    due.push(((next - lead).max(busy), k));
                }
            }
            Action::Cast { ability } if frame.summoned_by == Some(ability.as_str()) => {
                // ONCE: the pool never runs dry, so the channel is never dropped.
                if summon.at_seconds.is_finite() {
                    continue;
                }
                let secs = CAST_SECONDS_UNMEASURED / (1.0 + frame.caster.casting_speed_bonus);
                summon = Summon { at_seconds: start, strength: strength_at(base, &strength, start) };
                interrupts.push((start, secs));
                busy = start + secs;
            }
            Action::Cast { ability } => {
                let Some(a) = frame.assumed.iter().find(|a| a.id == ability && a.energy_cost.is_some()) else {
                    continue;
                };
                let Some(pick) = frame.picks.iter().find(|p| p.id == a.id) else { continue };
                let at_strength = Caster { strength: strength_at(base, &strength, start), ..frame.caster };
                let Some(mut cast) =
                    abilities::resolve(std::slice::from_ref(pick), &at_strength, frame.weapon_class, frame.weapon_slot)
                        .into_iter()
                        .next()
                else {
                    continue;
                };
                cast.starts_at_seconds = start;
                cast.ends_at_seconds = start + cast.window_seconds;
                if cast.interrupts_fire {
                    interrupts.push((start, cast.cast_seconds));
                    busy = start + cast.cast_seconds;
                }
                if cast.ends_at_seconds.is_finite() && lead.is_finite() {
                    due.push(((cast.ends_at_seconds - lead).max(busy), k));
                }
                abilities.push(cast);
            }
            _ => unreachable!("`Apl::planned` yields planned actions only"),
        }
    }
    FramePlan { abilities, interrupts, strength, summon }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::apl::{Rule, When};

    fn rule(action: Action, when: When) -> Rule {
        Rule { action, when }
    }
    fn cast(id: &str) -> Action {
        Action::Cast { ability: id.into() }
    }
    fn frame<'a>(
        picks: &'a [AbilityPick<'a>],
        assumed: &'a [ActiveAbility],
        school: &'a str,
        summoned_by: Option<&'a str>,
        strength: f64,
    ) -> Frame<'a> {
        Frame {
            caster: Caster { strength, ..Default::default() },
            picks,
            assumed,
            school,
            summoned_by,
            weapon_class: "rifle",
            weapon_slot: "primary",
        }
    }
    fn warcry_picked(secs: f64) -> (Vec<AbilityPick<'static>>, Vec<ActiveAbility>) {
        let picks = vec![AbilityPick { id: "warcry", duration_seconds: Some(secs), element: None }];
        let assumed = abilities::resolve(&picks, &Caster::default(), "rifle", "primary");
        (picks, assumed)
    }
    fn fire_rate(a: &ActiveAbility) -> f64 {
        a.effects
            .iter()
            .find_map(|e| match e {
                abilities::AbilityEffect::FireRate(v) => Some(*v),
                _ => None,
            })
            .expect("warcry grants attack speed")
    }

    /// **THE POOL IS UNLIMITED, SO A NAMED ABILITY IS UP FROM ITS FIRST CAST
    /// TO THE END**, one entry per cast, each opening where the last closed.
    #[test]
    fn a_named_ability_is_recast_at_every_lapse() {
        let (picks, assumed) = warcry_picked(20.0);
        let apl = Apl(vec![rule(cast("warcry"), When::Always)]);
        let p = plan(&apl, &frame(&picks, &assumed, "vazarin", None, 1.0), 100.0);
        assert_eq!(p.abilities.len(), 5, "{:?}", p.abilities);
        assert_eq!(p.abilities[0].starts_at_seconds, 0.0);
        assert_eq!(p.abilities[1].starts_at_seconds, 20.0);
        assert_eq!(p.interrupts.len(), 5, "each cast took the trigger finger");
        assert!(!p.abilities.iter().any(|a| a.live_at(-1.0)), "nothing was up before the first cast");
    }

    /// AN ABILITY THE LIST NEVER NAMES IS ASSUMED UP, untouched, and costs
    /// nothing.
    #[test]
    fn an_ability_the_list_never_names_is_assumed_up() {
        let (picks, assumed) = warcry_picked(20.0);
        let p = plan(&Apl::default(), &frame(&picks, &assumed, "vazarin", None, 1.0), 100.0);
        assert_eq!(p.abilities, assumed);
        assert!(p.interrupts.is_empty());
    }

    /// **A CAST SNAPSHOTS THE STRENGTH OF ITS INSTANT.** Sling first, and the
    /// Warcry cast inside Sling Strength's 20 s keeps the +40% for its whole
    /// window, after the sling's own window has lapsed.
    #[test]
    fn a_cast_keeps_the_strength_it_was_cast_at() {
        let (picks, assumed) = warcry_picked(30.0);
        let apl = Apl(vec![
            rule(Action::OperatorSling, When::BuffRemainsUnder { ability: "sling_strength".into(), seconds: 0.0 }),
            rule(cast("warcry"), When::Always),
        ]);
        let p = plan(&apl, &frame(&picks, &assumed, "madurai", None, 1.0), 60.0);
        let sling = OPERATOR_SLING_SECONDS_UNMEASURED;
        assert_eq!(p.strength[0].starts_at_seconds, sling, "earned on the switch back");
        assert_eq!(p.strength[0].ends_at_seconds, sling + 20.0);
        let first = &p.abilities[0];
        assert_eq!(first.starts_at_seconds, sling, "one thing at a time");
        assert!((fire_rate(first) - fire_rate(&assumed[0]) * 1.4).abs() < 1e-9, "cast at 140%");
        assert!(first.live_at(sling + 25.0), "the sling lapsed, the snapshot did not");
        assert_eq!(p.interrupts[0], (0.0, sling));
    }

    /// **AN EXALTED WEAPON IS THE STRENGTH ITS SUMMONING READ** — and the same
    /// sling after the summon earns the claws nothing.
    #[test]
    fn the_summon_snapshots_the_strength_of_its_cast() {
        let sling = rule(Action::OperatorSling, When::Always);
        let hysteria = rule(cast("hysteria"), When::Always);
        let before = plan(&Apl(vec![sling.clone(), hysteria.clone()]), &frame(&[], &[], "madurai", Some("hysteria"), 2.0), 60.0);
        assert_eq!(before.summon.at_seconds, OPERATOR_SLING_SECONDS_UNMEASURED);
        assert!((before.summon.strength - 2.4).abs() < 1e-9);
        let after = plan(&Apl(vec![hysteria, sling]), &frame(&[], &[], "madurai", Some("hysteria"), 2.0), 60.0);
        assert_eq!(after.summon.at_seconds, 0.0);
        assert_eq!(after.summon.strength, 2.0);
        // …and one the list never casts was out before the fight, at the frame's own.
        let assumed = plan(&Apl::default(), &frame(&[], &[], "madurai", Some("hysteria"), 2.0), 60.0);
        assert_eq!(assumed.summon, Summon { at_seconds: f64::NEG_INFINITY, strength: 2.0 });
    }

    /// A SCHOOL WITH NOTHING TO EARN STILL PAYS THE TIME, once.
    #[test]
    fn a_sling_that_earns_nothing_costs_its_time_once() {
        let apl = Apl(vec![rule(Action::OperatorSling, When::Always)]);
        let p = plan(&apl, &frame(&[], &[], "vazarin", None, 1.0), 60.0);
        assert!(p.strength.is_empty());
        assert_eq!(p.interrupts, vec![(0.0, OPERATOR_SLING_SECONDS_UNMEASURED)]);
    }

    /// `remains<N` RE-SLINGS N SECONDS EARLY, so the window never lapses.
    #[test]
    fn a_lead_keeps_the_window_up() {
        let apl = Apl(vec![rule(
            Action::OperatorSling,
            When::BuffRemainsUnder { ability: "sling_strength".into(), seconds: OPERATOR_SLING_SECONDS_UNMEASURED },
        )]);
        let p = plan(&apl, &frame(&[], &[], "madurai", None, 1.0), 60.0);
        for pair in p.strength.windows(2) {
            assert!(pair[1].starts_at_seconds <= pair[0].ends_at_seconds, "{pair:?}");
        }
        // …and the overlap is a refresh, not a second +40%.
        let t = p.strength[1].starts_at_seconds;
        assert!((strength_at(1.0, &p.strength, t) - 1.4).abs() < 1e-9);
    }

    /// `if=once` IS ONE SLING: the claws' snapshot is taken, and nothing after
    /// it is worth standing still for.
    #[test]
    fn once_is_done_once() {
        let apl = Apl(vec![rule(Action::OperatorSling, When::Once), rule(cast("hysteria"), When::Always)]);
        let p = plan(&apl, &frame(&[], &[], "madurai", Some("hysteria"), 1.0), 120.0);
        assert_eq!(p.strength.len(), 1);
        assert_eq!(p.interrupts.len(), 2, "one sling and one summon: {:?}", p.interrupts);
        assert!((p.summon.strength - 1.4).abs() < 1e-9);
    }
}
